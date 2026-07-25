//! Reproducible experiment contracts for Atom Arc driver phases.
//!
//! Synthetic records can exercise only the evaluator. A candidate primitive is
//! retained by live A380 feasibility evidence, and promoted only by native
//! `atom_arc` evidence that also preserves correctness, clears the effect-size
//! threshold, has a positive paired 95% confidence bound, and stays inside the
//! allowed p95 latency regression.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Ordered driver phases. Work advances one phase at a time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DriverPhase {
    SecureBringUp,
    HostToVramOnRamp,
    VramPlacement,
    CommandHighway,
    EngineOverlap,
    IntraGpuDataHopping,
    CrossDeviceFabric,
}

/// Cross-domain source of a candidate primitive.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PrimitiveDomain {
    CpuCache,
    Networking,
    StorageIo,
    RealTimeScheduling,
    DatabaseBufferPool,
    SpiderwebFabric,
    AedesMemoryTiering,
    GpuArchitecture,
}

/// Provenance tier for the measurements in an experiment record.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EvidenceKind {
    /// Parser/statistics fixture. It can never retain or promote a primitive.
    #[default]
    SyntheticFixture,
    /// Real A380 execution through a comparison path such as WSL GPU-PV.
    LiveHardwareFeasibility,
    /// Real A380 execution while the independent `atom_arc` adapter owns the PCI device.
    NativeDriver,
}

/// Device and runtime provenance attached to every experiment.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExperimentEvidence {
    pub kind: EvidenceKind,
    pub physical_device: bool,
    pub backend: String,
    pub driver_under_test: String,
    pub vendor_id: Option<u32>,
    pub device_id: Option<u32>,
    pub run_id: String,
}

impl ExperimentEvidence {
    fn validate(&self) -> Result<(), ExperimentError> {
        if self.kind == EvidenceKind::SyntheticFixture {
            return Ok(());
        }
        if !self.physical_device
            || self.backend.trim().is_empty()
            || self.driver_under_test.trim().is_empty()
            || self.run_id.trim().is_empty()
            || self.vendor_id != Some(0x8086)
            || self.device_id != Some(0x56a5)
        {
            return Err(ExperimentError::InvalidLiveEvidence);
        }
        if self.kind == EvidenceKind::NativeDriver
            && (self.backend != "native-linux-pci" || self.driver_under_test != "atom_arc")
        {
            return Err(ExperimentError::InvalidNativeDriverEvidence);
        }
        Ok(())
    }
}

/// Promotion thresholds for one baseline-versus-candidate experiment.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExperimentSpec {
    pub id: String,
    pub phase: DriverPhase,
    pub domain: PrimitiveDomain,
    pub primitive: String,
    /// Frozen primitive order used by every baseline sample.
    pub baseline_stack: Vec<String>,
    /// Candidate order. It must append exactly one primitive to the baseline.
    pub candidate_stack: Vec<String>,
    pub hypothesis: String,
    pub minimum_samples: usize,
    pub minimum_mean_improvement_percent: f64,
    pub maximum_p95_latency_regression_percent: f64,
}

impl ExperimentSpec {
    /// Validate that the experiment has enough evidence and meaningful gates.
    pub fn validate(&self) -> Result<(), ExperimentError> {
        if self.id.trim().is_empty() || self.primitive.trim().is_empty() {
            return Err(ExperimentError::MissingIdentity);
        }
        if self
            .baseline_stack
            .iter()
            .chain(self.candidate_stack.iter())
            .any(|primitive| primitive.trim().is_empty())
        {
            return Err(ExperimentError::EmptyStackEntry);
        }
        if self.candidate_stack.len() != self.baseline_stack.len() + 1
            || !self.candidate_stack.starts_with(&self.baseline_stack)
            || self.candidate_stack.last().map(String::as_str) != Some(self.primitive.as_str())
        {
            return Err(ExperimentError::StackOrderViolation {
                primitive: self.primitive.clone(),
            });
        }
        if self.minimum_samples < 10 {
            return Err(ExperimentError::TooFewRequiredSamples {
                minimum_samples: self.minimum_samples,
            });
        }
        if !self.minimum_mean_improvement_percent.is_finite()
            || self.minimum_mean_improvement_percent <= 0.0
            || !self.maximum_p95_latency_regression_percent.is_finite()
            || self.maximum_p95_latency_regression_percent < 0.0
        {
            return Err(ExperimentError::InvalidThreshold);
        }
        Ok(())
    }
}

