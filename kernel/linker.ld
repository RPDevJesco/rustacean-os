/* Rustacean OS Kernel Linker Script */
/* Kernel is loaded at 1MB (0x100000) by stage2 bootloader */

ENTRY(_start)

KERNEL_VMA = 0x100000;

SECTIONS
{
    . = KERNEL_VMA;

    /* Code section */
    .text ALIGN(4K) :
    {
        *(.text.boot)       /* Boot entry point first */
        *(.text .text.*)
    }

    /* Read-only data */
    .rodata ALIGN(4K) :
    {
        *(.rodata .rodata.*)
    }

    /* Initialized data */
    .data ALIGN(4K) :
    {
        *(.data .data.*)
    }

    /* Uninitialized data (BSS) */
    .bss ALIGN(4K) :
    {
        __bss_start = .;
        *(.bss .bss.*)
        *(COMMON)
        __bss_end = .;
    }

    /* Kernel end marker */
    . = ALIGN(4K);
    __kernel_end = .;

    /* Discard unwanted sections */
    /DISCARD/ :
    {
        *(.comment)
        *(.note*)
        *(.eh_frame*)
    }
}
