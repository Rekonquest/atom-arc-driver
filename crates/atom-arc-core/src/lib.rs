#![no_std]
//! OS-agnostic Intel Arc A380 bring-up state and kernel-service boundary.
//!
//! The core never owns PCI, maps memory, performs DMA, or publishes telemetry
//! itself. Atom OS and Linux adapters provide those mechanisms. The state
//! machine owns ordering and keeps submission fail-closed across faults,
//! authentication loss, and resets.

pub const INTEL_VENDOR_ID: u16 = 0x8086;
pub const ARC_A380_DEVICE_ID: u16 = 0x56a5;
pub const MINIMUM_DMA_ADDRESS_BITS: u8 = 46;
pub const PHASE0_HEARTBEATS_REQUIRED: u32 = 1_000;

pub const GUC_STATUS: u32 = 0x0000_c000;
pub const BOOT_HASH_CHECK: u32 = 0x0000_c010;
pub const GUC_WOPCM_SIZE: u32 = 0x0000_c050;
pub const DMA_GUC_WOPCM_OFFSET: u32 = 0x0000_c340;

const GUC_AUTH_MASK: u32 = 0xc000_0000;
const GUC_AUTH_GOOD: u32 = 0x8000_0000;
const GUC_MIA_IN_RESET: u32 = 0x0000_0001;
const BOOT_UKERNEL_VALID: u32 = 0x8000_0000;
const WOPCM_SIZE_LOCKED: u32 = 0x0000_0001;
const WOPCM_OFFSET_VALID: u32 = 0x0000_0001;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PciIdentity {
    pub vendor: u16,
    pub device: u16,
}

impl PciIdentity {
    #[must_use]
    pub const fn is_supported_a380(self) -> bool {
        self.vendor == INTEL_VENDOR_ID && self.device == ARC_A380_DEVICE_ID
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeviceResources {
    pub identity: PciIdentity,
    pub bar0_bytes: u64,
    pub local_memory_bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FirmwareVersion {
    pub major: u16,
    pub minor: u16,
    pub patch: u16,
    pub build: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FirmwareEvidence {
    pub guc: FirmwareVersion,
    pub huc: FirmwareVersion,
    pub huc_security_version: u32,
    pub guc_production: bool,
    pub blobs_validated: bool,
}

impl FirmwareEvidence {
    #[must_use]
    pub const fn is_acceptable_dg2(self) -> bool {
        self.blobs_validated
            && self.guc_production
            && self.guc.major == 70
            && self.huc_security_version > 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SecuritySnapshot {
    pub guc_status_raw: u32,
    pub boot_hash_raw: u32,
    pub wopcm_size_raw: u32,
    pub wopcm_offset_raw: u32,
}

impl SecuritySnapshot {
    #[must_use]
    pub const fn guc_auth_status(self) -> u8 {
        ((self.guc_status_raw >> 30) & 0x3) as u8
    }

    #[must_use]
    pub const fn guc_bootrom_status(self) -> u8 {
        ((self.guc_status_raw >> 1) & 0x7f) as u8
    }

    #[must_use]
    pub const fn guc_ukernel_status(self) -> u8 {
        ((self.guc_status_raw >> 8) & 0xff) as u8
    }

    #[must_use]
    pub const fn guc_in_reset(self) -> bool {
        self.guc_status_raw & GUC_MIA_IN_RESET != 0
    }

    #[must_use]
    pub const fn boot_hash_valid(self) -> bool {
        self.boot_hash_raw & BOOT_UKERNEL_VALID != 0
    }

    #[must_use]
    pub const fn wopcm_locked(self) -> bool {
        self.wopcm_size_raw & WOPCM_SIZE_LOCKED != 0
            && self.wopcm_offset_raw & WOPCM_OFFSET_VALID != 0
    }

    #[must_use]
    pub const fn authenticated(self) -> bool {
        self.guc_status_raw & GUC_AUTH_MASK == GUC_AUTH_GOOD
            && !self.guc_in_reset()
            && self.boot_hash_valid()
            && self.wopcm_locked()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FirmwareImage {
    Guc,
    Huc,
}

/// Kernel mechanisms supplied by Atom OS directly or mapped by the Linux adapter.
pub trait KernelAdapter {
    type Error;

    fn claim_a380(&mut self) -> Result<DeviceResources, Self::Error>;
    fn map_bar0(&mut self) -> Result<(), Self::Error>;
    fn configure_dma_addressing(&mut self, address_bits: u8) -> Result<(), Self::Error>;
    fn read_firmware(
        &mut self,
        image: FirmwareImage,
        destination: &mut [u8],
    ) -> Result<usize, Self::Error>;
    fn read_mmio32(&self, offset: u32) -> Result<u32, Self::Error>;
    fn monotonic_ticks(&self) -> u64;
    fn publish_phase0(&mut self, state: &Phase0State) -> Result<(), Self::Error>;
    fn lock_submission(&mut self) -> Result<(), Self::Error>;
}

pub fn read_security_snapshot<A: KernelAdapter>(adapter: &A) -> Result<SecuritySnapshot, A::Error> {
    Ok(SecuritySnapshot {
        guc_status_raw: adapter.read_mmio32(GUC_STATUS)?,
        boot_hash_raw: adapter.read_mmio32(BOOT_HASH_CHECK)?,
        wopcm_size_raw: adapter.read_mmio32(GUC_WOPCM_SIZE)?,
        wopcm_offset_raw: adapter.read_mmio32(DMA_GUC_WOPCM_OFFSET)?,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase0Stage {
    Detached,
    DeviceOwned,
    MmioMapped,
    DmaAddressingValidated,
    FirmwareContainersValidated,
    HardwareAuthenticated,
    HeartbeatQualified,
    SubmissionReady,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DriverFault {
    DeviceRemoved,
    FirmwareRejected,
    AuthenticationLost,
    HeartbeatStalled,
    ResetTimeout,
    DataMismatch,
    AdapterFailure,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase0Event {
    DeviceClaimed(DeviceResources),
    MmioMapped,
    DmaAddressingValidated { address_bits: u8 },
    FirmwareContainersValidated(FirmwareEvidence),
    SecuritySampled(SecuritySnapshot),
    Heartbeat { sequence: u64 },
    AuthorizeSubmission,
    Fault(DriverFault),
    Reset,
}

impl Phase0Event {
    const fn kind(self) -> Phase0EventKind {
        match self {
            Self::DeviceClaimed(_) => Phase0EventKind::DeviceClaimed,
            Self::MmioMapped => Phase0EventKind::MmioMapped,
            Self::DmaAddressingValidated { .. } => Phase0EventKind::DmaAddressingValidated,
            Self::FirmwareContainersValidated(_) => Phase0EventKind::FirmwareContainersValidated,
            Self::SecuritySampled(_) => Phase0EventKind::SecuritySampled,
            Self::Heartbeat { .. } => Phase0EventKind::Heartbeat,
            Self::AuthorizeSubmission => Phase0EventKind::AuthorizeSubmission,
            Self::Fault(_) => Phase0EventKind::Fault,
            Self::Reset => Phase0EventKind::Reset,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase0EventKind {
    DeviceClaimed,
    MmioMapped,
    DmaAddressingValidated,
    FirmwareContainersValidated,
    SecuritySampled,
    Heartbeat,
    AuthorizeSubmission,
    Fault,
    Reset,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransitionOutcome {
    pub stage: Phase0Stage,
    pub authentication_complete: bool,
    pub consecutive_heartbeats: u32,
    pub submission_enabled: bool,
    pub reset_count: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Phase0State {
    stage: Phase0Stage,
    resources: Option<DeviceResources>,
    dma_address_bits: Option<u8>,
    firmware: Option<FirmwareEvidence>,
    security: Option<SecuritySnapshot>,
    last_heartbeat_sequence: Option<u64>,
    consecutive_heartbeats: u32,
    reset_count: u32,
    last_fault: Option<DriverFault>,
}

impl Default for Phase0State {
    fn default() -> Self {
        Self::new()
    }
}

impl Phase0State {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            stage: Phase0Stage::Detached,
            resources: None,
            dma_address_bits: None,
            firmware: None,
            security: None,
            last_heartbeat_sequence: None,
            consecutive_heartbeats: 0,
            reset_count: 0,
            last_fault: None,
        }
    }

    #[must_use]
    pub const fn stage(&self) -> Phase0Stage {
        self.stage
    }

    #[must_use]
    pub const fn submission_enabled(&self) -> bool {
        matches!(self.stage, Phase0Stage::SubmissionReady)
    }

    #[must_use]
    pub const fn last_fault(&self) -> Option<DriverFault> {
        self.last_fault
    }

    #[must_use]
    pub const fn outcome(&self) -> TransitionOutcome {
        TransitionOutcome {
            stage: self.stage,
            authentication_complete: matches!(
                self.stage,
                Phase0Stage::HardwareAuthenticated
                    | Phase0Stage::HeartbeatQualified
                    | Phase0Stage::SubmissionReady
            ),
            consecutive_heartbeats: self.consecutive_heartbeats,
            submission_enabled: self.submission_enabled(),
            reset_count: self.reset_count,
        }
    }

    pub fn apply(&mut self, event: Phase0Event) -> Result<TransitionOutcome, Phase0Error> {
        match event {
            Phase0Event::DeviceClaimed(resources) if self.stage == Phase0Stage::Detached => {
                if !resources.identity.is_supported_a380() {
                    return Err(Phase0Error::UnsupportedDevice(resources.identity));
                }
                if resources.bar0_bytes == 0 {
                    return Err(Phase0Error::MissingBar0);
                }
                self.resources = Some(resources);
                self.stage = Phase0Stage::DeviceOwned;
            }
            Phase0Event::MmioMapped if self.stage == Phase0Stage::DeviceOwned => {
                self.stage = Phase0Stage::MmioMapped;
            }
            Phase0Event::DmaAddressingValidated { address_bits }
                if self.stage == Phase0Stage::MmioMapped =>
            {
                if address_bits < MINIMUM_DMA_ADDRESS_BITS {
                    return Err(Phase0Error::InsufficientDmaAddressBits { address_bits });
                }
                self.dma_address_bits = Some(address_bits);
                self.stage = Phase0Stage::DmaAddressingValidated;
            }
            Phase0Event::FirmwareContainersValidated(firmware)
                if self.stage == Phase0Stage::DmaAddressingValidated =>
            {
                if !firmware.is_acceptable_dg2() {
                    self.fail(DriverFault::FirmwareRejected);
                    return Err(Phase0Error::FirmwareEvidenceRejected);
                }
                self.firmware = Some(firmware);
                self.stage = Phase0Stage::FirmwareContainersValidated;
            }
            Phase0Event::SecuritySampled(snapshot)
                if matches!(
                    self.stage,
                    Phase0Stage::FirmwareContainersValidated
                        | Phase0Stage::HardwareAuthenticated
                        | Phase0Stage::HeartbeatQualified
                        | Phase0Stage::SubmissionReady
                ) =>
            {
                self.security = Some(snapshot);
                if snapshot.authenticated() {
                    if self.stage == Phase0Stage::FirmwareContainersValidated {
                        self.stage = Phase0Stage::HardwareAuthenticated;
                    }
                } else if self.stage != Phase0Stage::FirmwareContainersValidated {
                    self.fail(DriverFault::AuthenticationLost);
                    return Err(Phase0Error::AuthenticationRegressed);
                }
            }
            Phase0Event::Heartbeat { sequence }
                if matches!(
                    self.stage,
                    Phase0Stage::HardwareAuthenticated
                        | Phase0Stage::HeartbeatQualified
                        | Phase0Stage::SubmissionReady
                ) =>
            {
                if self
                    .last_heartbeat_sequence
                    .is_some_and(|previous| sequence <= previous)
                {
                    self.fail(DriverFault::HeartbeatStalled);
                    return Err(Phase0Error::NonMonotonicHeartbeat { sequence });
                }
                self.last_heartbeat_sequence = Some(sequence);
                self.consecutive_heartbeats = self.consecutive_heartbeats.saturating_add(1);
                if self.consecutive_heartbeats >= PHASE0_HEARTBEATS_REQUIRED
                    && self.stage == Phase0Stage::HardwareAuthenticated
                {
                    self.stage = Phase0Stage::HeartbeatQualified;
                }
            }
            Phase0Event::AuthorizeSubmission if self.stage == Phase0Stage::HeartbeatQualified => {
                self.stage = Phase0Stage::SubmissionReady;
            }
            Phase0Event::Fault(fault) if self.stage != Phase0Stage::Detached => {
                self.fail(fault);
            }
            Phase0Event::Reset if self.resources.is_some() => {
                self.stage = Phase0Stage::DeviceOwned;
                self.dma_address_bits = None;
                self.firmware = None;
                self.security = None;
                self.last_heartbeat_sequence = None;
                self.consecutive_heartbeats = 0;
                self.reset_count = self.reset_count.saturating_add(1);
            }
            _ => {
                return Err(Phase0Error::InvalidTransition {
                    stage: self.stage,
                    event: event.kind(),
                });
            }
        }
        Ok(self.outcome())
    }

    fn fail(&mut self, fault: DriverFault) {
        self.stage = Phase0Stage::Failed;
        self.last_fault = Some(fault);
        self.consecutive_heartbeats = 0;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase0Error {
    UnsupportedDevice(PciIdentity),
    MissingBar0,
    InsufficientDmaAddressBits {
        address_bits: u8,
    },
    FirmwareEvidenceRejected,
    AuthenticationRegressed,
    NonMonotonicHeartbeat {
        sequence: u64,
    },
    InvalidTransition {
        stage: Phase0Stage,
        event: Phase0EventKind,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    const RESOURCES: DeviceResources = DeviceResources {
        identity: PciIdentity {
            vendor: INTEL_VENDOR_ID,
            device: ARC_A380_DEVICE_ID,
        },
        bar0_bytes: 16 * 1024 * 1024,
        local_memory_bytes: 6 * 1024 * 1024 * 1024,
    };
    const FIRMWARE: FirmwareEvidence = FirmwareEvidence {
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
    };
    const AUTHENTICATED: SecuritySnapshot = SecuritySnapshot {
        guc_status_raw: GUC_AUTH_GOOD | 0x0000_0100,
        boot_hash_raw: BOOT_UKERNEL_VALID,
        wopcm_size_raw: WOPCM_SIZE_LOCKED,
        wopcm_offset_raw: WOPCM_OFFSET_VALID,
    };

    fn authenticated_state() -> Phase0State {
        let mut state = Phase0State::new();
        state
            .apply(Phase0Event::DeviceClaimed(RESOURCES))
            .expect("claim A380");
        state.apply(Phase0Event::MmioMapped).expect("map BAR0");
        state
            .apply(Phase0Event::DmaAddressingValidated { address_bits: 46 })
            .expect("validate DMA width");
        state
            .apply(Phase0Event::FirmwareContainersValidated(FIRMWARE))
            .expect("validate firmware");
        state
            .apply(Phase0Event::SecuritySampled(AUTHENTICATED))
            .expect("observe authentication");
        state
    }

    #[test]
    fn qualifies_only_after_one_thousand_monotonic_heartbeats() {
        let mut state = authenticated_state();
        for sequence in 1..PHASE0_HEARTBEATS_REQUIRED as u64 {
            state
                .apply(Phase0Event::Heartbeat { sequence })
                .expect("accept heartbeat");
        }
        assert_eq!(state.stage(), Phase0Stage::HardwareAuthenticated);
        state
            .apply(Phase0Event::Heartbeat {
                sequence: PHASE0_HEARTBEATS_REQUIRED as u64,
            })
            .expect("qualify heartbeat");
        assert_eq!(state.stage(), Phase0Stage::HeartbeatQualified);
        assert!(!state.submission_enabled());
        state
            .apply(Phase0Event::AuthorizeSubmission)
            .expect("authorize submission");
        assert!(state.submission_enabled());
    }

    #[test]
    fn rejects_a_device_other_than_the_a380() {
        let mut state = Phase0State::new();
        let mut resources = RESOURCES;
        resources.identity.device = 0xffff;
        assert_eq!(
            state.apply(Phase0Event::DeviceClaimed(resources)),
            Err(Phase0Error::UnsupportedDevice(resources.identity))
        );
        assert_eq!(state.stage(), Phase0Stage::Detached);
    }

    #[test]
    fn incomplete_authentication_remains_submission_locked() {
        let mut state = authenticated_state();
        state.apply(Phase0Event::Reset).expect("reset state");
        state.apply(Phase0Event::MmioMapped).expect("map BAR0");
        state
            .apply(Phase0Event::DmaAddressingValidated { address_bits: 46 })
            .expect("validate DMA width");
        state
            .apply(Phase0Event::FirmwareContainersValidated(FIRMWARE))
            .expect("validate firmware");
        let pending = SecuritySnapshot {
            guc_status_raw: 0,
            ..AUTHENTICATED
        };
        state
            .apply(Phase0Event::SecuritySampled(pending))
            .expect("observe pending authentication");
        assert_eq!(state.stage(), Phase0Stage::FirmwareContainersValidated);
        assert!(!state.submission_enabled());
    }

    #[test]
    fn authentication_loss_fails_closed() {
        let mut state = authenticated_state();
        let lost = SecuritySnapshot {
            boot_hash_raw: 0,
            ..AUTHENTICATED
        };
        assert_eq!(
            state.apply(Phase0Event::SecuritySampled(lost)),
            Err(Phase0Error::AuthenticationRegressed)
        );
        assert_eq!(state.stage(), Phase0Stage::Failed);
        assert_eq!(state.last_fault(), Some(DriverFault::AuthenticationLost));
        assert!(!state.submission_enabled());
    }

    #[test]
    fn reset_discards_qualification_and_relocks_submission() {
        let mut state = authenticated_state();
        for sequence in 1..=PHASE0_HEARTBEATS_REQUIRED as u64 {
            state
                .apply(Phase0Event::Heartbeat { sequence })
                .expect("accept heartbeat");
        }
        state
            .apply(Phase0Event::AuthorizeSubmission)
            .expect("authorize submission");
        state.apply(Phase0Event::Reset).expect("reset state");
        assert_eq!(state.stage(), Phase0Stage::DeviceOwned);
        assert_eq!(state.outcome().consecutive_heartbeats, 0);
        assert_eq!(state.outcome().reset_count, 1);
        assert!(!state.submission_enabled());
    }

    #[test]
    fn repeated_heartbeat_sequence_fails_closed() {
        let mut state = authenticated_state();
        state
            .apply(Phase0Event::Heartbeat { sequence: 1 })
            .expect("first heartbeat");
        assert_eq!(
            state.apply(Phase0Event::Heartbeat { sequence: 1 }),
            Err(Phase0Error::NonMonotonicHeartbeat { sequence: 1 })
        );
        assert_eq!(state.stage(), Phase0Stage::Failed);
        assert!(!state.submission_enabled());
    }

    #[test]
    fn security_register_decoder_matches_dg2_fields() {
        assert_eq!(AUTHENTICATED.guc_auth_status(), 2);
        assert_eq!(AUTHENTICATED.guc_ukernel_status(), 1);
        assert!(!AUTHENTICATED.guc_in_reset());
        assert!(AUTHENTICATED.boot_hash_valid());
        assert!(AUTHENTICATED.wopcm_locked());
        assert!(AUTHENTICATED.authenticated());
    }

    #[test]
    fn rejects_insufficient_dma_address_width() {
        let mut state = Phase0State::new();
        state
            .apply(Phase0Event::DeviceClaimed(RESOURCES))
            .expect("claim A380");
        state.apply(Phase0Event::MmioMapped).expect("map BAR0");
        assert_eq!(
            state.apply(Phase0Event::DmaAddressingValidated { address_bits: 32 }),
            Err(Phase0Error::InsufficientDmaAddressBits { address_bits: 32 })
        );
        assert_eq!(state.stage(), Phase0Stage::MmioMapped);
        assert!(!state.submission_enabled());
    }

    #[test]
    fn rejected_firmware_moves_to_failed_state() {
        let mut state = Phase0State::new();
        state
            .apply(Phase0Event::DeviceClaimed(RESOURCES))
            .expect("claim A380");
        state.apply(Phase0Event::MmioMapped).expect("map BAR0");
        state
            .apply(Phase0Event::DmaAddressingValidated { address_bits: 46 })
            .expect("validate DMA width");
        let rejected = FirmwareEvidence {
            guc_production: false,
            ..FIRMWARE
        };
        assert_eq!(
            state.apply(Phase0Event::FirmwareContainersValidated(rejected)),
            Err(Phase0Error::FirmwareEvidenceRejected)
        );
        assert_eq!(state.stage(), Phase0Stage::Failed);
        assert_eq!(state.last_fault(), Some(DriverFault::FirmwareRejected));
    }

    #[test]
    fn explicit_fault_relocks_a_qualified_device() {
        let mut state = authenticated_state();
        for sequence in 1..=PHASE0_HEARTBEATS_REQUIRED as u64 {
            state
                .apply(Phase0Event::Heartbeat { sequence })
                .expect("accept heartbeat");
        }
        state
            .apply(Phase0Event::AuthorizeSubmission)
            .expect("authorize submission");
        state
            .apply(Phase0Event::Fault(DriverFault::DataMismatch))
            .expect("record fault");
        assert_eq!(state.stage(), Phase0Stage::Failed);
        assert_eq!(state.last_fault(), Some(DriverFault::DataMismatch));
        assert!(!state.submission_enabled());
    }

    #[derive(Debug)]
    struct MockKernelAdapter;

    impl KernelAdapter for MockKernelAdapter {
        type Error = ();

        fn claim_a380(&mut self) -> Result<DeviceResources, Self::Error> {
            Ok(RESOURCES)
        }

        fn map_bar0(&mut self) -> Result<(), Self::Error> {
            Ok(())
        }

        fn configure_dma_addressing(&mut self, _address_bits: u8) -> Result<(), Self::Error> {
            Ok(())
        }

        fn read_firmware(
            &mut self,
            _image: FirmwareImage,
            _destination: &mut [u8],
        ) -> Result<usize, Self::Error> {
            Ok(0)
        }

        fn read_mmio32(&self, offset: u32) -> Result<u32, Self::Error> {
            match offset {
                GUC_STATUS => Ok(AUTHENTICATED.guc_status_raw),
                BOOT_HASH_CHECK => Ok(AUTHENTICATED.boot_hash_raw),
                GUC_WOPCM_SIZE => Ok(AUTHENTICATED.wopcm_size_raw),
                DMA_GUC_WOPCM_OFFSET => Ok(AUTHENTICATED.wopcm_offset_raw),
                _ => Err(()),
            }
        }

        fn monotonic_ticks(&self) -> u64 {
            1
        }

        fn publish_phase0(&mut self, _state: &Phase0State) -> Result<(), Self::Error> {
            Ok(())
        }

        fn lock_submission(&mut self) -> Result<(), Self::Error> {
            Ok(())
        }
    }

    #[test]
    fn adapter_contract_reads_the_common_security_snapshot() {
        let snapshot = read_security_snapshot(&MockKernelAdapter).expect("read security snapshot");
        assert_eq!(snapshot, AUTHENTICATED);
        assert!(snapshot.authenticated());
    }
}
