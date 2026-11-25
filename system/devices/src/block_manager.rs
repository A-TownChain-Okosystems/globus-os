//! Deterministic block-device registry and I/O boundary.

use super::block::{BlockDevice, BlockError, BlockGeometry, BlockRequest, validate_request};
use alloc::vec::Vec;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
/// Kennung eines verwalteten Blockgeraets.
pub struct BlockDeviceId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Fehler der Geraeteverwaltung.
pub enum BlockManagerError {
    /// Die Kennung ist bereits vergeben.
    Duplicate,
    /// Die Kennung ist unbekannt.
    Missing,
    /// Die Geometrie ist unbrauchbar.
    InvalidGeometry,
    /// Das Geraet hat abgelehnt.
    Io(BlockError),
}

/// Verwaltete Blockgeraete nach Kennung.
pub struct BlockDeviceRegistry<D> {
    devices: Vec<(BlockDeviceId, D)>,
}

impl<D: BlockDevice> Default for BlockDeviceRegistry<D> {
    fn default() -> Self {
        Self::new()
    }
}

impl<D: BlockDevice> BlockDeviceRegistry<D> {
    /// Leere Verwaltung.
    pub fn new() -> Self {
        Self {
            devices: Vec::new(),
        }
    }

    /// Nimmt ein Geraet auf und lehnt doppelte Kennungen ab.
    pub fn register(&mut self, id: BlockDeviceId, device: D) -> Result<(), BlockManagerError> {
        if self.devices.iter().any(|(existing, _)| *existing == id) {
            return Err(BlockManagerError::Duplicate);
        }
        if !device.geometry().valid() {
            return Err(BlockManagerError::InvalidGeometry);
        }
        self.devices.push((id, device));
        self.devices.sort_by_key(|(id, _)| *id);
        Ok(())
    }

    /// Geometrie eines verwalteten Geraets.
    pub fn geometry(&self, id: BlockDeviceId) -> Result<BlockGeometry, BlockManagerError> {
        self.devices
            .iter()
            .find(|(existing, _)| *existing == id)
            .map(|(_, device)| device.geometry())
            .ok_or(BlockManagerError::Missing)
    }

    /// Liest ueber ein verwaltetes Geraet.
    pub fn read(
        &mut self,
        id: BlockDeviceId,
        request: BlockRequest,
        destination: &mut [u8],
    ) -> Result<(), BlockManagerError> {
        let (_, device) = self
            .devices
            .iter_mut()
            .find(|(existing, _)| *existing == id)
            .ok_or(BlockManagerError::Missing)?;
        validate_request(device.geometry(), request).map_err(BlockManagerError::Io)?;
        device
            .read(request, destination)
            .map_err(BlockManagerError::Io)
    }

    /// Schreibt ueber ein verwaltetes Geraet.
    pub fn write(
        &mut self,
        id: BlockDeviceId,
        request: BlockRequest,
        source: &[u8],
    ) -> Result<(), BlockManagerError> {
        let (_, device) = self
            .devices
            .iter_mut()
            .find(|(existing, _)| *existing == id)
            .ok_or(BlockManagerError::Missing)?;
        validate_request(device.geometry(), request).map_err(BlockManagerError::Io)?;
        device.write(request, source).map_err(BlockManagerError::Io)
    }

    /// Leitet die Abschluss- und Persistenzbarriere an das Geraet weiter.
    pub fn flush(&mut self, id: BlockDeviceId) -> Result<(), BlockManagerError> {
        let (_, device) = self
            .devices
            .iter_mut()
            .find(|(existing, _)| *existing == id)
            .ok_or(BlockManagerError::Missing)?;
        device.flush().map_err(BlockManagerError::Io)
    }

    /// Alle vergebenen Kennungen.
    pub fn ids(&self) -> impl Iterator<Item = BlockDeviceId> + '_ {
        self.devices.iter().map(|(id, _)| *id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::MemoryBlockDevice;

    #[test]
    fn registers_and_reads_deterministically() {
        let mut r = BlockDeviceRegistry::new();
        r.register(
            BlockDeviceId(2),
            MemoryBlockDevice::new(BlockGeometry {
                block_size: 512,
                block_count: 2,
            })
            .unwrap(),
        )
        .unwrap();
        r.register(
            BlockDeviceId(1),
            MemoryBlockDevice::new(BlockGeometry {
                block_size: 512,
                block_count: 2,
            })
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            r.ids().collect::<Vec<_>>(),
            vec![BlockDeviceId(1), BlockDeviceId(2)]
        );
        r.flush(BlockDeviceId(1)).unwrap();
    }
    struct FlushFailDevice(MemoryBlockDevice);
    impl BlockDevice for FlushFailDevice {
        fn geometry(&self) -> BlockGeometry {
            self.0.geometry()
        }
        fn read(
            &mut self,
            request: BlockRequest,
            destination: &mut [u8],
        ) -> Result<(), BlockError> {
            self.0.read(request, destination)
        }
        fn write(
            &mut self,
            request: BlockRequest,
            source: &[u8],
        ) -> Result<(), BlockError> {
            self.0.write(request, source)
        }
        fn flush(&mut self) -> Result<(), BlockError> {
            Err(BlockError::Io)
        }
    }

    #[test]
    fn registry_propagates_flush_error() {
        let device = FlushFailDevice(
            MemoryBlockDevice::new(BlockGeometry {
                block_size: 512,
                block_count: 2,
            })
            .unwrap(),
        );
        let mut registry = BlockDeviceRegistry::new();
        registry.register(BlockDeviceId(7), device).unwrap();
        assert_eq!(
            registry.flush(BlockDeviceId(7)),
            Err(BlockManagerError::Io(BlockError::Io))
        );
    }

}
