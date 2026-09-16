//! Desktop media-window state independent of the compositor implementation.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaWindowMode { Windowed, Fullscreen, PictureInPicture }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaInput { PlayPause, Stop, SeekForward, SeekBackward, ToggleFullscreen, TogglePictureInPicture, VolumeUp, VolumeDown, Mute }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MediaWindow { pub id: u64, pub surface_id: u64, pub mode: MediaWindowMode, pub volume_percent: u8, pub muted: bool }

impl MediaWindow {
    pub fn new(id: u64, surface_id: u64) -> Self { Self { id, surface_id, mode: MediaWindowMode::Windowed, volume_percent: 100, muted: false } }
    pub fn apply(&mut self, input: MediaInput) {
        match input {
            MediaInput::ToggleFullscreen => self.mode = if self.mode == MediaWindowMode::Fullscreen { MediaWindowMode::Windowed } else { MediaWindowMode::Fullscreen },
            MediaInput::TogglePictureInPicture => self.mode = if self.mode == MediaWindowMode::PictureInPicture { MediaWindowMode::Windowed } else { MediaWindowMode::PictureInPicture },
            MediaInput::VolumeUp => self.volume_percent = self.volume_percent.saturating_add(5).min(100),
            MediaInput::VolumeDown => self.volume_percent = self.volume_percent.saturating_sub(5),
            MediaInput::Mute => self.muted = !self.muted,
            MediaInput::PlayPause | MediaInput::Stop | MediaInput::SeekForward | MediaInput::SeekBackward => {}
        }
    }
}
