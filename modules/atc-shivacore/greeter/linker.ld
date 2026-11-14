/* ShivaCore Anmeldefenster: feste Ladeadresse im vierten Userspace-Fenster.
 *
 * Der Kernel kopiert genau EIN ausfuehrbares Segment an die Basis
 * (0x01C0_0000, siehe main.rs "vierter Prozess") und bildet dafuer genau so
 * viele Seiten ab, wie das Segment Bytes hat. Deshalb liegt alles, was zur
 * Laufzeit erreichbar sein muss, in diesem einen Abschnitt: .rodata fuer die
 * Texte und .got fuer die Sprungziele, die der Compiler ueber eine Tabelle
 * aufloest (sonst zeigt der Aufruf ins Leere — genau das ist passiert, als
 * .got hier verworfen wurde).
 *
 * Seit 14.11.2026 traegt der Kernel auch das Datensegment mit (.data plus
 * bss-Nullen), deshalb liegen beschreibbare statische Werte jetzt im zweiten
 * Segment bei 0x1D00000 — genau der Wert, an den der Kernel das Datensegment
 * kopiert (main.rs, "vierter Prozess"). Vorher war das verboten, weil nur eine
 * ausfuehrbare Seite abgebildet wurde.
 */
ENTRY(_start)

SECTIONS
{
    . = 0x1C00000;

    .text : {
        *(.text._start)
        *(.text .text.*)
        *(.rodata .rodata.*)
        *(.data.rel.ro .data.rel.ro.*)
        *(.got .got.*)
    }

    . = 0x1D00000;

    .data : ALIGN(16) {
        *(.data .data.*)
    }

    .bss : ALIGN(16) {
        *(.bss .bss.*)
        *(COMMON)
    }

    /DISCARD/ : {
        *(.eh_frame)
        *(.comment)
    }
}
