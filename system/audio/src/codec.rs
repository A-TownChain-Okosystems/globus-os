//! Deterministic audio format and decoder boundary.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Sample format or container of an audio stream.
pub enum AudioFormat {
    /// 16-bit signed PCM, little endian.
    PcmS16Le,
    /// 32-bit float PCM, little endian.
    PcmF32Le,
    /// MPEG audio layer III.
    Mp3,
    /// FLAC, losslessly compressed PCM.
    Flac,
    /// Opus.
    Opus,
    /// Advanced Audio Coding.
    Aac,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Sample rate, channel count and format of one stream.
pub struct AudioSpec {
    /// Samples per second; 1 to 384000 are accepted.
    pub sample_rate: u32,
    /// Interleaved channel count; 1 to 32 are accepted.
    pub channels: u16,
    /// Sample or container format.
    pub format: AudioFormat,
}

impl AudioSpec {
    /// Rejects rate 0 or above 384 kHz and channel counts outside 1 to 32.
    pub fn validate(self) -> Result<(), CodecError> {
        if self.sample_rate == 0 || self.sample_rate > 384_000 {
            return Err(CodecError::InvalidSampleRate);
        }
        if self.channels == 0 || self.channels > 32 {
            return Err(CodecError::InvalidChannels);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
/// Interleaved PCM block together with its spec.
pub struct PcmFrame {
    /// Format of the samples.
    pub spec: AudioSpec,
    /// Interleaved samples; the length must be a multiple of the channel count.
    pub samples: Vec<f32>,
}

impl PcmFrame {
    /// Validates the spec and requires whole frames.
    pub fn new(spec: AudioSpec, samples: Vec<f32>) -> Result<Self, CodecError> {
        spec.validate()?;
        if !samples.len().is_multiple_of(spec.channels as usize) {
            return Err(CodecError::InvalidSampleCount);
        }
        Ok(Self { spec, samples })
    }

    /// Number of whole frames, independent of the channel count.
    pub fn frames(&self) -> usize {
        self.samples.len() / self.spec.channels as usize
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Failures of the codec boundary.
pub enum CodecError {
    /// Sample rate is 0 or above 384 kHz.
    InvalidSampleRate,
    /// Channel count is 0 or above 32.
    InvalidChannels,
    /// Sample count is not a multiple of the channel count.
    InvalidSampleCount,
    /// Seek position lies outside the stream.
    InvalidPosition,
    /// The format is not supported here.
    UnsupportedFormat,
}

/// Pull-based decoder boundary.
pub trait AudioDecoder {
    /// Format the decoder produces.
    fn spec(&self) -> AudioSpec;
    /// Decodes at most `max_frames` frames.
    fn decode(&mut self, max_frames: usize) -> Result<PcmFrame, CodecError>;
    /// Jumps to a position in milliseconds.
    fn seek_ms(&mut self, position_ms: u64) -> Result<(), CodecError>;
    /// True once the stream is fully decoded.
    fn finished(&self) -> bool;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_pcm_frame() {
        let spec = AudioSpec {
            sample_rate: 48_000,
            channels: 2,
            format: AudioFormat::PcmF32Le,
        };
        let frame = PcmFrame::new(spec, vec![0.0; 8]).expect("valid frame");
        assert_eq!(frame.frames(), 4);
    }

    #[test]
    fn rejects_partial_channel_frame() {
        let spec = AudioSpec {
            sample_rate: 48_000,
            channels: 2,
            format: AudioFormat::PcmF32Le,
        };
        assert_eq!(
            PcmFrame::new(spec, vec![0.0; 3]),
            Err(CodecError::InvalidSampleCount)
        );
    }
}
