//! Graphics-facing media surface contract.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceKind {
    /// Flaeche fuer Video.
    Video,
    /// Flaeche fuer Fotos.
    Photo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Zustand der Praesentation einer Flaeche.
pub enum PresentationState {
    /// Noch kein Bild vorhanden.
    Empty,
    /// Ein Bild liegt bereit.
    Ready,
    /// Ein Bild wurde gezeigt.
    Presented,
    /// Die Praesentation ist angehalten.
    Suspended,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Eine Flaeche fuer Medienausgabe.
pub struct MediaSurface {
    /// Kennung der Flaeche.
    pub id: u64,
    /// Art des Inhalts.
    pub kind: SurfaceKind,
    /// Breite in Pixel.
    pub width: u32,
    /// Hoehe in Pixel.
    pub height: u32,
    /// True, wenn die Flaeche sichtbar ist.
    pub visible: bool,
    /// True, wenn die Flaeche im Vollbild laeuft.
    pub fullscreen: bool,
    /// True, wenn die Flaeche als Bild im Bild laeuft.
    pub picture_in_picture: bool,
    /// Zustand der Praesentation.
    pub presentation: PresentationState,
    /// Zaehler der gezeigten Bilder.
    pub frame_sequence: u64,
    /// Zeitstempel des zuletzt gezeigten Bildes.
    pub last_pts_us: Option<u64>,
}

impl MediaSurface {
    /// Legt eine Flaeche mit Kennung, Art und Groesse an.
    pub fn new(id: u64, kind: SurfaceKind, width: u32, height: u32) -> Self {
        Self {
            id,
            kind,
            width,
            height,
            visible: true,
            fullscreen: false,
            picture_in_picture: false,
            presentation: PresentationState::Empty,
            frame_sequence: 0,
            last_pts_us: None,
        }
    }
    /// Aendert die Groesse der Flaeche.
    pub fn resize(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
    }
    /// Schaltet Vollbild ein oder aus.
    pub fn set_fullscreen(&mut self, value: bool) {
        self.fullscreen = value;
    }
    /// Schaltet Bild im Bild ein oder aus.
    pub fn set_picture_in_picture(&mut self, value: bool) {
        self.picture_in_picture = value;
    }
    /// Blendet die Flaeche ein oder aus.
    pub fn set_visible(&mut self, value: bool) {
        self.visible = value;
        if !value {
            self.presentation = PresentationState::Suspended;
        }
    }
    /// Meldet ein bereitliegendes Bild.
    pub fn mark_ready(&mut self) {
        if self.visible {
            self.presentation = PresentationState::Ready;
        }
    }
    /// Zeigt ein Bild; false, solange die Flaeche angehalten ist.
    pub fn present(&mut self, pts_us: u64) -> bool {
        if !self.visible || self.presentation == PresentationState::Suspended {
            return false;
        }
        self.frame_sequence = self.frame_sequence.saturating_add(1);
        self.last_pts_us = Some(pts_us);
        self.presentation = PresentationState::Presented;
        true
    }
    /// Haelt die Praesentation an.
    pub fn suspend(&mut self) {
        self.presentation = PresentationState::Suspended;
    }
    /// Nimmt die Praesentation wieder auf.
    pub fn resume(&mut self) {
        self.presentation = PresentationState::Ready;
    }
}
