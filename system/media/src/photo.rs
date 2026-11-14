//! Photo decoding, metadata and viewer state. Codec implementations plug in through `PhotoDecoder`.

use crate::{MediaError, PhotoTransform, PixelFormat};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Farbraum eines Fotos.
pub enum ColorSpace {
    /// sRGB.
    Srgb,
    /// Display P3.
    DisplayP3,
    /// Rec. 2020.
    Rec2020,
    /// Unbekannter Farbraum.
    /// Unbekannter Kodek.
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Fotokodek.
pub enum PhotoCodec {
    /// JPEG.
    Jpeg,
    /// PNG.
    Png,
    /// WebP.
    Webp,
    /// AVIF.
    Avif,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Ausrichtung aus den EXIF-Daten.
pub enum ExifOrientation {
    /// Unveraendert.
    Normal,
    /// Horizontal gespiegelt.
    FlipHorizontal,
    /// Um 180 Grad gedreht.
    Rotate180,
    /// Vertikal gespiegelt.
    FlipVertical,
    /// An der Hauptdiagonalen gespiegelt.
    Transpose,
    /// Um 90 Grad im Uhrzeigersinn gedreht.
    Rotate90,
    /// An der Nebendiagonalen gespiegelt.
    Transverse,
    /// Um 270 Grad im Uhrzeigersinn gedreht.
    Rotate270,
}

impl ExifOrientation {
    /// Transformation, die diese Ausrichtung auf das Bild anwendet.
    pub const fn transform(self) -> PhotoTransform {
        match self {
            Self::Normal => PhotoTransform::None,
            Self::Rotate90 => PhotoTransform::Rotate90,
            Self::Rotate180 => PhotoTransform::Rotate180,
            Self::Rotate270 => PhotoTransform::Rotate270,
            Self::FlipHorizontal | Self::FlipVertical | Self::Transpose | Self::Transverse => {
                PhotoTransform::None
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Was ueber ein Foto bekannt ist.
pub struct PhotoInfo {
    /// Breite in Pixel.
    pub width: u32,
    /// Hoehe in Pixel.
    pub height: u32,
    /// Farbraum.
    pub color_space: ColorSpace,
    /// Anzuwendende Transformation.
    pub transform: PhotoTransform,
    /// Kodek der Datei.
    pub codec: PhotoCodec,
    /// Ausrichtung laut EXIF.
    pub orientation: ExifOrientation,
}

impl PhotoInfo {
    /// Prueft Groesse und Farbraumangaben.
    pub fn validate(self) -> Result<(), MediaError> {
        if self.width == 0 || self.height == 0 {
            Err(MediaError::InvalidPhotoSpec)
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Ein dekodiertes Foto.
pub struct PhotoFrame {
    /// Angaben zum Foto.
    pub info: PhotoInfo,
    /// Speicherlayout der Nutzdaten.
    pub format: PixelFormat,
    /// Nutzdaten des Bildes.
    pub data: Vec<u8>,
}

/// Ein Fotodekoder.
pub trait PhotoDecoder {
    /// Angaben zum Foto.
    fn info(&self) -> PhotoInfo;
    /// Dekodiert das Foto vollstaendig.
    fn decode(&mut self) -> Result<PhotoFrame, MediaError>;
}

/// Fotodekoder, der schrittweise liefert.
pub trait ProgressivePhotoDecoder: PhotoDecoder {
    /// Naechster Schritt der Dekodierung, oder None am Ende.
    fn decode_next(&mut self) -> Result<Option<PhotoFrame>, MediaError>;
    /// True, wenn das ganze Bild vorliegt.
    fn is_complete(&self) -> bool;
}

#[derive(Debug, Clone, Copy, PartialEq)]
/// Ausschnitt und Drehung der Fotoansicht.
pub struct PhotoViewport {
    /// Vergroesserung.
    pub zoom: f32,
    /// Verschiebung auf der x-Achse.
    pub pan_x: f32,
    /// Verschiebung auf der y-Achse.
    pub pan_y: f32,
    /// Drehung in Grad.
    pub rotation: u16,
}

impl Default for PhotoViewport {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            pan_x: 0.0,
            pan_y: 0.0,
            rotation: 0,
        }
    }
}

#[derive(Debug, Default)]
/// Ansicht eines Fotos mit Ausschnitt und Drehung.
pub struct PhotoViewer {
    frame: Option<PhotoFrame>,
    viewport: PhotoViewport,
}

impl PhotoViewer {
    /// Ansicht ohne geladenes Bild.
    pub fn new() -> Self {
        Self::default()
    }
    /// Das geladene Bild.
    pub fn frame(&self) -> Option<&PhotoFrame> {
        self.frame.as_ref()
    }
    /// Aktueller Ausschnitt.
    pub fn viewport(&self) -> PhotoViewport {
        self.viewport
    }
    /// Dekodiert ueber den angegebenen Dekoder und setzt die Ansicht zurueck.
    pub fn load<D: PhotoDecoder>(&mut self, decoder: &mut D) -> Result<(), MediaError> {
        let frame = decoder.decode()?;
        frame.info.validate()?;
        self.frame = Some(frame);
        self.reset();
        Ok(())
    }
    /// Setzt Ausschnitt und Drehung auf den Ausgangszustand.
    pub fn reset(&mut self) {
        self.viewport = PhotoViewport::default();
    }
    /// Setzt die Vergroesserung und lehnt unbrauchbare Werte ab.
    pub fn set_zoom(&mut self, zoom: f32) -> Result<(), MediaError> {
        if !zoom.is_finite() || !(0.1..=16.0).contains(&zoom) {
            return Err(MediaError::InvalidPosition);
        }
        self.viewport.zoom = zoom;
        Ok(())
    }
    /// Verschiebt den Ausschnitt.
    pub fn pan(&mut self, dx: f32, dy: f32) {
        self.viewport.pan_x += dx;
        self.viewport.pan_y += dy;
    }
    /// Dreht die Ansicht um eine Vierteldrehung.
    pub fn rotate_quarter_turn(&mut self, clockwise: bool) {
        self.viewport.rotation = if clockwise {
            (self.viewport.rotation + 90) % 360
        } else {
            (self.viewport.rotation + 270) % 360
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct D;
    impl PhotoDecoder for D {
        fn info(&self) -> PhotoInfo {
            PhotoInfo {
                width: 2,
                height: 2,
                color_space: ColorSpace::Srgb,
                transform: PhotoTransform::None,
                codec: PhotoCodec::Png,
                orientation: ExifOrientation::Normal,
            }
        }
        fn decode(&mut self) -> Result<PhotoFrame, MediaError> {
            Ok(PhotoFrame {
                info: self.info(),
                format: PixelFormat::Rgba8,
                data: vec![0; 16],
            })
        }
    }
    #[test]
    fn viewer_controls_are_bounded() {
        let mut v = PhotoViewer::new();
        let mut d = D;
        v.load(&mut d).unwrap();
        assert!(v.set_zoom(16.1).is_err());
        v.rotate_quarter_turn(true);
        assert_eq!(v.viewport().rotation, 90);
    }
}
