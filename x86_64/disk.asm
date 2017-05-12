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

kernel_file:
  %defstr KERNEL_STR %[KERNEL]
  incbin KERNEL_STR
  align 512, db 0
.end:
.length equ kernel_file.end - kernel_file
.length_sectors equ .length / 512

%ifdef FILESYSTEM
    %defstr FILESYSTEM_STR %[FILESYSTEM]
    incbin FILESYSTEM_STR
%endif