/// One measured execution of an identical logical workload.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Measurement {
    pub bytes: u64,
    pub operations: u64,
    pub duration_ns: u64,
    pub correctness_digest: String,
    pub error: Option<String>,
}

/// Complete paired experiment input.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExperimentRecord {
    pub spec: ExperimentSpec,
    #[serde(default)]
    pub evidence: ExperimentEvidence,
    pub baseline: Vec<Measurement>,
    pub candidate: Vec<Measurement>,
}

/// Machine decision for a candidate primitive.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExperimentVerdict {
    /// Synthetic data exercised the evaluator but supplied no performance evidence.
    FixtureValid,
    /// A real A380 feasibility run passed; native-driver validation is still required.
    RetainForNativeValidation,
    Promote,
    Reject,
}

/// Statistics and every gate used to make a promotion decision.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExperimentReport {
    pub id: String,
    pub phase: DriverPhase,
    pub primitive: String,
    pub evidence_kind: EvidenceKind,
    pub evidence_backend: String,
    pub driver_under_test: String,
    pub run_id: String,
    pub baseline_stack: Vec<String>,
    pub candidate_stack: Vec<String>,
    pub sample_count: usize,
    pub baseline_mean_bytes_per_second: f64,
    pub candidate_mean_bytes_per_second: f64,
    pub mean_improvement_percent: f64,
    pub median_improvement_percent: f64,
    pub paired_95_percent_lower_bound: f64,
    pub baseline_p95_latency_ns: u64,
    pub candidate_p95_latency_ns: u64,
    pub p95_latency_regression_percent: f64,
    pub verdict: ExperimentVerdict,
    pub reasons: Vec<String>,
}

