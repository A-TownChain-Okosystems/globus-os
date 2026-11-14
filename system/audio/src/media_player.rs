//! Deterministic media-player state machine and queue.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Playback state of the player.
pub enum PlaybackState {
    /// Not playing; the position is 0.
    Stopped,
    /// Advancing through the current track.
    Playing,
    /// Held at a position, resumable.
    Paused,
    /// Position change in progress.
    Seeking,
    /// Reserved for a failed playback path.
    Error,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// How the queue wraps at the end of a track.
pub enum RepeatMode {
    /// Stop at the end of the queue.
    Off,
    /// Repeat the current track.
    One,
    /// Wrap around to the first track.
    All,
}
#[derive(Debug, Clone, PartialEq, Eq)]
/// One track in the queue.
pub struct MediaTrack {
    /// Identifier of the track.
    pub id: u64,
    /// Where the track is read from.
    pub uri: String,
    /// Title shown to the user.
    pub title: String,
    /// Artist shown to the user.
    pub artist: String,
    /// Album shown to the user.
    pub album: String,
    /// Length in milliseconds; seek positions are bounded by it.
    pub duration_ms: u64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Failures of queue and playback control.
pub enum MediaPlayerError {
    /// The queue holds no current track.
    EmptyQueue,
    /// The index lies outside the queue.
    InvalidIndex,
    /// The position lies beyond the track duration.
    InvalidPosition,
    /// Advancing the position would overflow.
    PositionOverflow,
    /// Playback is already running.
    AlreadyPlaying,
}
#[derive(Debug, Default)]
/// Track list with a current index.
pub struct MediaQueue {
    tracks: Vec<MediaTrack>,
    current: Option<usize>,
}
impl MediaQueue {
    /// Empty queue without a current track.
    pub fn new() -> Self {
        Self::default()
    }
    /// Appends a track; the first one becomes current.
    pub fn push(&mut self, t: MediaTrack) {
        self.tracks.push(t);
        if self.current.is_none() {
            self.current = Some(0);
        }
    }
    /// Removes all tracks and the current index.
    pub fn clear(&mut self) {
        self.tracks.clear();
        self.current = None;
    }
    /// Number of tracks.
    pub fn len(&self) -> usize {
        self.tracks.len()
    }

