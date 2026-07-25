//! Security policy for keeping Atom Arc compatible with Intel's authenticated firmware chain.

use atom_arc_model::{CommandBuffer, CommandLimits, CommandShapeError};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FirmwareOperation {
    InspectMetadata,
    HashBlob,
    KernelDelegatedAuthenticatedLoad,
    WriteBlob,
    FlashDevice,
    BypassAuthentication,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SecurityPolicy {
    require_authenticated_firmware: bool,
    max_single_allocation_bytes: u64,
    max_total_allocation_bytes: u64,
    command_limits: CommandLimits,
}

impl Default for SecurityPolicy {
    fn default() -> Self {
        Self {
            require_authenticated_firmware: true,
            max_single_allocation_bytes: 512 * 1024 * 1024,
            max_total_allocation_bytes: 2 * 1024 * 1024 * 1024,
            command_limits: CommandLimits::default(),
        }
    }
}

impl SecurityPolicy {
    pub fn validate_allocation(
        self,
        requested_bytes: u64,
        currently_allocated_bytes: u64,
    ) -> Result<(), SecurityError> {
        if requested_bytes == 0 {
            return Err(SecurityError::ZeroLengthAllocation);
        }
        if requested_bytes > self.max_single_allocation_bytes {
            return Err(SecurityError::AllocationTooLarge {
                requested: requested_bytes,
                maximum: self.max_single_allocation_bytes,
            });
        }
        let total = currently_allocated_bytes
            .checked_add(requested_bytes)
            .ok_or(SecurityError::AllocationAccountingOverflow)?;
        if total > self.max_total_allocation_bytes {
            return Err(SecurityError::AllocationBudgetExceeded {
                requested_total: total,
                maximum: self.max_total_allocation_bytes,
            });
        }
        Ok(())
    }

    pub fn validate_commands(self, commands: &CommandBuffer) -> Result<(), SecurityError> {
        commands
            .validate_shape(self.command_limits)
            .map_err(SecurityError::InvalidCommandBuffer)
    }

    pub fn authorize_firmware(self, operation: FirmwareOperation) -> Result<(), SecurityError> {
        match operation {
            FirmwareOperation::InspectMetadata | FirmwareOperation::HashBlob => Ok(()),
            FirmwareOperation::KernelDelegatedAuthenticatedLoad => {
                if self.require_authenticated_firmware {
                    Ok(())
                } else {
                    Err(SecurityError::AuthenticationRequirementDisabled)
                }
            }
            FirmwareOperation::WriteBlob | FirmwareOperation::FlashDevice => {
                Err(SecurityError::FirmwareMutationDenied { operation })
            }
            FirmwareOperation::BypassAuthentication => {
                Err(SecurityError::FirmwareAuthenticationBypassDenied)
            }
        }
    }

    pub fn authorize_pci_config_write(self) -> Result<(), SecurityError> {
        Err(SecurityError::PciConfigurationWriteDenied)
    }

    #[must_use]
    pub fn posture(self) -> SecurityPosture {
        SecurityPosture {
            authenticated_firmware_required: self.require_authenticated_firmware,
            firmware_mutation_disabled: true,
            userspace_pci_configuration_writes_disabled: true,
            userspace_raw_mmio_disabled: true,
            kernel_submission_requires_hardware_authentication: true,
            unsafe_rust_disabled: true,
            unsupported_submission_fails_closed: true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecurityPosture {
    pub authenticated_firmware_required: bool,
    pub firmware_mutation_disabled: bool,
    pub userspace_pci_configuration_writes_disabled: bool,
    pub userspace_raw_mmio_disabled: bool,
    pub kernel_submission_requires_hardware_authentication: bool,
    pub unsafe_rust_disabled: bool,
    pub unsupported_submission_fails_closed: bool,
}

impl SecurityPosture {
    #[must_use]
    pub const fn is_fail_closed(self) -> bool {
        self.authenticated_firmware_required
            && self.firmware_mutation_disabled
            && self.userspace_pci_configuration_writes_disabled
            && self.userspace_raw_mmio_disabled
            && self.kernel_submission_requires_hardware_authentication
            && self.unsafe_rust_disabled
            && self.unsupported_submission_fails_closed
    }
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum SecurityError {
    #[error("zero-length allocations are not permitted")]
    ZeroLengthAllocation,
    #[error("allocation requests {requested} bytes; maximum is {maximum}")]
    AllocationTooLarge { requested: u64, maximum: u64 },
    #[error("allocation accounting overflowed")]
    AllocationAccountingOverflow,
    #[error("allocation would raise the total to {requested_total} bytes; maximum is {maximum}")]
    AllocationBudgetExceeded { requested_total: u64, maximum: u64 },
    #[error("command buffer rejected: {0}")]
    InvalidCommandBuffer(CommandShapeError),
    #[error("firmware mutation operation {operation:?} is denied")]
    FirmwareMutationDenied { operation: FirmwareOperation },
    #[error("firmware authentication bypass is always denied")]
    FirmwareAuthenticationBypassDenied,
    #[error("authenticated firmware must remain required")]
    AuthenticationRequirementDisabled,
    #[error("PCI configuration writes are denied")]
    PciConfigurationWriteDenied,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_policy_is_fail_closed() {
        assert!(SecurityPolicy::default().posture().is_fail_closed());
    }

    #[test]
    fn firmware_bypass_is_never_authorized() {
        let result =
            SecurityPolicy::default().authorize_firmware(FirmwareOperation::BypassAuthentication);
        assert_eq!(
            result,
            Err(SecurityError::FirmwareAuthenticationBypassDenied)
        );
    }

    #[test]
    fn total_allocation_budget_is_checked_without_overflow() {
        let policy = SecurityPolicy::default();
        let result = policy.validate_allocation(2, u64::MAX);
        assert_eq!(result, Err(SecurityError::AllocationAccountingOverflow));
    }

    #[test]
    fn metadata_inspection_is_read_only_and_allowed() {
        assert_eq!(
            SecurityPolicy::default().authorize_firmware(FirmwareOperation::InspectMetadata),
            Ok(())
        );
    }
}
