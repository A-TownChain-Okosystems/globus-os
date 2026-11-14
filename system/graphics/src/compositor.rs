//! Deterministic desktop compositor contracts.

use crate::SurfaceId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Rechteck in Bildschirmkoordinaten.
pub struct Rect {
    /// Linke Kante.
    pub x: i32,
    /// Obere Kante.
    pub y: i32,
    /// Breite in Pixel.
    pub width: u32,
    /// Hoehe in Pixel.
    pub height: u32,
}

impl Rect {
    /// Baut ein Rechteck aus Position und Groesse.
    pub const fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// True, wenn der Punkt im Rechteck liegt; rechte und untere Kante zaehlen nicht dazu.
    pub fn contains(self, x: i32, y: i32) -> bool {
        x >= self.x
            && y >= self.y
            && x < self
                .x
                .saturating_add(self.width.min(i32::MAX as u32) as i32)
            && y < self
                .y
                .saturating_add(self.height.min(i32::MAX as u32) as i32)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Lage einer Oberflaeche im Kompositionsstapel.
pub struct SurfacePlacement {
    /// Oberflaeche, die platziert wird.
    pub surface: SurfaceId,
    /// Rechteck auf dem Bildschirm.
    pub bounds: Rect,
    /// Stapelreihenfolge; groessere Werte liegen oben.
    pub z: u32,
    /// True, wenn die Oberflaeche gezeichnet wird.
    pub visible: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Fehler des Kompositors.
pub enum CompositorError {
    /// Die Oberflaeche ist bereits platziert.
    SurfaceExists,
    /// Die Oberflaeche ist nicht platziert.
    SurfaceMissing,
    /// Breite oder Hoehe ist 0.
    InvalidSize,
    /// Die Obergrenze an Oberflaechen ist erreicht.
    TooManySurfaces,
}

#[derive(Debug, Default)]
/// Stapel platzierter Oberflaechen mit Vergabe der Z-Reihenfolge.
pub struct Compositor {
    surfaces: Vec<SurfacePlacement>,
    next_z: u32,
}

impl Compositor {
    /// Leerer Kompositor; die erste Oberflaeche bekommt z = 1.
    pub fn new() -> Self {
        Self {
            surfaces: Vec::new(),
            next_z: 1,
        }
    }

    /// Platziert eine Oberflaeche, vergibt die naechste Z-Stufe und lehnt Doppelungen sowie Groesse 0 ab.
    pub fn create(&mut self, surface: SurfaceId, bounds: Rect) -> Result<(), CompositorError> {
        if bounds.width == 0 || bounds.height == 0 {
            return Err(CompositorError::InvalidSize);
        }
        if self.surfaces.iter().any(|s| s.surface == surface) {
            return Err(CompositorError::SurfaceExists);
        }
        if self.surfaces.len() == 4096 {
            return Err(CompositorError::TooManySurfaces);
        }
        let placement = SurfacePlacement {
            surface,
            bounds,
            z: self.next_z,
            visible: true,
        };
        self.next_z = self.next_z.saturating_add(1);
        self.surfaces.push(placement);
        Ok(())
    }

    /// Atomically updates logical geometry, visibility and z-order for an
    /// already registered surface. Invalid geometry leaves the old state intact.
    pub fn set_placement(&mut self, placement: SurfacePlacement) -> Result<(), CompositorError> {
        if placement.bounds.width == 0 || placement.bounds.height == 0 {
            return Err(CompositorError::InvalidSize);
        }
        let item = self
            .surfaces
            .iter_mut()
            .find(|s| s.surface == placement.surface)
            .ok_or(CompositorError::SurfaceMissing)?;
        *item = placement;
        self.next_z = self.next_z.max(placement.z.saturating_add(1));
        Ok(())
    }

    /// Entfernt eine platzierte Oberflaeche.
    pub fn remove(&mut self, surface: SurfaceId) -> Result<(), CompositorError> {
        let index = self
            .surfaces
            .iter()
            .position(|s| s.surface == surface)
            .ok_or(CompositorError::SurfaceMissing)?;
        self.surfaces.remove(index);
        Ok(())
    }

    /// Hebt eine Oberflaeche auf die oberste Stufe.
    pub fn focus(&mut self, surface: SurfaceId) -> Result<(), CompositorError> {
        let max_z = self.next_z;
        let item = self
            .surfaces
            .iter_mut()
            .find(|s| s.surface == surface)
            .ok_or(CompositorError::SurfaceMissing)?;
        item.z = max_z;
        item.visible = true;
        self.next_z = self.next_z.saturating_add(1);
        Ok(())
    }

    /// Oberste sichtbare Oberflaeche an diesem Punkt.
    pub fn hit_test(&self, x: i32, y: i32) -> Option<SurfaceId> {
        self.surfaces
            .iter()
            .filter(|s| s.visible && s.bounds.contains(x, y))
            .max_by_key(|s| s.z)
            .map(|s| s.surface)
    }

    /// Platzierte Oberflaechen in Einfuegereihenfolge.
    pub fn surfaces(&self) -> &[SurfacePlacement] {
        &self.surfaces
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hit_testing_prefers_topmost_surface() {
        let mut c = Compositor::new();
        c.create(SurfaceId(1), Rect::new(0, 0, 100, 100)).unwrap();
        c.create(SurfaceId(2), Rect::new(20, 20, 100, 100)).unwrap();
        assert_eq!(c.hit_test(30, 30), Some(SurfaceId(2)));
        c.focus(SurfaceId(1)).unwrap();
        assert_eq!(c.hit_test(30, 30), Some(SurfaceId(1)));
    }

    #[test]
    fn placement_update_changes_geometry_and_visibility() {
        let mut c = Compositor::new();
        c.create(SurfaceId(1), Rect::new(0, 0, 100, 100)).unwrap();
        c.set_placement(SurfacePlacement {
            surface: SurfaceId(1),
            bounds: Rect::new(20, 30, 200, 120),
            z: 4,
            visible: false,
        })
        .unwrap();
        assert_eq!(c.hit_test(30, 40), None);
        assert_eq!(c.surfaces()[0].bounds, Rect::new(20, 30, 200, 120));
    }
}
