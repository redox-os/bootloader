sectalign off

; stage 1 is sector 0, loaded at 0x7C00
%include "stage1.asm"

; GPT area from sector 1 to 33, loaded at 0x7E00
times (33*512) db 0

; stage 2, loaded at 0xC000
stage2:
    %include "stage2.asm"
    align 512, db 0
stage2.end:

; the maximum size of stage2 is 4 KiB
times (4*1024)-($-stage2) db 0

; ISO compatibility, uses up space until 0x12400
%include "iso.asm"

times 3072 db 0 ; Pad to 0x13000

; stage3, loaded at 0x13000
stage3:
    %defstr STAGE3_STR %[STAGE3]
    incbin STAGE3_STR
    align 512, db 0
.end:

; the maximum size of the boot loader portion is 384 KiB
times (384*1024)-($-$$) db 0
