//! Hardware-neutral GPU/media buffer boundary.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BufferFormat {
    /// Rot, Gruen, Blau, Alpha mit 8 Bit je Kanal.
    Rgba8,
    /// Blau, Gruen, Rot, Alpha mit 8 Bit je Kanal.
    Bgra8,
    /// YUV 4:2:0, halbplanar.
    Nv12,
    /// YUV 4:2:0, planar.
    Yuv420p,
}

/// Ein Bildpuffer auf der GPU.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MediaBuffer {
    /// Kennung des Puffers.
    pub id: u64,
    /// Breite in Pixel.
    pub width: u32,
    /// Hoehe in Pixel.
    pub height: u32,
    /// Pufferformat.
    pub format: BufferFormat,
    /// Zeilenabstand in Byte.
    pub stride: u32,
    /// True, wenn der Puffer mit anderen Prozessen geteilt wird.
    pub shared: bool,
}

/// GPU-Pfad fuer die Medienausgabe.
pub trait MediaGpuBackend {
    /// Legt einen Puffer an.
    fn allocate(&mut self, width: u32, height: u32, format: BufferFormat) -> Option<MediaBuffer>;
    /// Gibt einen Puffer frei.
    fn release(&mut self, id: u64) -> bool;
    /// Zeigt einen Puffer zu einem Zeitpunkt.
    fn present(&mut self, buffer: MediaBuffer, timestamp_us: u64) -> bool;
}

/// Verweis auf einen geteilten Puffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SharedBufferHandle {
    /// Kennung des Puffers.
    pub id: u64,
    /// True, wenn nur gelesen werden darf.
    pub read_only: bool,
}

/// Umwandlung zwischen zwei Pufferformaten.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColorConversion {
    /// Ausgangsformat.
    pub source: BufferFormat,
    /// Zielformat.
    pub destination: BufferFormat,
}

impl ColorConversion {
    /// True, wenn fuer diese Richtung eine Umwandlung noetig ist.
    pub fn required(source: BufferFormat, destination: BufferFormat) -> bool {
        source != destination
    }
    /// True, wenn von YUV nach RGB gewandelt wird.
    pub const fn is_yuv_to_rgb(self) -> bool {
        matches!(self.source, BufferFormat::Nv12 | BufferFormat::Yuv420p)
            && matches!(self.destination, BufferFormat::Rgba8 | BufferFormat::Bgra8)
    }
}

/// Ein DMA-Puffer, wie ihn der Kernel herausgibt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DmaBufDescriptor {
    /// Dateideskriptor des Puffers.
    pub fd: i32,
    /// Groesse in Byte.
    pub size_bytes: u64,
    /// Zeilenabstand in Byte.
    pub stride: u32,
    /// Versatz des Bildanfangs im Puffer.
    pub offset: u32,
}

/// Hardware-Overlay ueber dem Bild.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HardwareOverlay {
    /// True, wenn das Overlay aktiv ist.
    pub enabled: bool,
    /// Pufferformat der Overlayquelle.
    pub source: BufferFormat,
    /// Ebene des Overlays.
    pub z_order: i32,
}

impl HardwareOverlay {
    /// True, wenn diese Form das Format als Overlay zeigen kann.
    pub const fn supports_format(format: BufferFormat) -> bool {
        matches!(
            format,
            BufferFormat::Nv12 | BufferFormat::Rgba8 | BufferFormat::Bgra8
        )
    }
}

/// Ausgabegeraet, an das Bilder gehen.
pub trait DisplayOutput {
    /// Aufloesung und Bildwiederholrate.
    fn mode(&self) -> DisplayMode;
    /// Schaltet die Synchronisation mit dem Bildaufbau.
    fn set_vsync(&mut self, enabled: bool);
    /// Stellt ein Bild zur Ausgabe an.
    fn queue_frame(&mut self, buffer: MediaBuffer, presentation_us: u64) -> bool;
}

/// Aufloesung und Bildwiederholrate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplayMode {
    /// Breite in Pixel.
    pub width: u32,
    /// Hoehe in Pixel.
    pub height: u32,
    /// Bildwiederholrate in Hertz.
    pub refresh_hz: u32,
}

/// Zeitraster der Ausgabe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameTiming {
    /// Bildwiederholrate in Hertz.
    pub refresh_hz: u32,
    /// True, wenn auf den Bildaufbau gewartet wird.
    pub vsync_enabled: bool,
}

impl FrameTiming {
    /// Zeitraster mit dieser Bildwiederholrate.
    pub const fn new(refresh_hz: u32) -> Self {
        Self {
            refresh_hz,
            vsync_enabled: true,
        }
    }
    /// Abstand zweier Bilder in Mikrosekunden; None bei Rate 0.
    pub const fn frame_interval_us(self) -> Option<u64> {
        if self.refresh_hz == 0 {
            None
        } else {
            Some(1_000_000 / self.refresh_hz as u64)
        }
    }
    /// Naechste Bildgrenze nach dem angegebenen Zeitpunkt.
    pub const fn deadline_us(self, vsync_us: u64) -> Option<u64> {
        if !self.vsync_enabled {
            None
        } else {
            match self.frame_interval_us() {
                Some(interval) => Some(vsync_us.saturating_add(interval)),
                None => None,
            }
        }
    }
}

/// Haelt die Ausgabe im Zeitraster.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FramePacer {
    /// Zeitraster.
    pub timing: FrameTiming,
    /// Zeitpunkt des zuletzt gezeigten Bildes.
    pub last_present_us: Option<u64>,
}

impl FramePacer {
    /// Taktgeber mit diesem Zeitraster.
    pub const fn new(timing: FrameTiming) -> Self {
        Self {
            timing,
            last_present_us: None,
        }
    }
    /// Zeitpunkt, zu dem das naechste Bild faellig ist.
    pub fn next_deadline(&self, now_us: u64) -> Option<u64> {
        self.timing.deadline_us(now_us)
    }
    /// Vermerkt, wann zuletzt ein Bild gezeigt wurde.
    pub fn presented(&mut self, timestamp_us: u64) {
        self.last_present_us = Some(timestamp_us);
    }
}
