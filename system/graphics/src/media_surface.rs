//! Desktop/compositor bridge for media surfaces.

use crate::{SurfaceId, compositor::Rect};
use globus_media::{
    MediaInput, MediaKind, MediaState, MediaWindow, MediaWindowMode, PresentationState,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Eine Medienflaeche mit Wiedergabezustand.
pub struct MediaSurface {
    /// Kennung der Flaeche.
    pub surface: SurfaceId,
    /// Art des Mediums.
    pub kind: MediaKind,
    /// Rechteck auf dem Bildschirm.
    pub bounds: Rect,
    /// Zustand der Wiedergabe.
    pub state: MediaState,
    /// True, wenn die Flaeche im Vollbild laeuft.
    pub fullscreen: bool,
    /// True, wenn die Flaeche als Bild im Bild laeuft.
    pub picture_in_picture: bool,
    /// Zustand der Praesentation.
    pub presentation: PresentationState,
    /// Zeitstempel des zuletzt praesentierten Bildes in Mikrosekunden.
    pub last_pts_us: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Fehler der Medienflaechen.
pub enum MediaSurfaceError {
    /// Das Rechteck ist leer oder ungueltig.
    InvalidBounds,
    /// Die Flaeche ist nicht registriert.
    Missing,
}

impl MediaSurface {
    /// Legt eine Medienflaeche mit geprueften Grenzen an.
    pub fn new(
        surface: SurfaceId,
        kind: MediaKind,
        bounds: Rect,
    ) -> Result<Self, MediaSurfaceError> {
        if bounds.width == 0 || bounds.height == 0 {
            return Err(MediaSurfaceError::InvalidBounds);
        }
        Ok(Self {
            surface,
            kind,
            bounds,
            state: MediaState::Stopped,
            fullscreen: false,
            picture_in_picture: false,
            presentation: PresentationState::Empty,
            last_pts_us: None,
        })
    }
    /// Setzt den Wiedergabezustand.
    pub fn set_state(&mut self, state: MediaState) {
        self.state = state;
    }
    /// Schaltet Vollbild ein oder aus.
    pub fn set_fullscreen(&mut self, fullscreen: bool) {
        self.fullscreen = fullscreen;
    }
    /// Schaltet Bild im Bild ein oder aus.
    pub fn set_picture_in_picture(&mut self, pip: bool) {
        self.picture_in_picture = pip;
    }
    /// Setzt neue Grenzen; leere Rechtecke werden abgelehnt.
    pub fn resize(&mut self, bounds: Rect) -> Result<(), MediaSurfaceError> {
        if bounds.width == 0 || bounds.height == 0 {
            return Err(MediaSurfaceError::InvalidBounds);
        }
        self.bounds = bounds;
        Ok(())
    }
    /// Meldet die Flaeche als bereit.
    pub fn mark_ready(&mut self) {
        self.presentation = PresentationState::Ready;
    }
    /// Praesentiert ein Bild; false, solange die Flaeche angehalten ist.
    pub fn present(&mut self, pts_us: u64) -> bool {
        if self.presentation == PresentationState::Suspended {
            return false;
        }
        self.presentation = PresentationState::Presented;
        self.last_pts_us = Some(pts_us);
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

#[derive(Debug)]
/// Verbindung aus Medienflaeche und Fenster.
pub struct MediaSurfaceBinding {
    /// Die Medienflaeche.
    pub surface: MediaSurface,
    /// Fenster, das die Flaeche zeigt.
    pub window: MediaWindow,
}

#[derive(Debug, Default)]
/// Registrierte Medienflaechen samt ihren Fenstern.
pub struct MediaSurfaceRegistry {
    surfaces: Vec<MediaSurfaceBinding>,
}

impl MediaSurfaceRegistry {
    /// Registriert eine Flaeche und ersetzt eine vorhandene mit derselben Kennung.
    pub fn register(&mut self, surface: MediaSurface) {
        let id = surface.surface;
        self.surfaces.retain(|x| x.surface.surface != id);
        self.surfaces.push(MediaSurfaceBinding {
            surface,
            window: MediaWindow::new(id.0, id.0),
        });
    }
    /// Entfernt die Flaeche.
    pub fn remove(&mut self, surface: SurfaceId) {
        self.surfaces.retain(|x| x.surface.surface != surface);
    }
    /// Flaeche zu einer Kennung.
    pub fn get(&self, surface: SurfaceId) -> Option<&MediaSurface> {
        self.surfaces
            .iter()
            .find(|x| x.surface.surface == surface)
            .map(|x| &x.surface)
    }
    /// Flaeche zu einer Kennung, veraenderbar.
    pub fn get_mut(&mut self, surface: SurfaceId) -> Option<&mut MediaSurface> {
        self.surfaces
            .iter_mut()
            .find(|x| x.surface.surface == surface)
            .map(|x| &mut x.surface)
    }
    /// Fenster zu einer Kennung, veraenderbar.
    pub fn window_mut(&mut self, surface: SurfaceId) -> Option<&mut MediaWindow> {
        self.surfaces
            .iter_mut()
            .find(|x| x.surface.surface == surface)
            .map(|x| &mut x.window)
    }
    /// Reicht Eingaben an das Fenster der Flaeche weiter.
    pub fn input(
        &mut self,
        surface: SurfaceId,
        input: MediaInput,
    ) -> Result<(), MediaSurfaceError> {
        let binding = self
            .surfaces
            .iter_mut()
            .find(|x| x.surface.surface == surface)
            .ok_or(MediaSurfaceError::Missing)?;
        binding.window.apply(input);
        binding
            .surface
            .set_fullscreen(binding.window.mode == MediaWindowMode::Fullscreen);
        binding
            .surface
            .set_picture_in_picture(binding.window.mode == MediaWindowMode::PictureInPicture);
        binding.surface.set_state(match binding.window.playback {
            globus_media::PlaybackState::Stopped => MediaState::Stopped,
            globus_media::PlaybackState::Playing => MediaState::Playing,
            globus_media::PlaybackState::Paused => MediaState::Paused,
            globus_media::PlaybackState::Seeking => MediaState::Seeking,
        });
        Ok(())
    }
    /// Kopien aller registrierten Flaechen.
    pub fn surfaces(&self) -> Vec<MediaSurface> {
        self.surfaces.iter().map(|x| x.surface).collect()
    }
}
