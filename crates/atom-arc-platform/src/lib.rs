//! Read-only platform discovery for Windows, WSL, Docker, and native Linux Xe.

#[cfg(target_os = "linux")]
use atom_arc_core::SecuritySnapshot;
use atom_arc_model::{DeviceProfile, PciId};
use atom_arc_security::{SecurityPolicy, SecurityPosture};
use serde::{Deserialize, Serialize};
#[cfg(target_os = "linux")]
use std::fs::{self, OpenOptions};
#[cfg(target_os = "linux")]
use std::path::Path;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EnvironmentKind {
    WindowsHost,
    Wsl2,
    Docker,
    NativeLinux,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TransportKind {
    WindowsWddmHost,
    WslDxg,
    NativeAtomPci,
    NativeXeDrm,
    Reference,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransportProbe {
    pub kind: TransportKind,
    pub device_visible: bool,
    pub submission_ready: bool,
    pub path: Option<PathBuf>,
    pub detail: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterObservation {
    pub name: String,
    pub pci_id: Option<PciId>,
    pub driver: Option<String>,
    pub driver_version: Option<String>,
    pub device_path: Option<PathBuf>,
    pub profile: Option<DeviceProfile>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FirmwareAuthority {
    pub owner: String,
    pub authenticated_chain_required: bool,
    pub mutation_attempted: bool,
    pub detail: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlatformReport {
    pub environment: EnvironmentKind,
    pub kernel: Option<String>,
    pub adapters: Vec<AdapterObservation>,
    pub transports: Vec<TransportProbe>,
    pub firmware: FirmwareAuthority,
    pub security: SecurityPosture,
    pub notes: Vec<String>,
}

impl PlatformReport {
    #[must_use]
    pub fn has_arc_a380(&self) -> bool {
        self.adapters.iter().any(|adapter| {
            adapter
                .pci_id
                .is_some_and(|pci_id| pci_id.vendor == 0x8086 && pci_id.device == 0x56a5)
                || adapter.name.to_ascii_lowercase().contains("arc")
        })
    }

    #[must_use]
    pub fn hardware_submission_ready(&self) -> bool {
        self.transports.iter().any(|transport| {
            transport.kind != TransportKind::Reference && transport.submission_ready
        })
    }
}

#[must_use]
pub fn probe_platform() -> PlatformReport {
    let policy = SecurityPolicy::default();
    #[cfg(target_os = "windows")]
    {
        probe_windows(policy)
    }
    #[cfg(target_os = "linux")]
    {
        probe_linux(policy)
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        PlatformReport {
            environment: EnvironmentKind::Unknown,
            kernel: None,
            adapters: Vec::new(),
            transports: vec![reference_transport()],
            firmware: FirmwareAuthority {
                owner: "unknown".to_owned(),
                authenticated_chain_required: true,
                mutation_attempted: false,
                detail: "Unsupported host; firmware access is disabled".to_owned(),
            },
            security: policy.posture(),
            notes: vec!["Only the reference backend is available on this host".to_owned()],
        }
    }
}

fn reference_transport() -> TransportProbe {
    TransportProbe {
        kind: TransportKind::Reference,
        device_visible: true,
        submission_ready: true,
        path: None,
        detail: "Memory and command semantics execute in the deterministic CPU reference backend"
            .to_owned(),
    }
}

#[cfg(target_os = "windows")]
fn probe_windows(policy: SecurityPolicy) -> PlatformReport {
    use serde_json::Value;
    use std::process::Command;

    const QUERY: &str = "Get-CimInstance Win32_VideoController | Select-Object Name,PNPDeviceID,DriverVersion,Status | ConvertTo-Json -Compress";
    let mut notes = Vec::new();
    let mut adapters = Vec::new();

    match Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", QUERY])
        .output()
    {
        Ok(output) if output.status.success() => {
            match serde_json::from_slice::<Value>(&output.stdout) {
                Ok(Value::Array(values)) => {
                    for value in values {
                        if let Some(adapter) = windows_adapter_from_json(&value) {
                            adapters.push(adapter);
                        }
                    }
                }
                Ok(value @ Value::Object(_)) => {
                    if let Some(adapter) = windows_adapter_from_json(&value) {
                        adapters.push(adapter);
                    }
                }
                Ok(_) => {
                    notes.push("Windows adapter query returned an unexpected shape".to_owned())
                }
                Err(error) => notes.push(format!("Could not parse Windows adapter query: {error}")),
            }
        }
        Ok(output) => notes.push(format!(
            "Windows adapter query failed with status {}",
            output.status
        )),
        Err(error) => notes.push(format!("Could not start Windows adapter query: {error}")),
    }

    let arc_visible = adapters.iter().any(|adapter| {
        adapter
            .pci_id
            .is_some_and(|pci| pci.vendor == 0x8086 && pci.device == 0x56a5)
    });

    PlatformReport {
        environment: EnvironmentKind::WindowsHost,
        kernel: windows_build(),
        adapters,
        transports: vec![
            TransportProbe {
                kind: TransportKind::WindowsWddmHost,
                device_visible: arc_visible,
                submission_ready: false,
                path: None,
                detail: "The signed Intel WDDM host driver owns the GPU; Atom Arc does not replace the KMD"
                    .to_owned(),
            },
            reference_transport(),
        ],
        firmware: FirmwareAuthority {
            owner: "signed Intel WDDM host driver".to_owned(),
            authenticated_chain_required: true,
            mutation_attempted: false,
            detail: "Atom Arc performs no Windows firmware or PCI configuration writes".to_owned(),
        },
        security: policy.posture(),
        notes,
    }
}

#[cfg(target_os = "windows")]
fn windows_adapter_from_json(value: &serde_json::Value) -> Option<AdapterObservation> {
    let name = value.get("Name")?.as_str()?.to_owned();
    let pnp_id = value
        .get("PNPDeviceID")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let pci_id = parse_windows_pnp_id(pnp_id);
    Some(AdapterObservation {
        name,
        pci_id,
        driver: Some("wddm".to_owned()),
        driver_version: value
            .get("DriverVersion")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned),
        device_path: None,
        profile: pci_id.map(DeviceProfile::from_pci_id),
    })
}

#[cfg(target_os = "windows")]
fn parse_windows_pnp_id(value: &str) -> Option<PciId> {
    fn marker(value: &str, marker: &str) -> Option<u16> {
        let start = value.find(marker)?.checked_add(marker.len())?;
        let end = start.checked_add(4)?;
        u16::from_str_radix(value.get(start..end)?, 16).ok()
    }

    Some(PciId::new(marker(value, "VEN_")?, marker(value, "DEV_")?))
}

#[cfg(target_os = "windows")]
fn windows_build() -> Option<String> {
    std::process::Command::new("cmd.exe")
        .args(["/c", "ver"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

#[cfg(target_os = "linux")]
fn probe_linux(policy: SecurityPolicy) -> PlatformReport {
    let kernel = read_trimmed("/proc/sys/kernel/osrelease");
    let is_wsl = kernel
        .as_deref()
        .is_some_and(|value| value.to_ascii_lowercase().contains("microsoft"));
    let is_docker = Path::new("/.dockerenv").exists()
        || std::env::var("container").is_ok_and(|value| !value.is_empty());
    let environment = if is_docker {
        EnvironmentKind::Docker
    } else if is_wsl {
        EnvironmentKind::Wsl2
    } else {
        EnvironmentKind::NativeLinux
    };

    let (mut adapters, mut notes) = probe_linux_drm();
    let (atom_transport, atom_adapter) = probe_native_atom();
    let atom_bound = atom_adapter
        .as_ref()
        .is_some_and(|adapter| adapter.driver.as_deref() == Some("atom_arc"));
    if let Some(adapter) = atom_adapter
        && !adapters
            .iter()
            .any(|candidate| candidate.pci_id == adapter.pci_id)
    {
        adapters.push(adapter);
    }
    let mut transports = vec![
        probe_dxg(),
        atom_transport,
        probe_native_xe(&adapters),
        reference_transport(),
    ];
    transports.retain(|transport| {
        transport.kind == TransportKind::Reference
            || transport.device_visible
            || transport.kind == TransportKind::WslDxg
    });

    let firmware = if is_wsl {
        FirmwareAuthority {
            owner: "signed Windows host driver through GPU-PV".to_owned(),
            authenticated_chain_required: true,
            mutation_attempted: false,
            detail: "WSL exposes /dev/dxg; the guest does not own GuC, HuC, GSC, or board firmware"
                .to_owned(),
        }
    } else if atom_bound {
        FirmwareAuthority {
            owner: "Atom Arc standalone PCI driver and Intel-signed device firmware".to_owned(),
            authenticated_chain_required: true,
            mutation_attempted: false,
            detail: "Atom Arc validates production firmware containers itself; hardware authentication and submission remain locked until the GuC/GSC sequence succeeds"
                .to_owned(),
        }
    } else {
        FirmwareAuthority {
            owner: "native Linux kernel driver".to_owned(),
            authenticated_chain_required: true,
            mutation_attempted: false,
            detail:
                "The standalone Atom Arc PCI module is not bound to the A380 in this environment"
                    .to_owned(),
        }
    };

    if is_docker && !Path::new("/dev/dxg").exists() {
        notes.push(
            "Docker does not expose /dev/dxg; build and reference tests work, GPU-PV tests must run in WSL"
                .to_owned(),
        );
    }

    PlatformReport {
        environment,
        kernel,
        adapters,
        transports,
        firmware,
        security: policy.posture(),
        notes,
    }
}

#[cfg(target_os = "linux")]
fn probe_native_atom() -> (TransportProbe, Option<AdapterObservation>) {
    let devices_path = Path::new("/sys/bus/pci/devices");
    let entries = match fs::read_dir(devices_path) {
        Ok(entries) => entries,
        Err(error) => {
            return (
                TransportProbe {
                    kind: TransportKind::NativeAtomPci,
                    device_visible: false,
                    submission_ready: false,
                    path: Some(devices_path.to_path_buf()),
                    detail: format!("Could not enumerate PCI devices: {error}"),
                },
                None,
            );
        }
    };

    for entry in entries.flatten() {
        let sys_device = entry.path();
        let pci_id = read_hex_u16(sys_device.join("vendor"))
            .zip(read_hex_u16(sys_device.join("device")))
            .map(|(vendor, device)| PciId::new(vendor, device));
        if pci_id != Some(PciId::new(0x8086, 0x56a5)) {
            continue;
        }

        let driver = fs::read_link(sys_device.join("driver"))
            .ok()
            .and_then(|path| {
                path.file_name()
                    .map(|name| name.to_string_lossy().into_owned())
            });
        let atom_bound = driver.as_deref() == Some("atom_arc");
        let driver_state = read_trimmed(sys_device.join("atom_arc/driver_state"));
        let firmware_authenticated =
            read_trimmed(sys_device.join("atom_arc/firmware_authenticated"));
        let guc_auth = read_trimmed(sys_device.join("atom_arc/guc_auth_status"));
        let guc_in_reset = read_trimmed(sys_device.join("atom_arc/guc_in_reset"));
        let boot_hash_valid = read_trimmed(sys_device.join("atom_arc/boot_hash_valid"));
        let wopcm_locked = read_trimmed(sys_device.join("atom_arc/wopcm_locked"));
        let core_security = read_hex_u32(sys_device.join("atom_arc/guc_status_raw"))
            .zip(read_hex_u32(sys_device.join("atom_arc/boot_hash_raw")))
            .zip(read_hex_u32(sys_device.join("atom_arc/wopcm_size_raw")))
            .zip(read_hex_u32(sys_device.join("atom_arc/wopcm_offset_raw")))
            .map(
                |(((guc_status_raw, boot_hash_raw), wopcm_size_raw), wopcm_offset_raw)| {
                    SecuritySnapshot {
                        guc_status_raw,
                        boot_hash_raw,
                        wopcm_size_raw,
                        wopcm_offset_raw,
                    }
                },
            );
        let core_authenticated = core_security.is_some_and(SecuritySnapshot::authenticated);
        let adapter_submission_enabled =
            read_trimmed(sys_device.join("atom_arc/submission_enabled")).as_deref() == Some("1");
        let submission_ready = atom_bound && core_authenticated && adapter_submission_enabled;
        let detail = if atom_bound {
            format!(
                "Atom Arc owns {}; state={}, adapter-authenticated={}, core-authenticated={}, GuC-auth={}, GuC-reset={}, boot-hash={}, WOPCM-locked={}, submission={}",
                entry.file_name().to_string_lossy(),
                driver_state.as_deref().unwrap_or("unreported"),
                firmware_authenticated.as_deref().unwrap_or("unreported"),
                core_authenticated,
                guc_auth.as_deref().unwrap_or("unreported"),
                guc_in_reset.as_deref().unwrap_or("unreported"),
                boot_hash_valid.as_deref().unwrap_or("unreported"),
                wopcm_locked.as_deref().unwrap_or("unreported"),
                if submission_ready {
                    "enabled"
                } else {
                    "locked"
                }
            )
        } else {
            format!(
                "Arc A380 is visible at {} but bound to {} instead of atom_arc",
                entry.file_name().to_string_lossy(),
                driver.as_deref().unwrap_or("no driver")
            )
        };

        return (
            TransportProbe {
                kind: TransportKind::NativeAtomPci,
                device_visible: true,
                submission_ready,
                path: Some(sys_device.clone()),
                detail,
            },
            Some(AdapterObservation {
                name: "Intel Arc A380".to_owned(),
                pci_id,
                driver,
                driver_version: None,
                device_path: Some(sys_device),
                profile: pci_id.map(DeviceProfile::from_pci_id),
            }),
        );
    }

    (
        TransportProbe {
            kind: TransportKind::NativeAtomPci,
            device_visible: false,
            submission_ready: false,
            path: None,
            detail: "No physical Intel Arc A380 PCI function is visible".to_owned(),
        },
        None,
    )
}

#[cfg(target_os = "linux")]
fn probe_dxg() -> TransportProbe {
    let path = PathBuf::from("/dev/dxg");
    if !path.exists() {
        return TransportProbe {
            kind: TransportKind::WslDxg,
            device_visible: false,
            submission_ready: false,
            path: Some(path),
            detail: "WSL GPU-PV device is not exposed in this environment".to_owned(),
        };
    }

    match OpenOptions::new().read(true).write(true).open(&path) {
        Ok(_) => {
            let helper = dxcore_helper_path();
            let submission_ready = helper.is_some();
            TransportProbe {
                kind: TransportKind::WslDxg,
                device_visible: true,
                submission_ready,
                path: Some(path),
                detail: helper.map_or_else(
                    || {
                        "/dev/dxg is accessible; build the DxCore helper to enable fenced submission"
                            .to_owned()
                    },
                    |helper| {
                        format!(
                            "/dev/dxg and the DxCore submission helper are available at {}",
                            helper.display()
                        )
                    },
                ),
            }
        }
        Err(error) => TransportProbe {
            kind: TransportKind::WslDxg,
            device_visible: true,
            submission_ready: false,
            path: Some(path),
            detail: format!("/dev/dxg exists but cannot be opened read/write: {error}"),
        },
    }
}

#[cfg(target_os = "linux")]
fn dxcore_helper_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("ATOM_ARC_DXCORE_HELPER") {
        let path = PathBuf::from(path);
        return path.is_file().then_some(path);
    }
    let current_exe = std::env::current_exe().ok()?;
    let helper = current_exe.parent()?.join("atom-arc-dxcore-helper");
    helper.is_file().then_some(helper)
}

#[cfg(target_os = "linux")]
fn probe_linux_drm() -> (Vec<AdapterObservation>, Vec<String>) {
    let mut adapters = Vec::new();
    let mut notes = Vec::new();
    let class_path = Path::new("/sys/class/drm");
    let entries = match fs::read_dir(class_path) {
        Ok(entries) => entries,
        Err(error) => {
            notes.push(format!("Could not read {}: {error}", class_path.display()));
            return (adapters, notes);
        }
    };

    for entry in entries.flatten() {
        let entry_name = entry.file_name().to_string_lossy().into_owned();
        if !entry_name.starts_with("renderD") {
            continue;
        }
        let sys_device = entry.path().join("device");
        let vendor = read_hex_u16(sys_device.join("vendor"));
        let device = read_hex_u16(sys_device.join("device"));
        let pci_id = vendor
            .zip(device)
            .map(|(vendor, device)| PciId::new(vendor, device));
        let driver = fs::read_link(sys_device.join("driver"))
            .ok()
            .and_then(|path| {
                path.file_name()
                    .map(|name| name.to_string_lossy().into_owned())
            });
        let node = PathBuf::from("/dev/dri").join(&entry_name);
        let profile = pci_id.map(DeviceProfile::from_pci_id);
        let name = profile.as_ref().map_or_else(
            || format!("DRM render node {entry_name}"),
            |value| value.name.clone(),
        );

        adapters.push(AdapterObservation {
            name,
            pci_id,
            driver,
            driver_version: None,
            device_path: Some(node),
            profile,
        });
    }
    (adapters, notes)
}

#[cfg(target_os = "linux")]
fn probe_native_xe(adapters: &[AdapterObservation]) -> TransportProbe {
    let xe_adapter = adapters
        .iter()
        .find(|adapter| adapter.driver.as_deref() == Some("xe"));
    let Some(adapter) = xe_adapter else {
        return TransportProbe {
            kind: TransportKind::NativeXeDrm,
            device_visible: false,
            submission_ready: false,
            path: None,
            detail: "No DRM render node bound to the xe kernel driver".to_owned(),
        };
    };
    let path = adapter.device_path.clone();
    let open_result = path
        .as_deref()
        .ok_or_else(|| "xe adapter has no render node".to_owned())
        .and_then(|path| {
            OpenOptions::new()
                .read(true)
                .write(true)
                .open(path)
                .map(|_| ())
                .map_err(|error| error.to_string())
        });

    TransportProbe {
        kind: TransportKind::NativeXeDrm,
        device_visible: true,
        submission_ready: false,
        path,
        detail: match open_result {
            Ok(()) => "Native xe render node is accessible; validated VM_BIND/exec wiring is not enabled yet"
                .to_owned(),
            Err(error) => format!("Native xe render node is present but inaccessible: {error}"),
        },
    }
}

#[cfg(target_os = "linux")]
fn read_hex_u16(path: impl AsRef<Path>) -> Option<u16> {
    let value = fs::read_to_string(path).ok()?;
    u16::from_str_radix(value.trim().trim_start_matches("0x"), 16).ok()
}

#[cfg(target_os = "linux")]
fn read_hex_u32(path: impl AsRef<Path>) -> Option<u32> {
    let value = fs::read_to_string(path).ok()?;
    u32::from_str_radix(value.trim().trim_start_matches("0x"), 16).ok()
}

#[cfg(target_os = "linux")]
fn read_trimmed(path: impl AsRef<Path>) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reference_transport_is_always_submission_ready() {
        let probe = reference_transport();
        assert_eq!(probe.kind, TransportKind::Reference);
        assert!(probe.submission_ready);
    }

    #[test]
    fn live_probe_keeps_security_fail_closed() {
        let report = probe_platform();
        assert!(report.security.is_fail_closed());
        assert!(!report.firmware.mutation_attempted);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn parses_windows_pci_identifier() {
        let value = "PCI\\VEN_8086&DEV_56A5&SUBSYS_3943172F&REV_05";
        assert_eq!(
            parse_windows_pnp_id(value),
            Some(PciId::new(0x8086, 0x56a5))
        );
    }
}
