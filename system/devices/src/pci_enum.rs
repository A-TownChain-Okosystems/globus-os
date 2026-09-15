//! PCIe ECAM enumeration primitives.

use crate::{pci::PciAddress, pci_ecam::PciEcam};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PciBdf { pub segment: u16, pub bus: u8, pub device: u8, pub function: u8 }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BarKind { Memory32, Memory64, Io }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PciBar { pub index: u8, pub kind: BarKind, pub base: u64, pub size: u64, pub prefetchable: bool }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PciFunction {
    pub bdf: PciBdf,
    pub vendor_id: u16,
    pub device_id: u16,
    pub class_code: u8,
    pub subclass: u8,
    pub prog_if: u8,
    pub header_type: u8,
    pub bars: [Option<PciBar>; 6],
}

impl PciFunction { pub fn is_multifunction(&self) -> bool { self.header_type & 0x80 != 0 } }

#[derive(Debug, Eq, PartialEq)]
pub enum PciEnumError { AddressOverflow }

pub struct PciEnumerator<'a> { ecam: &'a PciEcam, segment: u16, bus_start: u8, bus_end: u8 }

impl<'a> PciEnumerator<'a> {
    pub fn new(ecam: &'a PciEcam, segment: u16, bus_start: u8, bus_end: u8) -> Self {
        Self { ecam, segment, bus_start, bus_end }
    }

    pub fn enumerate<F>(&self, mut visit: F) -> Result<(), PciEnumError>
    where F: FnMut(PciFunction) {
        for bus in self.bus_start..=self.bus_end {
            for device in 0..32u8 {
                let addr0 = self.addr(bus, device, 0);
                if self.ecam.read16(addr0, 0x00) == 0xffff { continue; }
                let header = self.ecam.read8(addr0, 0x0e);
                let functions = if header & 0x80 != 0 { 8 } else { 1 };
                for function in 0..functions {
                    let address = self.addr(bus, device, function);
                    if self.ecam.read16(address, 0x00) == 0xffff { continue; }
                    visit(self.read_function(address)?);
                }
            }
        }
        Ok(())
    }

    fn addr(&self, bus: u8, device: u8, function: u8) -> PciAddress {
        PciAddress { segment: self.segment, bus, device, function }
    }

    fn read_function(&self, address: PciAddress) -> Result<PciFunction, PciEnumError> {
        let vendor_id = self.ecam.read16(address, 0x00);
        let device_id = self.ecam.read16(address, 0x02);
        let prog_if = self.ecam.read8(address, 0x09);
        let subclass = self.ecam.read8(address, 0x0a);
        let class_code = self.ecam.read8(address, 0x0b);
        let header_type = self.ecam.read8(address, 0x0e);
        let mut bars = [None; 6];
        let mut i = 0usize;
        while i < 6 {
            bars[i] = self.decode_bar(address, i as u8)?;
            if matches!(bars[i].map(|b| b.kind), Some(BarKind::Memory64)) { i += 1; }
            i += 1;
        }
        Ok(PciFunction {
            bdf: PciBdf { segment: address.segment, bus: address.bus, device: address.device, function: address.function },
            vendor_id, device_id, class_code, subclass, prog_if, header_type, bars,
        })
    }

    fn decode_bar(&self, address: PciAddress, index: u8) -> Result<Option<PciBar>, PciEnumError> {
        let offset = 0x10 + u16::from(index) * 4;
        let original = self.ecam.read32(address, offset);
        if original == 0 { return Ok(None); }
        let kind = if original & 1 != 0 { BarKind::Io }
            else if (original >> 1) & 0x3 == 0x2 { BarKind::Memory64 }
            else { BarKind::Memory32 };
        let prefetchable = original & 0x8 != 0;
        let base_low = u64::from(original & 0xfffffff0);
        let base = if kind == BarKind::Memory64 {
            let high = u64::from(self.ecam.read32(address, offset + 4));
            base_low | (high << 32)
        } else if kind == BarKind::Io { u64::from(original & 0xffff_fffc) } else { base_low };
        if base == 0 { return Ok(None); }
        let size = if kind == BarKind::Io { 4 } else { 4096 };
        base.checked_add(size - 1).ok_or(PciEnumError::AddressOverflow)?;
        Ok(Some(PciBar { index, kind, base, size, prefetchable }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bdf_is_structured() { assert_eq!(PciBdf { segment: 0, bus: 2, device: 3, function: 1 }.function, 1); }
}
