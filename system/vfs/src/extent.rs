//! Deterministic file extent mapping for persistent storage.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtentError {
    /// Laenge 0 oder eine unbrauchbare Angabe.
    Invalid,
    /// Die Rechnung laeuft ueber den Wertebereich hinaus.
    Overflow,
    /// Die logische Blocknummer liegt in keinem Extent.
    OutOfRange,
    /// Der neue Extent ueberlappt einen vorhandenen.
    Overlap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Zusammenhaengender Bereich, logisch auf physisch abgebildet.
pub struct Extent {
    /// Erster logischer Block.
    pub logical: u64,
    /// Erster physischer Block.
    pub physical: u64,
    /// Anzahl Bloecke im Bereich.
    pub blocks: u64,
}

impl Extent {
    /// Prueft Laenge und Ueberlauf und baut den Extent.
    pub fn new(logical: u64, physical: u64, blocks: u64) -> Result<Self, ExtentError> {
        if blocks == 0 {
            return Err(ExtentError::Invalid);
        }
        logical.checked_add(blocks).ok_or(ExtentError::Overflow)?;
        physical.checked_add(blocks).ok_or(ExtentError::Overflow)?;
        Ok(Self {
            logical,
            physical,
            blocks,
        })
    }

    /// True, wenn der logische Block im Bereich liegt.
    pub fn contains(&self, block: u64) -> bool {
        block >= self.logical && block < self.logical + self.blocks
    }
    /// Physischer Block zu einem logischen Block dieses Extents.
    pub fn physical_for(&self, logical: u64) -> Result<u64, ExtentError> {
        if !self.contains(logical) {
            return Err(ExtentError::OutOfRange);
        }
        self.physical
            .checked_add(logical - self.logical)
            .ok_or(ExtentError::Overflow)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
/// Extents eines Objekts, ueberlappungsfrei.
pub struct ExtentMap {
    extents: Vec<Extent>,
}

impl ExtentMap {
    /// Leere Abbildung.
    pub fn new() -> Self {
        Self::default()
    }
    /// Alle Extents in Einfuegereihenfolge.
    pub fn extents(&self) -> &[Extent] {
        &self.extents
    }

    /// Fuegt einen Extent ein und lehnt Ueberlappungen ab.
    pub fn insert(&mut self, extent: Extent) -> Result<(), ExtentError> {
        let end = extent
            .logical
            .checked_add(extent.blocks)
            .ok_or(ExtentError::Overflow)?;
        if self.extents.iter().any(|existing| {
            let existing_end = existing.logical + existing.blocks;
            extent.logical < existing_end && existing.logical < end
        }) {
            return Err(ExtentError::Overlap);
        }
        self.extents.push(extent);
        self.extents.sort_by_key(|e| e.logical);
        Ok(())
    }

    /// Physischer Block zu einem logischen Block ueber alle Extents.
    pub fn physical_for(&self, logical: u64) -> Result<u64, ExtentError> {
        self.extents
            .iter()
            .find(|e| e.contains(logical))
            .ok_or(ExtentError::OutOfRange)?
            .physical_for(logical)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_blocks_deterministically() {
        let mut map = ExtentMap::new();
        map.insert(Extent::new(4, 100, 3).unwrap()).unwrap();
        assert_eq!(map.physical_for(5).unwrap(), 101);
    }

    #[test]
    fn overlap_is_rejected() {
        let mut map = ExtentMap::new();
        map.insert(Extent::new(0, 10, 4).unwrap()).unwrap();
        assert_eq!(
            map.insert(Extent::new(3, 20, 2).unwrap()),
            Err(ExtentError::Overlap)
        );
    }
}
