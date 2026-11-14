//! Desktop media-window state independent of the compositor implementation.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaWindowMode {
    /// Fenster im normalen Fenstermodus.
    Windowed,
    /// Fenster im Vollbild.
    Fullscreen,
    /// Fenster als Bild im Bild.
    PictureInPicture,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Zustand der Wiedergabe in einer Medienflaeche.
pub enum PlaybackState {
    /// Angehalten und am Anfang.
    Stopped,
    /// Laeuft.
    Playing,
    /// Angehalten an der aktuellen Position.
    Paused,
    /// Springt gerade.
    Seeking,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Lebenszyklus einer Medienflaeche.
pub enum WindowLifecycle {
    /// Angelegt, aber noch nicht angehaengt.
    Created,
    /// An die Ausgabe angehaengt.
    Attached,
    /// Angehalten, bleibt angehaengt.
    Suspended,
    /// Von der Ausgabe geloest.
    Detached,
    /// Geschlossen.
    Closed,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Eingabe, die eine Medienflaeche versteht.
pub enum MediaInput {
    /// Wiedergabe umschalten.
    PlayPause,
    /// Anhalten.
    Stop,
    /// Vorwaertsspringen.
    SeekForward,
    /// Rueckwaertsspringen.
    SeekBackward,
    /// Vollbild umschalten.
    ToggleFullscreen,
    /// Bild im Bild umschalten.
    TogglePictureInPicture,
    /// Lautstaerke erhoehen.
    VolumeUp,
    /// Lautstaerke senken.
    VolumeDown,
    /// Stummschaltung umschalten.
    Mute,
}

/// Lautstaerkesteuerung, an die sich eine Medienflaeche haengt.
pub trait AudioControl {
    /// Setzt die Lautstaerke in Prozent.
    fn set_volume(&mut self, percent: u8);
    /// Schaltet die Stummschaltung.
    fn set_muted(&mut self, muted: bool);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Medienflaeche im Desktop mit Wiedergabezustand.
pub struct MediaWindow {
    /// Kennung des Fensters.
    pub id: u64,
    /// Kennung der zugehoerigen Ausgabeflaeche.
    pub surface_id: u64,
    /// Fenstermodus.
    pub mode: MediaWindowMode,
    /// Zustand der Wiedergabe.
    pub playback: PlaybackState,
    /// Lebenszyklus.
    pub lifecycle: WindowLifecycle,
    /// Aktuelle Position in Mikrosekunden.
    pub position_us: u64,
    /// Dauer in Mikrosekunden, falls bekannt.
    pub duration_us: Option<u64>,
    /// Schrittweite beim Spulen.
    pub seek_step_us: u64,
    /// Lautstaerke in Prozent.
    pub volume_percent: u8,
    /// True, wenn stumm geschaltet ist.
    pub muted: bool,
}

impl MediaWindow {
    /// Legt ein Fenster im Modus Fenster an, noch ohne Dauer.
    pub fn new(id: u64, surface_id: u64) -> Self {
        Self {
            id,
            surface_id,
            mode: MediaWindowMode::Windowed,
            playback: PlaybackState::Stopped,
            lifecycle: WindowLifecycle::Created,
            position_us: 0,
            duration_us: None,
            seek_step_us: 10_000_000,
            volume_percent: 100,
            muted: false,
        }
    }
    /// Haengt das Fenster an die Ausgabe.
    pub fn attach(&mut self) {
        if self.lifecycle != WindowLifecycle::Closed {
            self.lifecycle = WindowLifecycle::Attached;
        }
    }
    /// Haelt das Fenster an.
    pub fn suspend(&mut self) {
        if self.lifecycle == WindowLifecycle::Attached {
            self.lifecycle = WindowLifecycle::Suspended;
        }
    }
    /// Nimmt ein angehaltenes Fenster wieder auf.
    pub fn resume(&mut self) {
        if self.lifecycle == WindowLifecycle::Suspended {
            self.lifecycle = WindowLifecycle::Attached;
        }
    }
    /// Loest das Fenster von der Ausgabe.
    pub fn detach(&mut self) {
        if self.lifecycle != WindowLifecycle::Closed {
            self.lifecycle = WindowLifecycle::Detached;
        }
    }
    /// Schliesst das Fenster.
    pub fn close(&mut self) {
        self.lifecycle = WindowLifecycle::Closed;
        self.playback = PlaybackState::Stopped;
    }
    /// Setzt oder loescht die bekannte Dauer.
    pub fn set_duration(&mut self, duration_us: Option<u64>) {
        self.duration_us = duration_us;
        self.position_us = self.position_us.min(duration_us.unwrap_or(u64::MAX));
    }
    /// Setzt die Position, auf die Dauer begrenzt.
    pub fn set_position(&mut self, position_us: u64) {
        self.position_us = position_us.min(self.duration_us.unwrap_or(u64::MAX));
    }
    /// Startet die Wiedergabe.
    pub fn play(&mut self) {
        if self.lifecycle != WindowLifecycle::Closed {
            self.playback = PlaybackState::Playing;
        }
    }
    /// Haelt die Wiedergabe an.
    pub fn pause(&mut self) {
        if self.playback == PlaybackState::Playing {
            self.playback = PlaybackState::Paused;
        }
    }
    /// Haelt an und setzt die Position zurueck.
    pub fn stop(&mut self) {
        self.playback = PlaybackState::Stopped;
        self.position_us = 0;
    }
    /// Springt relativ zur aktuellen Position.
    pub fn seek_by(&mut self, delta_us: i64) {
        self.playback = PlaybackState::Seeking;
        if delta_us.is_negative() {
            self.position_us = self.position_us.saturating_sub(delta_us.unsigned_abs());
        } else {
            self.position_us = self
                .position_us
                .saturating_add(delta_us as u64)
                .min(self.duration_us.unwrap_or(u64::MAX));
        }
        self.playback = PlaybackState::Paused;
    }
    /// Wendet eine Eingabe auf das Fenster an.
    pub fn apply(&mut self, input: MediaInput) {
        match input {
            MediaInput::PlayPause => {
                if self.playback == PlaybackState::Playing {
                    self.pause()
                } else {
                    self.play()
                }
            }
            MediaInput::Stop => self.stop(),
            MediaInput::SeekForward => self.seek_by(self.seek_step_us as i64),
            MediaInput::SeekBackward => self.seek_by(-(self.seek_step_us as i64)),
            MediaInput::ToggleFullscreen => {
                self.mode = if self.mode == MediaWindowMode::Fullscreen {
                    MediaWindowMode::Windowed
                } else {
                    MediaWindowMode::Fullscreen
                }
            }
            MediaInput::TogglePictureInPicture => {
                self.mode = if self.mode == MediaWindowMode::PictureInPicture {
                    MediaWindowMode::Windowed
                } else {
                    MediaWindowMode::PictureInPicture
                }
            }
            MediaInput::VolumeUp => {
                self.volume_percent = self.volume_percent.saturating_add(5).min(100)
            }
            MediaInput::VolumeDown => self.volume_percent = self.volume_percent.saturating_sub(5),
            MediaInput::Mute => self.muted = !self.muted,
        }
    }
    /// Reicht Lautstaerke und Stummschaltung an die Tonsteuerung weiter.
    pub fn sync_audio<A: AudioControl>(&self, audio: &mut A) {
        audio.set_volume(self.volume_percent);
        audio.set_muted(self.muted);
    }
}
