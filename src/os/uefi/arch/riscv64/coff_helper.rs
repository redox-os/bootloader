use core::arch::{global_asm, naked_asm};

/// Unfortunately this can't be written in Rust because it might use some not-yet
/// relocated data such as jump tables
#[unsafe(naked)]
#[unsafe(no_mangle)]
extern "C" fn coff_relocate(dynentry: *const u8, base: usize) -> usize {
    unsafe {
        naked_asm!(
            "
    mv   t4, zero // RELA
    li   t5, -1   // RELASZ
    li   t6, -1   // RELAENT

5:
    ld   t0, 0(a0)
    beqz t0, 6f
    addi a0, a0, 16
    addi t0, t0, -4
    bltz t0, 3f     // fail on DT_NEEDED=1, DT_PLTRELSZ=2,  DT_PLTGOT=3
    addi t0, t0, -3
    bltz t0, 5b     // skip DT_HASH=4, DT_STRTAB=5, DT_SYMTAB=6
    bnez t0, 2f
    ld   t4, -8(a0) // DT_RELA=7
    j    5b
2:  addi t0, t0, -1 // DT_RELASZ=8
    bnez t0, 2f
    ld   t5, -8(a0)
    j    5b
2:  addi t0, t0, -1 // DT_RELAENT=9
    bnez t0, 2f
    ld   t6, -8(a0)
    j    5b
2:  addi t0, t0, -3
    bltz t0, 5b     // skip  DT_STRSZ=10, DT_SYMENT=11
    addi t0, t0, -2
    bltz t0, 3f     // fail on DT_INIT=12, DT_FINI=13
    beqz t0, 5b     // skip DT_SONAME=14
2:  addi t0, t0, -2
    bltz t0, 3f     // fail on DT_RPATH
    beqz t0, 5b     // skip SYMBOLIC=16
    li   t1, 0x6ffffef5-16
    sub  t0, t0, t1
    beqz t0, 5b     // skip DT_GNU_HASH=0x6ffffef5
    nop
3:  // error
    mv   a0, zero
    ret

6:
    bnez t4, 2f
4: // success
    li   a0, 1
    ret
2:  bltz t5, 3b
    blez t6, 3b

    add  t4, t4, a1
    add  t5, t5, t4
7:
    bge  t4, t5, 4b
    ld   t0, 0(t4)   // r_offset
    add  t0, t0, a1
    lwu  t1, 8(t4)   // r_type
    ld   t2, 16(t4)  // r_addend
    add  t4, t4, t6
    addi t1, t1, -3  // R_RISCV_RELATIVE=3
    bnez t1, 3b
    add  t2, t2, a1  // RELATIVE: *value = base + addend
    sd   t2, 0(t0)
    j    7b
    "
        )
    }
}

global_asm!(
    r#"
   .global coff_start
coff_start:
    .option norelax
    addi sp, sp, -24
    sd   a0, 0(sp)
    sd   a1, 8(sp)
    sd   ra, 16(sp)
    lla  a0, _DYNAMIC
    lla  a1, ImageBase  // actual loaded image base to relocate to
    jal  coff_relocate
    .option relax
    mv   t0, a0
    ld   a0, 0(sp)
    ld   a1, 8(sp)
    ld   ra, 16(sp)
    addi sp, sp, 24
    beqz t0, 2f
    j    efi_main
2:  ret
"#
);

// GNU-EFI .reloc trick to make objcopy say we are relocatable
global_asm!(
    r#"
    .section .data
    DUMMY_RELOCATION: .4byte 0
    .section .reloc, "a"

2:
    .4byte DUMMY_RELOCATION - ImageBase
    .4byte 12
    .4byte 0
"#
);
