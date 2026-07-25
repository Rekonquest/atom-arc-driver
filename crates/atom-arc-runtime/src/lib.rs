//! Validated memory and command execution used before hardware submission is enabled.

use atom_arc_core::{
    ARC_A380_DEVICE_ID, DeviceResources, FirmwareEvidence, FirmwareVersion, INTEL_VENDOR_ID,
    PHASE0_HEARTBEATS_REQUIRED, PciIdentity, Phase0Event, Phase0State, SecuritySnapshot,
};
use atom_arc_dxcore::{MAX_COMPUTE_ELEMENTS, run_arc_a380_compute};
use atom_arc_model::{BufferId, BufferInfo, Command, CommandBuffer};
use atom_arc_platform::{EnvironmentKind, PlatformReport, probe_platform};
use atom_arc_security::{SecurityError, SecurityPolicy};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::ops::Range;
use std::time::Instant;
use thiserror::Error;

#[derive(Debug)]
pub struct Runtime {
    policy: SecurityPolicy,
    buffers: BTreeMap<BufferId, Vec<u8>>,
    next_buffer_id: u64,
    allocated_bytes: u64,
}

impl Runtime {
    #[must_use]
    pub fn new(policy: SecurityPolicy) -> Self {
        Self {
            policy,
            buffers: BTreeMap::new(),
            next_buffer_id: 1,
            allocated_bytes: 0,
        }
    }

    pub fn allocate(&mut self, bytes: u64) -> Result<BufferInfo, RuntimeError> {
        self.policy
            .validate_allocation(bytes, self.allocated_bytes)?;
        let size = usize::try_from(bytes)
            .map_err(|_| RuntimeError::HostAddressSpaceTooSmall { requested: bytes })?;
        let id = BufferId(self.next_buffer_id);
        self.next_buffer_id = self
            .next_buffer_id
            .checked_add(1)
            .ok_or(RuntimeError::BufferIdentifierExhausted)?;
        self.buffers.insert(id, vec![0; size]);
        self.allocated_bytes += bytes;
        Ok(BufferInfo { id, size: bytes })
    }

    pub fn free(&mut self, id: BufferId) -> Result<(), RuntimeError> {
        let buffer = self
            .buffers
            .remove(&id)
            .ok_or(RuntimeError::MissingBuffer { id })?;
        self.allocated_bytes -= buffer.len() as u64;
        Ok(())
    }

    pub fn write(&mut self, id: BufferId, offset: u64, data: &[u8]) -> Result<(), RuntimeError> {
        let buffer = self
            .buffers
            .get_mut(&id)
            .ok_or(RuntimeError::MissingBuffer { id })?;
        let range = checked_range(id, offset, data.len() as u64, buffer.len())?;
        buffer[range].copy_from_slice(data);
        Ok(())
    }

    pub fn read(&self, id: BufferId, offset: u64, bytes: u64) -> Result<&[u8], RuntimeError> {
        let buffer = self
            .buffers
            .get(&id)
            .ok_or(RuntimeError::MissingBuffer { id })?;
        let range = checked_range(id, offset, bytes, buffer.len())?;
        Ok(&buffer[range])
    }

    pub fn submit(&mut self, commands: &CommandBuffer) -> Result<SubmissionReceipt, RuntimeError> {
        self.validate_submission(commands)?;

        let mut transferred_bytes = 0_u64;
        for command in commands.commands() {
            match command {
                Command::Noop | Command::Barrier => {}
                Command::Fill {
                    destination,
                    offset,
                    bytes,
                    value,
                } => {
                    let buffer = self
                        .buffers
                        .get_mut(destination)
                        .ok_or(RuntimeError::MissingBuffer { id: *destination })?;
                    let range = checked_range(*destination, *offset, *bytes, buffer.len())?;
                    buffer[range].fill(*value);
                    transferred_bytes += bytes;
                }
                Command::Copy {
                    source,
                    source_offset,
                    destination,
                    destination_offset,
                    bytes,
                } => {
                    let source_bytes = self.read(*source, *source_offset, *bytes)?.to_vec();
                    self.write(*destination, *destination_offset, &source_bytes)?;
                    transferred_bytes += bytes;
                }
                Command::Dispatch { .. } => return Err(RuntimeError::DispatchUnavailable),
            }
        }

        Ok(SubmissionReceipt {
            command_count: commands.len(),
            transferred_bytes,
            backend: "cpu-reference".to_owned(),
        })
    }