impl ExperimentRecord {
    /// Evaluate the candidate against correctness, effect, confidence, and tail latency gates.
    pub fn evaluate(&self) -> Result<ExperimentReport, ExperimentError> {
        self.spec.validate()?;
        self.evidence.validate()?;
        if self.baseline.len() != self.candidate.len() {
            return Err(ExperimentError::UnpairedSamples {
                baseline: self.baseline.len(),
                candidate: self.candidate.len(),
            });
        }
        if self.baseline.len() < self.spec.minimum_samples {
            return Err(ExperimentError::InsufficientSamples {
                actual: self.baseline.len(),
                required: self.spec.minimum_samples,
            });
        }

        let mut improvements = Vec::with_capacity(self.baseline.len());
        let mut baseline_rates = Vec::with_capacity(self.baseline.len());
        let mut candidate_rates = Vec::with_capacity(self.candidate.len());
        let mut baseline_latencies = Vec::with_capacity(self.baseline.len());
        let mut candidate_latencies = Vec::with_capacity(self.candidate.len());
        let mut reasons = Vec::new();

        for (index, (baseline, candidate)) in
            self.baseline.iter().zip(self.candidate.iter()).enumerate()
        {
            validate_measurement(index, "baseline", baseline)?;
            validate_measurement(index, "candidate", candidate)?;
            if baseline.bytes != candidate.bytes || baseline.operations != candidate.operations {
                return Err(ExperimentError::WorkloadMismatch { index });
            }
            if baseline.correctness_digest != candidate.correctness_digest {
                reasons.push(format!(
                    "sample {index} produced a different correctness digest"
                ));
            }

            let baseline_rate = throughput(baseline);
            let candidate_rate = throughput(candidate);
            baseline_rates.push(baseline_rate);
            candidate_rates.push(candidate_rate);
            improvements.push((candidate_rate / baseline_rate - 1.0) * 100.0);
            baseline_latencies.push(baseline.duration_ns);
            candidate_latencies.push(candidate.duration_ns);
        }

        let mean_improvement = mean(&improvements);
        let median_improvement = percentile_f64(&mut improvements.clone(), 0.5);
        let lower_bound = paired_lower_bound_95(&improvements);
        let baseline_p95 = percentile_u64(&mut baseline_latencies, 0.95);
        let candidate_p95 = percentile_u64(&mut candidate_latencies, 0.95);
        let p95_regression = (candidate_p95 as f64 / baseline_p95 as f64 - 1.0) * 100.0;

        if mean_improvement < self.spec.minimum_mean_improvement_percent {
            reasons.push(format!(
                "mean improvement {:.3}% is below the {:.3}% gate",
                mean_improvement, self.spec.minimum_mean_improvement_percent
            ));
        }
        if median_improvement < self.spec.minimum_mean_improvement_percent {
            reasons.push(format!(
                "median improvement {:.3}% is below the {:.3}% gate",
                median_improvement, self.spec.minimum_mean_improvement_percent
            ));
        }
        if lower_bound <= 0.0 {
            reasons.push(format!(
                "paired 95% lower confidence bound {:.3}% is not positive",
                lower_bound
            ));
        }
        if p95_regression > self.spec.maximum_p95_latency_regression_percent {
            reasons.push(format!(
                "p95 latency regression {:.3}% exceeds the {:.3}% gate",
                p95_regression, self.spec.maximum_p95_latency_regression_percent
            ));
        }

        let verdict = if !reasons.is_empty() {
            ExperimentVerdict::Reject
        } else {
            match self.evidence.kind {
                EvidenceKind::SyntheticFixture => ExperimentVerdict::FixtureValid,
                EvidenceKind::LiveHardwareFeasibility => {
                    ExperimentVerdict::RetainForNativeValidation
                }
                EvidenceKind::NativeDriver => ExperimentVerdict::Promote,
            }
        };

        Ok(ExperimentReport {
            id: self.spec.id.clone(),
            phase: self.spec.phase,
            primitive: self.spec.primitive.clone(),
            evidence_kind: self.evidence.kind,
            evidence_backend: self.evidence.backend.clone(),
            driver_under_test: self.evidence.driver_under_test.clone(),
            run_id: self.evidence.run_id.clone(),
            baseline_stack: self.spec.baseline_stack.clone(),
            candidate_stack: self.spec.candidate_stack.clone(),
            sample_count: self.baseline.len(),
            baseline_mean_bytes_per_second: mean(&baseline_rates),
            candidate_mean_bytes_per_second: mean(&candidate_rates),
            mean_improvement_percent: mean_improvement,
            median_improvement_percent: median_improvement,
            paired_95_percent_lower_bound: lower_bound,
            baseline_p95_latency_ns: baseline_p95,
            candidate_p95_latency_ns: candidate_p95,
            p95_latency_regression_percent: p95_regression,
            verdict,
            reasons,
        })
    }
}

fn validate_measurement(
    index: usize,
    side: &'static str,
    measurement: &Measurement,
) -> Result<(), ExperimentError> {
    if measurement.bytes == 0 || measurement.operations == 0 || measurement.duration_ns == 0 {
        return Err(ExperimentError::InvalidMeasurement { index, side });
    }
    if measurement.correctness_digest.trim().is_empty() {
        return Err(ExperimentError::MissingCorrectnessDigest { index, side });
    }
    if let Some(error) = &measurement.error {
        return Err(ExperimentError::MeasuredFailure {
            index,
            side,
            error: error.clone(),
        });
    }
    Ok(())
}

fn throughput(measurement: &Measurement) -> f64 {
    measurement.bytes as f64 * 1_000_000_000.0 / measurement.duration_ns as f64
}

