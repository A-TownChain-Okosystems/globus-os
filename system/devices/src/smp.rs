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
    /// Laeuft und nimmt Arbeit an.
    Online,
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
    /// Startet eine CPU; nur aus Offline oder Halted.
    pub fn start(&mut self, id: CpuId) -> Result<(), SmpError> {
        let cpu = self
            .cpus
            .iter_mut()
            .find(|c| c.id == id)
            .ok_or(SmpError::InvalidCpu)?;
        if cpu.state != CpuState::Offline {
            return Err(SmpError::InvalidTransition);
        }
        cpu.state = CpuState::Starting;
        cpu.state = CpuState::Online;
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
    fn cpu_lifecycle_is_explicit() {
        let mut t = CpuTopology::new();
        t.register(CpuId(1)).unwrap();
        t.start(CpuId(1)).unwrap();
        assert_eq!(t.get(CpuId(1)).unwrap().state, CpuState::Online);
        t.halt(CpuId(1)).unwrap();
        assert_eq!(t.get(CpuId(1)).unwrap().state, CpuState::Halted);
    }
}
