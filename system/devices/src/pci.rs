//! PCIe/IOMMU discovery contracts. Hardware access remains in isolated drivers.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PciAddress {
    pub segment: u16,
    pub bus: u8,
    pub device: u8,
    pub function: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PciId {
    pub vendor: u16,
    pub device: u16,
    pub class: u8,
    pub subclass: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IommuDomain(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DmaPolicy {
    pub domain: IommuDomain,
    pub allow_dma: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PciFunction {
    pub address: PciAddress,
    pub id: PciId,
    pub dma: DmaPolicy,
}
