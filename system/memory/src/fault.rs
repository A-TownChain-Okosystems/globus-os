//! Page-fault classification and fail-closed handling boundary.

use crate::paging::PhysicalAddress;
use crate::{PageFlags, VirtualAddress};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Zugriffsart, die den Fehler ausgeloest hat.
pub enum FaultAccess {
    /// Lesender Zugriff.
    Read,
    /// Schreibender Zugriff.
    Write,
    /// Ausfuehrender Zugriff.
    Execute,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Ein aufgetretener Seitenfehler.
pub struct PageFault {
    /// Adresse, auf die zugegriffen wurde.
    pub address: VirtualAddress,
    /// Art des Zugriffs.
    pub access: FaultAccess,
    /// True, wenn die Seite abgebildet war, der Fehler also an den Rechten lag.
    pub present: bool,
    /// True, wenn der Zugriff aus dem Userspace kam.
    pub user: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Was der Kernel mit dem Fehler tun soll.
pub enum FaultAction {
    /// Adresse auf den angegebenen Frame abbilden.
    Map {
        physical: PhysicalAddress,
        flags: PageFlags,
    },
    /// Zugriff verweigern.
    Deny,
}

/// Baut den Fehlerdatensatz aus den Trap-Informationen.
pub fn classify(
    access: FaultAccess,
    present: bool,
    user: bool,
    address: VirtualAddress,
) -> PageFault {
    PageFault {
        address,
        access,
        present,
        user,
    }
}

/// Fail-closed: nur eine fehlende Abbildung darf nachgeladen werden.
pub fn authorize_fault(
    fault: PageFault,
    mapping: Option<(PhysicalAddress, PageFlags)>,
) -> FaultAction {
    if fault.present {
        return FaultAction::Deny;
    }
    let Some((physical, flags)) = mapping else {
        return FaultAction::Deny;
    };
    if fault.user && !flags.user {
        return FaultAction::Deny;
    }
    let allowed = match fault.access {
        FaultAccess::Read => flags.read,
        FaultAccess::Write => flags.write,
        FaultAccess::Execute => flags.execute,
    };
    if allowed {
        FaultAction::Map { physical, flags }
    } else {
        FaultAction::Deny
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn allows_only_declared_access() {
        let flags = PageFlags::user_read_only();
        let read = classify(FaultAccess::Read, false, true, VirtualAddress(0x4000));
        let write = classify(FaultAccess::Write, false, true, VirtualAddress(0x4000));
        assert!(matches!(
            authorize_fault(read, Some((PhysicalAddress(0x8000), flags))),
            FaultAction::Map { .. }
        ));
        assert_eq!(
            authorize_fault(write, Some((PhysicalAddress(0x8000), flags))),
            FaultAction::Deny
        );
    }

    #[test]
    fn present_faults_fail_closed() {
        let fault = classify(FaultAccess::Read, true, true, VirtualAddress(0x4000));
        assert_eq!(
            authorize_fault(
                fault,
                Some((PhysicalAddress(0x8000), PageFlags::user_read_only()))
            ),
            FaultAction::Deny
        );
    }
}
