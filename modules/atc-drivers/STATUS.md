# STATUS — atc-drivers

**Stand:** 2026-11-07
**Version:** v1.0.0
**Status:** FRAMEWORK + SUPERVISION — Geraetetreiber liegen im Service Space

## Was hier wirklich liegt
- `driver_framework.rs`: Lebenszyklus der Treiber (`DriverState`
  Registered/Active/Faulted/Isolated, `DriverSupervisor`, Fehlerprotokoll ueber
  `globus_diagnostics`). Das ist die EINZIGE Stelle, an der Treiberzustand
  definiert ist — der Service Space benutzt sie (Abhaengigkeit
  `atc_drivers` in `service_space/Cargo.toml`).
- `src/components/HardwareDriversView.tsx`: Anzeige.

## Was hier NICHT liegt (bewusst)
Die Geraetetreiber selbst stehen in
`modules/atc-shivacore/service_space/src/hw_drivers.rs` (AD-012: Treiber sind
Service-Space-Code, nicht Kernel- und nicht Doppelcode). `net.rs` und
`storage.rs` exportieren deshalb nichts und verweisen dorthin. `audio.rs`,
`gpu.rs`, `hid.rs`, `usb.rs` sind PLANNED — im Baum existiert dafuer keine
Implementierung.

## Abhaengigkeiten
`globus-diagnostics` (Ereignisprotokoll). Umgekehrt haengt
`shivacore-service-space` an diesem Crate.

## Letzte Änderungen
- 2026-11-07: Stand richtiggestellt (vorher SKELETON/INITIALIZING vom 05.08.),
  Doppelung des Treiberzustands beseitigt, Verweise auf den Service Space.
- 2026-08-05: Repo initialisiert durch Aurora Sync Agent
