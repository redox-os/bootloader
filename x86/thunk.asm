USE32
thunk:
.int10:
    mov dword [.func], .int10_real
    jmp .enter

.int13:
    mov dword [.func], .int13_real
    jmp .enter

.func: dd 0
.esp: dd 0
.cr0: dd 0

.enter:
    ; save flags
    pushfd

    ; save registers
    pushad

    ; save esp
    mov [.esp], esp

    ; far jump to protected mode 16-bit
    jmp gdt.pm16_code:.pm16

.exit:
    ; set segment selectors to 32-bit protected mode
    mov eax, gdt.pm32_data
    mov ds, eax
    mov es, eax
    mov fs, eax
    mov gs, eax
    mov ss, eax

    ; restore esp
    mov esp, [.esp]

    ; restore registers
    popad

    ; restore flags
    popfd

    ; return
    ret

USE16
.int10_real:
    sti
    int 0x10
    cli
    ; ret

.int13_real:
    sti
    int 0x13
    cli
    ret

.pm16:
    ; set segment selectors to protected mode 16-bit
    mov eax, gdt.pm16_data
    mov ds, eax
    mov es, eax
    mov fs, eax
    mov gs, eax
    mov ss, eax

    ; save cr0
    mov eax, cr0
    mov [.cr0], eax

    ; disable paging and protected mode
    and eax, 0x7FFFFFFE
    mov cr0, eax

    ; far jump to real mode
    jmp 0:.real

.real:
    ; set segment selectors to real mode
    mov eax, 0
    mov ds, eax
    mov es, eax
    mov fs, eax
    mov gs, eax
    mov ss, eax

    ; set stack
    mov esp, 0x7C00 - 16

    ; load registers
    popa

    ; call real mode function
    call [.func]

    ; save registers
    pusha

    ; restore cr0, will enable protected mode
    mov eax, [.cr0]
    mov cr0, eax

    ; far jump to protected mode 32-bit
    jmp gdt.pm32_code:.exit
