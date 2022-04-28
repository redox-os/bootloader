SECTION .text
USE32

long_mode:
.func: dq 0
.page_table: dd 0

.entry:
    ; disable interrupts
    cli

    ; disable paging
    mov eax, cr0
    and eax, 0x7FFFFFFF
    mov cr0, eax

    ; enable FXSAVE/FXRSTOR, Page Global, Page Address Extension, and Page Size Extension
    mov eax, cr4
    or eax, 1 << 9 | 1 << 7 | 1 << 5 | 1 << 4
    mov cr4, eax

    ; load long mode GDT
    lgdt [gdtr]

    ; enable long mode
    mov ecx, 0xC0000080               ; Read from the EFER MSR.
    rdmsr
    or eax, 1 << 11 | 1 << 8          ; Set the Long-Mode-Enable and NXE bit.
    wrmsr

    ; set page table
    mov eax, [.page_table]
    mov cr3, eax

    ; enabling paging and protection simultaneously
    mov eax, cr0
    or eax, 1 << 31 | 1 << 16 | 1                ;Bit 31: Paging, Bit 16: write protect kernel, Bit 0: Protected Mode
    mov cr0, eax

    ; far jump to enable Long Mode and load CS with 64 bit segment
    jmp gdt.lm64_code:.inner

USE64

.inner:
    ; load all the other segments with 64 bit data segments
    mov rax, gdt.lm64_data
    mov ds, rax
    mov es, rax
    mov fs, rax
    mov gs, rax
    mov ss, rax

    ; jump to specified function
    mov rax, [.func]
    jmp rax