fn mean(values: &[f64]) -> f64 {
    values.iter().sum::<f64>() / values.len() as f64
}

fn paired_lower_bound_95(improvements: &[f64]) -> f64 {
    let average = mean(improvements);
    let variance = improvements
        .iter()
        .map(|value| {
            let delta = value - average;
            delta * delta
        })
        .sum::<f64>()
        / (improvements.len() - 1) as f64;
    let standard_error = variance.sqrt() / (improvements.len() as f64).sqrt();
    average - student_t_critical_95(improvements.len()) * standard_error
}

fn student_t_critical_95(sample_count: usize) -> f64 {
    match sample_count {
        0..=10 => 2.262,
        11..=15 => 2.145,
        16..=20 => 2.093,
        21..=30 => 2.045,
        _ => 1.96,
    }
}

fn percentile_f64(values: &mut [f64], quantile: f64) -> f64 {
    values.sort_by(f64::total_cmp);
    values[percentile_index(values.len(), quantile)]
}

fn percentile_u64(values: &mut [u64], quantile: f64) -> u64 {
    values.sort_unstable();
    values[percentile_index(values.len(), quantile)]
}

fn percentile_index(length: usize, quantile: f64) -> usize {
    ((length - 1) as f64 * quantile).ceil() as usize
}

#[derive(Clone, Debug, Error, PartialEq)]
pub enum ExperimentError {
    #[error("experiment id and primitive must be non-empty")]
    MissingIdentity,
    #[error("primitive stacks may not contain empty entries")]
    EmptyStackEntry,
    #[error(
        "candidate stack must preserve the baseline order and append only primitive {primitive:?}"
    )]
    StackOrderViolation { primitive: String },
    #[error("experiments must require at least 10 samples, received {minimum_samples}")]
    TooFewRequiredSamples { minimum_samples: usize },
    #[error("experiment thresholds must be finite and meaningful")]
    InvalidThreshold,
    #[error("paired sample counts differ: baseline={baseline}, candidate={candidate}")]
    UnpairedSamples { baseline: usize, candidate: usize },
    #[error("experiment has {actual} samples but requires {required}")]
    InsufficientSamples { actual: usize, required: usize },
    #[error("{side} sample {index} has a zero workload or duration")]
    InvalidMeasurement { index: usize, side: &'static str },
    #[error("{side} sample {index} has no correctness digest")]
    MissingCorrectnessDigest { index: usize, side: &'static str },
    #[error("{side} sample {index} failed: {error}")]
    MeasuredFailure {
        index: usize,
        side: &'static str,
        error: String,
    },
    #[error("paired sample {index} did not execute an identical workload")]
    WorkloadMismatch { index: usize },
    #[error("live evidence must identify a physical Intel Arc A380, backend, driver, and run")]
    InvalidLiveEvidence,
    #[error("native promotion requires backend native-linux-pci and driver atom_arc")]
    InvalidNativeDriverEvidence,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(candidate_duration_ns: u64) -> ExperimentRecord {
        let baseline = (0..12)
            .map(|index| Measurement {
                bytes: 1_048_576,
                operations: 1,
                duration_ns: 1_000_000 + index * 1_000,
                correctness_digest: format!("payload-{index}"),
                error: None,
            })
            .collect::<Vec<_>>();
        let candidate = baseline
            .iter()
            .enumerate()
            .map(|(index, baseline)| Measurement {
                bytes: baseline.bytes,
                operations: baseline.operations,
                duration_ns: candidate_duration_ns + index as u64 * 800,
                correctness_digest: baseline.correctness_digest.clone(),
                error: None,
            })
            .collect();
        ExperimentRecord {
            spec: ExperimentSpec {
                id: "phase1-pinned-ring".to_owned(),
                phase: DriverPhase::HostToVramOnRamp,
                domain: PrimitiveDomain::SpiderwebFabric,
                primitive: "reusable-pinned-multilane-ring".to_owned(),
                baseline_stack: vec!["secure-bring-up".to_owned()],
                candidate_stack: vec![
                    "secure-bring-up".to_owned(),
                    "reusable-pinned-multilane-ring".to_owned(),
                ],
                hypothesis: "promoted transfer lanes overlap staging with DMA".to_owned(),
                minimum_samples: 10,
                minimum_mean_improvement_percent: 10.0,
                maximum_p95_latency_regression_percent: 5.0,
            },
            evidence: ExperimentEvidence::default(),
            baseline,
            candidate,
        }
    }

    #[test]
    fn accepts_a_correct_stable_fixture_without_promoting_it() {
        let report = record(800_000).evaluate().expect("valid experiment");
        assert_eq!(report.verdict, ExperimentVerdict::FixtureValid);
        assert!(report.mean_improvement_percent > 20.0);
        assert!(report.paired_95_percent_lower_bound > 0.0);
    }

    #[test]
    fn retains_a_live_a380_feasibility_win_without_promoting_it() {
        let mut experiment = record(800_000);
        experiment.evidence = ExperimentEvidence {
            kind: EvidenceKind::LiveHardwareFeasibility,
            physical_device: true,
            backend: "wsl-dxcore-d3d12".to_owned(),
            driver_under_test: "signed-windows-driver-gpu-pv".to_owned(),
            vendor_id: Some(0x8086),
            device_id: Some(0x56a5),
            run_id: "live-test-1".to_owned(),
        };
        let report = experiment.evaluate().expect("valid live experiment");
        assert_eq!(report.verdict, ExperimentVerdict::RetainForNativeValidation);
    }

    #[test]
    fn promotes_only_native_atom_arc_evidence() {
        let mut experiment = record(800_000);
        experiment.evidence = ExperimentEvidence {
            kind: EvidenceKind::NativeDriver,
            physical_device: true,
            backend: "native-linux-pci".to_owned(),
            driver_under_test: "atom_arc".to_owned(),
            vendor_id: Some(0x8086),
            device_id: Some(0x56a5),
            run_id: "native-test-1".to_owned(),
        };
        let report = experiment.evaluate().expect("valid native experiment");
        assert_eq!(report.verdict, ExperimentVerdict::Promote);
    }

    #[test]
    fn rejects_spoofed_live_evidence() {
        let mut experiment = record(800_000);
        experiment.evidence.kind = EvidenceKind::LiveHardwareFeasibility;
        assert_eq!(
            experiment.evaluate(),
            Err(ExperimentError::InvalidLiveEvidence)
        );
    }

    #[test]
    fn rejects_an_effect_below_the_gate() {
        let report = record(950_000).evaluate().expect("valid experiment");
        assert_eq!(report.verdict, ExperimentVerdict::Reject);
        assert!(!report.reasons.is_empty());
    }

    #[test]
    fn rejects_changed_output_even_when_faster() {
        let mut experiment = record(700_000);
        experiment.candidate[3].correctness_digest = "corrupt".to_owned();
        let report = experiment.evaluate().expect("valid experiment");
        assert_eq!(report.verdict, ExperimentVerdict::Reject);
        assert!(
            report
                .reasons
                .iter()
                .any(|reason| reason.contains("correctness digest"))
        );
    }

    #[test]
    fn rejects_unpaired_measurements() {
        let mut experiment = record(800_000);
        experiment.candidate.pop();
        assert_eq!(
            experiment.evaluate(),
            Err(ExperimentError::UnpairedSamples {
                baseline: 12,
                candidate: 11,
            })
        );
    }

    #[test]
    fn rejects_a_reordered_primitive_stack() {
        let mut experiment = record(800_000);
        experiment.spec.candidate_stack.swap(0, 1);
        assert!(matches!(
            experiment.evaluate(),
            Err(ExperimentError::StackOrderViolation { .. })
        ));
    }
}
