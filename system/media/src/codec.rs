//! Codec backend contracts for real software or hardware decoder implementations.

use crate::{MediaError, PixelFormat, VideoFrame, VideoFrameSpec};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoCodec { H264, Hevc, Av1, Vp9 }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhotoCodec { Jpeg, Png, Webp, Avif }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecoderKind { Software, VaApi, Amd, GenericGpu }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecoderCapabilities {
    pub codec: VideoCodec,
    pub kind: DecoderKind,
    pub output: PixelFormat,
    pub zero_copy: bool,
}

pub trait VideoCodecDecoder {
    fn codec(&self) -> VideoCodec;
    fn capabilities(&self) -> DecoderCapabilities;
    fn spec(&self) -> VideoFrameSpec;
    fn submit(&mut self, packet: &[u8], pts_us: u64, dts_us: u64, keyframe: bool) -> Result<(), MediaError>;
    fn receive(&mut self) -> Result<Option<TimedVideoFrame>, MediaError>;
    fn seek(&mut self, position_us: u64) -> Result<(), MediaError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimedVideoFrame {
    pub frame: VideoFrame,
    pub pts_us: u64,
    pub dts_us: u64,
    pub keyframe: bool,
}

pub trait PhotoCodecDecoder {
    fn codec(&self) -> PhotoCodec;
    fn decode_incremental(&mut self, input: &[u8], final_chunk: bool) -> Result<Option<crate::PhotoFrame>, MediaError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VideoScaling { pub source_width: u32, pub source_height: u32, pub target_width: u32, pub target_height: u32 }

impl VideoScaling {
    pub fn validate(self) -> Result<(), MediaError> {
        if self.source_width == 0 || self.source_height == 0 || self.target_width == 0 || self.target_height == 0 {
            Err(MediaError::InvalidVideoSpec)
        } else { Ok(()) }
    }
}
