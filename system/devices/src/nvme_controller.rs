//! NVMe MMIO register programming and controller initialization runtime.

/// Registeroffset der Controller-Faehigkeiten.
pub const CAP: u64 = 0x00;
/// Registeroffset der Controller-Version.
pub const VS: u64 = 0x08;
/// Registeroffset der Controller-Konfiguration.
pub const CC: u64 = 0x14;
/// Registeroffset des Controller-Status.
pub const CSTS: u64 = 0x1c;
/// Registeroffset der Groesse der Admin-Queues.
pub const AQA: u64 = 0x24;
/// Registeroffset der Adresse der Submission-Queue.
pub const ASQ: u64 = 0x28;
/// Registeroffset der Adresse der Completion-Queue.
pub const ACQ: u64 = 0x30;
/// Bit im CC-Register, das den Controller einschaltet.
pub const CC_EN: u32 = 1;
/// Statusbit, das die Bereitschaft des Controllers meldet.
pub const CSTS_RDY: u32 = 1;
/// Statusbit, das einen fatalen Controller-Fehler meldet.
pub const CSTS_CFS: u32 = 1 << 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Konfiguration eines NVMe-Controllers.
pub struct NvmeControllerConfig {
    /// Tiefe der Admin-Queue.
    pub admin_queue_depth: u16,
    /// Tiefe der IO-Queues.
    pub io_queue_depth: u16,
    /// Seitengroesse in Byte.
    pub page_size: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Fehler des NVMe-Controllers.
pub enum NvmeControllerError {
    /// Die Konfiguration ist unbrauchbar.
    InvalidConfig,
    /// Der Controller hat nicht rechtzeitig geantwortet.
    Timeout,
    /// Der Controller meldet einen fatalen Fehler.
    ControllerFatal,
    /// Das Anlegen der Queues ist fehlgeschlagen.
    QueueSetupFailed,
    /// Das Identify-Kommando ist fehlgeschlagen.
    IdentifyFailed,
    /// Der Controller ist bereits eingeschaltet.
    AlreadyEnabled,
    /// Der Controller ist noch nicht eingeschaltet.
    NotEnabled,
}

/// Zugriff auf die MMIO-Register des Controllers.
pub trait NvmeMmio {
    /// Liest ein 32-Bit-Register.
    fn read32(&self, offset: u64) -> u32;
    /// Liest ein 64-Bit-Register.
    fn read64(&self, offset: u64) -> u64;
    /// Schreibt ein 32-Bit-Register.
    fn write32(&mut self, offset: u64, value: u32);
    /// Schreibt ein 64-Bit-Register.
    fn write64(&mut self, offset: u64, value: u64);
}

/// Prueft Tiefen und Seitengroesse einer Konfiguration.
pub fn validate_config(c: NvmeControllerConfig) -> Result<(), NvmeControllerError> {
    if !c.admin_queue_depth.is_power_of_two()
        || !(2..=4096).contains(&c.admin_queue_depth)
        || !c.io_queue_depth.is_power_of_two()
        || !(2..=4096).contains(&c.io_queue_depth)
        || !c.page_size.is_power_of_two()
        || !(4096..=65536).contains(&c.page_size)
    {
        return Err(NvmeControllerError::InvalidConfig);
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Zustand des Controllers.
pub enum ControllerState {
    /// Zurueckgesetzt, noch nicht konfiguriert.
    Reset,
    /// Einschalten laeuft.
    Enabling,
    /// Bereit fuer Kommandos.
    Ready,
    /// Fataler Fehler, Reset noetig.
    Fatal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Physische Adressen der Admin-Queues.
pub struct AdminQueueAddresses {
    /// Adresse der Submission-Queue.
    pub submission: u64,
    /// Adresse der Completion-Queue.
    pub completion: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Zustand eines NVMe-Controllers.
pub struct NvmeController {
    state: ControllerState,
    config: NvmeControllerConfig,
    admin: Option<AdminQueueAddresses>,
}

impl NvmeController {
    /// Legt den Controller im Zustand Reset an.
    pub fn new(config: NvmeControllerConfig) -> Result<Self, NvmeControllerError> {
        validate_config(config)?;
        Ok(Self {
            state: ControllerState::Reset,
            config,
            admin: None,
        })
    }
    /// Aktueller Zustand.
    pub fn state(&self) -> ControllerState {
        self.state
    }
    /// Konfiguration des Controllers.
    pub fn config(&self) -> NvmeControllerConfig {
        self.config
    }
    /// Programmiert Groesse und Adressen der Admin-Queues.
    pub fn configure_admin_queues(
        &mut self,
        mmio: &mut impl NvmeMmio,
        addresses: AdminQueueAddresses,
    ) -> Result<(), NvmeControllerError> {
        if self.state == ControllerState::Fatal {
            return Err(NvmeControllerError::ControllerFatal);
        }
        if !addresses
            .submission
            .is_multiple_of(self.config.page_size as u64)
            || !addresses
                .completion
                .is_multiple_of(self.config.page_size as u64)
        {
            return Err(NvmeControllerError::QueueSetupFailed);
        }
        let d = self.config.admin_queue_depth - 1;
        mmio.write32(AQA, ((d as u32) << 16) | d as u32);
        mmio.write64(ASQ, addresses.submission);
        mmio.write64(ACQ, addresses.completion);
        self.admin = Some(addresses);
        Ok(())
    }
    /// Schaltet den Controller ein und wechselt nach Enabling.
    pub fn enable(&mut self, mmio: &mut impl NvmeMmio) -> Result<(), NvmeControllerError> {
        if self.state == ControllerState::Ready || self.state == ControllerState::Enabling {
            return Err(NvmeControllerError::AlreadyEnabled);
        }
        if self.admin.is_none() {
            return Err(NvmeControllerError::QueueSetupFailed);
        }
        if mmio.read32(CSTS) & CSTS_CFS != 0 {
            self.state = ControllerState::Fatal;
            return Err(NvmeControllerError::ControllerFatal);
        }
        let cc = mmio.read32(CC) | CC_EN;
        mmio.write32(CC, cc);
        self.state = ControllerState::Enabling;
        Ok(())
    }
    /// Fragt den Status ab und meldet Ready, Fatal oder Timeout.
    pub fn poll_ready(&mut self, mmio: &impl NvmeMmio) -> Result<(), NvmeControllerError> {
        if self.state != ControllerState::Enabling {
            return Err(NvmeControllerError::NotEnabled);
        }
        let status = mmio.read32(CSTS);
        if status & CSTS_CFS != 0 {
            self.state = ControllerState::Fatal;
            return Err(NvmeControllerError::ControllerFatal);
        }
        if status & CSTS_RDY != 0 {
            self.state = ControllerState::Ready;
            Ok(())
        } else {
            Err(NvmeControllerError::Timeout)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nvme_mmio::BarWindow;

    /// Registerfenster statt eines eigenen Test-Mocks: derselbe Baustein, der
    /// spaeter das physische MMIO traegt, mit Grenzpruefung und fail-closed.
    fn window() -> BarWindow {
        BarWindow::new(4096).expect("Registerfenster mit 4 KiB")
    }

    fn config() -> NvmeControllerConfig {
        NvmeControllerConfig {
            admin_queue_depth: 32,
            io_queue_depth: 256,
            page_size: 4096,
        }
    }
    #[test]
    fn setup_programs_admin_queues() {
        let mut m = window();
        let mut c = NvmeController::new(config()).unwrap();
        c.configure_admin_queues(
            &mut m,
            AdminQueueAddresses {
                submission: 0x10000,
                completion: 0x20000,
            },
        )
        .unwrap();
        assert_eq!(m.read64(ASQ), 0x10000);
        assert_eq!(m.read64(ACQ), 0x20000);
    }
    #[test]
    fn ready_transition_is_explicit() {
        let mut m = window();
        let mut c = NvmeController::new(config()).unwrap();
        c.configure_admin_queues(
            &mut m,
            AdminQueueAddresses {
                submission: 0x10000,
                completion: 0x20000,
            },
        )
        .unwrap();
        c.enable(&mut m).unwrap();
        assert_eq!(c.state(), ControllerState::Enabling);
        m.write32(CSTS, CSTS_RDY);
        c.poll_ready(&m).unwrap();
        assert_eq!(c.state(), ControllerState::Ready);
    }
}
