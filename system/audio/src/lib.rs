//! Audio device, media-session, and playback policy boundary.

pub mod media_player;

pub use media_player::{MediaPlayer, MediaPlayerError, MediaQueue, MediaTrack, PlaybackState, RepeatMode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioDirection { Playback, Capture }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioSessionId(pub u64);
