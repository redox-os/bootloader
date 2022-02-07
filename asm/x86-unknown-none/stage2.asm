SECTION .text
USE16

stage2.entry:
    ; check for required features
    call cpuid_check

    ; enable A20-Line via IO-Port 92, might not work on all motherboards
    in al, 0x92
    or al, 2
    out 0x92, al

    ; load memory map
    ;TODO: rewrite this in Rust
    call memory_map

    mov edi, [args.stage3_base]
    mov ecx, (stage3.end - stage3)
    mov [args.stage3_size], ecx

    mov eax, (stage3 - stage1)/512
    add ecx, 511
    shr ecx, 9
    call load_extent

    mov dword [protected_mode.func], stage3.entry
    jmp protected_mode.entry

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

%include "cpuid.asm"
%include "gdt.asm"
%include "long_mode.asm"
%include "memory_map.asm"
%include "protected_mode.asm"
%include "thunk.asm"
%include "unreal.asm"

USE32
stage3.entry:
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
