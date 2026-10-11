//! Kernel-side adapters for the stable service-facing ABI.
//!
//! Adapters bind ABI calls to trusted kernel context. They do not expose the
//! concrete kernel data structures through the ABI crate.
use crate::ats1000::Pid;
use crate::capability::{CapId, CapabilityTable};
use libshivacore::contracts::{
    CapabilityAuthority, CapabilityCheckRequest, CapabilityCheckResult,
};
use libshivacore::{AbiError, CapabilityRights};

/// Capability authority bound to a caller identity supplied by trusted syscall
/// dispatch, never from untrusted request payload data.
pub struct KernelCapabilityAuthority<'a> {
    table: &'a CapabilityTable,
    caller: Pid,
}

impl<'a> KernelCapabilityAuthority<'a> {
    pub const fn new(table: &'a CapabilityTable, caller: Pid) -> Self {
        Self { table, caller }
    }
}

impl CapabilityAuthority for KernelCapabilityAuthority<'_> {
    fn check(
        &self,
        request: CapabilityCheckRequest,
    ) -> Result<CapabilityCheckResult, AbiError> {
        const VALID_RIGHTS: u8 = 0x1f;
        let requested = request.required.bits();
        if requested & !VALID_RIGHTS != 0 {
            return Err(AbiError::InvalidPayload);
        }

        let cap = self
            .table
            .get(CapId(request.handle.0))
            .ok_or(AbiError::InvalidHandle)?;

        if cap.owner != self.caller {
            return Err(AbiError::PermissionDenied);
        }

        let effective = cap.rights.bits() & VALID_RIGHTS;
        Ok(CapabilityCheckResult {
            granted: effective & requested == requested,
            effective_rights: CapabilityRights(effective),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::{ResourceType, Rights};

    #[test]
    fn adapter_grants_only_owned_capabilities_with_requested_rights() {
        let mut table = CapabilityTable::new();
        let caller = Pid(7);
        let handle = table.create(
            caller,
            ResourceType::Network,
            99,
            Rights::READ | Rights::INSPECT,
        );
        let authority = KernelCapabilityAuthority::new(&table, caller);
        let result = authority
            .check(CapabilityCheckRequest {
                handle: libshivacore::CapabilityHandle(handle.0),
                required: CapabilityRights::READ,
            })
            .unwrap();
        assert!(result.granted);
        assert_eq!(result.effective_rights.bits(), Rights::READ.bits() | Rights::INSPECT.bits());
    }

    #[test]
    fn adapter_denies_foreign_and_unknown_handles() {
        let mut table = CapabilityTable::new();
        let handle = table.create(Pid(1), ResourceType::Memory, 5, Rights::READ);
        let foreign = KernelCapabilityAuthority::new(&table, Pid(2));
        assert_eq!(
            foreign.check(CapabilityCheckRequest {
                handle: libshivacore::CapabilityHandle(handle.0),
                required: CapabilityRights::READ,
            }),
            Err(AbiError::PermissionDenied)
        );
        assert_eq!(
            foreign.check(CapabilityCheckRequest {
                handle: libshivacore::CapabilityHandle(999),
                required: CapabilityRights::READ,
            }),
            Err(AbiError::InvalidHandle)
        );
    }

    #[test]
    fn adapter_rejects_unknown_right_bits() {
        let table = CapabilityTable::new();
        let authority = KernelCapabilityAuthority::new(&table, Pid(1));
        assert_eq!(
            authority.check(CapabilityCheckRequest {
                handle: libshivacore::CapabilityHandle(1),
                required: CapabilityRights(0x80),
            }),
            Err(AbiError::InvalidPayload)
        );
    }
}
