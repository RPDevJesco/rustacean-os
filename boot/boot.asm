; ============================================================================
; boot.asm - Stage 1 Bootloader for Rustacean OS
; ============================================================================
; 
; Fits in 512-byte boot sector. Loads stage 2 and jumps to it.
; Works with both floppy and El Torito CD-ROM boot.
;
; Memory map during boot:
;   0x0000:0x7C00  - This bootloader (512 bytes)
;   0x0000:0x7E00  - Stage 2 loaded here
;   0x0000:0x0500  - Boot info structure
;
; Assemble: nasm -f bin -o boot.bin boot.asm
; ============================================================================

[BITS 16]
[ORG 0x7C00]

; ============================================================================
; Constants
; ============================================================================

STAGE2_SEGMENT  equ 0x0000
STAGE2_OFFSET   equ 0x7E00      ; Right after boot sector
STAGE2_SECTORS  equ 32          ; 16KB for stage 2
BOOT_DRIVE      equ 0x0500      ; Store boot drive here

; Floppy geometry (1.44MB)
SECTORS_PER_TRACK equ 18
HEADS           equ 2

; ============================================================================
; Entry Point
; ============================================================================

start:
    ; Set up segments
    cli
    xor     ax, ax
    mov     ds, ax
    mov     es, ax
    mov     ss, ax
    mov     sp, 0x7C00          ; Stack grows down from bootloader
    sti

    ; Save boot drive (BIOS passes it in DL)
    mov     [BOOT_DRIVE], dl

    ; Clear screen and set video mode (80x25 text)
    mov     ax, 0x0003
    int     0x10

    ; Display loading message
    mov     si, msg_loading
    call    print_string

    ; Reset disk system
    xor     ax, ax
    mov     dl, [BOOT_DRIVE]
    int     0x13
    jc      disk_error

    ; Load stage 2 one sector at a time (handles track boundaries)
    mov     word [cur_lba], 1           ; Start at LBA 1 (sector after boot)
    mov     word [sectors_rem], STAGE2_SECTORS
    mov     word [dest_ptr], STAGE2_OFFSET  ; Store destination in memory

.load_loop:
    cmp     word [sectors_rem], 0
    je      .load_done

    ; Convert LBA to CHS using stack to preserve values
    ; LBA = (C * HEADS + H) * SECTORS_PER_TRACK + (S - 1)
    mov     ax, [cur_lba]
    xor     dx, dx
    mov     cx, SECTORS_PER_TRACK
    div     cx                          ; AX = track*heads + head, DX = sector-1
    push    dx                          ; Save sector-1
    xor     dx, dx
    mov     cx, HEADS
    div     cx                          ; AX = cylinder, DX = head
    mov     ch, al                      ; CH = cylinder
    mov     dh, dl                      ; DH = head
    pop     ax
    inc     al
    mov     cl, al                      ; CL = sector (1-based)

    ; Set up for read
    mov     bx, [dest_ptr]              ; ES:BX = destination
    mov     si, 3                       ; Retry count

.retry:
    mov     ah, 0x02                    ; BIOS read sectors
    mov     al, 1                       ; One sector at a time
    mov     dl, [BOOT_DRIVE]
    int     0x13
    jnc     .read_ok

    ; Reset disk and retry
    xor     ax, ax
    mov     dl, [BOOT_DRIVE]
    int     0x13
    dec     si
    jnz     .retry
    jmp     disk_error

.read_ok:
    ; Progress dot
    mov     ax, 0x0E2E                  ; Print '.'
    int     0x10

    ; Advance destination pointer
    add     word [dest_ptr], 512
    inc     word [cur_lba]
    dec     word [sectors_rem]
    jmp     .load_loop

.load_done:
    ; Newline after dots
    mov     si, msg_newline
    call    print_string

    ; Verify magic number at start of stage 2
    cmp     word [STAGE2_OFFSET], 0x5441  ; 'AT' magic
    jne     stage2_error

    ; Jump to stage 2
    mov     dl, [BOOT_DRIVE]    ; Pass boot drive
    jmp     STAGE2_SEGMENT:STAGE2_OFFSET + 2  ; Skip magic

; ============================================================================
; Error Handlers
; ============================================================================

disk_error:
    mov     si, msg_disk_err
    call    print_string
    jmp     halt

stage2_error:
    mov     si, msg_stage2_err
    call    print_string
    jmp     halt

halt:
    mov     si, msg_halt
    call    print_string
.loop:
    cli
    hlt
    jmp     .loop

; ============================================================================
; Print String (SI = string pointer, null terminated)
; ============================================================================

print_string:
    pusha
    mov     ah, 0x0E            ; BIOS teletype
    mov     bh, 0               ; Page 0
.loop:
    lodsb
    test    al, al
    jz      .done
    int     0x10
    jmp     .loop
.done:
    popa
    ret

; ============================================================================
; Data
; ============================================================================

msg_loading:    db '[RUSTACEAN] Loading stage 2...', 13, 10, 0
msg_newline:    db 13, 10, 0
msg_disk_err:   db '[RUSTACEAN] Disk read error!', 13, 10, 0
msg_stage2_err: db '[RUSTACEAN] Stage 2 corrupt!', 13, 10, 0
msg_halt:       db '[RUSTACEAN] System halted.', 13, 10, 0

; Variables
cur_lba:        dw 0
sectors_rem:    dw 0
dest_ptr:       dw 0

; ============================================================================
; Boot Sector Padding and Signature
; ============================================================================

times 510 - ($ - $$) db 0       ; Pad to 510 bytes
dw 0xAA55                       ; Boot signature
