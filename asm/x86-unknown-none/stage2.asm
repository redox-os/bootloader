SECTION .text
USE16

stage2.entry:
    ; check for required features
    call cpuid_check

    ; enable A20-Line via IO-Port 92, might not work on all motherboards
    in al, 0x92
    or al, 2
    out 0x92, al

    mov dword [protected_mode.func], stage3.entry
    jmp protected_mode.entry

%include "cpuid.asm"
%include "gdt.asm"
%include "long_mode.asm"
%include "protected_mode.asm"
%include "thunk.asm"

USE32

stage3.entry:
    ; stage3 stack at 256 KiB
    mov esp, 0x40000

    ; push arguments
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
    mov eax, [stage3 + 0x18]
    call eax
.halt:
    cli
    hlt
    jmp .halt
