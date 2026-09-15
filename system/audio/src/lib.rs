//! Audio device and session policy boundary.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioDirection {
    Playback,
    Capture,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioSessionId(pub u64);
