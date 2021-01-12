ENTRY(_start)

STACKTOP = 0x41000000;

SECTIONS
{
    . = 0x40008000; /* Load the kernel at this address. */

    .text :
    {
        *(.text._start) *(.text*)
    }

    .rodata :
    {
        *(.rodata*)
    }

    .data :
    {
        *(.data*)
    }

    /* Section is zeroed in u64 chunks, align start and end to 8 bytes */
    .bss ALIGN(8):
    {
        __bss_start = .;
        *(.bss*);
        . = ALIGN(8);

        /* Fill for the bss == 0 case, so that __bss_start <= __bss_end_inclusive holds */
        . += 8;
        __bss_end_inclusive = . - 8;
    }
    . = STACKTOP ;
	stack_top = .;

    /DISCARD/ : { *(.comment*) }
}
