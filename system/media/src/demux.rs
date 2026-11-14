//! Container demuxing contracts and deterministic timestamp handling.

use crate::MediaError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
/// Containerformat einer Datei.
pub enum ContainerFormat {
    /// MP4.
    Mp4,
    /// Matroska.
    Mkv,
    /// WebM.
    Webm,
    #[default]
    /// Unbekannt oder nicht erkannt.
    Unknown,
}

impl ContainerFormat {
    /// Erkennt das Format an der Dateiendung.
    pub fn from_uri(uri: &str) -> Self {
        match uri
            .rsplit('.')
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str()
        {
            "mp4" => Self::Mp4,
            "mkv" => Self::Mkv,
            "webm" => Self::Webm,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Art einer Spur.
pub enum TrackKind {
    /// Tonspur.
    Audio,
    /// Bildspur.
    Video,
    /// Untertitelspur.
    Subtitle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Zeitangaben eines Pakets.
pub struct Timestamp {
    /// Praesentationszeitpunkt in Mikrosekunden.
    pub pts_us: u64,
    /// Dekodierzeitpunkt in Mikrosekunden.
    pub dts_us: u64,
    /// Dauer in Mikrosekunden.
    pub duration_us: u64,
    /// True, wenn das Paket einen Keyframe beginnt.
    pub keyframe: bool,
}

impl Timestamp {
    /// Baut einen Zeitstempel aus seinen Teilen.
    /// Legt den Demultiplexer ueber diesen Paketen an.
    pub fn new(pts_us: u64, dts_us: u64, duration_us: u64, keyframe: bool) -> Self {
        Self {
            pts_us,
            dts_us,
            duration_us,
            keyframe,
        }
    }
    /// Abstand zwischen Dekodier- und Praesentationszeitpunkt.
    pub fn presentation_delay_us(self) -> i64 {
        self.pts_us as i64 - self.dts_us as i64
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Ein ausgelesenes Paket.
pub struct DemuxPacket {
    /// Spur, zu der das Paket gehoert.
    pub track: TrackKind,
    /// Zeitangaben des Pakets.
    pub timestamp: Timestamp,
    /// Nutzdaten des Pakets.
    pub data: Vec<u8>,
}

/// Ein Demultiplexer.
pub trait Demuxer {
    /// Format der Quelle.
    fn format(&self) -> ContainerFormat;
    /// Naechstes Paket, oder None am Ende.
    fn next_packet(&mut self) -> Result<Option<DemuxPacket>, MediaError>;
    /// Springt an eine Position.
    fn seek_us(&mut self, position_us: u64) -> Result<(), MediaError>;
}

#[derive(Debug, Default)]
/// Demultiplexer ueber einer festen Paketliste, fuer Tests und Vorstufen.
pub struct PacketDemuxer {
    format: ContainerFormat,
    packets: Vec<DemuxPacket>,
    cursor: usize,
}

impl PacketDemuxer {
    pub fn new(format: ContainerFormat, packets: Vec<DemuxPacket>) -> Self {
        Self {
            format,
            packets,
            cursor: 0,
        }
    }
    /// Anzahl noch nicht ausgelesener Pakete.
    pub fn remaining(&self) -> usize {
        self.packets.len().saturating_sub(self.cursor)
    }
}

impl Demuxer for PacketDemuxer {
    fn format(&self) -> ContainerFormat {
        self.format
    }
    fn next_packet(&mut self) -> Result<Option<DemuxPacket>, MediaError> {
        let packet = self.packets.get(self.cursor).cloned();
        if packet.is_some() {
            self.cursor += 1;
        }
        Ok(packet)
    }
    fn seek_us(&mut self, position_us: u64) -> Result<(), MediaError> {
        self.cursor = self
            .packets
            .iter()
            .position(|p| p.timestamp.pts_us >= position_us)
            .unwrap_or(self.packets.len());
        Ok(())
    }
}

/// Baut einen Zeitstempel fuer ein Paket, das neu einsortiert wurde.
pub fn reorder_timestamp(pts_us: u64, dts_us: u64) -> Timestamp {
    Timestamp {
        pts_us,
        dts_us,
        duration_us: 0,
        keyframe: false,
    }
}
