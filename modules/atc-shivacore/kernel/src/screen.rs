// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
// ShivaCore — Bildschirm-Rechnung fuer die Framebuffer-Konsole (L-7).
//
// Der Kernel loggt bisher nur seriell. Auf echter Hardware fehlt der Port, und
// ein Fehlschlag ist dann ein schwarzer Bildschirm. Dieses Modul rechnet die
// Cursorpositionen dafuer aus — ohne Hardware, damit Umbruch und Scrollgrenze
// pruefbar sind statt "sieht im Screenshot okay aus".

/// Naechste Position nach einem Zeichen fester Breite.
/// Rueckgabe: (naechste_x, naechste_y, scroll_noetig)
pub fn naechste_position(
    x: usize,
    y: usize,
    zeichen_breite: usize,
    zeichen_hoehe: usize,
    durchschuss: usize,
    breite: usize,
    hoehe: usize,
) -> (usize, usize, bool) {
    if x + zeichen_breite >= breite {
        let (neu_y, scroll) = zeilen_vorschub(y, zeichen_hoehe, durchschuss, hoehe);
        return (0, neu_y, scroll);
    }
    (x + zeichen_breite, y, false)
}

/// Naechste Zeilenposition. Liegt die Zeile nicht mehr vollstaendig auf dem
/// Bildschirm, meldet die Funktion Scrollbedarf und behaelt y bei; der
/// Aufrufer schiebt dann den Inhalt und setzt den Cursor auf die letzte Zeile.
pub fn zeilen_vorschub(
    y: usize,
    zeichen_hoehe: usize,
    durchschuss: usize,
    hoehe: usize,
) -> (usize, bool) {
    let naechste = y + zeichen_hoehe + durchschuss;
    if naechste + zeichen_hoehe > hoehe {
        (y, true)
    } else {
        (naechste, false)
    }
}

#[cfg(test)]
mod tests {
    use super::{naechste_position, zeilen_vorschub};

    const CW: usize = 8;
    const CH: usize = 16;
    const DS: usize = 2;

    #[test]
    fn zeichen_passt_setzt_cursor_weiter() {
        assert_eq!(naechste_position(0, 0, CW, CH, DS, 1280, 720), (8, 0, false));
    }

    #[test]
    fn zeichen_am_rechten_rand_bricht_um() {
        assert_eq!(naechste_position(1272, 0, CW, CH, DS, 1280, 720), (0, 18, false));
    }

    #[test]
    fn letzte_zeile_am_boden_fordert_scroll() {
        assert_eq!(naechste_position(1272, 702, CW, CH, DS, 1280, 720), (0, 702, true));
    }

    #[test]
    fn vorletzte_zeile_bricht_noch_ohne_scroll_um() {
        assert_eq!(naechste_position(1272, 684, CW, CH, DS, 1280, 720), (0, 702, false));
    }

    #[test]
    fn zeilenanfang_der_letzten_zeile_bleibt_stehen() {
        assert_eq!(naechste_position(0, 702, CW, CH, DS, 1280, 720), (8, 702, false));
    }

    #[test]
    fn vorschub_auf_normaler_zeile_ohne_scroll() {
        assert_eq!(zeilen_vorschub(0, CH, DS, 720), (18, false));
    }

    #[test]
    fn vorschub_an_der_kante_meldet_scroll() {
        assert_eq!(zeilen_vorschub(700, CH, DS, 720), (700, true));
    }

    #[test]
    fn bildschirm_kleiner_als_eine_zeile_bleibt_stabil() {
        assert_eq!(zeilen_vorschub(0, CH, DS, 10), (0, true));
    }
}
