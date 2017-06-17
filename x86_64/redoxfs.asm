struc Extent
    .block: resq 1,
    .length: resq 1
endstruc

struc Node
    .mode: resw 1
    .uid: resd 1
    .gid: resd 1
    .name: resb 246
    .parent: resq 1
    .next: resq 1
    .extents: resb Extent_size * 15
endstruc

struc Header
    ; Signature, should be b"RedoxFS\0"
    .signature: resb 8
    ; Version, should be 1
    .version: resq 1,
    ; Disk ID, a 128-bit unique identifier
    .uuid: resb 16,
    ; Disk size, in 512-byte sectors
    .size: resq 1,
    ; Block of root node
    .root: resq 1,
    ; Block of free space node
    .free: resq 1
    ; Padding
    .padding: resb 456
endstruc

redoxfs:
        call redoxfs.open
        test eax, eax
        jz .good_header
        ret

    .good_header:
        mov eax, [.header + Header.root]
        mov bx, .dir
        call .node

        jmp redoxfs.root

    ; node in eax, buffer in bx
    .node:
        add eax, (filesystem - boot) / 512
        mov cx, 1
        mov dx, 0
        call load
        call print_line
        ret

        align 512, db 0

    .header:
        times 512 db 0

    .dir:
        times 512 db 0

    .file:
        times 512 db 0

redoxfs.open:
        mov eax, 0
        mov bx, redoxfs.header
        call redoxfs.node

        mov bx, 0
    .sig:
        mov al, [redoxfs.header + Header.signature + bx]
        mov ah, [.signature + bx]
        cmp al, ah
        jne .sig_err
        inc bx
        cmp bx, 8
        jl .sig

        mov bx, 0
    .ver:
        mov al, [redoxfs.header + Header.version + bx]
        mov ah, [.version + bx]
        cmp al, ah
        jne .ver_err
        inc bx
        jl .ver

        lea si, [redoxfs.header + Header.signature]
        call printrm
        call print_line

        xor ax, ax
        ret

    .err_msg: db "Failed to open RedoxFS: ",0
    .sig_err_msg: db "Signature error",13,10,0
    .ver_err_msg: db "Version error",13,10,0

    .sig_err:
        mov si, .err_msg
        call printrm

        mov si, .sig_err_msg
        call printrm

        mov ax, 1
        ret

    .ver_err:
        mov si, .err_msg
        call printrm

        mov si, .ver_err_msg
        call printrm

        mov ax, 1
        ret

    .signature: db "RedoxFS",0
    .version: dq 1


redoxfs.root:
        lea si, [redoxfs.dir + Node.name]
        call printrm
        call print_line

    .lp:
        mov bx, 0
    .ext:
        mov eax, [redoxfs.dir + Node.extents + bx + Extent.block]
        test eax, eax
        jz .next

        mov ecx, [redoxfs.dir + Node.extents + bx + Extent.length]
        test ecx, ecx
        jz .next

        add ecx, 511
        shr ecx, 9

        push bx

    .ext_sec:
        push eax
        push ecx

        mov bx, redoxfs.file
        call redoxfs.node

        mov bx, 0
    .ext_sec_kernel:
        mov al, [redoxfs.file + Node.name + bx]
        mov ah, [.kernel_name + bx]

        cmp al, ah
        jne .ext_sec_kernel_break

        inc bx

        test ah, ah
        jnz .ext_sec_kernel

        pop ecx
        pop eax
        pop bx
        jmp redoxfs.kernel

    .ext_sec_kernel_break:
        pop ecx
        pop eax

        inc eax
        dec ecx
        jnz .ext_sec

        pop bx

        add bx, Extent_size
        cmp bx, Extent_size * 16
        jb .ext

    .next:
        mov eax, [redoxfs.dir + Node.next]
        test eax, eax
        jz .no_kernel

        mov bx, redoxfs.dir
        call redoxfs.node
        jmp .lp

    .no_kernel:
        mov si, .no_kernel_msg
        call printrm

        mov si, .kernel_name
        call printrm

        call print_line

        mov eax, 1
        ret

    .kernel_name: db "kernel",0
    .no_kernel_msg: db "Did not find: ",0

redoxfs.kernel:
        lea si, [redoxfs.file + Node.name]
        call printrm
        call print_line

        mov edi, [kernel_base]
    .lp:
        mov bx, 0
    .ext:
        mov eax, [redoxfs.file + Node.extents + bx + Extent.block]
        test eax, eax
        jz .next

        mov ecx, [redoxfs.file + Node.extents + bx + Extent.length]
        test ecx, ecx
        jz .next

        push bx

        push eax
        push ecx
        push edi

        add eax, (filesystem - boot)/512
        add ecx, 511
        shr ecx, 9
        call load_extent

        pop edi
        pop ecx
        pop eax

        add edi, ecx

        pop bx

        add bx, Extent_size
        cmp bx, Extent_size * 16
        jb .ext

    .next:
        mov eax, [redoxfs.file + Node.next]
        test eax, eax
        jz .done

        push edi

        mov bx, redoxfs.file
        call redoxfs.node

        pop edi
        jmp .lp

    .done:
        sub edi, [kernel_base]
        mov [kernel_size], edi

        xor eax, eax
        ret
