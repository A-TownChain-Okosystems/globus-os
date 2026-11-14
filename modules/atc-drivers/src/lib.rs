//! Hardware-Treiber — USB, GPU, Audio, SATA/NVMe, HID, Network
//!
//! Part of the A-TownChain-Okosystems ecosystem.
//! Copyright (c) Michael Wroblewski. All Rights Reserved.

#![no_std]

/// Audio-Subsystem. Geplant, im Baum noch nicht implementiert.
pub mod audio;
/// Re-Export des Treiber-Lebenszyklus aus dem Service Space.
pub mod driver_framework;
/// Grafik-/GPU-Subsystem. Geplant, im Baum noch nicht implementiert.
pub mod gpu;
/// Eingabegeraete (HID). Geplant, im Baum noch nicht implementiert.
pub mod hid;
/// Netzwerktreiber. Implementierung liegt im Service Space (AD-012).
pub mod net;
/// Block-Speichertreiber. Implementierung liegt im Service Space (AD-012).
pub mod storage;
/// USB-Subsystem. Geplant, im Baum noch nicht implementiert.
pub mod usb;
