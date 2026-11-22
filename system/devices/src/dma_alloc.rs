//! Seitenrahmen-Allokator fuer DMA-Puffer.
//!
//! Ein Geraet braucht physisch zusammenhaengenden, seitengerichteten Speicher.
//! Dieser Allokator verwaltet einen zusammenhaengenden Bereich und liefert nur
//! vollstaendig belegte Seitenlaeufe; Teilstuecke werden nicht herausgegeben.
//!
//! Fail-closed: Bei erschoepftem Pool, unbrauchbarer Geometrie oder Freigabe
//! eines nicht eigenen Puffers gibt es einen Fehler, niemals einen Puffer mit
//! Adresse null.

use alloc::vec;
use alloc::vec::Vec;

use crate::iommu::DmaRegion;

/// Groesse einer Speicherseite in Byte.
pub const PAGE_SIZE: u64 = 4096;

/// Fehler des Seitenrahmen-Allokators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DmaAllocError {
    /// Der Pool hat keine Seite.
    EmptyPool,
    /// Basis nicht seitengerichtet, Groesse unstimmig oder Adressraum ueberlaufend.
    InvalidGeometry,
    /// Kein ausreichend grosser zusammenhaengender Lauf frei.
    OutOfMemory,
    /// Der Puffer gehoert nicht zu diesem Pool oder wurde bereits freigegeben.
    NotOwned,
}

/// Ein zugewiesener DMA-Puffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DmaAllocation {
    /// Physische Startadresse, seitengerichtet.
    pub physical: u64,
    /// Belegte Laenge in Byte, Vielfaches der Seitengroesse.
    pub length: u64,
}

impl DmaAllocation {
    /// Abbildung fuer die IOMMU: Geraeteadresse auf diesen Puffer.
    pub fn region(&self, iova: u64, writable: bool) -> DmaRegion {
        DmaRegion {
            iova,
            physical: self.physical,
            length: self.length,
            writable,
        }
    }
}

/// Verwaltet einen zusammenhaengenden Seitenbereich.
#[derive(Debug, Clone)]
pub struct FrameAllocator {
    base: u64,
    pages: usize,
    used: Vec<bool>,
}

impl FrameAllocator {
    /// Neuer Allokator ueber `pages` Seiten ab `base`.
    ///
    /// `base` muss seitengerichtet sein, `pages` mindestens eins, und das Ende
    /// darf den Adressraum nicht ueberschreiten.
    pub fn new(base: u64, pages: usize) -> Result<Self, DmaAllocError> {
        if pages == 0 {
            return Err(DmaAllocError::EmptyPool);
        }
        if !base.is_multiple_of(PAGE_SIZE) {
            return Err(DmaAllocError::InvalidGeometry);
        }
        let bytes = (pages as u64)
            .checked_mul(PAGE_SIZE)
            .ok_or(DmaAllocError::InvalidGeometry)?;
        if base.checked_add(bytes).is_none() {
            return Err(DmaAllocError::InvalidGeometry);
        }
        Ok(Self {
            base,
            pages,
            used: vec![false; pages],
        })
    }

    /// Gesamtzahl der Seiten im Pool.
    pub fn total_pages(&self) -> usize {
        self.pages
    }

    /// Noch freie Seiten.
    pub fn free_pages(&self) -> usize {
        self.used.iter().filter(|u| !**u).count()
    }

    /// Belegt den ersten ausreichend grossen zusammenhaengenden Lauf.
    ///
    /// `bytes` wird auf volle Seiten aufgerundet. Ein Lauf, der ueber das
    /// Poolende hinausreicht, wird nicht in Teilstuecken vergeben.
    pub fn allocate(&mut self, bytes: u64) -> Result<DmaAllocation, DmaAllocError> {
        if bytes == 0 {
            return Err(DmaAllocError::InvalidGeometry);
        }
        let needed = bytes.div_ceil(PAGE_SIZE) as usize;
        if needed > self.pages {
            return Err(DmaAllocError::OutOfMemory);
        }
        let mut start = 0usize;
        while start + needed <= self.pages {
            let frei = (start..start + needed).all(|i| !self.used[i]);
            if frei {
                for slot in self.used.iter_mut().skip(start).take(needed) {
                    *slot = true;
                }
                let physical = self.base + (start as u64) * PAGE_SIZE;
                return Ok(DmaAllocation {
                    physical,
                    length: (needed as u64) * PAGE_SIZE,
                });
            }
            start += 1;
        }
        Err(DmaAllocError::OutOfMemory)
    }

