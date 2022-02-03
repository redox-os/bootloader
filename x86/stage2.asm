SECTION .text
USE16

stage2.entry:
    ; enable A20-Line via IO-Port 92, might not work on all motherboards
    in al, 0x92
    or al, 2
    out 0x92, al

    mov edi, [args.stage3_base]
    mov ecx, (stage3.end - stage3)
    mov [args.stage3_size], ecx

    mov eax, (stage3 - stage1)/512
    add ecx, 511
    shr ecx, 9
    call load_extent

    ; load protected mode GDT and IDT
    cli
    lgdt [gdtr]
    ; set protected mode bit of cr0
    mov eax, cr0
    or eax, 1
    mov cr0, eax

    ; far jump to load CS with 32 bit segment
    jmp gdt.pm32_code:protected_mode

args:
    .stage3_base dq 0x100000
    .stage3_size dq 0

; load a disk extent into high memory
; eax - sector address
; ecx - sector count
; edi - destination
load_extent:
    ; loading stage3 to 1MiB
    ; move part of stage3 to stage2.end via bootsector#load and then copy it up
    ; repeat until all of the stage3 is loaded
    buffer_size_sectors equ 127

.lp:
    cmp ecx, buffer_size_sectors
    jb .break

    ; saving counter
    push eax
    push ecx

    push edi

    ; populating buffer
    mov ecx, buffer_size_sectors
    mov bx, stage2.end
    mov dx, 0x0

    ; load sectors
    call load

    ; set up unreal mode
    call unreal

    pop edi

    ; move data
    mov esi, stage2.end
    mov ecx, buffer_size_sectors * 512 / 4
    cld
    a32 rep movsd

    pop ecx
    pop eax

    add eax, buffer_size_sectors
    sub ecx, buffer_size_sectors
    jmp .lp

.break:
    ; load the part of the stage3 that does not fill the buffer completely
    test ecx, ecx
    jz .finish ; if cx = 0 => skip

    push ecx
    push edi

    mov bx, stage2.end
    mov dx, 0x0
    call load

    ; moving remnants of stage3
    call unreal

    pop edi
    pop ecx

    mov esi, stage2.end
    shl ecx, 7 ; * 512 / 4
    cld
    a32 rep movsd

.finish:
    call print_line
    ret

%include "descriptor_flags.inc"
%include "gdt_entry.inc"
%include "unreal.asm"
%include "thunk.asm"

USE32
protected_mode:
    ; load all the other segments with 32 bit data segments
    mov eax, gdt.pm32_data
    mov ds, eax
    mov es, eax
    mov fs, eax
    mov gs, eax
    mov ss, eax

    mov esp, 0x800000 - 128

    ; entry point
    mov eax, thunk.int16
    push eax
    mov eax, thunk.int15
    push eax
    mov eax, thunk.int13
    push eax
    mov eax, thunk.int10
    push eax
    xor eax, eax
    mov al, [disk]
    push eax
    mov eax, [args.stage3_base]
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

.pm32_code equ $ - gdt
    istruc GDTEntry
        at GDTEntry.limitl, dw 0xFFFF
        at GDTEntry.basel, dw 0
        at GDTEntry.basem, db 0
        at GDTEntry.attribute, db attrib.present | attrib.user | attrib.code | attrib.readable
        at GDTEntry.flags__limith, db 0xF | flags.granularity | flags.default_operand_size
        at GDTEntry.baseh, db 0
    iend

.pm32_data equ $ - gdt
    istruc GDTEntry
        at GDTEntry.limitl, dw 0xFFFF
        at GDTEntry.basel, dw 0
        at GDTEntry.basem, db 0
        at GDTEntry.attribute, db attrib.present | attrib.user | attrib.writable
        at GDTEntry.flags__limith, db 0xF | flags.granularity | flags.default_operand_size
        at GDTEntry.baseh, db 0
    iend

.pm16_code equ $ - gdt
    istruc GDTEntry
        at GDTEntry.limitl, dw 0xFFFF
        at GDTEntry.basel, dw 0
        at GDTEntry.basem, db 0
        at GDTEntry.attribute, db attrib.present | attrib.user | attrib.code | attrib.readable
        at GDTEntry.flags__limith, db 0xF
        at GDTEntry.baseh, db 0
    iend

.pm16_data equ $ - gdt
    istruc GDTEntry
        at GDTEntry.limitl, dw 0xFFFF
        at GDTEntry.basel, dw 0
        at GDTEntry.basem, db 0
        at GDTEntry.attribute, db attrib.present | attrib.user | attrib.writable
        at GDTEntry.flags__limith, db 0xF
        at GDTEntry.baseh, db 0
    iend

.end equ $ - gdt
