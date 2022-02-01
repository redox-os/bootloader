%include "startup-common.asm"

startup_arch:
    ; load protected mode GDT and IDT
    cli
    lgdt [gdtr]
    ; set protected mode bit of cr0
    mov eax, cr0
    or eax, 1
    mov cr0, eax

    ; far jump to load CS with 32 bit segment
    jmp gdt.kernel_code:protected_mode

USE32
protected_mode:
    ; load all the other segments with 32 bit data segments
    mov eax, gdt.kernel_data
    mov ds, eax
    mov es, eax
    mov fs, eax
    mov gs, eax
    mov ss, eax

    mov esp, 0x800000 - 128

    ; entry point
    mov eax, [args.kernel_base]
    call [eax + 0x18]
.halt:
    cli
    hlt
    jmp .halt

gdtr:
    dw gdt.end + 1  ; size
    dd gdt          ; offset

gdt:
.null equ $ - gdt
    dq 0

.kernel_code equ $ - gdt
    istruc GDTEntry
        at GDTEntry.limitl, dw 0xFFFF
        at GDTEntry.basel, dw 0
        at GDTEntry.basem, db 0
        at GDTEntry.attribute, db attrib.present | attrib.user | attrib.code | attrib.readable
        at GDTEntry.flags__limith, db 0xFF | flags.granularity | flags.default_operand_size
        at GDTEntry.baseh, db 0
    iend

.kernel_data equ $ - gdt
    istruc GDTEntry
        at GDTEntry.limitl, dw 0xFFFF
        at GDTEntry.basel, dw 0
        at GDTEntry.basem, db 0
        at GDTEntry.attribute, db attrib.present | attrib.user | attrib.writable
        at GDTEntry.flags__limith, db 0xFF | flags.granularity | flags.default_operand_size
        at GDTEntry.baseh, db 0
    iend

.end equ $ - gdt
