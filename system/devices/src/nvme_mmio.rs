//! Registerfenster eines NVMe-Controllers als MMIO-Backend.
//!
//! Diese Schicht schliesst die Luecke zwischen dem Vertrag `NvmeMmio` und dem
//! Bussystem: Sie bildet die BAR-Register eines Controllers als 32-Bit-Fenster
//! ab und beantwortet Zugriffe fail closed statt zu panicken. Der Vertrag in
//! `nvme_controller` bleibt unveraendert; ein spaeteres physisches Mapping
//! implementiert dieselben vier Methoden ueber echte MMIO-Adressen.
//!
//! Bewusste Entscheidungen:
//! - Zugriffe ausserhalb des Fensters liefern [`UNMAPPED_READ`] und werden beim
//!   Schreiben verworfen. Ein ungemapptes Register ist ein Fehlerzustand, kein
//!   Panic und kein stiller Nullwert.
//! - Nicht auf Vier-Byte-Grenzen ausgerichtete Zugriffe gelten als ungueltig.
//!   Ein Controller-Register ist immer wortbreit adressiert.

use alloc::vec;
use alloc::vec::Vec;

use crate::nvme_controller::NvmeMmio;

/// Groesstes zulaessiges Registerfenster (64 KiB).
pub const MAX_BAR_BYTES: usize = 64 * 1024;

/// Wert eines Lesezugriffs auf ein nicht gemapptes Register.
pub const UNMAPPED_READ: u32 = 0xFFFF_FFFF;

/// Fehler beim Aufbau eines Registerfensters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarError {
    /// Die angeforderte Groesse ist null.
    Empty,
    /// Die Groesse ist kein Vielfaches der Registerbreite von vier Bytes.
    UnalignedSize,
    /// Die Groesse ueberschreitet [`MAX_BAR_BYTES`].
    TooLarge,
}

/// 32-Bit-Registerfenster eines Controllers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BarWindow {
    regs: Vec<u32>,
}

impl BarWindow {
    /// Legt ein Fenster der angegebenen Groesse in Bytes an, alle Register null.
    ///
    /// # Errors
    /// [`BarError::Empty`], [`BarError::UnalignedSize`] oder [`BarError::TooLarge`].
    pub fn new(size_bytes: usize) -> Result<Self, BarError> {
        if size_bytes == 0 {
            return Err(BarError::Empty);
        }
        if !size_bytes.is_multiple_of(4) {
            return Err(BarError::UnalignedSize);
        }
        if size_bytes > MAX_BAR_BYTES {
            return Err(BarError::TooLarge);
        }
        Ok(Self {
            regs: vec![0u32; size_bytes / 4],
        })
    }

    /// Groesse des Fensters in Bytes.
    pub fn size_bytes(&self) -> usize {
        self.regs.len() * 4
    }

    /// Setzt ein Register direkt, ohne den MMIO-Vertrag zu benutzen.
    ///
    /// Gibt `false` zurueck, wenn der Zugriff abgelehnt wurde. Fuer das
    /// Geraetemodell in Tests und fuer das Vorspannen von Registern gedacht.
    pub fn poke32(&mut self, offset: u64, value: u32) -> bool {
        match Self::index(offset, self.regs.len()) {
            Some(i) => {
                self.regs[i] = value;
                true
            }
            None => false,
        }
    }

    /// Liest ein Register direkt, ohne den MMIO-Vertrag zu benutzen.
    ///
    /// Liefert [`UNMAPPED_READ`], wenn der Zugriff abgelehnt wurde.
    pub fn peek32(&self, offset: u64) -> u32 {
        match Self::index(offset, self.regs.len()) {
            Some(i) => self.regs[i],
            None => UNMAPPED_READ,
        }
    }

    fn index(offset: u64, len: usize) -> Option<usize> {
        if !offset.is_multiple_of(4) {
            return None;
        }
        let index = usize::try_from(offset / 4).ok()?;
        if index >= len {
            return None;
        }
        Some(index)
    }
}

impl NvmeMmio for BarWindow {
    fn read32(&self, offset: u64) -> u32 {
        self.peek32(offset)
    }

