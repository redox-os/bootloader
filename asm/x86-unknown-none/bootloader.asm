sectalign off

%include "stage1.asm"

stage2:
    %include "stage2.asm"
    align 512, db 0
stage2.end:

; the maximum size of stage1 + stage2 is 5 KiB
; this places stage3 at 0x9000
times 5120-($-$$) db 0

stage3:
    %defstr STAGE3_STR %[STAGE3]
    incbin STAGE3_STR
    align 512, db 0
.end:

; the maximum size of the boot loader portion is 384 KiB
times (384*1024)-($-$$) db 0
