//! IOMMU/DMA isolation contract.
//!
//! The implementation must be backed by the platform IOMMU; device services
//! receive only explicitly authorized DMA domains.

use super::DeviceId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DmaDomainId(pub u64);
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DmaRegion { pub iova: u64, pub physical: u64, pub length: u64, pub writable: bool }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IommuError { InvalidRegion, DomainNotFound, MappingDenied, DeviceAlreadyAssigned }

pub trait Iommu {
    fn create_domain(&mut self, domain: DmaDomainId) -> Result<(), IommuError>;
    fn assign_device(&mut self, domain: DmaDomainId, device: DeviceId) -> Result<(), IommuError>;
    fn map(&mut self, domain: DmaDomainId, region: DmaRegion) -> Result<(), IommuError>;
    fn unmap(&mut self, domain: DmaDomainId, iova: u64) -> Result<(), IommuError>;
}

pub fn validate_region(region: DmaRegion) -> Result<(), IommuError> {
    if region.length == 0 || region.iova.checked_add(region.length).is_none() || region.physical.checked_add(region.length).is_none() { return Err(IommuError::InvalidRegion); }
    Ok(())
}

#[cfg(test)] mod tests { use super::*; #[test] fn rejects_zero_length_dma() { assert_eq!(validate_region(DmaRegion{iova:0,physical:0,length:0,writable:false}),Err(IommuError::InvalidRegion)); } }
