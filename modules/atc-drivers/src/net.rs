// # Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
//! `net` — kein eigener Code. Die Implementierung liegt im Service Space:
//! `modules/atc-shivacore/service_space/src/hw_drivers.rs` (`VirtioNetDevice` mit eingerahmten Frames ueber `NetworkDevice`), weil Treiber
//! laut AD-012 dorthin gehoeren und nicht in eine zweite Kopie. Dieses Modul bleibt
//! als Platzhalter des Modul-Crates bestehen und exportiert bewusst nichts.
