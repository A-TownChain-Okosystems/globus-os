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

    /* PT_LOAD permissions are applied at 4-KiB page granularity.
     * Keep writable BSS pages separate from the executable text page.
     */
    .bss : ALIGN(4096) {
        *(.bss .bss.*)
        *(COMMON)
    }

    /DISCARD/ : {
        *(.eh_frame)
        *(.comment)
    }
}
