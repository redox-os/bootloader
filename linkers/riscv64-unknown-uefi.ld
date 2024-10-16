OUTPUT_FORMAT("elf64-littleriscv", "elf64-littleriscv", "elf64-littleriscv")
OUTPUT_ARCH(riscv)
ENTRY(coff_start)
SECTIONS
{
  PROVIDE(ImageBase = .);
  . = SEGMENT_START("text-segment", 0) + SIZEOF_HEADERS;
  .note.gnu.build-id  : { *(.note.gnu.build-id) }
  .hash           : { *(.hash) *(.gnu.hash) }

  . = ALIGN(4096);
  .text :
  {
    PROVIDE(_text = .);
    *(.text.unlikely .text.*_unlikely .text.unlikely.*)
    *(.text.exit .text.exit.*)
    *(.text.startup .text.startup.*)
    *(.text.hot .text.hot.*)
    *(SORT(.text.sorted.*))
    *(.text .stub .text.* .gnu.linkonce.t.*)
    /* .gnu.warning sections are handled specially by elf.em.  */
    *(.gnu.warning)
  }
  PROVIDE (__etext = .);
  PROVIDE (_etext = .);
  PROVIDE (etext = .);
  . = ALIGN(4096);

  .rdata :
   {
     *(.rodata .rodata.* .gnu.linkonce.r.*)
     *(.rodata1)
     KEEP (*(.eh_frame))
     *(.eh_frame.*)
     *(.dynamic)
   }
  . = ALIGN(4096);
  .data :
  {
    *(.got) *(.igot)
    *(.got.plt) *(.igot.plt)
    *(.data .data.* .gnu.linkonce.d.*)
    *(.data1)
    *(.srodata.cst16) *(.srodata.cst8) *(.srodata.cst4) *(.srodata.cst2) *(.srodata .srodata.*)
    *(.sdata2 .sdata2.* .gnu.linkonce.s2.*)
    *(.sdata .sdata.* .gnu.linkonce.s.*)
  PROVIDE (_edata = .); PROVIDE (edata = .);
  . = ALIGN(4096);
  PROVIDE (__bss_start = .);
    *(.sbss2 .sbss2.* .gnu.linkonce.sb2.*)
    *(.dynsbss)
    *(.sbss .sbss.* .gnu.linkonce.sb.*)
    *(.scommon)
   *(.dynbss)
   *(.bss .bss.* .gnu.linkonce.b.*)
   *(COMMON)
  . = ALIGN(4096);
  }
  .reloc :
  {
    KEEP(*(.reloc*))
  }
  .rela :
   {
     *(.rela.*)
   }
  .data.rel.ro :
   {
     *(.data.rel.ro.local* .gnu.linkonce.d.rel.ro.local.*)
     *(.data.rel.ro .data.rel.ro.* .gnu.linkonce.d.rel.ro.*)
   }
  . = ALIGN(4096);

  .dynsym         : { *(.dynsym) }
  .dynstr         : { *(.dynstr) }

   /DISCARD/ : { *(.note.GNU-stack) *(.gnu_debuglink) *(.gnu.lto_*) }
}