    /// Gibt einen Puffer frei, der zu diesem Pool gehoert.
    pub fn free(&mut self, allocation: DmaAllocation) -> Result<(), DmaAllocError> {
        if allocation.length == 0
            || !allocation.length.is_multiple_of(PAGE_SIZE)
            || allocation.physical < self.base
        {
            return Err(DmaAllocError::NotOwned);
        }
        let offset = allocation.physical - self.base;
        if !offset.is_multiple_of(PAGE_SIZE) {
            return Err(DmaAllocError::NotOwned);
        }
        let start = (offset / PAGE_SIZE) as usize;
        let count = (allocation.length / PAGE_SIZE) as usize;
        if start >= self.pages || start + count > self.pages {
            return Err(DmaAllocError::NotOwned);
        }
        if !(start..start + count).all(|i| self.used[i]) {
            return Err(DmaAllocError::NotOwned);
        }
        for slot in self.used.iter_mut().skip(start).take(count) {
            *slot = false;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: u64 = 0x1000_0000;

    #[test]
    fn rejects_invalid_geometry() {
        assert_eq!(
            FrameAllocator::new(BASE, 0).unwrap_err(),
            DmaAllocError::EmptyPool
        );
        assert_eq!(
            FrameAllocator::new(BASE + 8, 4).unwrap_err(),
            DmaAllocError::InvalidGeometry
        );
        assert_eq!(
            FrameAllocator::new(u64::MAX - 8, 4).unwrap_err(),
            DmaAllocError::InvalidGeometry
        );
    }

    #[test]
    fn allocates_page_aligned_contiguous_memory() {
        let mut a = FrameAllocator::new(BASE, 4).unwrap();
        let first = a.allocate(1).unwrap();
        let second = a.allocate(1).unwrap();
        assert_eq!(first.physical, BASE);
        assert_eq!(first.length, PAGE_SIZE);
        assert_eq!(second.physical, BASE + PAGE_SIZE);
        assert_eq!(a.free_pages(), 2);
    }

    #[test]
    fn multi_page_request_is_rounded_up() {
        let mut a = FrameAllocator::new(BASE, 4).unwrap();
        let block = a.allocate(PAGE_SIZE + 1).unwrap();
        assert_eq!(block.length, 2 * PAGE_SIZE);
        assert_eq!(a.free_pages(), 2);
    }

    #[test]
    fn exhaustion_fails_closed() {
        let mut a = FrameAllocator::new(BASE, 2).unwrap();
        a.allocate(PAGE_SIZE).unwrap();
        a.allocate(PAGE_SIZE).unwrap();
        assert_eq!(a.allocate(1).unwrap_err(), DmaAllocError::OutOfMemory);
        assert_eq!(a.allocate(0).unwrap_err(), DmaAllocError::InvalidGeometry);
        assert_eq!(a.free_pages(), 0);
    }

    #[test]
    fn freed_pages_are_reused() {
        let mut a = FrameAllocator::new(BASE, 2).unwrap();
        let first = a.allocate(PAGE_SIZE).unwrap();
        let second = a.allocate(PAGE_SIZE).unwrap();
        a.free(first).unwrap();
        let wieder = a.allocate(PAGE_SIZE).unwrap();
        assert_eq!(wieder.physical, first.physical);
        assert_eq!(a.free_pages(), 0);
        assert!(second.length > 0);
    }

    #[test]
    fn double_free_is_rejected() {
        let mut a = FrameAllocator::new(BASE, 2).unwrap();
        let block = a.allocate(PAGE_SIZE).unwrap();
        a.free(block).unwrap();
        assert_eq!(a.free(block).unwrap_err(), DmaAllocError::NotOwned);
    }

    #[test]
    fn foreign_buffer_is_rejected() {
        let mut a = FrameAllocator::new(BASE, 2).unwrap();
        let fremd = DmaAllocation {
            physical: BASE + 8,
            length: PAGE_SIZE,
        };
        assert_eq!(a.free(fremd).unwrap_err(), DmaAllocError::NotOwned);
        let ausserhalb = DmaAllocation {
            physical: BASE + 4 * PAGE_SIZE,
            length: PAGE_SIZE,
        };
        assert_eq!(a.free(ausserhalb).unwrap_err(), DmaAllocError::NotOwned);
    }

    #[test]
    fn allocation_maps_to_valid_iommu_region() {
        let mut a = FrameAllocator::new(BASE, 2).unwrap();
        let block = a.allocate(2 * PAGE_SIZE).unwrap();
        let region = block.region(0x8000, true);
        assert_eq!(region.physical, block.physical);
        assert_eq!(region.length, block.length);
        assert!(crate::iommu::validate_region(region).is_ok());
    }
}
