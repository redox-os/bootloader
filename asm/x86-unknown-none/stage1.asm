ORG 0x7C00
SECTION .text
USE16

stage1: ; dl comes with disk
    ; initialize segment registers
    xor ax, ax
    mov ds, ax
    mov es, ax
    mov ss, ax

    ; initialize stack
    mov sp, 0x7C00

    ; initialize CS
    push ax
    push word .set_cs
    retf

.set_cs:

    ; save disk number
    mov [disk], dl

    mov si, stage_msg
    call print
    mov al, '1'
    call print_char
    call print_line

    ; read CHS gemotry
    ;  CL (bits 0-5) = maximum sector number
    ;  CL (bits 6-7) = high bits of max cylinder number
    ;  CH = low bits of maximum cylinder number
    ;  DH = maximum head number
    mov ah, 0x08
    mov dl, [disk]
    xor di, di
    int 0x13
    jc error ; carry flag set on error
    mov bl, ch
    mov bh, cl
    shr bh, 6
    mov [chs.c], bx
    shr dx, 8
    inc dx ; returns heads - 1
    mov [chs.h], dx
    and cl, 0x3f
    mov [chs.s], cl

    mov eax, (stage2 - stage1) / 512
    mov bx, stage2
    mov cx, (stage3.end - stage2) / 512
    mov dx, 0
    call load

    mov si, stage_msg
    call print
    mov al, '2'
    call print_char
    call print_line

    jmp stage2.entry

; load some sectors from disk to a buffer in memory
; buffer has to be below 1MiB
; IN
;   ax: start sector
;   bx: offset of buffer
;   cx: number of sectors (512 Bytes each)
;   dx: segment of buffer
; CLOBBER
;   ax, bx, cx, dx, si
; TODO rewrite to (eventually) move larger parts at once
; if that is done increase buffer_size_sectors in startup-common to that (max 0x80000 - startup_end)
load:
    cmp cx, 127
    jbe .good_size

    pusha
    mov cx, 127
    call load
    popa
    add eax, 127
    add dx, 127 * 512 / 16
    sub cx, 127

    jmp load
.good_size:
    mov [DAPACK.addr], eax
    mov [DAPACK.buf], bx
    mov [DAPACK.count], cx
    mov [DAPACK.seg], dx

    call print_dapack

    cmp byte [chs.s], 0
    jne .chs
    ;INT 0x13 extended read does not work on CDROM!
    mov dl, [disk]
    mov si, DAPACK
    mov ah, 0x42
    int 0x13
    jc error ; carry flag set on error
    ret

.chs:
    ; calculate CHS
    xor edx, edx
    mov eax, [DAPACK.addr]
    div dword [chs.s] ; divide by sectors
    mov ecx, edx ; move sector remainder to ecx
    xor edx, edx
    div dword [chs.h] ; divide by heads
    ; eax has cylinders, edx has heads, ecx has sectors

    ; Sector cannot be greater than 63
    inc ecx ; Sector is base 1
    cmp ecx, 63
    ja error_chs

    ; Head cannot be greater than 255
    cmp edx, 255
    ja error_chs

    ; Cylinder cannot be greater than 1023
    cmp eax, 1023
    ja error_chs

    ; Move CHS values to parameters
    mov ch, al
    shl ah, 6
    and cl, 0x3f
    or cl, ah
    shl dx, 8

    ; read from disk using CHS
    mov al, [DAPACK.count]
    mov ah, 0x02 ; disk read (CHS)
    mov bx, [DAPACK.buf]
    mov dl, [disk]
    push es ; save ES
    mov es, [DAPACK.seg]
    int 0x13
    pop es ; restore EC
    jc error ; carry flag set on error
    ret

print_dapack:
    mov bx, [DAPACK.addr + 2]
    call print_hex

    mov bx, [DAPACK.addr]
    call print_hex

    mov al, '#'
    call print_char

    mov bx, [DAPACK.count]
    call print_hex

    mov al, ' '
    call print_char

    mov bx, [DAPACK.seg]
    call print_hex

    mov al, ':'
    call print_char

    mov bx, [DAPACK.buf]
    call print_hex

    call print_line

    ret

error_chs:
    mov ah, 0

error:
    call print_line

    mov bh, 0
    mov bl, ah
    call print_hex

    mov al, ' '
    call print_char

    mov si, error_msg
    call print
    call print_line
.halt:
    cli
    hlt
    jmp .halt

%include "print.asm"

stage_msg: db "Stage ",0
error_msg: db "ERROR",0

disk: db 0

chs:
.c: dd 0
.h: dd 0
.s: dd 0

DAPACK:
        db 0x10
        db 0
.count: dw 0 ; int 13 resets this to # of blocks actually read/written
.buf:   dw 0 ; memory buffer destination address (0:7c00)
.seg:   dw 0 ; in memory page zero
.addr:  dq 0 ; put the lba to read in this spot

times 446-($-$$) db 0
partitions: times 4 * 16 db 0
db 0x55
db 0xaa
