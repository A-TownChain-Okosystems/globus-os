//! ShivaCore kernel fault/event boundary.

use alloc::string::String;
use globus_diagnostics::{CrashEvent, EventLog};

/// Zustand eines Treibers aus Sicht der Fehlerbehandlung.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriverState {
    /// Der Treiber laeuft.
    Active,
    /// Der Treiber hat einen Absturz gemeldet.
    Faulted,
    /// Der Treiber ist abgeschaltet, der Kernel laeuft weiter.
    Isolated,
}

/// Ein gemeldeter Treiberabsturz mit seinem Bearbeitungsstand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriverFault {
    /// Name des Treibers.
    pub driver: String,
    /// Geraet, an dem der Absturz auftrat.
    pub device_id: u64,
    /// Bearbeitungsstand dieses Eintrags.
    pub state: DriverState,
    /// Laufende Nummer des Ereignisses im Ereignisprotokoll; sie bindet den Eintrag an seinen Protokolleintrag.
    pub event_sequence: u64,
}

/// Sammelt Treiberabstuerze im Ereignisprotokoll.
pub struct DriverFaultManager {
    events: EventLog,
}

impl Default for DriverFaultManager {
    fn default() -> Self {
        Self::new()
    }
}

impl DriverFaultManager {
    /// Leeres Protokoll.
    pub const fn new() -> Self {
        Self {
            events: EventLog::new(),
        }
    }

    /// Traegt den Absturz ins Protokoll ein und gibt den Eintrag im Zustand Faulted zurueck.
    pub fn report_crash(&mut self, driver: &str, device_id: u64, timestamp_ns: u64) -> DriverFault {
        let sequence = self
            .events
            .record_driver_crash(driver, device_id, timestamp_ns);
        DriverFault {
            driver: String::from(driver),
            device_id,
            state: DriverState::Faulted,
            event_sequence: sequence,
        }
    }

    /// Isolate a faulted driver without bringing down the kernel.
    pub fn isolate(&mut self, fault: &mut DriverFault) -> bool {
        if fault.state != DriverState::Faulted {
            return false;
        }
        if !self.events.mark_recovered(fault.event_sequence) {
            return false;
        }
        fault.state = DriverState::Isolated;
        true
    }

    /// Alle Ereignisse in der Reihenfolge, in der sie eingetragen wurden.
    pub fn events(&self) -> &[CrashEvent] {
        self.events.events()
    }
    /// Juengstes Ereignis; None, solange keines vorliegt.
    pub fn latest(&self) -> Option<&CrashEvent> {
        self.events.latest()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn driver_fault_is_reported_and_isolated() {
        let mut manager = DriverFaultManager::new();
        let mut fault = manager.report_crash("e1000", 0x0300, 123);
        assert_eq!(fault.state, DriverState::Faulted);
        assert_eq!(manager.latest().unwrap().code.as_str(), "SC-DRV-0007");
        assert!(manager.isolate(&mut fault));
        assert_eq!(fault.state, DriverState::Isolated);
        assert!(manager.latest().unwrap().recovered);
    }
}
