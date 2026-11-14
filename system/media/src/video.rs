//! Codec-independent video playback pipeline.

use crate::{MediaError, MediaState, Timeline, VideoFrame, VideoFrameSpec};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Ein Videopaket aus dem Demultiplexer.
pub struct VideoPacket {
    /// Praesentationszeitpunkt in Mikrosekunden.
    pub pts_us: u64,
    /// Dauer in Mikrosekunden.
    pub duration_us: u64,
    /// True, wenn das Paket einen Keyframe beginnt.
    pub keyframe: bool,
    /// Laenge der Nutzdaten in Byte.
    pub data_len: usize,
}

/// Ein Videodekoder.
pub trait VideoDecoder {
    /// Format der erzeugten Bilder.
    fn spec(&self) -> VideoFrameSpec;
    /// Naechstes dekodiertes Bild, oder None am Ende.
    fn decode_next(&mut self) -> Result<Option<VideoFrame>, MediaError>;
    /// Springt an eine Position.
    fn seek_us(&mut self, position_us: u64) -> Result<(), MediaError>;
    /// True, wenn die Quelle ausgeschoepft ist.
    fn finished(&self) -> bool;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Was die Zeit vorgibt.
pub enum ClockMaster {
    /// Die Tonspur.
    Audio,
    /// Die Bildspur.
    Video,
    /// Eine externe Uhr.
    External,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Positionsuhr der Wiedergabe.
pub struct SyncClock {
    /// Aktuelle Position in Mikrosekunden.
    pub position_us: u64,
    /// Was die Zeit vorgibt.
    pub master: ClockMaster,
}

impl SyncClock {
    /// Uhr bei 0 mit diesem Master.
    pub const fn new(master: ClockMaster) -> Self {
        Self {
            position_us: 0,
            master,
        }
    }
    /// Rueckt die Uhr vor.
    pub fn advance(&mut self, delta_us: u64) {
        self.position_us = self.position_us.saturating_add(delta_us);
    }
    /// Versatz eines Zeitstempels zur aktuellen Position.
    pub fn delta_us(self, pts_us: u64) -> i64 {
        pts_us as i64 - self.position_us as i64
    }
}

#[derive(Debug)]
/// Wiedergabekette aus Dekoder, Uhr, Position und aktuellem Bild.
pub struct VideoPipeline<D> {
    decoder: D,
    state: MediaState,
    timeline: Timeline,
    clock: SyncClock,
    last_frame: Option<VideoFrame>,
}

impl<D: VideoDecoder> VideoPipeline<D> {
    /// Legt die Kette an; die Dauer ist optional.
    pub fn new(decoder: D, duration_ms: Option<u64>) -> Result<Self, MediaError> {
        decoder.spec().validate()?;
        Ok(Self {
            decoder,
            state: MediaState::Stopped,
            timeline: Timeline::new(duration_ms),
            clock: SyncClock::new(ClockMaster::Video),
            last_frame: None,
        })
    }
    /// Zustand der Wiedergabe.
    pub fn state(&self) -> MediaState {
        self.state
    }
    /// Aktuelle Uhr.
    pub fn clock(&self) -> SyncClock {
        self.clock
    }
    /// Aktuell gezeigtes Bild.
    pub fn current_frame(&self) -> Option<&VideoFrame> {
        self.last_frame.as_ref()
    }
    /// Startet die Wiedergabe.
    pub fn play(&mut self) {
        self.state = MediaState::Playing;
    }
    /// Haelt die Wiedergabe an.
    pub fn pause(&mut self) {
        if self.state == MediaState::Playing {
            self.state = MediaState::Paused;
        }
    }
    /// Haelt an und setzt die Position zurueck.
    pub fn stop(&mut self) {
        self.state = MediaState::Stopped;
        self.clock.position_us = 0;
        let _ = self.timeline.seek(0);
    }
    /// Springt an eine Position.
    pub fn seek_us(&mut self, position_us: u64) -> Result<(), MediaError> {
        self.state = MediaState::Seeking;
        self.decoder.seek_us(position_us)?;
        self.clock.position_us = position_us;
        self.timeline.seek(position_us / 1_000)?;
        self.last_frame = None;
        self.state = MediaState::Paused;
        Ok(())
    }
    /// Dekodiert bis zum naechsten faelligen Bild und liefert es.
    pub fn pump(&mut self) -> Result<Option<&VideoFrame>, MediaError> {
        if self.state != MediaState::Playing {
            return Ok(self.last_frame.as_ref());
        }
        match self.decoder.decode_next()? {
            Some(frame) => {
                self.last_frame = Some(frame);
                Ok(self.last_frame.as_ref())
            }
            None => {
                self.state = MediaState::Ended;
                Ok(None)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PixelFormat;
    struct Decoder {
        spec: VideoFrameSpec,
        done: bool,
    }
    impl VideoDecoder for Decoder {
        fn spec(&self) -> VideoFrameSpec {
            self.spec
        }
        fn decode_next(&mut self) -> Result<Option<VideoFrame>, MediaError> {
            if self.done {
                return Ok(None);
            }
            self.done = true;
            Ok(Some(VideoFrame {
                spec: self.spec,
                format: PixelFormat::Rgba8,
                data: vec![0; 4],
            }))
        }
        fn seek_us(&mut self, _: u64) -> Result<(), MediaError> {
            self.done = false;
            Ok(())
        }
        fn finished(&self) -> bool {
            self.done
        }
    }
    #[test]
    fn pipeline_reaches_end() {
        let spec = VideoFrameSpec {
            width: 1,
            height: 1,
            fps_num: 30,
            fps_den: 1,
        };
        let mut p = VideoPipeline::new(Decoder { spec, done: false }, None).unwrap();
        p.play();
        assert!(p.pump().unwrap().is_some());
        assert!(p.pump().unwrap().is_none());
        assert_eq!(p.state(), MediaState::Ended);
    }
}