    /// Execute the built-in kernel-1 compute workload through the legacy WSL backend.
    ///
    /// Kernel 1 has a fixed 64-thread X dimension and writes deterministic values
    /// into a backend-owned output buffer. User buffer bindings remain fail-closed
    /// until the cross-process memory protocol is implemented.
    pub fn submit_hardware(
        &self,
        commands: &CommandBuffer,
        addend: u32,
        timeout_ms: u32,
    ) -> Result<HardwareSubmissionReceipt, RuntimeError> {
        let dispatch = self.prepare_hardware_dispatch(commands)?;
        let platform = probe_platform();
        if platform.environment != EnvironmentKind::Wsl2 {
            return Err(RuntimeError::LegacyDxcoreTransportUnavailable {
                environment: format!("{:?}", platform.environment),
            });
        }
        let report = run_arc_a380_compute(dispatch.elements, addend, timeout_ms)
            .map_err(|error| RuntimeError::HardwareBackend(error.to_string()))?;
        if !report.is_verified_arc_a380_compute(dispatch.elements, addend) {
            return Err(RuntimeError::HardwareVerificationFailed {
                stage: report.stage,
                detail: report.detail,
            });
        }

        Ok(HardwareSubmissionReceipt {
            command_count: commands.len(),
            kernel_id: dispatch.kernel_id,
            groups: dispatch.groups,
            compute_elements: report.compute_elements,
            compute_input_bytes: report.compute_input_bytes,
            compute_output_bytes: report.compute_output_bytes,
            compute_checksum: report.compute_checksum,
            elapsed_microseconds: report.elapsed_microseconds,
            backend: report.backend,
            detail: report.detail,
        })
    }

    #[must_use]
    pub const fn allocated_bytes(&self) -> u64 {
        self.allocated_bytes
    }

    fn validate_submission(&self, commands: &CommandBuffer) -> Result<(), RuntimeError> {
        self.policy.validate_commands(commands)?;
        for (index, command) in commands.commands().iter().enumerate() {
            match command {
                Command::Fill {
                    destination,
                    offset,
                    bytes,
                    ..
                } => {
                    self.validate_buffer_range(index, *destination, *offset, *bytes)?;
                }
                Command::Copy {
                    source,
                    source_offset,
                    destination,
                    destination_offset,
                    bytes,
                } => {
                    self.validate_buffer_range(index, *source, *source_offset, *bytes)?;
                    self.validate_buffer_range(index, *destination, *destination_offset, *bytes)?;
                }
                Command::Dispatch { .. } => return Err(RuntimeError::DispatchUnavailable),
                Command::Noop | Command::Barrier => {}
            }
        }
        Ok(())
    }

    fn prepare_hardware_dispatch(
        &self,
        commands: &CommandBuffer,
    ) -> Result<PreparedHardwareDispatch, RuntimeError> {
        self.policy.validate_commands(commands)?;
        let mut prepared = None;
        for (command_index, command) in commands.commands().iter().enumerate() {
            match command {
                Command::Noop | Command::Barrier => {}
                Command::Dispatch {
                    kernel_id,
                    groups,
                    bindings,
                } => {
                    if prepared.is_some() {
                        return Err(RuntimeError::MultipleHardwareDispatches);
                    }
                    if *kernel_id != 1 {
                        return Err(RuntimeError::UnknownHardwareKernel {
                            kernel_id: *kernel_id,
                        });
                    }
                    if !bindings.is_empty() {
                        return Err(RuntimeError::HardwareBindingsUnavailable {
                            binding_count: bindings.len(),
                        });
                    }
                    if groups[1] != 1 || groups[2] != 1 {
                        return Err(RuntimeError::UnsupportedHardwareGrid { groups: *groups });
                    }
                    let elements = groups[0].checked_mul(64).ok_or(
                        RuntimeError::HardwareDispatchTooLarge {
                            groups_x: groups[0],
                            maximum_elements: MAX_COMPUTE_ELEMENTS,
                        },
                    )?;
                    if elements > MAX_COMPUTE_ELEMENTS {
                        return Err(RuntimeError::HardwareDispatchTooLarge {
                            groups_x: groups[0],
                            maximum_elements: MAX_COMPUTE_ELEMENTS,
                        });
                    }
                    prepared = Some(PreparedHardwareDispatch {
                        kernel_id: *kernel_id,
                        groups: *groups,
                        elements,
                    });
                }
                Command::Fill { .. } | Command::Copy { .. } => {
                    return Err(RuntimeError::UnsupportedHardwareCommand { command_index });
                }
            }
        }
        prepared.ok_or(RuntimeError::HardwareDispatchMissing)
    }

