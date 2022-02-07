SECTION .text ; cannot use .data

struc GDTEntry
    .limitl resw 1
    .basel resw 1
    .basem resb 1
    .attribute resb 1
    .flags__limith resb 1
    .baseh resb 1
endstruc

gdt_attr:
    .present              equ 1 << 7
    .ring1                equ 1 << 5
    .ring2                equ 1 << 6
    .ring3                equ 1 << 5 | 1 << 6
    .user                 equ 1 << 4
;user
    .code                 equ 1 << 3
;   code
    .conforming           equ 1 << 2
    .readable             equ 1 << 1
;   data
    .expand_down          equ 1 << 2
    .writable             equ 1 << 1
    .accessed             equ 1 << 0
;system
;   legacy
    .tssAvailabe16        equ 0x1
    .ldt                  equ 0x2
    .tssBusy16            equ 0x3
    .call16               equ 0x4
    .task                 equ 0x5
    .interrupt16          equ 0x6
    .trap16               equ 0x7
    .tssAvailabe32        equ 0x9
    .tssBusy32            equ 0xB
    .call32               equ 0xC
    .interrupt32          equ 0xE
    .trap32               equ 0xF
;   long mode
    .ldt32                equ 0x2
    .tssAvailabe64        equ 0x9
    .tssBusy64            equ 0xB
    .call64               equ 0xC
    .interrupt64          equ 0xE
    .trap64               equ 0xF

gdt_flag:
    .granularity equ 1 << 7
    .available equ 1 << 4
;user
    .default_operand_size equ 1 << 6
;   code
    .long_mode equ 1 << 5
;   data
    .reserved equ 1 << 5

gdtr:
    dw gdt.end + 1  ; size
    dq gdt          ; offset

gdt:
.null equ $ - gdt
    dq 0

.lm64_code equ $ - gdt
istruc GDTEntry
    at GDTEntry.limitl, dw 0
    at GDTEntry.basel, dw 0
    at GDTEntry.basem, db 0
    at GDTEntry.attribute, db gdt_attr.present | gdt_attr.user | gdt_attr.code
    at GDTEntry.flags__limith, db gdt_flag.long_mode
    at GDTEntry.baseh, db 0
iend

.lm64_data equ $ - gdt
istruc GDTEntry
    at GDTEntry.limitl, dw 0
    at GDTEntry.basel, dw 0
    at GDTEntry.basem, db 0
; AMD System Programming Manual states that the writeable bit is ignored in long mode, but ss can not be set to this descriptor without it
    at GDTEntry.attribute, db gdt_attr.present | gdt_attr.user | gdt_attr.writable
    at GDTEntry.flags__limith, db 0
    at GDTEntry.baseh, db 0
iend

.pm32_code equ $ - gdt
    istruc GDTEntry
        at GDTEntry.limitl, dw 0xFFFF
        at GDTEntry.basel, dw 0
        at GDTEntry.basem, db 0
        at GDTEntry.attribute, db gdt_attr.present | gdt_attr.user | gdt_attr.code | gdt_attr.readable
        at GDTEntry.flags__limith, db 0xF | gdt_flag.granularity | gdt_flag.default_operand_size
        at GDTEntry.baseh, db 0
    iend

.pm32_data equ $ - gdt
    istruc GDTEntry
        at GDTEntry.limitl, dw 0xFFFF
        at GDTEntry.basel, dw 0
        at GDTEntry.basem, db 0
        at GDTEntry.attribute, db gdt_attr.present | gdt_attr.user | gdt_attr.writable
        at GDTEntry.flags__limith, db 0xF | gdt_flag.granularity | gdt_flag.default_operand_size
        at GDTEntry.baseh, db 0
    iend

.pm16_code equ $ - gdt
    istruc GDTEntry
        at GDTEntry.limitl, dw 0xFFFF
        at GDTEntry.basel, dw 0
        at GDTEntry.basem, db 0
        at GDTEntry.attribute, db gdt_attr.present | gdt_attr.user | gdt_attr.code | gdt_attr.readable
        at GDTEntry.flags__limith, db 0xF
        at GDTEntry.baseh, db 0
    iend

.pm16_data equ $ - gdt
    istruc GDTEntry
        at GDTEntry.limitl, dw 0xFFFF
        at GDTEntry.basel, dw 0
        at GDTEntry.basem, db 0
        at GDTEntry.attribute, db gdt_attr.present | gdt_attr.user | gdt_attr.writable
        at GDTEntry.flags__limith, db 0xF
        at GDTEntry.baseh, db 0
    iend

.end equ $ - gdt
