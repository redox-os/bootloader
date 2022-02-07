SECTION .text
USE16

protected_mode:

.func: dd 0

.entry:
    ; disable interrupts
    cli

    ; load protected mode GDT
    lgdt [gdtr]

    ; set protected mode bit of cr0
    mov eax, cr0
    or eax, 1
    mov cr0, eax

    ; far jump to load CS with 32 bit segment
    jmp gdt.pm32_code:.inner

USE32

.inner:
    ; load all the other segments with 32 bit data segments
    mov eax, gdt.pm32_data
    mov ds, eax
    mov es, eax
    mov fs, eax
    mov gs, eax
    mov ss, eax

    ; jump to specified function
    mov eax, [.func]
    jmp eax
