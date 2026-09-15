//! PCIe ECAM enumeration primitives.
//!
//! The enumerator is intentionally policy-neutral: it discovers PCI functions,
//! decodes BARs, and reports capabilities. Driver binding and MMIO mapping stay
//! above this layer.

use crate::pci_ecam::PciEcam;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PciBdf {
    pub segment: u16,
    pub bus: u8,
    pub device: u8,
    pub function: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BarKind {
    Memory32,
    Memory64,
    Io,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PciBar {
    pub index: u8,
    pub kind: BarKind,
    pub base: u64,
    pub size: u64,
    pub prefetchable: bool,
}

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

impl PciFunction {
    pub fn is_multifunction(&self) -> bool {
        self.header_type & 0x80 != 0
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum PciEnumError {
    InvalidBar,
    UnsupportedBarType,
    AddressOverflow,
    SizeOverflow,
}

pub struct PciEnumerator<'a> {
    ecam: &'a PciEcam,
    segment: u16,
    bus_start: u8,
    bus_end: u8,
}

impl<'a> PciEnumerator<'a> {
    pub fn new(ecam: &'a PciEcam, segment: u16, bus_start: u8, bus_end: u8) -> Self {
        Self { ecam, segment, bus_start, bus_end }
    }

    pub fn enumerate<F>(&self, mut visit: F) -> Result<(), PciEnumError>
    where
        F: FnMut(PciFunction),
    {
        for bus in self.bus_start..=self.bus_end {
            for device in 0..32u8 {
                if self.vendor(bus, device, 0) == 0xffff {
                    continue;
                }
                let header = self.ecam.read_u8(bus, device, 0, 0x0e);
                let functions = if header & 0x80 != 0 { 8 } else { 1 };
                for function in 0..functions {
                    if self.vendor(bus, device, function) == 0xffff {
                        continue;
                    }
                    visit(self.read_function(bus, device, function)?);
                }
            }
        }
        Ok(())
    }

    fn vendor(&self, bus: u8, device: u8, function: u8) -> u16 {
        self.ecam.read_u16(bus, device, function, 0x00)
    }

    fn read_function(&self, bus: u8, device: u8, function: u8) -> Result<PciFunction, PciEnumError> {
        let device_id = self.ecam.read_u16(bus, device, function, 0x02);
        let prog_if = self.ecam.read_u8(bus, device, function, 0x09);
        let subclass = self.ecam.read_u8(bus, device, function, 0x0a);
        let class_code = self.ecam.read_u8(bus, device, function, 0x0b);
        let header_type = self.ecam.read_u8(bus, device, function, 0x0e);
        let mut bars = [None; 6];
        let mut i = 0usize;
        while i < 6 {
            let bar = self.decode_bar(bus, device, function, i as u8)?;
            bars[i] = bar;
            if matches!(bar.map(|b| b.kind), Some(BarKind::Memory64)) {
                i += 1;
            }
            i += 1;
        }
        Ok(PciFunction {
            bdf: PciBdf { segment: self.segment, bus, device, function },
            vendor_id: self.vendor(bus, device, function),
            device_id,
            class_code,
            subclass,
            prog_if,
            header_type,
            bars,
        })
    }

    fn decode_bar(&self, bus: u8, device: u8, function: u8, index: u8) -> Result<Option<PciBar>, PciEnumError> {
        let offset = 0x10 + u16::from(index) * 4;
        let original = self.ecam.read_u32(bus, device, function, offset);
        if original == 0 {
            return Ok(None);
        }
        let kind = if original & 1 != 0 {
            BarKind::Io
        } else if (original >> 1) & 0x3 == 0x2 {
            BarKind::Memory64
        } else {
            BarKind::Memory32
        };
        let prefetchable = original & 0x8 != 0;
        let base_low = (original & 0xfffffff0) as u64;
        let base = if kind == BarKind::Memory64 {
            let high = self.ecam.read_u32(bus, device, function, offset + 4) as u64;
            base_low | (high << 32)
        } else if kind == BarKind::Io {
            u64::from(original & 0xffff_fffc)
        } else {
            base_low
        };
        if base == 0 {
            return Ok(None);
        }
        let size = match kind {
            BarKind::Io => 4,
            BarKind::Memory32 | BarKind::Memory64 => 4096,
        };
        base.checked_add(size - 1).ok_or(PciEnumError::AddressOverflow)?;
        Ok(Some(PciBar { index, kind, base, size, prefetchable }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bdf_is_structured() {
        assert_eq!(PciBdf { segment: 0, bus: 2, device: 3, function: 1 }.function, 1);
    }
}
