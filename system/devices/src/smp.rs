//! Symmetric multiprocessing topology and CPU lifecycle contract.

use alloc::vec::Vec;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Kennung einer CPU.
pub struct CpuId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Zustand einer CPU.
pub enum CpuState {
    /// Nicht gestartet.
    Offline,
    /// Startet gerade.
    Starting,
    /// Laeuft und nimmt Arbeit an; nur nach bestaetigter AP-Initialisierung.
    Online,
    /// Startup ist fehlgeschlagen; die CPU darf keine Arbeit erhalten.
    Failed,
    /// Angehalten.
    Halted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Zustand einer CPU in der Topologie.
pub struct CpuInfo {
    /// Kennung der CPU.
    pub id: CpuId,
    /// Aktueller Zustand.
    pub state: CpuState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Fehler der CPU-Topologie.
pub enum SmpError {
    /// Die Kennung ist ungueltig.
    InvalidCpu,
    /// Die CPU ist bereits eingetragen.
    DuplicateCpu,
    /// Der Zustandswechsel ist nicht erlaubt.
    InvalidTransition,
}

#[derive(Debug, Default)]
/// Eingetragene CPUs und ihre Zustaende.
pub struct CpuTopology {
    cpus: Vec<CpuInfo>,
}

impl CpuTopology {
    /// Leere Topologie.
    pub fn new() -> Self {
        Self::default()
    }
    /// Traegt eine CPU als offline ein und lehnt Doppelungen ab.
    pub fn register(&mut self, id: CpuId) -> Result<(), SmpError> {
        if id.0 == u32::MAX {
            return Err(SmpError::InvalidCpu);
        }
        if self.cpus.iter().any(|c| c.id == id) {
            return Err(SmpError::DuplicateCpu);
        }
        self.cpus.push(CpuInfo {
            id,
            state: CpuState::Offline,
        });
        self.cpus.sort_by_key(|c| c.id.0);
        Ok(())
    }
    /// Beginnt den Startup-Zustand. Dies sendet selbst keine Hardware-IPI.
    /// `Online` darf erst nach AP-eigener Initialisierung und Ready-Ack folgen.
    pub fn begin_startup(&mut self, id: CpuId) -> Result<(), SmpError> {
        let cpu = self
            .cpus
            .iter_mut()
            .find(|c| c.id == id)
            .ok_or(SmpError::InvalidCpu)?;
        if cpu.state != CpuState::Offline {
            return Err(SmpError::InvalidTransition);
        }
        cpu.state = CpuState::Starting;
        Ok(())
    }

    /// Markiert eine CPU erst nach bestaetigter AP-Initialisierung als online.
    /// Der Aufrufer muss zuvor den Hardware-Startup und das AP-Ready-Ack pruefen.
    pub fn mark_online(&mut self, id: CpuId) -> Result<(), SmpError> {
        let cpu = self
            .cpus
            .iter_mut()
            .find(|c| c.id == id)
            .ok_or(SmpError::InvalidCpu)?;
        if cpu.state != CpuState::Starting {
            return Err(SmpError::InvalidTransition);
        }
        cpu.state = CpuState::Online;
        Ok(())
    }

    /// Zeichnet einen fehlgeschlagenen Startup auf; fehlgeschlagene CPUs bleiben nicht online.
    pub fn mark_failed(&mut self, id: CpuId) -> Result<(), SmpError> {
        let cpu = self
            .cpus
            .iter_mut()
            .find(|c| c.id == id)
            .ok_or(SmpError::InvalidCpu)?;
        if cpu.state != CpuState::Starting {
            return Err(SmpError::InvalidTransition);
        }
        cpu.state = CpuState::Failed;
        Ok(())
    }
    /// Haelt eine laufende CPU an.
    pub fn halt(&mut self, id: CpuId) -> Result<(), SmpError> {
        let cpu = self
            .cpus
            .iter_mut()
            .find(|c| c.id == id)
            .ok_or(SmpError::InvalidCpu)?;
        if cpu.state != CpuState::Online {
            return Err(SmpError::InvalidTransition);
        }
        cpu.state = CpuState::Halted;
        Ok(())
    }
    /// Zustand einer CPU.
    pub fn get(&self, id: CpuId) -> Option<CpuInfo> {
        self.cpus.iter().find(|c| c.id == id).copied()
    }
    /// Alle CPUs in Einfuegereihenfolge.
    pub fn cpus(&self) -> &[CpuInfo] {
        &self.cpus
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cpu_is_not_online_until_startup_is_confirmed() {
        let mut t = CpuTopology::new();
        t.register(CpuId(1)).unwrap();
        assert_eq!(t.get(CpuId(1)).unwrap().state, CpuState::Offline);
        t.begin_startup(CpuId(1)).unwrap();
        assert_eq!(t.get(CpuId(1)).unwrap().state, CpuState::Starting);
        t.mark_online(CpuId(1)).unwrap();
        assert_eq!(t.get(CpuId(1)).unwrap().state, CpuState::Online);
        t.halt(CpuId(1)).unwrap();
        assert_eq!(t.get(CpuId(1)).unwrap().state, CpuState::Halted);
    }

    #[test]
    fn failed_startup_cannot_be_marked_online_or_halted() {
        let mut t = CpuTopology::new();
        t.register(CpuId(2)).unwrap();
        t.begin_startup(CpuId(2)).unwrap();
        t.mark_failed(CpuId(2)).unwrap();
        assert_eq!(t.get(CpuId(2)).unwrap().state, CpuState::Failed);
        assert_eq!(t.mark_online(CpuId(2)), Err(SmpError::InvalidTransition));
        assert_eq!(t.halt(CpuId(2)), Err(SmpError::InvalidTransition));
    }

    #[test]
    fn online_transition_is_rejected_before_startup_begins() {
        let mut t = CpuTopology::new();
        t.register(CpuId(4)).unwrap();
        assert_eq!(t.mark_online(CpuId(4)), Err(SmpError::InvalidTransition));
        assert_eq!(t.get(CpuId(4)).unwrap().state, CpuState::Offline);
    }

    #[test]
    fn startup_cannot_be_started_twice() {
        let mut t = CpuTopology::new();
        t.register(CpuId(3)).unwrap();
        t.begin_startup(CpuId(3)).unwrap();
        assert_eq!(t.begin_startup(CpuId(3)), Err(SmpError::InvalidTransition));
    }
}
