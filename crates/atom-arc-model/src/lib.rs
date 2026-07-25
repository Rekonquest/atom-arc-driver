//! Hardware-neutral types shared by the Atom Arc runtime and platform backends.

use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;

pub use atom_arc_core::{ARC_A380_DEVICE_ID, INTEL_VENDOR_ID};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PciId {
    pub vendor: u16,
    pub device: u16,
}

impl PciId {
    #[must_use]
    pub const fn new(vendor: u16, device: u16) -> Self {
        Self { vendor, device }
    }

    #[must_use]
    pub const fn is_intel(self) -> bool {
        self.vendor == INTEL_VENDOR_ID
    }
}

impl fmt::Display for PciId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:04x}:{:04x}", self.vendor, self.device)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GpuArchitecture {
    AlchemistDg2,
    UnknownIntel,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceProfile {
    pub name: String,
    pub pci_id: PciId,
    pub architecture: GpuArchitecture,
    pub authenticated_firmware_required: bool,
}

impl DeviceProfile {
    #[must_use]
    pub fn from_pci_id(pci_id: PciId) -> Self {
        match (pci_id.vendor, pci_id.device) {
            (INTEL_VENDOR_ID, ARC_A380_DEVICE_ID) => Self {
                name: "Intel Arc A380".to_owned(),
                pci_id,
                architecture: GpuArchitecture::AlchemistDg2,
                authenticated_firmware_required: true,
            },
            (INTEL_VENDOR_ID, _) => Self {
                name: "Unknown Intel GPU".to_owned(),
                pci_id,
                architecture: GpuArchitecture::UnknownIntel,
                authenticated_firmware_required: true,
            },
            _ => Self {
                name: "Unknown GPU".to_owned(),
                pci_id,
                architecture: GpuArchitecture::Unknown,
                authenticated_firmware_required: true,
            },
        }
    }
}

/// A never-reused process-local buffer identifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct BufferId(pub u64);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BufferInfo {
    pub id: BufferId,
    pub size: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Command {
    Noop,
    Fill {
        destination: BufferId,
        offset: u64,
        bytes: u64,
        value: u8,
    },
    Copy {
        source: BufferId,
        source_offset: u64,
        destination: BufferId,
        destination_offset: u64,
        bytes: u64,
    },
    Barrier,
    Dispatch {
        kernel_id: u64,
        groups: [u32; 3],
        bindings: Vec<BufferId>,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandBuffer {
    commands: Vec<Command>,
}

impl CommandBuffer {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            commands: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_command(mut self, command: Command) -> Self {
        self.commands.push(command);
        self
    }

    pub fn push(&mut self, command: Command) {
        self.commands.push(command);
    }

    #[must_use]
    pub fn commands(&self) -> &[Command] {
        &self.commands
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.commands.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    pub fn validate_shape(&self, limits: CommandLimits) -> Result<(), CommandShapeError> {
        if self.commands.len() > limits.max_commands {
            return Err(CommandShapeError::TooManyCommands {
                actual: self.commands.len(),
                maximum: limits.max_commands,
            });
        }

        for (index, command) in self.commands.iter().enumerate() {
            match command {
                Command::Fill { bytes, .. } | Command::Copy { bytes, .. } => {
                    if *bytes == 0 {
                        return Err(CommandShapeError::ZeroLengthTransfer { index });
                    }
                    if *bytes > limits.max_transfer_bytes {
                        return Err(CommandShapeError::TransferTooLarge {
                            index,
                            bytes: *bytes,
                            maximum: limits.max_transfer_bytes,
                        });
                    }
                }
                Command::Dispatch {
                    kernel_id,
                    groups,
                    bindings,
                } => {
                    if *kernel_id == 0 {
                        return Err(CommandShapeError::InvalidKernelId { index });
                    }
                    if groups.contains(&0) {
                        return Err(CommandShapeError::ZeroDispatchDimension { index });
                    }
                    if bindings.len() > limits.max_bindings {
                        return Err(CommandShapeError::TooManyBindings {
                            index,
                            actual: bindings.len(),
                            maximum: limits.max_bindings,
                        });
                    }
                }
                Command::Noop | Command::Barrier => {}
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandLimits {
    pub max_commands: usize,
    pub max_transfer_bytes: u64,
    pub max_bindings: usize,
}

impl Default for CommandLimits {
    fn default() -> Self {
        Self {
            max_commands: 4_096,
            max_transfer_bytes: 256 * 1024 * 1024,
            max_bindings: 64,
        }
    }
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum CommandShapeError {
    #[error("command buffer has {actual} commands; maximum is {maximum}")]
    TooManyCommands { actual: usize, maximum: usize },
    #[error("command {index} has a zero-length transfer")]
    ZeroLengthTransfer { index: usize },
    #[error("command {index} transfers {bytes} bytes; maximum is {maximum}")]
    TransferTooLarge {
        index: usize,
        bytes: u64,
        maximum: u64,
    },
    #[error("command {index} uses reserved kernel identifier zero")]
    InvalidKernelId { index: usize },
    #[error("command {index} has a zero dispatch dimension")]
    ZeroDispatchDimension { index: usize },
    #[error("command {index} has {actual} bindings; maximum is {maximum}")]
    TooManyBindings {
        index: usize,
        actual: usize,
        maximum: usize,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_installed_arc_a380() {
        let profile = DeviceProfile::from_pci_id(PciId::new(0x8086, 0x56a5));
        assert_eq!(profile.architecture, GpuArchitecture::AlchemistDg2);
        assert_eq!(profile.name, "Intel Arc A380");
        assert!(profile.authenticated_firmware_required);
    }

    #[test]
    fn rejects_zero_length_transfer() {
        let commands = CommandBuffer::new().with_command(Command::Fill {
            destination: BufferId(1),
            offset: 0,
            bytes: 0,
            value: 0,
        });

        assert_eq!(
            commands.validate_shape(CommandLimits::default()),
            Err(CommandShapeError::ZeroLengthTransfer { index: 0 })
        );
    }

    #[test]
    fn accepts_bounded_reference_commands() {
        let commands = CommandBuffer::new()
            .with_command(Command::Fill {
                destination: BufferId(1),
                offset: 0,
                bytes: 32,
                value: 0xa5,
            })
            .with_command(Command::Barrier)
            .with_command(Command::Copy {
                source: BufferId(1),
                source_offset: 0,
                destination: BufferId(2),
                destination_offset: 16,
                bytes: 32,
            });

        assert_eq!(commands.validate_shape(CommandLimits::default()), Ok(()));
    }
}