    /// Counterpart to `len`.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// Current track, if any.
    pub fn current(&self) -> Option<&MediaTrack> {
        self.current.and_then(|i| self.tracks.get(i))
    }
    /// Makes the track at `i` current.
    pub fn select(&mut self, i: usize) -> Result<(), MediaPlayerError> {
        if i >= self.tracks.len() {
            return Err(MediaPlayerError::InvalidIndex);
        }
        self.current = Some(i);
        Ok(())
    }
    /// Moves to the next track, honouring the repeat mode.
    pub fn next(&mut self, r: RepeatMode) -> Result<&MediaTrack, MediaPlayerError> {
        if self.tracks.is_empty() {
            return Err(MediaPlayerError::EmptyQueue);
        }
        let i = self.current.unwrap_or(0);
        let n = if i + 1 < self.tracks.len() {
            i + 1
        } else {
            match r {
                RepeatMode::All => 0,
                RepeatMode::One => i,
                RepeatMode::Off => return Err(MediaPlayerError::InvalidIndex),
            }
        };
        self.current = Some(n);
        Ok(&self.tracks[n])
    }
    /// Moves to the previous track, honouring the repeat mode.
    pub fn previous(&mut self, r: RepeatMode) -> Result<&MediaTrack, MediaPlayerError> {
        if self.tracks.is_empty() {
            return Err(MediaPlayerError::EmptyQueue);
        }
        let i = self.current.unwrap_or(0);
        let n = if i > 0 {
            i - 1
        } else {
            match r {
                RepeatMode::All => self.tracks.len() - 1,
                _ => 0,
            }
        };
        self.current = Some(n);
        Ok(&self.tracks[n])
    }
}
/// Player state: queue, position, volume and repeat mode.
pub struct MediaPlayer {
    queue: MediaQueue,
    state: PlaybackState,
    repeat: RepeatMode,
    position_ms: u64,
    volume: u8,
    muted: bool,
}
impl MediaPlayer {
    /// Stopped, volume 100, repeat off, empty queue.
    pub fn new() -> Self {
        Self {
            queue: MediaQueue::new(),
            state: PlaybackState::Stopped,
            repeat: RepeatMode::Off,
            position_ms: 0,
            volume: 100,
            muted: false,
        }
    }
    /// Queue of the player.
    pub fn queue(&self) -> &MediaQueue {
        &self.queue
    }
    /// Queue of the player, mutable.
    pub fn queue_mut(&mut self) -> &mut MediaQueue {
        &mut self.queue
    }
    /// Current playback state.
    pub fn state(&self) -> PlaybackState {
        self.state
    }
    /// Current repeat mode.
    pub fn repeat(&self) -> RepeatMode {
        self.repeat
    }
    /// Position within the current track.
    pub fn position_ms(&self) -> u64 {
        self.position_ms
    }
    /// Volume from 0 to 100.
    pub fn volume(&self) -> u8 {
        self.volume
    }
    /// True when muted.
    pub fn muted(&self) -> bool {
        self.muted
    }
    /// Sets the repeat mode.
    pub fn set_repeat(&mut self, r: RepeatMode) {
        self.repeat = r
    }
    /// Sets the volume, clamped to 100.
    pub fn set_volume(&mut self, v: u8) {
        self.volume = v.min(100)
    }
    /// Mutes or unmutes.
    pub fn set_muted(&mut self, v: bool) {
        self.muted = v
    }
    /// Starts playback; fails on an empty queue or when already playing.
    pub fn play(&mut self) -> Result<(), MediaPlayerError> {
        if self.queue.current().is_none() {
            return Err(MediaPlayerError::EmptyQueue);
        }
        if self.state == PlaybackState::Playing {
            return Err(MediaPlayerError::AlreadyPlaying);
        }
        self.state = PlaybackState::Playing;
        Ok(())
    }
    /// Pauses when playing, otherwise no change.
    pub fn pause(&mut self) {
        if self.state == PlaybackState::Playing {
            self.state = PlaybackState::Paused
        }
    }
    /// Stops and resets the position to 0.
    pub fn stop(&mut self) {
        self.state = PlaybackState::Stopped;
        self.position_ms = 0
    }
    /// Sets the position within the track and leaves the player paused.
    pub fn seek_ms(&mut self, p: u64) -> Result<(), MediaPlayerError> {
        let t = self.queue.current().ok_or(MediaPlayerError::EmptyQueue)?;
        if p > t.duration_ms {
            return Err(MediaPlayerError::InvalidPosition);
        }
        self.state = PlaybackState::Seeking;
        self.position_ms = p;
        self.state = PlaybackState::Paused;
        Ok(())
    }
    /// Advances the position; wraps or stops at the end per repeat mode.
    pub fn advance_ms(&mut self, d: u64) -> Result<(), MediaPlayerError> {
        let n = self
            .position_ms
            .checked_add(d)
            .ok_or(MediaPlayerError::PositionOverflow)?;
        let dur = self
            .queue
            .current()
            .ok_or(MediaPlayerError::EmptyQueue)?
            .duration_ms;
        if n < dur {
            self.position_ms = n;
            return Ok(());
        }
        self.position_ms = 0;
        match self.repeat {
            RepeatMode::One => self.state = PlaybackState::Playing,
            RepeatMode::All => {
                self.queue.next(RepeatMode::All)?;
                self.state = PlaybackState::Playing
            }
            RepeatMode::Off => {
                if self.queue.next(RepeatMode::Off).is_err() {
                    self.state = PlaybackState::Stopped
                } else {
                    self.state = PlaybackState::Playing
                }
            }
        }
        Ok(())
    }
    /// Skips to the next track and starts playing.
    pub fn next_track(&mut self) -> Result<(), MediaPlayerError> {
        self.queue.next(self.repeat)?;
        self.position_ms = 0;
        self.state = PlaybackState::Playing;
        Ok(())
    }
    /// Goes back one track and starts playing.
    pub fn previous(&mut self) -> Result<(), MediaPlayerError> {
        self.queue.previous(self.repeat)?;
        self.position_ms = 0;
        self.state = PlaybackState::Playing;
        Ok(())
    }
}
impl Default for MediaPlayer {
    fn default() -> Self {
        Self::new()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn t(id: u64, d: u64) -> MediaTrack {
        MediaTrack {
            id,
            uri: format!("file:///track-{id}.ogg"),
            title: format!("Track {id}"),
            artist: "Globus".into(),
            album: "Test".into(),
            duration_ms: d,
        }
    }
    #[test]
    fn playback() {
        let mut p = MediaPlayer::new();
        p.queue_mut().push(t(1, 1000));
        p.queue_mut().push(t(2, 1000));
        p.play().unwrap();
        p.advance_ms(1000).unwrap();
        assert_eq!(p.queue().current().unwrap().id, 2);
    }
    #[test]
    fn repeat_all() {
        let mut p = MediaPlayer::new();
        p.queue_mut().push(t(1, 10));
        p.queue_mut().push(t(2, 10));
        p.set_repeat(RepeatMode::All);
        p.play().unwrap();
        p.advance_ms(20).unwrap();
        assert_eq!(p.queue().current().unwrap().id, 2);
        p.advance_ms(10).unwrap();
        assert_eq!(p.queue().current().unwrap().id, 1);
    }
    #[test]
    fn seek_bounds() {
        let mut p = MediaPlayer::new();
        p.queue_mut().push(t(1, 100));
        assert!(p.seek_ms(101).is_err());
        assert!(p.seek_ms(100).is_ok());
    }
}
