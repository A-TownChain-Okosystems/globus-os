//! Unified media model for audio, video, and photos.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKind { Audio, Video, Photo }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaFormat { Mp3, Flac, Opus, Aac, Mp4, Webm, Mkv, Mov, Jpeg, Png, Webp, Avif, Unknown }

impl MediaFormat {
    pub fn from_extension(extension: &str) -> Self {
        match extension.trim_start_matches('.').to_ascii_lowercase().as_str() {
            "mp3" => Self::Mp3, "flac" => Self::Flac, "opus" => Self::Opus, "aac" => Self::Aac,
            "mp4" => Self::Mp4, "webm" => Self::Webm, "mkv" => Self::Mkv, "mov" => Self::Mov,
            "jpg" | "jpeg" => Self::Jpeg, "png" => Self::Png, "webp" => Self::Webp, "avif" => Self::Avif,
            _ => Self::Unknown,
        }
    }

    pub fn kind(self) -> MediaKind {
        match self {
            Self::Mp3 | Self::Flac | Self::Opus | Self::Aac => MediaKind::Audio,
            Self::Mp4 | Self::Webm | Self::Mkv | Self::Mov => MediaKind::Video,
            Self::Jpeg | Self::Png | Self::Webp | Self::Avif | Self::Unknown => MediaKind::Photo,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaItem {
    pub id: u64,
    pub uri: String,
    pub title: String,
    pub format: MediaFormat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VideoFrameSpec { pub width: u32, pub height: u32, pub fps_num: u32, pub fps_den: u32 }

impl VideoFrameSpec {
    pub fn validate(self) -> Result<(), MediaError> {
        if self.width == 0 || self.height == 0 || self.fps_num == 0 || self.fps_den == 0 { return Err(MediaError::InvalidVideoSpec); }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat { Rgba8, Bgra8, Yuv420p, Nv12 }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoFrame { pub spec: VideoFrameSpec, pub format: PixelFormat, pub data: Vec<u8> }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhotoSpec { pub width: u32, pub height: u32, pub format: PixelFormat }

impl PhotoSpec {
    pub fn validate(self) -> Result<(), MediaError> {
        if self.width == 0 || self.height == 0 { return Err(MediaError::InvalidPhotoSpec); }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhotoTransform { None, Rotate90, Rotate180, Rotate270, FlipHorizontal, FlipVertical }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaError { InvalidVideoSpec, InvalidPhotoSpec, InvalidPosition, UnsupportedFormat }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaState { Stopped, Playing, Paused, Seeking, Ended, Error }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timeline { position_ms: u64, duration_ms: Option<u64> }

impl Timeline {
    pub fn new(duration_ms: Option<u64>) -> Self { Self { position_ms: 0, duration_ms } }
    pub fn position_ms(&self) -> u64 { self.position_ms }
    pub fn duration_ms(&self) -> Option<u64> { self.duration_ms }
    pub fn seek(&mut self, position_ms: u64) -> Result<(), MediaError> {
        if let Some(duration) = self.duration_ms { if position_ms > duration { return Err(MediaError::InvalidPosition); } }
        self.position_ms = position_ms;
        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct MediaLibrary { items: Vec<MediaItem> }

impl MediaLibrary {
    pub fn new() -> Self { Self::default() }
    pub fn items(&self) -> &[MediaItem] { &self.items }
    pub fn add(&mut self, item: MediaItem) { self.items.retain(|existing| existing.id != item.id); self.items.push(item); self.items.sort_by_key(|item| item.id); }
    pub fn remove(&mut self, id: u64) -> bool { let old = self.items.len(); self.items.retain(|item| item.id != id); old != self.items.len() }
    pub fn find(&self, id: u64) -> Option<&MediaItem> { self.items.iter().find(|item| item.id == id) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn detects_media_kind() { assert_eq!(MediaFormat::from_extension("movie.MP4").kind(), MediaKind::Video); }
    #[test]
    fn timeline_rejects_out_of_range_seek() { let mut t = Timeline::new(Some(1000)); assert_eq!(t.seek(1001), Err(MediaError::InvalidPosition)); }
}
