%include "bootsector.asm"

startup_start:
%ifdef ARCH_i386
    %include "startup-i386.asm"
%endif

%ifdef ARCH_x86_64
    %include "startup-x86_64.asm"
%endif
align 512, db 0
startup_end:

filesystem:
    %defstr FILESYSTEM_STR %[FILESYSTEM]
    incbin FILESYSTEM_STR
    align 512, db 0
