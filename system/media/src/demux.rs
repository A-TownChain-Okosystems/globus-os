//! Container demuxing contracts and deterministic timestamp handling.

use crate::MediaError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerFormat { Mp4, Mkv, Webm, Unknown }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackKind { Audio, Video, Subtitle }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timestamp { pub pts_us: u64, pub dts_us: u64, pub duration_us: u64, pub keyframe: bool }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DemuxPacket { pub track: TrackKind, pub timestamp: Timestamp, pub data: Vec<u8> }

pub trait Demuxer {
    fn format(&self) -> ContainerFormat;
    fn next_packet(&mut self) -> Result<Option<DemuxPacket>, MediaError>;
    fn seek_us(&mut self, position_us: u64) -> Result<(), MediaError>;
}

pub fn reorder_timestamp(pts_us: u64, dts_us: u64) -> Timestamp {
    Timestamp { pts_us: pts_us.max(dts_us), dts_us, duration_us: 0, keyframe: false }
}
