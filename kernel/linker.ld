ENTRY(openos_kernel_entry)

SECTIONS {
  . = 1M;

  .text : {
    *(.start)
    *(.text .text.*)
  }

  .rodata : {
    *(.rodata .rodata.*)
  }

  .data : {
    *(.data .data.*)
  }

  .bss : {
    *(.bss .bss.* COMMON)
  }
}
