%include "cpuid.inc"

required_features:
    .edx equ features_edx.fpu | features_edx.sse | features_edx.pae | features_edx.pse | features_edx.pge | features_edx.fxsr
    .ecx equ features_ecx.xsave

check_cpuid:
    mov eax, 1
    cpuid

    and edx, required_features.edx
    cmp edx, required_features.edx
    jne .error

    and ecx, required_features.ecx
    cmp ecx, required_features.ecx
    jne .error

    ret

.error:
    mov si, .err_features
    call print
.halt:
    jmp .halt

.err_features: db "Required CPU features are not present",13,10,0
