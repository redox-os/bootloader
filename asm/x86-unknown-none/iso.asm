; Simple ISO emulation with el torito

; Fill until CD sector 0x10
times (0x10*2048)-($-$$) db 0

; Volume record
;TODO: fill in more fields
iso_volume_record:
db 1 ; Type volume record
db "CD001" ; Identifier
db 1 ; Version
db 0 ; Unused
times 32 db ' ' ; System identifier
.volume_id: ; Volume identifier
db 'Redox OS'
times 32-($-.volume_id) db ' '
times 8 db 0 ; Unused
db 0x15, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x15 ; Volume space size (0x15)
times 32 db 0 ; Unused
db 0x01, 0x00, 0x00, 0x01 ; Volume set size
db 0x01, 0x00, 0x00, 0x01 ; Volume sequence number
db 0x00, 0x08, 0x08, 0x00 ; Logical block size in little and big endian

times 156-($-iso_volume_record) db 0

; Root directory entry
.root_directory:
db 0x22 ; Length of entry
db 0x00 ; Length of extended attributes
db 0x14, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x14 ; Location of extent (0x14)
db 0x00, 0x08, 0x00, 0x00, 0x00, 0x00, 0x08, 0x00 ; Size of extent
db 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00 ; Recording time
db 0x02 ; File flags
db 0x00 ; Interleaved file unit size
db 0x00 ; Interleaved gap size
db 0x01, 0x00, 0x00, 0x01 ; Volume sequence number
db 0x01 ; Length of file identifier
db 0x00 ; File identifier

times 128 db ' ' ; Volume set identifier
times 128 db ' ' ; Publisher identifier
times 128 db ' ' ; Data preparer identifier
times 128 db ' ' ; Application identifier
times 37 db ' ' ; Copyright file ID
times 37 db ' ' ; Abstract file ID
times 37 db ' ' ; Bibliographic file ID

times 881-($-iso_volume_record) db 0

db 1 ; File structure version

; Fill until CD sector 0x11
times (0x11*2048)-($-$$) db 0

; Boot record
iso_boot_record:
db 0 ; Type boot record
db "CD001" ; Identifier
db 1 ; Version
db "EL TORITO SPECIFICATION" ; Boot system identifier
times 0x47-($ - iso_boot_record) db 0 ; Padding
dd 0x13 ; Sector of boot catalog

; Fill until CD sector 0x12
times (0x12*2048)-($-$$) db 0

; Terminator
iso_terminator:
db 0xFF ; Type terminator
db "CD001" ; Identifier
db 1 ; Version

; Fill until CD sector 0x13
times (0x13*2048)-($-$$) db 0

; Boot catalog
iso_boot_catalog:

; Validation entry
.validation:
db 1 ; Header ID
db 0 ; Platform ID (x86)
dw 0 ; Reserved
times 24 db 0 ; ID string
dw 0x55aa ; Checksum
dw 0xaa55 ; Key

; Default entry
.default:
db 0x88 ; Bootable
db 4 ; Hard drive emulation
dw 0 ; Load segment (0 is platform default)
db 0xEE ; Partition type (0xEE is protective MBR)
db 0 ; Unused
dw 1 ; Sector count
dd 0 ; Start address for virtual disk
times 20 db 0 ; Padding

; EFI section header entry
.efi_section_header:
db 0x91 ; Final header
db 0xEF ; Platform ID (EFI)
dw 1 ; Number of section header entries
times 28 db 0 ; ID string

; EFI section entry
.efi_section_entry:
db 0x88 ; Bootable
db 0 ; No emulation
dw 0 ; Load segment (0 is platform default)
db 0 ; Partition type (not used)
db 0 ; Unused
dw 512 ; Sector count (1 MiB = 512 CD sectors)
dd 512 ; Start address for virtual disk (1 MiB = 512 CD sectors)
times 20 db 0 ; Padding

; Fill until CD sector 0x14
times (0x14*2048)-($-$$) db 0

iso_root_directory:
.self:
db 0x22 ; Length of entry
db 0x00 ; Length of extended attributes
db 0x14, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x14 ; Location of extent (0x14)
db 0x00, 0x08, 0x00, 0x00, 0x00, 0x00, 0x08, 0x00 ; Size of extent
db 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00 ; Recording time
db 0x02 ; File flags
db 0x00 ; Interleaved file unit size
db 0x00 ; Interleaved gap size
db 0x01, 0x00, 0x00, 0x01 ; Volume sequence number
db 0x01 ; Length of file identifier
db 0x00 ; File identifier

.parent:
db 0x22 ; Length of entry
db 0x00 ; Length of extended attributes
db 0x14, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x14 ; Location of extent (0x14)
db 0x00, 0x08, 0x00, 0x00, 0x00, 0x00, 0x08, 0x00 ; Size of extent
db 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00 ; Recording time
db 0x02 ; File flags
db 0x00 ; Interleaved file unit size
db 0x00 ; Interleaved gap size
db 0x01, 0x00, 0x00, 0x01 ; Volume sequence number
db 0x01 ; Length of file identifier
db 0x01 ; File identifier

.boot_cat:
db 0x2C ; Length of entry
db 0x00 ; Length of extended attributes
db 0x13, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x13 ; Location of extent (0x13)
db 0x00, 0x08, 0x00, 0x00, 0x00, 0x00, 0x08, 0x00 ; Size of extent
db 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00 ; Recording time
db 0x00 ; File flags
db 0x00 ; Interleaved file unit size
db 0x00 ; Interleaved gap size
db 0x01, 0x00, 0x00, 0x01 ; Volume sequence number
db 0x0A ; Length of file identifier
db "BOOT.CAT;1",0 ; File identifier

; Fill until CD sector 0x15
times (0x15*2048)-($-$$) db 0
