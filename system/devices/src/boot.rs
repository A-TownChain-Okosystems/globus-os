//! Boot hand-off contracts for UEFI/firmware memory discovery.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Art eines Speicherbereichs, wie die Firmware ihn meldet.
pub enum MemoryKind {
    /// Frei nutzbar.
    Usable,
    /// Reserviert, nicht nutzen.
    Reserved,
    /// ACPI-Speicher, nach dem Auslesen nutzbar.
    AcpiReclaimable,
    /// ACPI-Speicher, der erhalten bleiben muss.
    AcpiNvs,
    /// Speicherabbildung von Geraeten.
    Mmio,
    /// Portabbildung von Geraeten.
    MmioPort,
    /// Speicher der Boot-Services, nach dem Hand-off frei.
    BootServices,
    /// Speicher der Laufzeit-Services, bleibt belegt.
    RuntimeServices,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Ein Speicherbereich aus der Firmware-Karte.
pub struct MemoryRegion {
    /// Physische Startadresse.
    pub physical_start: u64,
    /// Anzahl Seiten zu 4 KiB.
    pub page_count: u64,
    /// Art des Bereichs.
    pub kind: MemoryKind,
    /// True, wenn der Bereich cachebar ist.
    pub cacheable: bool,
}

impl MemoryRegion {
    /// Ende des Bereichs exklusiv; None bei Ueberlauf.
    pub fn end_exclusive(&self) -> Option<u64> {
        self.page_count
            .checked_mul(4096)?
            .checked_add(self.physical_start)
    }
    /// True, wenn der Bereich fuer DMA geeignet ist.
    pub fn is_dma_safe(&self) -> bool {
        matches!(self.kind, MemoryKind::Usable) && self.cacheable
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Bildpuffer, den die Firmware hinterlaesst.
pub struct Framebuffer {
    /// Physische Basisadresse.
    pub base: u64,
    /// Groesse in Byte.
    pub size: u64,
    /// Breite in Pixel.
    pub width: u32,
    /// Hoehe in Pixel.
    pub height: u32,
    /// Zeilenabstand in Pixel.
    pub stride: u32,
    /// Bytes je Pixel.
    pub bytes_per_pixel: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Was die Firmware an das Betriebssystem uebergibt.
pub struct BootInfo<'a> {
    /// Speicherkarte.
    pub memory: &'a [MemoryRegion],
    /// Bildpuffer, falls vorhanden.
    pub framebuffer: Option<Framebuffer>,
    /// Physische Adresse der RSDP, falls ACPI vorhanden ist.
    pub rsdp: Option<u64>,
}

/// True, wenn die Angaben des Bildpuffers zusammenpassen.
pub fn validate_framebuffer(fb: Framebuffer) -> bool {
    fb.width != 0
        && fb.height != 0
        && fb.bytes_per_pixel >= 1
        && fb.bytes_per_pixel <= 8
        && fb.stride >= fb.width.saturating_mul(u32::from(fb.bytes_per_pixel))
        && fb.size >= u64::from(fb.stride).saturating_mul(u64::from(fb.height))
}
