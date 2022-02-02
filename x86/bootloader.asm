sectalign off

%include "stage1.asm"

stage2:
    %include "stage2.asm"
    align 512, db 0
stage2.end:

stage3:
  %defstr STAGE3_STR %[STAGE3]
  incbin STAGE3_STR
align 512, db 0
.end:
