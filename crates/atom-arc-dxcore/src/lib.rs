//! Safe process boundary for the native WSL DxCore/D3D12 submission helper.

use serde::{Deserialize, Serialize};
use std::env;
use std::path::PathBuf;
use std::process::Command;
use thiserror::Error;

pub const ARC_A380_VENDOR_ID: u32 = 0x8086;
pub const ARC_A380_DEVICE_ID: u32 = 0x56a5;
pub const DEFAULT_COMPUTE_ELEMENTS: u32 = 1024;
pub const DEFAULT_COMPUTE_ADDEND: u32 = 0x1357_9bdf;
pub const MAX_COMPUTE_ELEMENTS: u32 = 1024 * 1024;
pub const MIN_TRANSFER_PAYLOAD_BYTES: u32 = 4096;
pub const MAX_TRANSFER_PAYLOAD_BYTES: u32 = 256 * 1024 * 1024;
pub const MIN_TRANSFER_SAMPLES: u32 = 10;
pub const MAX_TRANSFER_SAMPLES: u32 = 100;
const HELPER_NAME: &str = "atom-arc-dxcore-helper";
const SMOKE_COPY_BYTES: u64 = 4096;
const SMOKE_COPY_CHECKSUM: u64 = 522_240;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DxcoreSmokeReport {
    pub schema: u32,
    pub passed: bool,
    pub backend: String,
    pub stage: String,
    pub detail: String,
    pub adapter_name: String,
    pub vendor_id: u32,
    pub device_id: u32,
    pub adapters_seen: u32,
    pub hresult: u32,
    pub device_created: bool,
    pub queue_created: bool,
    pub command_list_closed: bool,
    pub resources_created: bool,
    pub copy_recorded: bool,
    pub submitted: bool,
    pub fence_completed: bool,
    pub copy_verified: bool,
    pub bytes_copied: u64,
    pub copy_checksum: u64,
    pub compute_pipeline_created: bool,
    pub compute_input_uploaded: bool,
    pub compute_dispatched: bool,
    pub compute_verified: bool,
    pub compute_elements: u32,
    pub compute_addend: u32,
    pub compute_input_bytes: u64,
    pub compute_output_bytes: u64,
    pub compute_checksum: u64,
    pub device_removed_reason: u32,
    pub elapsed_microseconds: u64,
    #[serde(skip)]
    pub helper_path: Option<PathBuf>,
}

impl DxcoreSmokeReport {
    #[must_use]
    pub fn is_verified_arc_a380_submission(&self) -> bool {
        self.is_verified_arc_a380_compute(DEFAULT_COMPUTE_ELEMENTS, DEFAULT_COMPUTE_ADDEND)
    }

    #[must_use]
    pub fn is_verified_arc_a380_compute(
        &self,
        expected_elements: u32,
        expected_addend: u32,
    ) -> bool {
        self.schema == 1
            && self.passed
            && self.backend == "wsl-dxcore-d3d12"
            && self.stage == "complete"
            && self.vendor_id == ARC_A380_VENDOR_ID
            && self.device_id == ARC_A380_DEVICE_ID
            && self.device_created
            && self.queue_created
            && self.command_list_closed
            && self.resources_created
            && self.copy_recorded
            && self.submitted
            && self.fence_completed
            && self.copy_verified
            && self.bytes_copied == SMOKE_COPY_BYTES
            && self.copy_checksum == SMOKE_COPY_CHECKSUM
            && self.compute_pipeline_created
            && self.compute_input_uploaded
            && self.compute_dispatched
            && self.compute_verified
            && self.compute_elements == expected_elements
            && self.compute_addend == expected_addend
            && self.compute_input_bytes == u64::from(expected_elements) * 4
            && self.compute_output_bytes == u64::from(expected_elements) * 4
            && self.compute_checksum
                == expected_compute_checksum(self.compute_elements, self.compute_addend)
            && self.device_removed_reason == 0
    }
}

/// Paired live capture comparing fresh staging allocations with a reusable pool.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DxcoreTransferLabReport {
    pub schema: u32,
    pub passed: bool,
    pub backend: String,
    pub stage: String,
    pub detail: String,
    pub adapter_name: String,
    pub vendor_id: u32,
    pub device_id: u32,
    pub device_created: bool,
    pub queue_created: bool,
    pub resources_created: bool,
    pub copy_recorded: bool,
    pub submitted: bool,
    pub fence_completed: bool,
    pub copy_verified: bool,
    pub device_removed_reason: u32,
    pub elapsed_microseconds: u64,
    pub mode: String,
    pub primitive: String,
    pub payload_bytes: u64,
    pub operations_per_sample: u32,
    pub samples: u32,
    pub warmups: u32,
    pub pool_setup_ns: u64,
    #[serde(default)]
    pub candidate_setup_ns: u64,
    pub baseline_durations_ns: Vec<u64>,
    pub candidate_durations_ns: Vec<u64>,
    pub correctness_digests: Vec<u64>,
    #[serde(skip)]
    pub helper_path: Option<PathBuf>,
}

