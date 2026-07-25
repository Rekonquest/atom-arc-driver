use atom_arc_dxcore::{
    DEFAULT_COMPUTE_ADDEND, DxcoreSmokeReport, run_arc_a380_compound_lab, run_arc_a380_smoke,
    run_arc_a380_transfer_lab,
};
use atom_arc_lab::{
    DriverPhase, EvidenceKind, ExperimentEvidence, ExperimentRecord, ExperimentReport,
    ExperimentSpec, ExperimentVerdict, Measurement, PrimitiveDomain,
};
use atom_arc_model::{Command, CommandBuffer};
use atom_arc_platform::{EnvironmentKind, PlatformReport, TransportKind, probe_platform};
use atom_arc_runtime::{
    HardwareStressReport, HardwareSubmissionReceipt, Runtime, SelfTestReport, run_hardware_stress,
    run_self_test,
};
use atom_arc_security::{FirmwareOperation, SecurityPolicy};
use clap::{Parser, Subcommand};
use serde::Serialize;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Parser)]
#[command(
    name = "atom-arcctl",
    version,
    about = "Security-preserving Intel Arc driver foundation"
)]
struct Cli {
    #[command(subcommand)]
    command: CliCommand,
}

#[derive(Debug, Subcommand)]
enum CliCommand {
    /// Discover the host, Arc adapters, and usable transport boundaries.
    Probe {
        /// Emit machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
    /// Exercise allocation, validation, copy, stale-handle, and fail-closed behavior.
    SelfTest {
        /// Emit machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
    /// Report the enforced firmware and device security policy.
    SecurityAudit {
        /// Emit machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
    /// Evaluate paired baseline/candidate measurements against the phase gate.
    ExperimentEvaluate {
        /// JSON experiment record containing paired measurements.
        #[arg(long)]
        input: PathBuf,
        /// Emit machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
    /// Run a paired fresh-versus-reusable staging experiment on the live Arc A380.
    ExperimentLiveTransfer {
        /// Logical host-to-VRAM bytes transferred by each operation.
        #[arg(long, default_value_t = 4096)]
        payload_bytes: u32,
        /// Transfers in each paired sample.
        #[arg(long, default_value_t = 16)]
        operations: u32,
        /// Paired measured samples (10 through 100).
        #[arg(long, default_value_t = 12)]
        samples: u32,
        /// Unmeasured warmup pairs.
        #[arg(long, default_value_t = 3)]
        warmups: u32,
        /// Maximum time to wait for each GPU fence completion.
        #[arg(long, default_value_t = 5000)]
        timeout_ms: u32,
        /// Write the exact live experiment record here.
        #[arg(
            long,
            default_value = "target/wsl/experiments/reusable-staging-buffer-pool.json"
        )]
        output: PathBuf,
        /// Emit the evaluated statistics as machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
    /// Compound the retained reusable pool with two preloaded transfer lanes.
    ExperimentLiveCompound {
        /// Logical host-to-VRAM bytes transferred by each operation.
        #[arg(long, default_value_t = 4096)]
        payload_bytes: u32,
        /// Transfers in each paired sample (at least two).
        #[arg(long, default_value_t = 16)]
        operations: u32,
        /// Paired measured samples (10 through 100).
        #[arg(long, default_value_t = 12)]
        samples: u32,
        /// Unmeasured warmup pairs.
        #[arg(long, default_value_t = 3)]
        warmups: u32,
        /// Maximum time to wait for each GPU fence completion.
        #[arg(long, default_value_t = 5000)]
        timeout_ms: u32,
        /// Write the exact live experiment record here.
        #[arg(
            long,
            default_value = "target/wsl/experiments/double-buffered-preload.json"
        )]
        output: PathBuf,
        /// Emit the evaluated statistics as machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
    /// Exercise the legacy WSL/DxCore Arc comparison path.
    HardwareSmoke {
        /// Maximum time to wait for GPU fence completion.
        #[arg(long, default_value_t = 5000)]
        timeout_ms: u32,
        /// Emit machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
    /// Run registered kernel 1 through the legacy WSL hardware-dispatch path.
    Compute {
        /// Number of 64-thread X groups to dispatch.
        #[arg(long, default_value_t = 16)]
        groups: u32,
        /// Constant added by kernel 1 before its output transform.
        #[arg(long, default_value_t = DEFAULT_COMPUTE_ADDEND)]
        addend: u32,
        /// Maximum time to wait for GPU fence completion.
        #[arg(long, default_value_t = 5000)]
        timeout_ms: u32,
        /// Emit machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
    /// Stress the legacy WSL path; this does not exercise atom_arc.ko.
    Stress {
        /// Number of 64-thread X groups per dispatch.
        #[arg(long, default_value_t = 256)]
        groups: u32,
        /// Number of independently verified dispatches (1 through 100).
        #[arg(long, default_value_t = 10)]
        iterations: u32,
        /// Initial kernel addend; each iteration uses the next wrapping value.
        #[arg(long, default_value_t = DEFAULT_COMPUTE_ADDEND)]
        addend: u32,
        /// Maximum time to wait for each GPU fence completion.
        #[arg(long, default_value_t = 5000)]
        timeout_ms: u32,
        /// Emit machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
    /// Apply the current readiness gates.
    Doctor {
        /// Fail unless a real GPU submission backend is enabled.
        #[arg(long)]
        require_hardware: bool,
        /// Emit machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        CliCommand::Probe { json } => {
            let report = probe_platform();
            if json {
                print_json(&report)
            } else {
                print_probe(&report);
                ExitCode::SUCCESS
            }
        }
        CliCommand::SelfTest { json } => {
            let report = run_self_test();
            let passed = report.passed;
            let output = if json {
                print_json(&report)
            } else {
                print_self_test(&report);
                ExitCode::SUCCESS
            };
            if output != ExitCode::SUCCESS {
                output
            } else if passed {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        CliCommand::SecurityAudit { json } => {
            let report = security_audit();
            let passed = report.passed;
            let output = if json {
                print_json(&report)
            } else {
                print_security_audit(&report);
                ExitCode::SUCCESS
            };
            if output != ExitCode::SUCCESS {
                output
            } else if passed {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        CliCommand::ExperimentEvaluate { input, json } => evaluate_experiment(&input, json),
        CliCommand::ExperimentLiveTransfer {
            payload_bytes,
            operations,
            samples,
            warmups,
            timeout_ms,
            output,
            json,
        } => run_live_transfer_experiment(
            payload_bytes,
            operations,
            samples,
            warmups,
            timeout_ms,
            &output,
            json,
        ),
        CliCommand::ExperimentLiveCompound {
            payload_bytes,
            operations,
            samples,
            warmups,
            timeout_ms,
            output,
            json,
        } => run_live_compound_experiment(
            payload_bytes,
            operations,
            samples,
            warmups,
            timeout_ms,
            &output,
            json,
        ),
        CliCommand::HardwareSmoke { timeout_ms, json } => {
            run_legacy_hardware_smoke(timeout_ms, json)
        }
        CliCommand::Compute {
            groups,
            addend,
            timeout_ms,
            json,
        } => {
            let commands = CommandBuffer::new().with_command(Command::Dispatch {
                kernel_id: 1,
                groups: [groups, 1, 1],
                bindings: Vec::new(),
            });
            match Runtime::default().submit_hardware(&commands, addend, timeout_ms) {
                Ok(receipt) => {
                    if json {
                        print_json(&receipt)
                    } else {
                        print_compute(&receipt);
                        ExitCode::SUCCESS
                    }
                }
                Err(error) => {
                    eprintln!("Atom Arc compute: ERROR\n  {error}");
                    ExitCode::from(2)
                }
            }
        }
        CliCommand::Stress {
            groups,
            iterations,
            addend,
            timeout_ms,
            json,
        } => match run_hardware_stress(groups, addend, iterations, timeout_ms) {
            Ok(report) => {
                if json {
                    print_json(&report)
                } else {
                    print_stress(&report);
                    ExitCode::SUCCESS
                }
            }
            Err(error) => {
                eprintln!("Atom Arc stress: ERROR\n  {error}");
                ExitCode::from(2)
            }
        },
        CliCommand::Doctor {
            require_hardware,
            json,
        } => {
            let report = doctor(require_hardware);
            let passed = report.passed;
            let output = if json {
                print_json(&report)
            } else {
                print_doctor(&report);
                ExitCode::SUCCESS
            };
            if output != ExitCode::SUCCESS {
                output
            } else if passed {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
    }
}

fn print_json(value: &impl Serialize) -> ExitCode {
    match serde_json::to_string_pretty(value) {
        Ok(json) => {
            println!("{json}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("could not serialize report: {error}");
            ExitCode::from(2)
        }
    }
}

fn run_legacy_hardware_smoke(timeout_ms: u32, json: bool) -> ExitCode {
    let platform = probe_platform();
    if platform.environment != EnvironmentKind::Wsl2 {
        eprintln!(
            "Atom Arc legacy hardware smoke: ERROR\n  DxCore is WSL-only; detected {:?}",
            platform.environment
        );
        return ExitCode::from(2);
    }
    match run_arc_a380_smoke(timeout_ms) {
        Ok(report) => {
            let passed = report.is_verified_arc_a380_submission();
            let output = if json {
                print_json(&report)
            } else {
                print_hardware_smoke(&report);
                ExitCode::SUCCESS
            };
            if output != ExitCode::SUCCESS {
                output
            } else if passed {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        Err(error) => {
            eprintln!("Atom Arc legacy hardware smoke: ERROR\n  {error}");
            ExitCode::from(2)
        }
    }
}

fn run_live_transfer_experiment(
    payload_bytes: u32,
    operations: u32,
    samples: u32,
    warmups: u32,
    timeout_ms: u32,
    output: &std::path::Path,
    json: bool,
) -> ExitCode {
    let capture =
        match run_arc_a380_transfer_lab(payload_bytes, operations, samples, warmups, timeout_ms) {
            Ok(capture) => capture,
            Err(error) => {
                eprintln!("Atom Arc live transfer experiment: ERROR\n  {error}");
                return ExitCode::from(2);
            }
        };
    if !capture.is_verified_arc_a380_capture(payload_bytes, operations, samples, warmups) {
        eprintln!(
            "Atom Arc live transfer experiment: ERROR\n  helper returned an incomplete or unverified A380 capture"
        );
        return ExitCode::from(2);
    }

    let run_timestamp = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_secs(),
        Err(error) => {
            eprintln!("Atom Arc live transfer experiment: ERROR\n  clock error: {error}");
            return ExitCode::from(2);
        }
    };
    let run_id = format!("wsl-dxcore-a380-{payload_bytes}-{operations}-{samples}-{run_timestamp}");
    let logical_bytes = u64::from(payload_bytes) * u64::from(operations);
    let baseline = capture
        .baseline_durations_ns
        .iter()
        .zip(capture.correctness_digests.iter())
        .map(|(duration_ns, digest)| Measurement {
            bytes: logical_bytes,
            operations: u64::from(operations),
            duration_ns: *duration_ns,
            correctness_digest: format!("fnv64-{digest:016x}"),
            error: None,
        })
        .collect();
    let candidate = capture
        .candidate_durations_ns
        .iter()
        .zip(capture.correctness_digests.iter())
        .map(|(duration_ns, digest)| Measurement {
            bytes: logical_bytes,
            operations: u64::from(operations),
            duration_ns: *duration_ns,
            correctness_digest: format!("fnv64-{digest:016x}"),
            error: None,
        })
        .collect();
    let record = ExperimentRecord {
        spec: ExperimentSpec {
            id: format!("phase1-reusable-staging-{payload_bytes}-{operations}"),
            phase: DriverPhase::HostToVramOnRamp,
            domain: PrimitiveDomain::AedesMemoryTiering,
            primitive: "reusable-staging-buffer-pool".to_owned(),
            baseline_stack: vec!["secure-bring-up".to_owned()],
            candidate_stack: vec![
                "secure-bring-up".to_owned(),
                "reusable-staging-buffer-pool".to_owned(),
            ],
            hypothesis: "reusing persistently mapped staging buffers removes allocation and mapping churn from repeated host-to-VRAM transfers".to_owned(),
            minimum_samples: usize::try_from(samples).unwrap_or(usize::MAX),
            minimum_mean_improvement_percent: 10.0,
            maximum_p95_latency_regression_percent: 5.0,
        },
        evidence: ExperimentEvidence {
            kind: EvidenceKind::LiveHardwareFeasibility,
            physical_device: true,
            backend: capture.backend.clone(),
            driver_under_test: "signed-windows-driver-gpu-pv".to_owned(),
            vendor_id: Some(capture.vendor_id),
            device_id: Some(capture.device_id),
            run_id,
        },
        baseline,
        candidate,
    };
    let report = match record.evaluate() {
        Ok(report) => report,
        Err(error) => {
            eprintln!("Atom Arc live transfer experiment: ERROR\n  {error}");
            return ExitCode::from(2);
        }
    };

    if let Some(parent) = output.parent()
        && let Err(error) = fs::create_dir_all(parent)
    {
        eprintln!(
            "Atom Arc live transfer experiment: ERROR\n  could not create {}: {error}",
            parent.display()
        );
        return ExitCode::from(2);
    }
    let serialized = match serde_json::to_vec_pretty(&record) {
        Ok(serialized) => serialized,
        Err(error) => {
            eprintln!("Atom Arc live transfer experiment: ERROR\n  {error}");
            return ExitCode::from(2);
        }
    };
    if let Err(error) = fs::write(output, serialized) {
        eprintln!(
            "Atom Arc live transfer experiment: ERROR\n  could not write {}: {error}",
            output.display()
        );
        return ExitCode::from(2);
    }

    let verdict = report.verdict;
    let print_status = if json {
        print_json(&report)
    } else {
        print_experiment(&report);
        println!("  live record: {}", output.display());
        println!("  reusable pool setup: {} ns", capture.pool_setup_ns);
        ExitCode::SUCCESS
    };
    if print_status != ExitCode::SUCCESS {
        print_status
    } else if verdict == ExperimentVerdict::RetainForNativeValidation {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn run_live_compound_experiment(
    payload_bytes: u32,
    operations: u32,
    samples: u32,
    warmups: u32,
    timeout_ms: u32,
    output: &std::path::Path,
    json: bool,
) -> ExitCode {
    let capture =
        match run_arc_a380_compound_lab(payload_bytes, operations, samples, warmups, timeout_ms) {
            Ok(capture) => capture,
            Err(error) => {
                eprintln!("Atom Arc live compound experiment: ERROR\n  {error}");
                return ExitCode::from(2);
            }
        };
    if !capture.is_verified_arc_a380_compound_capture(payload_bytes, operations, samples, warmups) {
        eprintln!(
            "Atom Arc live compound experiment: ERROR\n  helper returned an incomplete or unverified A380 compound capture"
        );
        return ExitCode::from(2);
    }

    let run_timestamp = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_secs(),
        Err(error) => {
            eprintln!("Atom Arc live compound experiment: ERROR\n  clock error: {error}");
            return ExitCode::from(2);
        }
    };
    let run_id =
        format!("wsl-dxcore-a380-compound-{payload_bytes}-{operations}-{samples}-{run_timestamp}");
    let logical_bytes = u64::from(payload_bytes) * u64::from(operations);
    let baseline = capture
        .baseline_durations_ns
        .iter()
        .zip(capture.correctness_digests.iter())
        .map(|(duration_ns, digest)| Measurement {
            bytes: logical_bytes,
            operations: u64::from(operations),
            duration_ns: *duration_ns,
            correctness_digest: format!("fnv64-{digest:016x}"),
            error: None,
        })
        .collect();
    let candidate = capture
        .candidate_durations_ns
        .iter()
        .zip(capture.correctness_digests.iter())
        .map(|(duration_ns, digest)| Measurement {
            bytes: logical_bytes,
            operations: u64::from(operations),
            duration_ns: *duration_ns,
            correctness_digest: format!("fnv64-{digest:016x}"),
            error: None,
        })
        .collect();
    let record = ExperimentRecord {
        spec: ExperimentSpec {
            id: format!("phase1-compound-double-buffer-{payload_bytes}-{operations}"),
            phase: DriverPhase::HostToVramOnRamp,
            domain: PrimitiveDomain::SpiderwebFabric,
            primitive: "double-buffered-preload".to_owned(),
            baseline_stack: vec![
                "secure-bring-up".to_owned(),
                "reusable-staging-buffer-pool".to_owned(),
            ],
            candidate_stack: vec![
                "secure-bring-up".to_owned(),
                "reusable-staging-buffer-pool".to_owned(),
                "double-buffered-preload".to_owned(),
            ],
            hypothesis: "two persistent transfer lanes overlap preparation of the next payload with the current GPU copy without changing firmware authority".to_owned(),
            minimum_samples: usize::try_from(samples).unwrap_or(usize::MAX),
            minimum_mean_improvement_percent: 10.0,
            maximum_p95_latency_regression_percent: 5.0,
        },
        evidence: ExperimentEvidence {
            kind: EvidenceKind::LiveHardwareFeasibility,
            physical_device: true,
            backend: capture.backend.clone(),
            driver_under_test: "signed-windows-driver-gpu-pv".to_owned(),
            vendor_id: Some(capture.vendor_id),
            device_id: Some(capture.device_id),
            run_id,
        },
        baseline,
        candidate,
    };
    let report = match record.evaluate() {
        Ok(report) => report,
        Err(error) => {
            eprintln!("Atom Arc live compound experiment: ERROR\n  {error}");
            return ExitCode::from(2);
        }
    };

    if let Some(parent) = output.parent()
        && let Err(error) = fs::create_dir_all(parent)
    {
        eprintln!(
            "Atom Arc live compound experiment: ERROR\n  could not create {}: {error}",
            parent.display()
        );
        return ExitCode::from(2);
    }
    let serialized = match serde_json::to_vec_pretty(&record) {
        Ok(serialized) => serialized,
        Err(error) => {
            eprintln!("Atom Arc live compound experiment: ERROR\n  {error}");
            return ExitCode::from(2);
        }
    };
    if let Err(error) = fs::write(output, serialized) {
        eprintln!(
            "Atom Arc live compound experiment: ERROR\n  could not write {}: {error}",
            output.display()
        );
        return ExitCode::from(2);
    }

    let verdict = report.verdict;
    let print_status = if json {
        print_json(&report)
    } else {
        print_experiment(&report);
        println!("  live record: {}", output.display());
        println!("  retained-pool setup: {} ns", capture.pool_setup_ns);
        println!(
            "  double-buffer candidate setup: {} ns",
            capture.candidate_setup_ns
        );
        ExitCode::SUCCESS
    };
    if print_status != ExitCode::SUCCESS {
        print_status
    } else if verdict == ExperimentVerdict::RetainForNativeValidation {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn evaluate_experiment(path: &std::path::Path, json: bool) -> ExitCode {
    let input = match std::fs::read(path) {
        Ok(input) => input,
        Err(error) => {
            eprintln!(
                "Atom Arc experiment: ERROR\n  could not read {}: {error}",
                path.display()
            );
            return ExitCode::from(2);
        }
    };
    let record = match serde_json::from_slice::<ExperimentRecord>(&input) {
        Ok(record) => record,
        Err(error) => {
            eprintln!(
                "Atom Arc experiment: ERROR\n  could not parse {}: {error}",
                path.display()
            );
            return ExitCode::from(2);
        }
    };
    let report = match record.evaluate() {
        Ok(report) => report,
        Err(error) => {
            eprintln!("Atom Arc experiment: ERROR\n  {error}");
            return ExitCode::from(2);
        }
    };
    let verdict = report.verdict;
    let output = if json {
        print_json(&report)
    } else {
        print_experiment(&report);
        ExitCode::SUCCESS
    };
    if output != ExitCode::SUCCESS {
        output
    } else if verdict != ExperimentVerdict::Reject {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn print_experiment(report: &ExperimentReport) {
    println!("Atom Arc experiment: {:?}", report.verdict);
    println!("  id: {}", report.id);
    println!("  phase: {:?}", report.phase);
    println!("  primitive: {}", report.primitive);
    println!("  evidence: {:?}", report.evidence_kind);
    if !report.evidence_backend.is_empty() {
        println!("  backend: {}", report.evidence_backend);
        println!("  driver under test: {}", report.driver_under_test);
        println!("  run id: {}", report.run_id);
    }
    println!("  baseline stack: {}", report.baseline_stack.join(" -> "));
    println!("  candidate stack: {}", report.candidate_stack.join(" -> "));
    println!("  paired samples: {}", report.sample_count);
    println!(
        "  throughput: {:.3} -> {:.3} bytes/s",
        report.baseline_mean_bytes_per_second, report.candidate_mean_bytes_per_second
    );
    println!(
        "  improvement mean/median/lower95: {:.3}% / {:.3}% / {:.3}%",
        report.mean_improvement_percent,
        report.median_improvement_percent,
        report.paired_95_percent_lower_bound
    );
    println!(
        "  p95 latency: {} -> {} ns ({:.3}%)",
        report.baseline_p95_latency_ns,
        report.candidate_p95_latency_ns,
        report.p95_latency_regression_percent
    );
    for reason in &report.reasons {
        println!("  reject: {reason}");
    }
}

fn print_probe(report: &PlatformReport) {
    println!("Atom Arc platform probe");
    println!("environment: {:?}", report.environment);
    println!(
        "kernel: {}",
        report.kernel.as_deref().unwrap_or("not reported")
    );
    if report.adapters.is_empty() {
        println!("adapters: none directly enumerated");
    } else {
        println!("adapters:");
        for adapter in &report.adapters {
            let pci = adapter
                .pci_id
                .map_or_else(|| "unknown PCI id".to_owned(), |value| value.to_string());
            println!("  - {} ({pci})", adapter.name);
            if let Some(version) = &adapter.driver_version {
                println!("    driver version: {version}");
            }
        }
    }
    println!("transports:");
    for transport in &report.transports {
        println!(
            "  - {:?}: visible={}, submission_ready={}",
            transport.kind, transport.device_visible, transport.submission_ready
        );
        println!("    {}", transport.detail);
    }
    println!("firmware authority: {}", report.firmware.owner);
    println!("security fail-closed: {}", report.security.is_fail_closed());
    for note in &report.notes {
        println!("note: {note}");
    }
}

fn print_self_test(report: &SelfTestReport) {
    println!("Atom Arc self-test: {}", pass_word(report.passed));
    for check in &report.checks {
        println!("  [{}] {}", pass_word(check.passed), check.name);
        println!("      {}", check.detail);
    }
}

fn print_hardware_smoke(report: &DxcoreSmokeReport) {
    let passed = report.is_verified_arc_a380_submission();
    println!("Atom Arc hardware smoke: {}", pass_word(passed));
    println!("  adapter: {}", report.adapter_name);
    println!(
        "  PCI id: {:04x}:{:04x}",
        report.vendor_id, report.device_id
    );
    println!("  adapters seen: {}", report.adapters_seen);
    println!("  D3D12 device created: {}", report.device_created);
    println!("  compute queue created: {}", report.queue_created);
    println!("  GPU resources created: {}", report.resources_created);
    println!("  GPU copy recorded: {}", report.copy_recorded);
    println!("  command list submitted: {}", report.submitted);
    println!("  fence completed: {}", report.fence_completed);
    println!("  GPU copy verified: {}", report.copy_verified);
    println!("  bytes copied: {}", report.bytes_copied);
    println!("  copy checksum: {}", report.copy_checksum);
    println!(
        "  compute pipeline created: {}",
        report.compute_pipeline_created
    );
    println!(
        "  compute input uploaded: {}",
        report.compute_input_uploaded
    );
    println!("  compute dispatched: {}", report.compute_dispatched);
    println!("  compute verified: {}", report.compute_verified);
    println!("  compute elements: {}", report.compute_elements);
    println!("  compute addend: 0x{:08x}", report.compute_addend);
    println!("  compute input bytes: {}", report.compute_input_bytes);
    println!("  compute output bytes: {}", report.compute_output_bytes);
    println!("  compute checksum: {}", report.compute_checksum);
    println!(
        "  device removed reason: 0x{:08x}",
        report.device_removed_reason
    );
    println!("  elapsed: {} us", report.elapsed_microseconds);
    println!("  detail: {}", report.detail);
}

fn print_compute(receipt: &HardwareSubmissionReceipt) {
    println!("Atom Arc compute: PASS");
    println!("  backend: {}", receipt.backend);
    println!("  kernel: {}", receipt.kernel_id);
    println!("  groups: {:?}", receipt.groups);
    println!("  elements verified: {}", receipt.compute_elements);
    println!("  input bytes: {}", receipt.compute_input_bytes);
    println!("  output bytes: {}", receipt.compute_output_bytes);
    println!("  checksum: {}", receipt.compute_checksum);
    println!("  elapsed: {} us", receipt.elapsed_microseconds);
    println!("  detail: {}", receipt.detail);
}

fn print_stress(report: &HardwareStressReport) {
    println!("Atom Arc stress: {}", pass_word(report.passed));
    println!("  backend: {}", report.backend);
    println!("  dispatches verified: {}", report.iterations);
    println!("  groups per dispatch: {:?}", report.groups);
    println!("  elements per dispatch: {}", report.elements_per_iteration);
    println!("  total elements: {}", report.total_elements);
    println!("  wall time: {} us", report.wall_microseconds);
    println!("  backend time sum: {} us", report.backend_microseconds);
    println!(
        "  backend time range: {}..={} us",
        report.minimum_backend_microseconds, report.maximum_backend_microseconds
    );
    println!("  process dispatches/s: {}", report.dispatches_per_second);
    println!("  verified elements/s: {}", report.elements_per_second);
}

#[derive(Debug, Serialize)]
struct SecurityAuditReport {
    passed: bool,
    authenticated_firmware_required: bool,
    firmware_mutation_denied: bool,
    authentication_bypass_denied: bool,
    userspace_pci_configuration_writes_denied: bool,
    userspace_raw_mmio_disabled: bool,
    kernel_submission_requires_hardware_authentication: bool,
    unsafe_rust_disabled: bool,
}

fn security_audit() -> SecurityAuditReport {
    let policy = SecurityPolicy::default();
    let posture = policy.posture();
    let firmware_mutation_denied = policy
        .authorize_firmware(FirmwareOperation::WriteBlob)
        .is_err()
        && policy
            .authorize_firmware(FirmwareOperation::FlashDevice)
            .is_err();
    let authentication_bypass_denied = policy
        .authorize_firmware(FirmwareOperation::BypassAuthentication)
        .is_err();
    let userspace_pci_configuration_writes_denied = policy.authorize_pci_config_write().is_err();
    let passed = posture.is_fail_closed()
        && firmware_mutation_denied
        && authentication_bypass_denied
        && userspace_pci_configuration_writes_denied;

    SecurityAuditReport {
        passed,
        authenticated_firmware_required: posture.authenticated_firmware_required,
        firmware_mutation_denied,
        authentication_bypass_denied,
        userspace_pci_configuration_writes_denied,
        userspace_raw_mmio_disabled: posture.userspace_raw_mmio_disabled,
        kernel_submission_requires_hardware_authentication: posture
            .kernel_submission_requires_hardware_authentication,
        unsafe_rust_disabled: posture.unsafe_rust_disabled,
    }
}

fn print_security_audit(report: &SecurityAuditReport) {
    println!("Atom Arc security audit: {}", pass_word(report.passed));
    println!(
        "  authenticated firmware required: {}",
        report.authenticated_firmware_required
    );
    println!(
        "  firmware mutation denied: {}",
        report.firmware_mutation_denied
    );
    println!(
        "  authentication bypass denied: {}",
        report.authentication_bypass_denied
    );
    println!(
        "  userspace PCI configuration writes denied: {}",
        report.userspace_pci_configuration_writes_denied
    );
    println!(
        "  userspace raw MMIO disabled: {}",
        report.userspace_raw_mmio_disabled
    );
    println!(
        "  kernel submission requires hardware firmware authentication: {}",
        report.kernel_submission_requires_hardware_authentication
    );
    println!("  unsafe Rust disabled: {}", report.unsafe_rust_disabled);
}

#[derive(Debug, Serialize)]
struct DoctorCheck {
    name: String,
    passed: bool,
    detail: String,
}

#[derive(Debug, Serialize)]
struct DoctorReport {
    passed: bool,
    require_hardware: bool,
    checks: Vec<DoctorCheck>,
}

fn doctor(require_hardware: bool) -> DoctorReport {
    let platform = probe_platform();
    let self_test = run_self_test();
    let security = security_audit();
    let reference_ready = platform
        .transports
        .iter()
        .any(|transport| transport.kind == TransportKind::Reference && transport.submission_ready);
    let mut checks = vec![
        DoctorCheck {
            name: "security-policy".to_owned(),
            passed: security.passed,
            detail: "Firmware and privileged device operations remain fail-closed".to_owned(),
        },
        DoctorCheck {
            name: "reference-runtime".to_owned(),
            passed: reference_ready && self_test.passed,
            detail: "Validated memory and command behavior is executable".to_owned(),
        },
    ];
    if require_hardware {
        let hardware_check = match run_arc_a380_smoke(5000) {
            Ok(report) => {
                let passed = report.is_verified_arc_a380_submission();
                DoctorCheck {
                    name: "hardware-submission".to_owned(),
                    passed,
                    detail: if passed {
                        format!(
                            "{} ({:04x}:{:04x}) copied {} verified bytes and dispatched {} verified compute elements with fence completion in {} us",
                            report.adapter_name,
                            report.vendor_id,
                            report.device_id,
                            report.bytes_copied,
                            report.compute_elements,
                            report.elapsed_microseconds
                        )
                    } else {
                        format!(
                            "DxCore helper stopped at {}: {}",
                            report.stage, report.detail
                        )
                    },
                }
            }
            Err(error) => DoctorCheck {
                name: "hardware-submission".to_owned(),
                passed: false,
                detail: error.to_string(),
            },
        };
        checks.push(hardware_check);
    }

    DoctorReport {
        passed: checks.iter().all(|check| check.passed),
        require_hardware,
        checks,
    }
}

fn print_doctor(report: &DoctorReport) {
    println!("Atom Arc doctor: {}", pass_word(report.passed));
    for check in &report.checks {
        println!("  [{}] {}", pass_word(check.passed), check.name);
        println!("      {}", check.detail);
    }
}

const fn pass_word(passed: bool) -> &'static str {
    if passed { "PASS" } else { "FAIL" }
}
