//! Audio output device abstraction and deterministic ring-buffer backend.

use crate::codec::{AudioSpec, PcmFrame};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Failures of an output device.
pub enum OutputError {
    /// The output was not opened.
    NotOpen,
    /// The output is already open.
    AlreadyOpen,
    /// The frame does not match the opened spec.
    SpecMismatch,
    /// The buffer cannot take another frame.
    BufferFull,
    /// Capacity of zero frames.
    InvalidCapacity,
}

/// Boundary every output device implements.
pub trait AudioOutput {
    /// Opens the device for a fixed spec.
    fn open(&mut self, spec: AudioSpec) -> Result<(), OutputError>;
    /// Starts playback.
    fn start(&mut self) -> Result<(), OutputError>;
    /// Stops playback and drops queued frames.
    fn stop(&mut self) -> Result<(), OutputError>;
    /// Writes one frame and returns how many frames were taken.
    fn write(&mut self, frame: &PcmFrame) -> Result<usize, OutputError>;
    /// Frames waiting in the buffer.
    fn queued_frames(&self) -> usize;
    /// Frames between write and audible output.
    fn latency_frames(&self) -> usize;
}

#[derive(Debug, Clone)]
/// Output backed by a fixed ring buffer of frames.
pub struct RingAudioOutput {
    capacity_frames: usize,
    spec: Option<AudioSpec>,
    running: bool,
    queued: usize,
}

impl RingAudioOutput {
    /// Creates a ring output with a non-zero capacity.
    pub fn new(capacity_frames: usize) -> Result<Self, OutputError> {
        if capacity_frames == 0 {
            return Err(OutputError::InvalidCapacity);
        }
        Ok(Self {
            capacity_frames,
            spec: None,
            running: false,
            queued: 0,
        })
    }

    /// True between start and stop.
    pub fn is_running(&self) -> bool {
        self.running
    }
}

impl AudioOutput for RingAudioOutput {
    fn open(&mut self, spec: AudioSpec) -> Result<(), OutputError> {
        spec.validate().map_err(|_| OutputError::SpecMismatch)?;
        if self.spec.is_some() {
            return Err(OutputError::AlreadyOpen);
        }
        self.spec = Some(spec);
        self.queued = 0;
        Ok(())
    }

    fn start(&mut self) -> Result<(), OutputError> {
        if self.spec.is_none() {
            return Err(OutputError::NotOpen);
        }
        self.running = true;
        Ok(())
    }

    fn stop(&mut self) -> Result<(), OutputError> {
        if self.spec.is_none() {
            return Err(OutputError::NotOpen);
        }
        self.running = false;
        Ok(())
    }

    fn write(&mut self, frame: &PcmFrame) -> Result<usize, OutputError> {
        let spec = self.spec.ok_or(OutputError::NotOpen)?;
        if frame.spec != spec {
            return Err(OutputError::SpecMismatch);
        }
        let available = self.capacity_frames.saturating_sub(self.queued);
        if frame.frames() > available {
            return Err(OutputError::BufferFull);
        }
        self.queued += frame.frames();
        Ok(frame.frames())
    }

    fn queued_frames(&self) -> usize {
        self.queued
    }
    fn latency_frames(&self) -> usize {
        self.queued
    }
}

impl Default for RingAudioOutput {
    fn default() -> Self {
        Self::new(48_000).expect("non-zero default capacity")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::AudioFormat;

    #[test]
    fn ring_output_accepts_matching_pcm() {
        let spec = AudioSpec {
            sample_rate: 48_000,
            channels: 2,
            format: AudioFormat::PcmF32Le,
        };
        let frame = PcmFrame::new(spec, vec![0.0; 8]).unwrap();
        let mut output = RingAudioOutput::new(8).unwrap();
        output.open(spec).unwrap();
        output.start().unwrap();
        assert_eq!(output.write(&frame).unwrap(), 4);
        assert_eq!(output.queued_frames(), 4);
    }
}
