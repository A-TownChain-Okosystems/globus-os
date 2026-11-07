/* ShivaCore Init: feste Ladeadresse im Userspace-Fenster.
 *
 * Die Adresse muss unter der Kernelbasis bleiben und ueber der Nullseite: der
 * Kernel laedt das Programm spaeter dorthin, und ein Fehler im Programm darf
 * nicht in den Kernelbereich zeigen.
 */
ENTRY(_start)

SECTIONS
{
    . = 0x400000;

    .text : {
        *(.text._start)
        *(.text .text.*)
    }

    .rodata : ALIGN(16) {
        *(.rodata .rodata.*)
    }

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
