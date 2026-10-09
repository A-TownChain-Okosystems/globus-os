//! Intel VT-d register programming primitives and IOMMU/DMA isolation contracts.

use super::DeviceId;
use super::mmio::MmioWindow;

pub struct IntelVtd {
    regs: MmioWindow,
}

impl IntelVtd {
    pub const GCMD: usize = 0x18;
    pub const GSTS: usize = 0x1c;
    pub const RTADDR: usize = 0x20;
    pub const CCMD: usize = 0x28;
    pub const IOTLB: usize = 0x08;

    pub const GCMD_SRTP: u32 = 1 << 30;
    pub const GCMD_TE: u32 = 1 << 31;
    pub const GSTS_SRTP: u32 = 1 << 30;
    pub const GSTS_TE: u32 = 1 << 31;

    pub const unsafe fn new(base: usize, len: usize) -> Self {
        Self {
            regs: MmioWindow::new(base, len),
        }
    }

    pub fn version(&self) -> u32 {
        self.regs.read32(0x00)
    }

    pub fn capabilities(&self) -> u64 {
        self.regs.read64(0x08)
    }

    pub fn set_root_table(&self, physical_address: u64) {
        assert_eq!(physical_address & 0xfff, 0);
        self.regs.write64(Self::RTADDR, physical_address);
        self.regs.write32(Self::GCMD, Self::GCMD_SRTP);
        self.wait_for(Self::GSTS, Self::GSTS_SRTP, true);
    }

    pub fn enable_translation(&self) {
        let command = self.regs.read32(Self::GCMD) | Self::GCMD_TE;
        self.regs.write32(Self::GCMD, command);
        self.wait_for(Self::GSTS, Self::GSTS_TE, true);
    }

    pub fn disable_translation(&self) {
        let command = self.regs.read32(Self::GCMD) & !Self::GCMD_TE;
        self.regs.write32(Self::GCMD, command);
        self.wait_for(Self::GSTS, Self::GSTS_TE, false);
    }

    fn wait_for(&self, offset: usize, mask: u32, set: bool) {
        for _ in 0..1_000_000 {
            let value = self.regs.read32(offset) & mask;
            if (value != 0) == set {
                return;
            }
            core::hint::spin_loop();
        }
        panic!("VT-d register transition timed out");
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DmaDomainId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DmaRegion {
    pub iova: u64,
    pub physical: u64,
    pub length: u64,
    pub writable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IommuError {
    InvalidRegion,
    DomainNotFound,
    MappingDenied,
    DeviceAlreadyAssigned,
}

pub trait Iommu {
    fn create_domain(&mut self, domain: DmaDomainId) -> Result<(), IommuError>;
    fn assign_device(&mut self, domain: DmaDomainId, device: DeviceId) -> Result<(), IommuError>;
    fn map(&mut self, domain: DmaDomainId, region: DmaRegion) -> Result<(), IommuError>;
    fn unmap(&mut self, domain: DmaDomainId, iova: u64) -> Result<(), IommuError>;
}

pub fn validate_region(region: DmaRegion) -> Result<(), IommuError> {
    if region.length == 0
        || region.iova.checked_add(region.length).is_none()
        || region.physical.checked_add(region.length).is_none()
    {
        return Err(IommuError::InvalidRegion);
    }
    Ok(())
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SoftwareIommu {
    domains: Vec<DomainState>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DomainState {
    id: DmaDomainId,
    devices: Vec<DeviceId>,
    mappings: Vec<DmaRegion>,
}

impl SoftwareIommu {
    fn domain_mut(&mut self, id: DmaDomainId) -> Result<&mut DomainState, IommuError> {
        self.domains
            .iter_mut()
            .find(|d| d.id == id)
            .ok_or(IommuError::DomainNotFound)
    }
}

impl Iommu for SoftwareIommu {
    fn create_domain(&mut self, domain: DmaDomainId) -> Result<(), IommuError> {
        if self.domains.iter().any(|d| d.id == domain) {
            return Ok(());
        }
        self.domains.push(DomainState {
            id: domain,
            devices: Vec::new(),
            mappings: Vec::new(),
        });
        Ok(())
    }

    fn assign_device(&mut self, domain: DmaDomainId, device: DeviceId) -> Result<(), IommuError> {
        if self.domains.iter().any(|d| d.devices.contains(&device)) {
            return Err(IommuError::DeviceAlreadyAssigned);
        }
        let target = self.domain_mut(domain)?;
        target.devices.push(device);
        Ok(())
    }

    fn map(&mut self, domain: DmaDomainId, region: DmaRegion) -> Result<(), IommuError> {
        validate_region(region)?;
        let target = self.domain_mut(domain)?;
        if target.mappings.iter().any(|m| {
            m.iova < region.iova + region.length && region.iova < m.iova + m.length
        }) {
            return Err(IommuError::MappingDenied);
        }
        target.mappings.push(region);
        Ok(())
    }

    fn unmap(&mut self, domain: DmaDomainId, iova: u64) -> Result<(), IommuError> {
        let target = self.domain_mut(domain)?;
        let prev_len = target.mappings.len();
        target.mappings.retain(|m| m.iova != iova);
        if target.mappings.len() == prev_len {
            Err(IommuError::InvalidRegion)
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_dma_region_bounds() {
        assert!(
            validate_region(DmaRegion {
                iova: 0x1000,
                physical: 0x2000,
                length: 0x1000,
                writable: true,
            })
            .is_ok()
        );
        assert!(
            validate_region(DmaRegion {
                iova: u64::MAX,
                physical: 0x1000,
                length: 0x1000,
                writable: true,
            })
            .is_err()
        );
    }
}
