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
 * Es gibt absichtlich kein .data und kein nutzbares .bss: ein beschreibbarer
 * statischer Wert liegt ausserhalb der abgebildeten Seiten.
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

    .bss : ALIGN(16) {
        *(.bss .bss.*)
        *(COMMON)
    }

    /DISCARD/ : {
        *(.eh_frame)
        *(.comment)
    }
}