    fn validate_buffer_range(
        &self,
        command_index: usize,
        id: BufferId,
        offset: u64,
        bytes: u64,
    ) -> Result<(), RuntimeError> {
        let buffer = self
            .buffers
            .get(&id)
            .ok_or(RuntimeError::CommandUsesMissingBuffer { command_index, id })?;
        checked_range(id, offset, bytes, buffer.len())?;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PreparedHardwareDispatch {
    kernel_id: u64,
    groups: [u32; 3],
    elements: u32,
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new(SecurityPolicy::default())
    }
}

fn checked_range(
    id: BufferId,
    offset: u64,
    bytes: u64,
    buffer_len: usize,
) -> Result<Range<usize>, RuntimeError> {
    let end = offset
        .checked_add(bytes)
        .ok_or(RuntimeError::BufferRangeOverflow { id, offset, bytes })?;
    let buffer_size = buffer_len as u64;
    if end > buffer_size {
        return Err(RuntimeError::BufferRangeOutOfBounds {
            id,
            offset,
            bytes,
            buffer_size,
        });
    }
    let start = usize::try_from(offset)
        .map_err(|_| RuntimeError::HostAddressSpaceTooSmall { requested: offset })?;
    let end = usize::try_from(end)
        .map_err(|_| RuntimeError::HostAddressSpaceTooSmall { requested: end })?;
    Ok(start..end)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubmissionReceipt {
    pub command_count: usize,
    pub transferred_bytes: u64,
    pub backend: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HardwareSubmissionReceipt {
    pub command_count: usize,
    pub kernel_id: u64,
    pub groups: [u32; 3],
    pub compute_elements: u32,
    pub compute_input_bytes: u64,
    pub compute_output_bytes: u64,
    pub compute_checksum: u64,
    pub elapsed_microseconds: u64,
    pub backend: String,
    pub detail: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HardwareStressReport {
    pub passed: bool,
    pub iterations: u32,
    pub groups: [u32; 3],
    pub elements_per_iteration: u32,
    pub total_elements: u64,
    pub wall_microseconds: u64,
    pub backend_microseconds: u64,
    pub minimum_backend_microseconds: u64,
    pub maximum_backend_microseconds: u64,
    pub dispatches_per_second: u64,
    pub elements_per_second: u64,
    pub backend: String,
}

pub fn run_hardware_stress(
    groups_x: u32,
    addend: u32,
    iterations: u32,
    timeout_ms: u32,
) -> Result<HardwareStressReport, RuntimeError> {
    if iterations == 0 || iterations > 100 {
        return Err(RuntimeError::InvalidStressIterations { iterations });
    }
    let commands = CommandBuffer::new().with_command(Command::Dispatch {
        kernel_id: 1,
        groups: [groups_x, 1, 1],
        bindings: Vec::new(),
    });
    let runtime = Runtime::default();
    let started = Instant::now();
    let mut backend_microseconds = 0_u64;
    let mut minimum_backend_microseconds = u64::MAX;
    let mut maximum_backend_microseconds = 0_u64;
    let mut elements_per_iteration = 0_u32;
    let mut backend = String::new();

    for iteration in 0..iterations {
        let receipt =
            runtime.submit_hardware(&commands, addend.wrapping_add(iteration), timeout_ms)?;
        elements_per_iteration = receipt.compute_elements;
        backend_microseconds = backend_microseconds
            .checked_add(receipt.elapsed_microseconds)
            .ok_or(RuntimeError::StressAccountingOverflow)?;
        minimum_backend_microseconds =
            minimum_backend_microseconds.min(receipt.elapsed_microseconds);
        maximum_backend_microseconds =
            maximum_backend_microseconds.max(receipt.elapsed_microseconds);
        backend = receipt.backend;
    }

    let wall_microseconds = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX);
    let total_elements = u64::from(elements_per_iteration)
        .checked_mul(u64::from(iterations))
        .ok_or(RuntimeError::StressAccountingOverflow)?;
    let dispatches_per_second = rate_per_second(u64::from(iterations), wall_microseconds);
    let elements_per_second = rate_per_second(total_elements, wall_microseconds);

    Ok(HardwareStressReport {
        passed: true,
        iterations,
        groups: [groups_x, 1, 1],
        elements_per_iteration,
        total_elements,
        wall_microseconds,
        backend_microseconds,
        minimum_backend_microseconds,
        maximum_backend_microseconds,
        dispatches_per_second,
        elements_per_second,
        backend,
    })
}

fn rate_per_second(units: u64, microseconds: u64) -> u64 {
    if microseconds == 0 {
        return 0;
    }
    units.saturating_mul(1_000_000) / microseconds
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelfTestCheck {
    pub name: String,
    pub passed: bool,
    pub detail: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelfTestReport {
    pub passed: bool,
    pub checks: Vec<SelfTestCheck>,
    pub platform: PlatformReport,
}

#[must_use]
pub fn run_self_test() -> SelfTestReport {
    let mut checks = Vec::new();
    let mut runtime = Runtime::default();

    let transfer_result = (|| -> Result<SubmissionReceipt, RuntimeError> {
        let source = runtime.allocate(64)?;
        let destination = runtime.allocate(64)?;
        let commands = CommandBuffer::new()
            .with_command(Command::Fill {
                destination: source.id,
                offset: 0,
                bytes: 32,
                value: 0xa5,
            })
            .with_command(Command::Barrier)
            .with_command(Command::Copy {
                source: source.id,
                source_offset: 0,
                destination: destination.id,
                destination_offset: 16,
                bytes: 32,
            });
        let receipt = runtime.submit(&commands)?;
        if runtime.read(destination.id, 16, 32)? != [0xa5; 32] {
            return Err(RuntimeError::SelfTestMismatch);
        }
        runtime.free(source.id)?;
        runtime.free(destination.id)?;
        Ok(receipt)
    })();
    checks.push(match transfer_result {
        Ok(receipt) => SelfTestCheck {
            name: "validated-fill-copy".to_owned(),
            passed: true,
            detail: format!(
                "{} commands transferred {} bytes through {}",
                receipt.command_count, receipt.transferred_bytes, receipt.backend
            ),
        },
        Err(error) => SelfTestCheck {
            name: "validated-fill-copy".to_owned(),
            passed: false,
            detail: error.to_string(),
        },
    });

    let stale_result = (|| -> Result<(), RuntimeError> {
        let stale = runtime.allocate(8)?;
        runtime.free(stale.id)?;
        let replacement = runtime.allocate(8)?;
        if replacement.id == stale.id {
            return Err(RuntimeError::SelfTestIdentifierReuse);
        }
        match runtime.read(stale.id, 0, 1) {
            Err(RuntimeError::MissingBuffer { .. }) => {}
            Err(error) => return Err(error),
            Ok(_) => return Err(RuntimeError::SelfTestAcceptedStaleHandle),
        }
        runtime.free(replacement.id)?;
        Ok(())
    })();
    checks.push(SelfTestCheck {
        name: "stale-handle-rejection".to_owned(),
        passed: stale_result.is_ok(),
        detail: stale_result.map_or_else(
            |error| error.to_string(),
            |()| "Freed identifiers are never reused and stale access is rejected".to_owned(),
        ),
    });

    let dispatch = CommandBuffer::new().with_command(Command::Dispatch {
        kernel_id: 1,
        groups: [1, 1, 1],
        bindings: Vec::new(),
    });
    let dispatch_result = runtime.submit(&dispatch);
    let dispatch_passed = matches!(dispatch_result, Err(RuntimeError::DispatchUnavailable));
    checks.push(SelfTestCheck {
        name: "reference-dispatch-fails-closed".to_owned(),
        passed: dispatch_passed,
        detail: if dispatch_passed {
            "The CPU reference path rejects dispatch; hardware dispatch requires the explicit validated backend".to_owned()
        } else {
            format!("Unexpected dispatch result: {dispatch_result:?}")
        },
    });

    let no_leak = runtime.allocated_bytes() == 0;
    checks.push(SelfTestCheck {
        name: "allocation-accounting".to_owned(),
        passed: no_leak,
        detail: format!("{} bytes remain allocated", runtime.allocated_bytes()),
    });

    let phase0_result = run_future_policy_model_self_test();
    checks.push(SelfTestCheck {
        name: "future-phase-policy-model".to_owned(),
        passed: phase0_result.is_ok(),
        detail: phase0_result
            .map(|detail| format!("In-memory model only; no hardware exercised: {detail}"))
            .unwrap_or_else(|error| error),
    });

    SelfTestReport {
        passed: checks.iter().all(|check| check.passed),
        checks,
        platform: probe_platform(),
    }
}

fn run_future_policy_model_self_test() -> Result<String, String> {
    let mut state = Phase0State::new();
    let events = [
        Phase0Event::DeviceClaimed(DeviceResources {
            identity: PciIdentity {
                vendor: INTEL_VENDOR_ID,
                device: ARC_A380_DEVICE_ID,
            },
            bar0_bytes: 16 * 1024 * 1024,
            local_memory_bytes: 6 * 1024 * 1024 * 1024,
        }),
        Phase0Event::MmioMapped,
        Phase0Event::DmaAddressingValidated { address_bits: 46 },
        Phase0Event::FirmwareContainersValidated(FirmwareEvidence {
            guc: FirmwareVersion {
                major: 70,
                minor: 36,
                patch: 0,
                build: 0,
            },
            huc: FirmwareVersion {
                major: 7,
                minor: 10,
                patch: 3,
                build: 1416,
            },
            huc_security_version: 1,
            guc_production: true,
            blobs_validated: true,
        }),
        Phase0Event::SecuritySampled(SecuritySnapshot {
            guc_status_raw: 0x8000_0100,
            boot_hash_raw: 0x8000_0000,
            wopcm_size_raw: 1,
            wopcm_offset_raw: 1,
        }),
    ];
    for event in events {
        state
            .apply(event)
            .map_err(|error| format!("Phase 0 transition failed: {error:?}"))?;
    }
    for sequence in 1..=u64::from(PHASE0_HEARTBEATS_REQUIRED) {
        state
            .apply(Phase0Event::Heartbeat { sequence })
            .map_err(|error| format!("Phase 0 heartbeat failed: {error:?}"))?;
    }
    state
        .apply(Phase0Event::AuthorizeSubmission)
        .map_err(|error| format!("Phase 0 authorization failed: {error:?}"))?;
    if !state.submission_enabled() {
        return Err("Phase 0 did not authorize submission after qualification".to_owned());
    }
    state
        .apply(Phase0Event::Reset)
        .map_err(|error| format!("Phase 0 reset failed: {error:?}"))?;
    if state.submission_enabled() {
        return Err("Phase 0 reset did not relock submission".to_owned());
    }
    Ok(format!(
        "future policy qualified {PHASE0_HEARTBEATS_REQUIRED} ordered heartbeats, then reset relocked submission"
    ))
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RuntimeError {
    #[error(transparent)]
    Security(#[from] SecurityError),
    #[error("buffer {id:?} does not exist or has been freed")]
    MissingBuffer { id: BufferId },
    #[error("command {command_index} references missing buffer {id:?}")]
    CommandUsesMissingBuffer { command_index: usize, id: BufferId },
    #[error("buffer {id:?} range overflow: offset {offset}, bytes {bytes}")]
    BufferRangeOverflow {
        id: BufferId,
        offset: u64,
        bytes: u64,
    },
    #[error(
        "buffer {id:?} range is out of bounds: offset {offset}, bytes {bytes}, size {buffer_size}"
    )]
    BufferRangeOutOfBounds {
        id: BufferId,
        offset: u64,
        bytes: u64,
        buffer_size: u64,
    },
    #[error("host address space cannot represent {requested} bytes")]
    HostAddressSpaceTooSmall { requested: u64 },
    #[error("buffer identifier space is exhausted")]
    BufferIdentifierExhausted,
    #[error("GPU dispatch is unavailable on the CPU reference path; use submit_hardware")]
    DispatchUnavailable,
    #[error(
        "legacy DxCore submission is WSL-only, not the standalone driver path; detected {environment}"
    )]
    LegacyDxcoreTransportUnavailable { environment: String },
    #[error("hardware submission requires exactly one dispatch command")]
    HardwareDispatchMissing,
    #[error("hardware submission currently accepts one dispatch command at a time")]
    MultipleHardwareDispatches,
    #[error("hardware kernel {kernel_id} is not registered; kernel 1 is available")]
    UnknownHardwareKernel { kernel_id: u64 },
    #[error("hardware buffer bindings are not enabled yet; received {binding_count} bindings")]
    HardwareBindingsUnavailable { binding_count: usize },
    #[error("hardware kernel 1 only supports an X grid; received {groups:?}")]
    UnsupportedHardwareGrid { groups: [u32; 3] },
    #[error(
        "hardware dispatch with {groups_x} X groups exceeds {maximum_elements} output elements"
    )]
    HardwareDispatchTooLarge {
        groups_x: u32,
        maximum_elements: u32,
    },
    #[error("command {command_index} is not supported by the WSL hardware backend")]
    UnsupportedHardwareCommand { command_index: usize },
    #[error("hardware backend failed: {0}")]
    HardwareBackend(String),
    #[error("hardware result failed independent verification at {stage}: {detail}")]
    HardwareVerificationFailed { stage: String, detail: String },
    #[error("hardware stress iterations {iterations} is outside the range 1..=100")]
    InvalidStressIterations { iterations: u32 },
    #[error("hardware stress accounting overflowed")]
    StressAccountingOverflow,
    #[error("self-test copied bytes did not match the source")]
    SelfTestMismatch,
    #[error("self-test observed buffer identifier reuse")]
    SelfTestIdentifierReuse,
    #[error("self-test accepted a stale buffer handle")]
    SelfTestAcceptedStaleHandle,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn self_test_passes() {
        let report = run_self_test();
        assert!(report.passed, "{:#?}", report.checks);
    }

    #[test]
    fn invalid_late_command_does_not_partially_execute() {
        let mut runtime = Runtime::default();
        let destination = runtime.allocate(16).expect("test allocation");
        let commands = CommandBuffer::new()
            .with_command(Command::Fill {
                destination: destination.id,
                offset: 0,
                bytes: 8,
                value: 0xff,
            })
            .with_command(Command::Copy {
                source: BufferId(999),
                source_offset: 0,
                destination: destination.id,
                destination_offset: 8,
                bytes: 8,
            });

        assert!(runtime.submit(&commands).is_err());
        assert_eq!(
            runtime.read(destination.id, 0, 16).expect("test read"),
            [0; 16]
        );
    }

    #[test]
    fn overlapping_copy_uses_memmove_semantics() {
        let mut runtime = Runtime::default();
        let buffer = runtime.allocate(8).expect("test allocation");
        runtime
            .write(buffer.id, 0, &[1, 2, 3, 4, 5, 6, 7, 8])
            .expect("test write");
        let commands = CommandBuffer::new().with_command(Command::Copy {
            source: buffer.id,
            source_offset: 0,
            destination: buffer.id,
            destination_offset: 2,
            bytes: 6,
        });

        runtime.submit(&commands).expect("test submit");
        assert_eq!(
            runtime.read(buffer.id, 0, 8).expect("test read"),
            [1, 2, 1, 2, 3, 4, 5, 6]
        );
    }

    #[test]
    fn prepares_registered_hardware_dispatch() {
        let runtime = Runtime::default();
        let commands = CommandBuffer::new()
            .with_command(Command::Noop)
            .with_command(Command::Dispatch {
                kernel_id: 1,
                groups: [16, 1, 1],
                bindings: Vec::new(),
            });

        let prepared = runtime
            .prepare_hardware_dispatch(&commands)
            .expect("valid built-in dispatch");
        assert_eq!(prepared.elements, 1024);
    }

    #[test]
    fn rejects_unregistered_hardware_kernel() {
        let runtime = Runtime::default();
        let commands = CommandBuffer::new().with_command(Command::Dispatch {
            kernel_id: 99,
            groups: [1, 1, 1],
            bindings: Vec::new(),
        });

        assert_eq!(
            runtime.prepare_hardware_dispatch(&commands),
            Err(RuntimeError::UnknownHardwareKernel { kernel_id: 99 })
        );
    }

    #[test]
    fn rejects_hardware_dispatch_over_element_limit() {
        let runtime = Runtime::default();
        let commands = CommandBuffer::new().with_command(Command::Dispatch {
            kernel_id: 1,
            groups: [16_385, 1, 1],
            bindings: Vec::new(),
        });

        assert_eq!(
            runtime.prepare_hardware_dispatch(&commands),
            Err(RuntimeError::HardwareDispatchTooLarge {
                groups_x: 16_385,
                maximum_elements: MAX_COMPUTE_ELEMENTS,
            })
        );
    }

    #[test]
    fn stress_iteration_limit_fails_before_hardware_start() {
        assert_eq!(
            run_hardware_stress(1, 0, 0, 5_000),
            Err(RuntimeError::InvalidStressIterations { iterations: 0 })
        );
    }

    #[test]
    fn integer_rate_is_bounded_and_deterministic() {
        assert_eq!(rate_per_second(10, 2_000_000), 5);
        assert_eq!(rate_per_second(10, 0), 0);
    }
}
