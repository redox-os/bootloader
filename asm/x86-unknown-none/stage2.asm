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
    ; stage3 stack at 512 KiB
    mov esp, 0x80000

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
    mov eax, kernel.entry
    push eax
    mov eax, [stage3 + 0x18]
    call eax
.halt:
    cli
    hlt
    jmp .halt

kernel:
.stack: dq 0
.func: dq 0
.args: dq 0

.entry:
    ; page_table: usize
    mov eax, [esp + 4]
    mov [long_mode.page_table], eax

    ; stack: u64
    mov eax, [esp + 8]
    mov [.stack], eax
    mov eax, [esp + 12]
    mov [.stack + 4], eax

    ; func: u64
    mov eax, [esp + 16]
    mov [.func], eax
    mov eax, [esp + 20]
    mov [.func + 4], eax

    ; args: *const KernelArgs
    mov eax, [esp + 24]
    mov [.args], eax

    mov eax, .inner
    mov [long_mode.func], eax
    jmp long_mode.entry

USE64

.inner:
    mov rsp, [.stack]
    mov rax, [.func]
    mov rdi, [.args]
    call rax
.halt:
    cli
    hlt
    jmp .halt
