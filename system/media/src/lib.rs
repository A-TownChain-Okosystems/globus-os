//! Unified media model for audio, video, and photos.

pub mod av_sync;
/// Dekoder und Codec-Richtlinie des freien Kerns.
pub mod codec;
/// Containerformate, Spuren und Pakete.
pub mod demux;
/// Medienflaechen im Desktop.
pub mod desktop;
/// GPU-Beschleunigung fuer die Wiedergabe.
pub mod gpu;
/// Sammlung und Ordnung der Medieneintraege.
pub mod library;
/// Fotokodeks und Bildtransformationen.
pub mod photo;
/// Wiedergabezustand und Positionsfuehrung.
pub mod playback;
/// Warteschlange dekodierter Bilder.
pub mod queue;
/// Erkennung neuer und geaenderter Dateien.
pub mod scanner;
/// Flaeche fuer die Medienausgabe.
pub mod surface;
/// Vorschaubilder.
pub mod thumbnail;
/// Zwischenspeicher fuer Vorschaubilder.
pub mod thumbnail_cache;
/// Ablage der Vorschaubilder.
pub mod thumbnail_store;
/// Videospur und Bildfolge.
pub mod video;

pub use av_sync::{AvSyncPolicy, SyncAction, SyncDecision};
pub use codec::{
    DecoderCapabilities, DecoderKind, PhotoCodecDecoder, TimedVideoFrame, VideoCodec,
    VideoCodecDecoder, VideoScaling,
};
pub use demux::{ContainerFormat, DemuxPacket, Demuxer, PacketDemuxer, Timestamp, TrackKind};
pub use desktop::{
    AudioControl, MediaInput, MediaWindow, MediaWindowMode, PlaybackState, WindowLifecycle,
};
pub use gpu::{
    BufferFormat, ColorConversion, DisplayMode, DisplayOutput, DmaBufDescriptor, FramePacer,
    FrameTiming, HardwareOverlay, MediaBuffer, MediaGpuBackend, SharedBufferHandle,
};
pub use library::{MediaCatalog, MediaMetadata, MimeType, SortOrder, mime_from_extension};
pub use photo::{
    ColorSpace, ExifOrientation, PhotoCodec, PhotoDecoder, PhotoFrame, PhotoInfo, PhotoViewer,
    PhotoViewport, ProgressivePhotoDecoder,
};
pub use playback::{CodecVideoSession, PlaybackStats};
pub use queue::FrameQueue;
pub use scanner::{
    FileRecord, MediaFileSource, MediaScanner, MediaWatchSource, ScanChange, ScanEvent,
};
pub use surface::{MediaSurface, PresentationState, SurfaceKind};
pub use thumbnail::{Thumbnail, ThumbnailCache};
pub use thumbnail_cache::{ThumbnailCache as BoundedThumbnailCache, ThumbnailKey, ThumbnailSize};
pub use thumbnail_store::{
    MemoryThumbnailStore, StoredThumbnail, ThumbnailDimensions, ThumbnailStore,
};
pub use video::{ClockMaster, SyncClock, VideoDecoder, VideoPacket, VideoPipeline};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Art eines Medieneintrags.
pub enum MediaKind {
    /// Ton.
    Audio,
    /// Video.
    Video,
    /// Foto.
    Photo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Dateiformat eines Medieneintrags.
pub enum MediaFormat {
    /// MP3.
    Mp3,
    /// FLAC.
    Flac,
    /// Opus.
    Opus,
    /// AAC.
    Aac,
    /// MP4.
    Mp4,
    /// WebM.
    Webm,
    /// Matroska.
    Mkv,
    /// QuickTime.
    Mov,
    /// JPEG.
    Jpeg,
    /// PNG.
    Png,
    /// WebP.
    Webp,
    /// AVIF.
    Avif,
    /// Unbekannt.
    Unknown,
}

impl MediaFormat {
    /// Erkennt das Format an der Dateiendung.
    pub fn from_extension(extension: &str) -> Self {
        match extension
            .rsplit('.')
            .next()
            .unwrap_or(extension)
            .trim_start_matches('.')
            .to_ascii_lowercase()
            .as_str()
        {
            "mp3" => Self::Mp3,
            "flac" => Self::Flac,
            "opus" => Self::Opus,
            "aac" => Self::Aac,
            "mp4" => Self::Mp4,
            "webm" => Self::Webm,
            "mkv" => Self::Mkv,
            "mov" => Self::Mov,
            "jpg" | "jpeg" => Self::Jpeg,
            "png" => Self::Png,
            "webp" => Self::Webp,
            "avif" => Self::Avif,
            _ => Self::Unknown,
        }
    }
    /// Art, zu der das Format gehoert; None bei unbekanntem Format.
    pub fn kind(self) -> Option<MediaKind> {
        match self {
            Self::Mp3 | Self::Flac | Self::Opus | Self::Aac => Some(MediaKind::Audio),
            Self::Mp4 | Self::Webm | Self::Mkv | Self::Mov => Some(MediaKind::Video),
            Self::Jpeg | Self::Png | Self::Webp | Self::Avif => Some(MediaKind::Photo),
            Self::Unknown => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Ein Eintrag in der Mediensammlung.
pub struct MediaItem {
    /// Kennung des Eintrags.
    pub id: u64,
    /// Pfad oder URI der Datei.
    pub uri: String,
    /// Anzeigetitel.
    pub title: String,
    /// Format der Datei.
    pub format: MediaFormat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Format einer Bildfolge.
pub struct VideoFrameSpec {
    /// Breite in Pixel.
    pub width: u32,
    /// Hoehe in Pixel.
    pub height: u32,
    /// Zaehler der Bildrate.
    pub fps_num: u32,
    /// Nenner der Bildrate.
    pub fps_den: u32,
}
impl VideoFrameSpec {
    /// Prueft Groesse und Bildrate.
    pub fn validate(self) -> Result<(), MediaError> {
        if self.width == 0 || self.height == 0 || self.fps_num == 0 || self.fps_den == 0 {
            Err(MediaError::InvalidVideoSpec)
        } else {
            Ok(())
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Speicherlayout eines Bildes.
pub enum PixelFormat {
    /// Rot, Gruen, Blau, Alpha mit 8 Bit je Kanal.
    Rgba8,
    /// Blau, Gruen, Rot, Alpha mit 8 Bit je Kanal.
    Bgra8,
    /// YUV 4:2:0, planar.
    Yuv420p,
    /// YUV 4:2:0, halbplanar.
    Nv12,
}
#[derive(Debug, Clone, PartialEq, Eq)]
/// Ein Bild mit Format und Nutzdaten.
pub struct VideoFrame {
    /// Format der Bildfolge.
    pub spec: VideoFrameSpec,
    /// Speicherlayout der Nutzdaten.
    pub format: PixelFormat,
    /// Nutzdaten des Bildes.
    pub data: Vec<u8>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Format eines Fotos.
pub struct PhotoSpec {
    /// Breite in Pixel.
    pub width: u32,
    /// Hoehe in Pixel.
    pub height: u32,
    /// Speicherlayout der Nutzdaten.
    pub format: PixelFormat,
}
impl PhotoSpec {
    /// Prueft die Bildgroesse eines Fotos.
    pub fn validate(self) -> Result<(), MediaError> {
        if self.width == 0 || self.height == 0 {
            Err(MediaError::InvalidPhotoSpec)
        } else {
            Ok(())
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Drehung oder Spiegelung, die auf ein Foto anzuwenden ist.
pub enum PhotoTransform {
    /// Keine Transformation.
    None,
    /// Um 90 Grad drehen.
    Rotate90,
    /// Um 180 Grad drehen.
    Rotate180,
    /// Um 270 Grad drehen.
    Rotate270,
    /// Horizontal spiegeln.
    FlipHorizontal,
    /// Vertikal spiegeln.
    FlipVertical,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Fehler der Medienebene.
pub enum MediaError {
    /// Das Bildformat ist unbrauchbar.
    InvalidVideoSpec,
    /// Das Fotoformat ist unbrauchbar.
    InvalidPhotoSpec,
    /// Die Position liegt ausserhalb der Dauer.
    InvalidPosition,
    /// Das Format wird nicht unterstuetzt.
    UnsupportedFormat,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Zustand der Wiedergabe.
pub enum MediaState {
    /// Angehalten und am Anfang.
    Stopped,
    /// Laeuft.
    Playing,
    /// Angehalten an der aktuellen Position.
    Paused,
    /// Springt gerade.
    Seeking,
    /// Am Ende angekommen.
    Ended,
    /// Fehlerzustand.
    Error,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Position und Dauer eines Medieneintrags.
pub struct Timeline {
    position_ms: u64,
    duration_ms: Option<u64>,
}
impl Timeline {
    /// Legt eine Position bei 0 an; d ist die bekannte Dauer.
    pub fn new(d: Option<u64>) -> Self {
        Self {
            position_ms: 0,
            duration_ms: d,
        }
    }
    /// Aktuelle Position in Millisekunden.
    pub fn position_ms(&self) -> u64 {
        self.position_ms
    }
    /// Bekannte Dauer in Millisekunden.
    pub fn duration_ms(&self) -> Option<u64> {
        self.duration_ms
    }
    /// Setzt die Position; Positionen hinter der Dauer werden abgelehnt.
    pub fn seek(&mut self, p: u64) -> Result<(), MediaError> {
        if let Some(d) = self.duration_ms
            && p > d
        {
            return Err(MediaError::InvalidPosition);
        }
        self.position_ms = p;
        Ok(())
    }
}
#[derive(Debug, Default)]
/// Sammlung von Medieneintraegen.
pub struct MediaLibrary {
    items: Vec<MediaItem>,
}
impl MediaLibrary {
    /// Leere Sammlung.
    pub fn new() -> Self {
        Self::default()
    }
    /// Alle Eintraege.
    pub fn items(&self) -> &[MediaItem] {
        &self.items
    }
    /// Nimmt einen Eintrag auf.
    pub fn add(&mut self, item: MediaItem) {
        self.items.retain(|x| x.id != item.id);
        self.items.push(item);
        self.items.sort_by_key(|x| x.id)
    }
    /// Entfernt einen Eintrag; false, wenn die Kennung unbekannt ist.
    pub fn remove(&mut self, id: u64) -> bool {
        let n = self.items.len();
        self.items.retain(|x| x.id != id);
        n != self.items.len()
    }
    /// Eintrag zu einer Kennung.
    pub fn find(&self, id: u64) -> Option<&MediaItem> {
        self.items.iter().find(|x| x.id == id)
    }
}