    fn read64(&self, offset: u64) -> u64 {
        let low = u64::from(self.peek32(offset));
        let high = u64::from(self.peek32(offset.wrapping_add(4)));
        (high << 32) | low
    }

    fn write32(&mut self, offset: u64, value: u32) {
        let _ = self.poke32(offset, value);
    }

    fn write64(&mut self, offset: u64, value: u64) {
        let _ = self.poke32(offset, value as u32);
        let _ = self.poke32(offset.wrapping_add(4), (value >> 32) as u32);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nvme_controller::{
        AdminQueueAddresses, CC, CC_EN, CSTS, CSTS_CFS, CSTS_RDY, ControllerState, NvmeController,
        NvmeControllerConfig,
    };

    fn config() -> NvmeControllerConfig {
        NvmeControllerConfig {
            admin_queue_depth: 32,
            io_queue_depth: 256,
            page_size: 4096,
        }
    }

    fn controller_with_queues(
        window: &mut BarWindow,
    ) -> Result<NvmeController, crate::nvme_controller::NvmeControllerError> {
        let mut controller = NvmeController::new(config())?;
        controller.configure_admin_queues(
            window,
            AdminQueueAddresses {
                submission: 0x10000,
                completion: 0x20000,
            },
        )?;
        Ok(controller)
    }

    #[test]
    fn window_rejects_invalid_sizes() {
        assert_eq!(BarWindow::new(0), Err(BarError::Empty));
        assert_eq!(BarWindow::new(6), Err(BarError::UnalignedSize));
        assert_eq!(BarWindow::new(MAX_BAR_BYTES + 4), Err(BarError::TooLarge));
        assert_eq!(BarWindow::new(4096).unwrap().size_bytes(), 4096);
    }

    #[test]
    fn register_roundtrip_stays_inside_window() {
        let mut w = BarWindow::new(64).unwrap();
        assert!(w.poke32(0x1c, 0xDEAD_BEEF));
        assert_eq!(w.peek32(0x1c), 0xDEAD_BEEF);
        w.write32(0x1c, 1);
        assert_eq!(w.read32(0x1c), 1);
    }

    #[test]
    fn access_outside_window_fails_closed() {
        let mut w = BarWindow::new(64).unwrap();
        assert_eq!(w.peek32(0x40), UNMAPPED_READ);
        assert_eq!(w.read32(0x1000), UNMAPPED_READ);
        assert!(!w.poke32(0x40, 1));
        w.write32(0x40, 1);
        assert_eq!(w.peek32(0x40), UNMAPPED_READ);
    }

    #[test]
    fn unaligned_access_is_rejected() {
        let mut w = BarWindow::new(64).unwrap();
        assert_eq!(w.peek32(0x1e), UNMAPPED_READ);
        assert!(!w.poke32(0x1e, 1));
        w.write32(0x1e, 1);
        assert_eq!(w.peek32(0x1c), 0);
    }

    #[test]
    fn sixty_four_bit_access_splits_into_two_registers() {
        let mut w = BarWindow::new(64).unwrap();
        w.write64(0x20, 0x1122_3344_5566_7788);
        assert_eq!(w.read32(0x20), 0x5566_7788);
        assert_eq!(w.read32(0x24), 0x1122_3344);
        assert_eq!(w.read64(0x20), 0x1122_3344_5566_7788);
    }

    #[test]
    fn controller_enables_against_window() {
        let mut w = BarWindow::new(4096).unwrap();
        let mut controller = controller_with_queues(&mut w).unwrap();
        controller.enable(&mut w).unwrap();
        assert_eq!(controller.state(), ControllerState::Enabling);
        assert_eq!(w.read32(CC) & CC_EN, CC_EN);
        w.poke32(CSTS, CSTS_RDY);
        controller.poll_ready(&w).unwrap();
        assert_eq!(controller.state(), ControllerState::Ready);
    }

    #[test]
    fn fatal_status_stops_enable() {
        let mut w = BarWindow::new(4096).unwrap();
        w.poke32(CSTS, CSTS_CFS);
        let mut controller = controller_with_queues(&mut w).unwrap();
        assert!(controller.enable(&mut w).is_err());
        assert_eq!(controller.state(), ControllerState::Fatal);
    }
}
