# STATUS — atc-drivers

**Stand:** 2026-11-07
**Version:** v1.0.0
**Status:** FRAMEWORK + SUPERVISION — Geraetetreiber liegen im Service Space

## Was hier wirklich liegt
- `driver_framework.rs`: **Re-Export** des Treiber-Lebenszyklus. Die Definition
  liegt in `system/devices/src/driver_lifecycle.rs` (`DriverState`
  Registered/Active/Faulted/Isolated, `DriverSupervisor`, Fehlerprotokoll ueber
  `globus_diagnostics`); der Pfad `atc_drivers::driver_framework::…` bleibt
  gueltig. Der Service Space benutzt die Systemschicht direkt, nicht dieses
  Crate — ein Produktmodul darf nicht Voraussetzung der Kernschicht sein.
- `src/components/HardwareDriversView.tsx`: Anzeige.

## Was hier NICHT liegt (bewusst)
Die Geraetetreiber selbst stehen in
`modules/atc-shivacore/service_space/src/hw_drivers.rs` (AD-012: Treiber sind
Service-Space-Code, nicht Kernel- und nicht Doppelcode). `net.rs` und
`storage.rs` exportieren deshalb nichts und verweisen dorthin. `audio.rs`,
`gpu.rs`, `hid.rs`, `usb.rs` sind PLANNED — im Baum existiert dafuer keine
Implementierung.

## Abhaengigkeiten
`globus-diagnostics` (Ereignisprotokoll) und `globus-devices` (Re-Export-Ziel).
Umgekehrt haengt **nichts** an diesem Crate — die Kante zeigt nur nach unten.

## Letzte Änderungen
- 2026-11-07: Stand richtiggestellt (vorher SKELETON/INITIALIZING vom 05.08.),
  Doppelung des Treiberzustands beseitigt, Verweise auf den Service Space.
- 2026-08-05: Repo initialisiert durch Aurora Sync Agent
