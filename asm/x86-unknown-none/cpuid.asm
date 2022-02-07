SECTION .text
USE16

cpuid_required_features:
    .edx equ cpuid_edx.fpu | cpuid_edx.sse | cpuid_edx.pae | cpuid_edx.pse | cpuid_edx.pge | cpuid_edx.fxsr
    .ecx equ 0

cpuid_check:
    mov eax, 1
    cpuid

    and edx, cpuid_required_features.edx
    cmp edx, cpuid_required_features.edx
    jne .error

    and ecx, cpuid_required_features.ecx
    cmp ecx, cpuid_required_features.ecx
    jne .error

    ret

.error:
    push ecx
    push edx

    mov si, .msg_features
    call print

    mov si, .msg_line
    call print

    mov si, .msg_edx
    call print

    pop ebx
    push ebx
    shr ebx, 16
    call print_hex

    pop ebx
    call print_hex

    mov si, .msg_must_contain
    call print

    mov ebx, cpuid_required_features.edx
    shr ebx, 16
    call print_hex

    mov ebx, cpuid_required_features.edx
    call print_hex

    mov si, .msg_line
    call print

    mov si, .msg_ecx
    call print

    pop ebx
    push ebx
    shr ebx, 16
    call print_hex

    pop ebx
    call print_hex

    mov si, .msg_must_contain
    call print

    mov ebx, cpuid_required_features.ecx
    shr ebx, 16
    call print_hex

    mov ebx, cpuid_required_features.ecx
    call print_hex

    mov si, .msg_line
    call print

.halt:
    cli
    hlt
    jmp .halt

.msg_features: db "Required CPU features are not present",0
.msg_line: db 13,10,0
.msg_edx: db "EDX ",0
.msg_ecx: db "ECX ",0
.msg_must_contain: db " must contain ",0

cpuid_edx:
    .fpu                 equ 1 << 0
    .vme                 equ 1 << 1
    .de                  equ 1 << 2
    .pse                 equ 1 << 3
    .tsc                 equ 1 << 4
    .msr                 equ 1 << 5
    .pae                 equ 1 << 6
    .mce                 equ 1 << 7
    .cx8                 equ 1 << 8
    .apic                equ 1 << 9
    .sep                 equ 1 << 11
    .mtrr                equ 1 << 12
    .pge                 equ 1 << 13
    .mca                 equ 1 << 14
    .cmov                equ 1 << 15
    .pat                 equ 1 << 16
    .pse_36              equ 1 << 17
    .psn                 equ 1 << 18
    .clfsh               equ 1 << 19
    .ds                  equ 1 << 21
    .acpi                equ 1 << 22
    .mmx                 equ 1 << 23
    .fxsr                equ 1 << 24
    .sse                 equ 1 << 25
    .sse2                equ 1 << 26
    .ss                  equ 1 << 27
    .htt                 equ 1 << 28
    .tm                  equ 1 << 29
    .ia64                equ 1 << 30
    .pbe                 equ 1 << 31

cpuid_ecx:
    .sse3                equ 1 << 0
    .pclmulqdq           equ 1 << 1
    .dtes64              equ 1 << 2
    .monitor             equ 1 << 3
    .ds_cpl              equ 1 << 4
    .vmx                 equ 1 << 5
    .smx                 equ 1 << 6
    .est                 equ 1 << 7
    .tm2                 equ 1 << 8
    .ssse3               equ 1 << 9
    .cnxt_id             equ 1 << 10
    .sdbg                equ 1 << 11
    .fma                 equ 1 << 12
    .cmpxchg16b          equ 1 << 13
    .xtpr                equ 1 << 14
    .pdcm                equ 1 << 15
    .pcid                equ 1 << 17
    .dca                 equ 1 << 18
    .sse4_1              equ 1 << 19
    .sse4_2              equ 1 << 20
    .x2apic              equ 1 << 21
    .movbe               equ 1 << 22
    .popcnt              equ 1 << 23
    .tsc_deadline        equ 1 << 24
    .aes                 equ 1 << 25
    .xsave               equ 1 << 26
    .osxsave             equ 1 << 27
    .avx                 equ 1 << 28
    .f16c                equ 1 << 29
    .rdrand              equ 1 << 30
    .hypervisor          equ 1 << 31