impl DxcoreTransferLabReport {
    #[must_use]
    pub fn is_verified_arc_a380_capture(
        &self,
        payload_bytes: u32,
        operations_per_sample: u32,
        samples: u32,
        warmups: u32,
    ) -> bool {
        self.is_verified_arc_a380_capture_kind(
            "paired-transfer-lab",
            "reusable-staging-buffer-pool",
            false,
            payload_bytes,
            operations_per_sample,
            samples,
            warmups,
        )
    }

    #[must_use]
    pub fn is_verified_arc_a380_compound_capture(
        &self,
        payload_bytes: u32,
        operations_per_sample: u32,
        samples: u32,
        warmups: u32,
    ) -> bool {
        self.is_verified_arc_a380_capture_kind(
            "paired-compound-transfer-lab",
            "double-buffered-preload",
            true,
            payload_bytes,
            operations_per_sample,
            samples,
            warmups,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn is_verified_arc_a380_capture_kind(
        &self,
        expected_mode: &str,
        expected_primitive: &str,
        require_candidate_setup: bool,
        payload_bytes: u32,
        operations_per_sample: u32,
        samples: u32,
        warmups: u32,
    ) -> bool {
        let sample_count = samples as usize;
        self.schema == 1
            && self.passed
            && self.backend == "wsl-dxcore-d3d12"
            && self.stage == "complete"
            && self.mode == expected_mode
            && self.primitive == expected_primitive
            && self.vendor_id == ARC_A380_VENDOR_ID
            && self.device_id == ARC_A380_DEVICE_ID
            && self.device_created
            && self.queue_created
            && self.resources_created
            && self.copy_recorded
            && self.submitted
            && self.fence_completed
            && self.copy_verified
            && self.device_removed_reason == 0
            && self.payload_bytes == u64::from(payload_bytes)
            && self.operations_per_sample == operations_per_sample
            && self.samples == samples
            && self.warmups == warmups
            && self.pool_setup_ns > 0
            && (!require_candidate_setup || self.candidate_setup_ns > 0)
            && self.baseline_durations_ns.len() == sample_count
            && self.candidate_durations_ns.len() == sample_count
            && self.correctness_digests.len() == sample_count
            && self.baseline_durations_ns.iter().all(|value| *value > 0)
            && self.candidate_durations_ns.iter().all(|value| *value > 0)
    }
}

fn expected_compute_checksum(elements: u32, addend: u32) -> u64 {
    (0..elements)
        .map(|index| {
            let value = index.wrapping_mul(1_664_525).wrapping_add(addend) ^ 0xa5a5_a5a5;
            u64::from(value)
        })
        .sum()
}

pub fn run_arc_a380_smoke(timeout_ms: u32) -> Result<DxcoreSmokeReport, DxcoreError> {
    run_arc_a380_compute(DEFAULT_COMPUTE_ELEMENTS, DEFAULT_COMPUTE_ADDEND, timeout_ms)
}

pub fn run_arc_a380_compute(
    elements: u32,
    addend: u32,
    timeout_ms: u32,
) -> Result<DxcoreSmokeReport, DxcoreError> {
    if timeout_ms == 0 || timeout_ms > 60_000 {
        return Err(DxcoreError::InvalidTimeout { timeout_ms });
    }
    if elements == 0 || elements > MAX_COMPUTE_ELEMENTS {
        return Err(DxcoreError::InvalidElements { elements });
    }
    let helper = resolve_helper();
    let output = Command::new(&helper)
        .args([
            "--vendor",
            "0x8086",
            "--device",
            "0x56a5",
            "--timeout-ms",
            &timeout_ms.to_string(),
            "--elements",
            &elements.to_string(),
            "--addend",
            &addend.to_string(),
        ])
        .output()
        .map_err(|source| DxcoreError::StartHelper {
            path: helper.clone(),
            source,
        })?;

    let mut report: DxcoreSmokeReport =
        serde_json::from_slice(&output.stdout).map_err(|source| DxcoreError::InvalidReport {
            path: helper.clone(),
            source,
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })?;
    report.helper_path = Some(helper.clone());

    if output.status.success() != report.passed {
        return Err(DxcoreError::InconsistentExitStatus {
            path: helper,
            status: output.status.code(),
            report_passed: report.passed,
        });
    }
    Ok(report)
}

/// Run a correctness-checked paired staging-pool experiment on the live A380.
pub fn run_arc_a380_transfer_lab(
    payload_bytes: u32,
    operations_per_sample: u32,
    samples: u32,
    warmups: u32,
    timeout_ms: u32,
) -> Result<DxcoreTransferLabReport, DxcoreError> {
    run_arc_a380_paired_transfer_lab(
        "--transfer-lab",
        payload_bytes,
        operations_per_sample,
        samples,
        warmups,
        timeout_ms,
    )
}

/// Run the reusable-pool baseline against double-buffered preload on the live A380.
pub fn run_arc_a380_compound_lab(
    payload_bytes: u32,
    operations_per_sample: u32,
    samples: u32,
    warmups: u32,
    timeout_ms: u32,
) -> Result<DxcoreTransferLabReport, DxcoreError> {
    if operations_per_sample < 2 {
        return Err(DxcoreError::InvalidCompoundOperations {
            operations: operations_per_sample,
        });
    }
    run_arc_a380_paired_transfer_lab(
        "--compound-lab",
        payload_bytes,
        operations_per_sample,
        samples,
        warmups,
        timeout_ms,
    )
}

fn run_arc_a380_paired_transfer_lab(
    lab_flag: &str,
    payload_bytes: u32,
    operations_per_sample: u32,
    samples: u32,
    warmups: u32,
    timeout_ms: u32,
) -> Result<DxcoreTransferLabReport, DxcoreError> {
    if timeout_ms == 0 || timeout_ms > 60_000 {
        return Err(DxcoreError::InvalidTimeout { timeout_ms });
    }
    if !(MIN_TRANSFER_PAYLOAD_BYTES..=MAX_TRANSFER_PAYLOAD_BYTES).contains(&payload_bytes) {
        return Err(DxcoreError::InvalidTransferPayload { payload_bytes });
    }
    if operations_per_sample == 0 || operations_per_sample > 1024 {
        return Err(DxcoreError::InvalidTransferOperations {
            operations: operations_per_sample,
        });
    }
    if !(MIN_TRANSFER_SAMPLES..=MAX_TRANSFER_SAMPLES).contains(&samples) {
        return Err(DxcoreError::InvalidTransferSamples { samples });
    }
    if warmups > 20 {
        return Err(DxcoreError::InvalidTransferWarmups { warmups });
    }

    let helper = resolve_helper();
    let output = Command::new(&helper)
        .args([
            lab_flag,
            "--vendor",
            "0x8086",
            "--device",
            "0x56a5",
            "--timeout-ms",
            &timeout_ms.to_string(),
            "--payload-bytes",
            &payload_bytes.to_string(),
            "--operations",
            &operations_per_sample.to_string(),
            "--samples",
            &samples.to_string(),
            "--warmups",
            &warmups.to_string(),
        ])
        .output()
        .map_err(|source| DxcoreError::StartHelper {
            path: helper.clone(),
            source,
        })?;

    let mut report: DxcoreTransferLabReport =
        serde_json::from_slice(&output.stdout).map_err(|source| DxcoreError::InvalidReport {
            path: helper.clone(),
            source,
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })?;
    report.helper_path = Some(helper.clone());
    if output.status.success() != report.passed {
        return Err(DxcoreError::InconsistentExitStatus {
            path: helper,
            status: output.status.code(),
            report_passed: report.passed,
        });
    }
    Ok(report)
}

fn resolve_helper() -> PathBuf {
    if let Some(path) = env::var_os("ATOM_ARC_DXCORE_HELPER") {
        return PathBuf::from(path);
    }
    if let Ok(current_exe) = env::current_exe()
        && let Some(directory) = current_exe.parent()
    {
        let sibling = directory.join(HELPER_NAME);
        if sibling.is_file() {
            return sibling;
        }
    }
    PathBuf::from(HELPER_NAME)
}

#[derive(Debug, Error)]
pub enum DxcoreError {
    #[error("hardware-smoke timeout {timeout_ms} ms is outside the range 1..=60000")]
    InvalidTimeout { timeout_ms: u32 },
    #[error(
        "hardware compute element count {elements} is outside the range 1..={MAX_COMPUTE_ELEMENTS}"
    )]
    InvalidElements { elements: u32 },
    #[error(
        "transfer payload {payload_bytes} is outside the range {MIN_TRANSFER_PAYLOAD_BYTES}..={MAX_TRANSFER_PAYLOAD_BYTES}"
    )]
    InvalidTransferPayload { payload_bytes: u32 },
    #[error("transfer operations {operations} is outside the range 1..=1024")]
    InvalidTransferOperations { operations: u32 },
    #[error("compound transfer operations {operations} is outside the range 2..=1024")]
    InvalidCompoundOperations { operations: u32 },
    #[error(
        "transfer samples {samples} is outside the range {MIN_TRANSFER_SAMPLES}..={MAX_TRANSFER_SAMPLES}"
    )]
    InvalidTransferSamples { samples: u32 },
    #[error("transfer warmups {warmups} is outside the range 0..=20")]
    InvalidTransferWarmups { warmups: u32 },
    #[error("could not start DxCore helper at {path}: {source}")]
    StartHelper {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error(
        "DxCore helper at {path} returned invalid JSON: {source}; stdout={stdout:?}; stderr={stderr:?}"
    )]
    InvalidReport {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
        stdout: String,
        stderr: String,
    },
    #[error(
        "DxCore helper at {path} has inconsistent status {status:?} and report passed={report_passed}"
    )]
    InconsistentExitStatus {
        path: PathBuf,
        status: Option<i32>,
        report_passed: bool,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn success_report() -> DxcoreSmokeReport {
        serde_json::from_str(
            r#"{
                "schema":1,
                "passed":true,
                "backend":"wsl-dxcore-d3d12",
                "stage":"complete",
                "detail":"complete",
                "adapter_name":"Intel(R) Arc(TM) A380 Graphics",
                "vendor_id":32902,
                "device_id":22181,
                "adapters_seen":3,
                "hresult":0,
                "device_created":true,
                "queue_created":true,
                "command_list_closed":true,
                "resources_created":true,
                "copy_recorded":true,
                "submitted":true,
                "fence_completed":true,
                "copy_verified":true,
                "bytes_copied":4096,
                "copy_checksum":522240,
                "compute_pipeline_created":true,
                "compute_input_uploaded":true,
                "compute_dispatched":true,
                "compute_verified":true,
                "compute_elements":1024,
                "compute_addend":324508639,
                "compute_input_bytes":4096,
                "compute_output_bytes":4096,
                "compute_checksum":3334973233664,
                "device_removed_reason":0,
                "elapsed_microseconds":1000
            }"#,
        )
        .expect("valid test report")
    }

    #[test]
    fn accepts_complete_arc_a380_submission() {
        assert!(success_report().is_verified_arc_a380_submission());
    }

    #[test]
    fn rejects_wrong_device_even_when_helper_claims_pass() {
        let mut report = success_report();
        report.device_id = 0x1234;
        assert!(!report.is_verified_arc_a380_submission());
    }

    #[test]
    fn rejects_missing_fence_completion() {
        let mut report = success_report();
        report.fence_completed = false;
        assert!(!report.is_verified_arc_a380_submission());
    }

    #[test]
    fn rejects_unverified_copy_data() {
        let mut report = success_report();
        report.copy_checksum ^= 1;
        assert!(!report.is_verified_arc_a380_submission());
    }

    #[test]
    fn rejects_unverified_compute_data() {
        let mut report = success_report();
        report.compute_checksum ^= 1;
        assert!(!report.is_verified_arc_a380_submission());
    }

    #[test]
    fn rejects_compute_without_a_complete_input_upload() {
        let mut report = success_report();
        report.compute_input_uploaded = false;
        assert!(!report.is_verified_arc_a380_submission());

        report.compute_input_uploaded = true;
        report.compute_input_bytes -= 4;
        assert!(!report.is_verified_arc_a380_submission());
    }

    #[test]
    fn accepts_custom_verified_compute_shape() {
        let mut report = success_report();
        report.compute_elements = 257;
        report.compute_addend = 7;
        report.compute_input_bytes = 257 * 4;
        report.compute_output_bytes = 257 * 4;
        report.compute_checksum = expected_compute_checksum(257, 7);
        assert!(report.is_verified_arc_a380_compute(257, 7));
        assert!(!report.is_verified_arc_a380_submission());
    }

    #[test]
    fn timeout_range_is_fail_closed_before_process_start() {
        assert!(matches!(
            run_arc_a380_smoke(0),
            Err(DxcoreError::InvalidTimeout { timeout_ms: 0 })
        ));
    }

    #[test]
    fn compute_range_is_fail_closed_before_process_start() {
        assert!(matches!(
            run_arc_a380_compute(0, 0, 5_000),
            Err(DxcoreError::InvalidElements { elements: 0 })
        ));
    }

    #[test]
    fn transfer_sample_range_is_fail_closed_before_process_start() {
        assert!(matches!(
            run_arc_a380_transfer_lab(4096, 1, 9, 0, 5_000),
            Err(DxcoreError::InvalidTransferSamples { samples: 9 })
        ));
    }

    #[test]
    fn compound_requires_two_lanes_before_process_start() {
        assert!(matches!(
            run_arc_a380_compound_lab(4096, 1, 10, 0, 5_000),
            Err(DxcoreError::InvalidCompoundOperations { operations: 1 })
        ));
    }
}
