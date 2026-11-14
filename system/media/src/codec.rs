//! Codec backend contracts and GlobusOS free-media policy.
//!
//! The mandatory GlobusOS media core deliberately exposes only codecs that
//! are intended for royalty-free/open deployment. H.264/AVC and HEVC/H.265
//! are not part of the default codec API because their patent/licensing
//! situation must not become a mandatory cost or legal dependency.

use crate::{MediaError, PixelFormat, VideoFrame, VideoFrameSpec};

/// Video codecs allowed in the mandatory, no-fee GlobusOS media core.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoCodec {
    /// AOMedia AV1, subject to the AOMedia Patent License 1.0 conditions.
    Av1,
    /// VP9, for which GlobusOS may use a free/open implementation.
    Vp9,
}

impl VideoCodec {
    /// True, wenn der Codec ohne Lizenzkosten im Kern genutzt werden darf.
    pub const fn is_core_free(self) -> bool {
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Art des Dekoders.
pub enum DecoderKind {
    /// Dekodierung auf der CPU.
    Software,
    /// Hardware-Dekodierung ueber VA-API.
    VaApi,
    /// Hardware-Dekodierung auf AMD-Hardware.
    Amd,
    /// Hardware-Dekodierung ueber einen allgemeinen GPU-Pfad.
    GenericGpu,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Was ein Dekoder leisten kann.
pub struct DecoderCapabilities {
    /// Codec, den der Dekoder versteht.
    pub codec: VideoCodec,
    /// Art des Dekoders.
    pub kind: DecoderKind,
    /// Pixelformat der Ausgabe.
    pub output: PixelFormat,
    /// True, wenn das Bild ohne Kopie weitergereicht wird.
    pub zero_copy: bool,
}

/// Ein Videodekoder.
pub trait VideoCodecDecoder {
    /// Codec, den der Dekoder versteht.
    /// Codec, den der Dekoder versteht.
    fn codec(&self) -> VideoCodec;
    /// Faehigkeiten des Dekoders.
    fn capabilities(&self) -> DecoderCapabilities;
    /// Format der erzeugten Bilder.
    fn spec(&self) -> VideoFrameSpec;
    /// Gibt einen Datenblock zum Dekodieren.
    fn submit(
        &mut self,
        packet: &[u8],
        pts_us: u64,
        dts_us: u64,
        keyframe: bool,
    ) -> Result<(), MediaError>;
    /// Holt das naechste fertige Bild, falls eines vorliegt.
    fn receive(&mut self) -> Result<Option<TimedVideoFrame>, MediaError>;
    /// Setzt die Wiedergabeposition.
    fn seek(&mut self, position_us: u64) -> Result<(), MediaError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Ein dekodiertes Bild mit Zeitstempeln.
pub struct TimedVideoFrame {
    /// Das Bild selbst.
    pub frame: VideoFrame,
    /// Praesentationszeitpunkt in Mikrosekunden.
    pub pts_us: u64,
    /// Dekodierzeitpunkt in Mikrosekunden.
    pub dts_us: u64,
    /// True, wenn das Bild ein Keyframe ist.
    pub keyframe: bool,
}

/// Ein Fotodekoder.
pub trait PhotoCodecDecoder {
    /// Codec, den der Dekoder versteht.
    fn codec(&self) -> crate::photo::PhotoCodec;
    /// Dekodiert schrittweise, damit grosse Bilder den Speicher nicht sprengen.
    fn decode_incremental(
        &mut self,
        input: &[u8],
        final_chunk: bool,
    ) -> Result<Option<crate::PhotoFrame>, MediaError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Abbildung eines Bildes auf eine Zielgroesse.
pub struct VideoScaling {
    /// Breite der Quelle in Pixel.
    pub source_width: u32,
    /// Hoehe der Quelle in Pixel.
    pub source_height: u32,
    /// Zielbreite in Pixel.
    pub target_width: u32,
    /// Zielhoehe in Pixel.
    pub target_height: u32,
}

impl VideoScaling {
    /// Prueft, ob Quell- und Zielgroesse brauchbar sind.
    pub fn validate(self) -> Result<(), MediaError> {
        if self.source_width == 0
            || self.source_height == 0
            || self.target_width == 0
            || self.target_height == 0
        {
            Err(MediaError::InvalidVideoSpec)
        } else {
            Ok(())
        }
    }
}
