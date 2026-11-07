// # Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
//! `driver_framework` — kein eigener Code mehr, sondern ein Re-Export.
//!
//! Der Treiber-Lebenszyklus (`DriverState`, `DriverRecord`, `DriverSupervisor`)
//! liegt in `system/devices/src/driver_lifecycle.rs`, weil er eine Systemvertrags-
//! Frage ist und der Service Space ihn benutzt. Produktmodule duerfen auf die
//! Systemschicht zeigen — die Gegenrichtung war eine Architekturverletzung.
//!
//! Der Pfad `atc_drivers::driver_framework::…` bleibt damit gueltig.
pub use globus_devices::driver_lifecycle::{
    Driver, DriverError, DriverRecord, DriverState, DriverSupervisor,
};
