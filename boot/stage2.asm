; ============================================================================
; stage2.asm - Stage 2 Bootloader for Rustacean OS
; ============================================================================
;
; Performs:
;   1. Query E820 memory map
;   2. Enable A20 line
;   3. Set VESA graphics mode (for Plan 9-style GUI)
;   4. Load kernel to 0x100000 (1MB mark)
;   5. Build GDT
;   6. Switch to 32-bit protected mode
;   7. Jump to kernel with boot info
;
; Assemble: nasm -f bin -o stage2.bin stage2.asm
; ============================================================================

[BITS 16]
[ORG 0x7E00]

; ============================================================================
; Magic Number (verified by stage 1)
; ============================================================================

dw 0x5441                       ; 'AT' magic - stage 1 checks this

; ============================================================================
; Entry Point (jumped to from stage 1)
; ============================================================================

stage2_entry:
    ; DL contains boot drive from stage 1
    mov     [boot_drive], dl

    ; Display banner
    mov     si, msg_stage2
    call    print_string

    ; ========================================================================
    ; Step 1: Query E820 Memory Map
    ; ========================================================================

    mov     si, msg_e820
    call    print_string

    call    query_e820
    jc      e820_error

    mov     si, msg_ok
    call    print_string

    ; ========================================================================
    ; Step 2: Enable A20 Line
    ; ========================================================================

    mov     si, msg_a20
    call    print_string

    call    enable_a20
    call    verify_a20
    jc      a20_error

    mov     si, msg_ok
    call    print_string

    ; ========================================================================
    ; Step 3: Query and Set VESA Mode
    ; ========================================================================

    mov     si, msg_vesa
    call    print_string

%ifdef SKIP_VESA
    ; Skip VESA entirely - use VGA text mode
    jmp     vesa_fallback
%else
    call    setup_vesa
    jc      vesa_fallback       ; Fall back to VGA text if VESA fails
%endif

    mov     si, msg_ok
    call    print_string
    jmp     load_kernel

vesa_fallback:
    mov     si, msg_vesa_fallback
    call    print_string

    ; Set 80x25 text mode (mode 3)
    mov     ax, 0x0003
    int     0x10

    ; Mark as VGA text mode
    mov     byte [vesa_enabled], 0
    mov     dword [vesa_framebuffer], 0xB8000
    mov     word [vesa_width], 80
    mov     word [vesa_height], 25
    mov     byte [vesa_bpp], 16
    mov     word [vesa_pitch], 160

load_kernel:
    ; ========================================================================
    ; Step 4: Load Kernel
    ; ========================================================================

    mov     si, msg_kernel
    call    print_string

    call    do_load_kernel
    jc      kernel_error

    mov     si, msg_ok
    call    print_string

    ; ========================================================================
    ; Step 5: Switch to Protected Mode
    ; ========================================================================

    mov     si, msg_pmode
    call    print_string

    ; Disable interrupts for mode switch
    cli

    ; Load GDT
    lgdt    [gdt_descriptor]

    ; Set PE bit in CR0
    mov     eax, cr0
    or      eax, 1
    mov     cr0, eax

    ; Far jump to flush pipeline and load CS
    jmp     0x08:protected_mode_entry

; ============================================================================
; Error Handlers
; ============================================================================

e820_error:
    mov     si, msg_e820_fail
    call    print_string
    jmp     halt

a20_error:
    mov     si, msg_a20_fail
    call    print_string
    jmp     halt

kernel_error:
    mov     si, msg_kernel_fail
    call    print_string
    jmp     halt

halt:
    cli
    hlt
    jmp     halt

; ============================================================================
; E820 Memory Map Query
; ============================================================================

; E820 entry structure (20 or 24 bytes)
E820_BASE       equ 0x1000      ; Store map at 0x1000
E820_MAX        equ 64          ; Max entries

query_e820:
    push    es
    push    di
    push    ebx
    push    ecx
    push    edx

    mov     di, E820_BASE + 4   ; Leave space for entry count
    xor     ebx, ebx            ; Continuation value
    xor     bp, bp              ; Entry counter
    mov     edx, 0x534D4150     ; 'SMAP' signature

.loop:
    mov     eax, 0xE820
    mov     ecx, 24             ; Ask for 24 bytes (ACPI 3.0)
    push    edx
    int     0x15
    pop     edx

    jc      .done               ; Carry set = end or error
    cmp     eax, 0x534D4150     ; Verify SMAP signature
    jne     .error

    ; Valid entry
    add     di, 24
    inc     bp
    cmp     bp, E820_MAX
    jge     .done

    test    ebx, ebx            ; EBX = 0 means end
    jnz     .loop

.done:
    ; Store entry count
    mov     [E820_BASE], bp

    pop     edx
    pop     ecx
    pop     ebx
    pop     di
    pop     es
    clc                         ; Success
    ret

.error:
    pop     edx
    pop     ecx
    pop     ebx
    pop     di
    pop     es
    stc                         ; Failure
    ret

; ============================================================================
; A20 Line Enable
; ============================================================================

enable_a20:
    ; Try BIOS method first
    mov     ax, 0x2401
    int     0x15
    jnc     .done

    ; Try keyboard controller method
    call    .wait_kbd
    mov     al, 0xAD            ; Disable keyboard
    out     0x64, al

    call    .wait_kbd
    mov     al, 0xD0            ; Read output port
    out     0x64, al

    call    .wait_kbd_data
    in      al, 0x60
    push    ax

    call    .wait_kbd
    mov     al, 0xD1            ; Write output port
    out     0x64, al

    call    .wait_kbd
    pop     ax
    or      al, 2               ; Set A20 bit
    out     0x60, al

    call    .wait_kbd
    mov     al, 0xAE            ; Enable keyboard
    out     0x64, al

    call    .wait_kbd

.done:
    ret

.wait_kbd:
    in      al, 0x64
    test    al, 2
    jnz     .wait_kbd
    ret

.wait_kbd_data:
    in      al, 0x64
    test    al, 1
    jz      .wait_kbd_data
    ret

verify_a20:
    push    es
    push    ds
    push    di
    push    si

    xor     ax, ax
    mov     es, ax
    mov     di, 0x0500

    mov     ax, 0xFFFF
    mov     ds, ax
    mov     si, 0x0510

    mov     byte [es:di], 0x00
    mov     byte [ds:si], 0xFF

    cmp     byte [es:di], 0xFF
    je      .disabled

    pop     si
    pop     di
    pop     ds
    pop     es
    clc
    ret

.disabled:
    pop     si
    pop     di
    pop     ds
    pop     es
    stc
    ret

; ============================================================================
; VESA Setup - Target 800x600 for Plan 9 style GUI
; ============================================================================

VESA_INFO       equ 0x2000      ; VBE info block
VESA_MODE_INFO  equ 0x2200      ; Mode info block
PREFERRED_MODE  equ 0x115       ; 800x600x32 (common)
FALLBACK_MODE   equ 0x112       ; 640x480x32
FALLBACK_MODE2  equ 0x111       ; 640x480x16
FALLBACK_MODE3  equ 0x101       ; 640x480x8

setup_vesa:
    push    es

    ; Get VBE info
    mov     ax, 0x4F00
    mov     di, VESA_INFO
    push    ds
    pop     es
    int     0x10
    cmp     ax, 0x004F
    jne     .use_vga_text

    ; Try preferred mode first (800x600x32)
    mov     cx, PREFERRED_MODE
    call    .try_mode
    jnc     .set_mode

    ; Try 640x480x32
    mov     cx, FALLBACK_MODE
    call    .try_mode
    jnc     .set_mode

    ; Try 640x480x16
    mov     cx, FALLBACK_MODE2
    call    .try_mode
    jnc     .set_mode

    ; Try 640x480x8
    mov     cx, FALLBACK_MODE3
    call    .try_mode
    jnc     .set_mode

    ; All VESA modes failed - use VGA text mode
.use_vga_text:
    ; Set 80x25 text mode (mode 3)
    mov     ax, 0x0003
    int     0x10

    ; Mark as VGA text mode
    mov     byte [vesa_enabled], 0
    mov     dword [vesa_framebuffer], 0xB8000
    mov     word [vesa_width], 80
    mov     word [vesa_height], 25
    mov     byte [vesa_bpp], 16          ; 2 bytes per character cell
    mov     word [vesa_pitch], 160       ; 80 * 2

    pop     es
    clc                                  ; Success (text mode works)
    ret

.try_mode:
    ; Get mode info
    push    cx
    mov     ax, 0x4F01
    mov     di, VESA_MODE_INFO
    int     0x10
    pop     cx
    cmp     ax, 0x004F
    jne     .try_fail

    ; Check if mode has linear framebuffer
    test    byte [VESA_MODE_INFO], 0x80
    jz      .try_fail

    clc
    ret
.try_fail:
    stc
    ret

.set_mode:
    ; Set the mode with linear framebuffer
    mov     ax, 0x4F02
    mov     bx, cx
    or      bx, 0x4000          ; Linear framebuffer bit
    int     0x10
    cmp     ax, 0x004F
    jne     .use_vga_text

    ; Save mode info for kernel
    mov     [vesa_mode], cx
    mov     byte [vesa_enabled], 1

    ; Copy relevant info to boot_info
    mov     eax, [VESA_MODE_INFO + 40]  ; Physical framebuffer address
    mov     [vesa_framebuffer], eax
    mov     ax, [VESA_MODE_INFO + 18]   ; Width
    mov     [vesa_width], ax
    mov     ax, [VESA_MODE_INFO + 20]   ; Height
    mov     [vesa_height], ax
    mov     al, [VESA_MODE_INFO + 25]   ; BPP
    mov     [vesa_bpp], al
    mov     ax, [VESA_MODE_INFO + 16]   ; Pitch
    mov     [vesa_pitch], ax

    pop     es
    clc
    ret

; ============================================================================
; Load Kernel (uses standard CHS reads to low memory)
; ============================================================================

KERNEL_SECTOR   equ 33          ; After stage2 (sector 1-32)
KERNEL_SECTORS  equ 128         ; 64KB kernel (adjust as needed)
KERNEL_LOAD_SEG equ 0x2000      ; Load at 0x20000 (128KB mark)
KERNEL_LOAD_OFF equ 0x0000
KERNEL_DEST     equ 0x100000    ; Final destination: 1MB

; Floppy geometry for 1.44MB
SECTORS_PER_TRACK equ 18
HEADS           equ 2

do_load_kernel:
    push    es
    push    bp

    ; Reset disk before loading (important after VESA/A20 operations)
    xor     ax, ax
    mov     dl, [boot_drive]
    int     0x13

    ; Set up for loading
    mov     word [sectors_left], KERNEL_SECTORS
    mov     word [current_lba], KERNEL_SECTOR
    mov     word [load_segment], KERNEL_LOAD_SEG
    mov     word [load_offset], KERNEL_LOAD_OFF

.load_loop:
    ; Check if done
    cmp     word [sectors_left], 0
    je      .done

    ; Convert LBA to CHS (using CX for division to avoid clobbering)
    mov     ax, [current_lba]
    xor     dx, dx
    mov     cx, SECTORS_PER_TRACK
    div     cx                      ; AX = track*heads + head, DX = sector-1
    push    dx                      ; Save sector-1 on stack
    xor     dx, dx
    mov     cx, HEADS
    div     cx                      ; AX = cylinder, DX = head
    mov     ch, al                  ; CH = cylinder (low 8 bits)
    mov     dh, dl                  ; DH = head
    pop     ax                      ; Get sector-1 back
    inc     al
    mov     cl, al                  ; CL = sector (1-based)

    ; Set up read destination
    mov     ax, [load_segment]
    mov     es, ax
    mov     bx, [load_offset]

    ; Retry loop for flaky hardware
    mov     bp, 3
.retry:
    push    bx                      ; Save destination
    push    cx                      ; Save CHS
    push    dx
    push    es

    mov     ah, 0x02                ; Read sectors
    mov     al, 1                   ; One sector at a time (safe)
    mov     dl, [boot_drive]
    int     0x13

    pop     es
    pop     dx
    pop     cx
    pop     bx

    jnc     .read_ok

    ; Save error code for debugging
    mov     [disk_error_code], ah

    ; Reset disk and retry
    xor     ax, ax
    mov     dl, [boot_drive]
    int     0x13
    dec     bp
    jnz     .retry

    ; Failed after retries - display error code
    mov     al, [disk_error_code]
    call    print_hex_byte
    jmp     .error

.read_ok:
    ; Print a dot for progress
    mov     ax, 0x0E2E
    int     0x10

    ; Update load address
    add     word [load_offset], 512
    jnc     .no_segment_wrap
    add     word [load_segment], 0x1000  ; Add 64KB to segment
    mov     word [load_offset], 0
.no_segment_wrap:

    ; Update counters
    inc     word [current_lba]
    dec     word [sectors_left]
    jmp     .load_loop

.done:
    pop     bp
    pop     es
    clc
    ret

.error:
    pop     bp
    pop     es
    stc
    ret

; Variables for kernel loading
sectors_left:   dw 0
current_lba:    dw 0
load_segment:   dw 0
load_offset:    dw 0
disk_error_code: db 0

; ============================================================================
; Print String (16-bit)
; ============================================================================

print_string:
    pusha
    mov     ah, 0x0E
.loop:
    lodsb
    test    al, al
    jz      .done
    int     0x10
    jmp     .loop
.done:
    popa
    ret

; Print AL as hex byte (for debugging)
print_hex_byte:
    pusha
    mov     cl, al              ; Save byte

    ; Print "Err:"
    mov     ah, 0x0E
    mov     al, 'E'
    int     0x10
    mov     al, 'r'
    int     0x10
    mov     al, 'r'
    int     0x10
    mov     al, ':'
    int     0x10

    ; Print high nibble
    mov     al, cl
    shr     al, 4
    call    .print_nibble

    ; Print low nibble
    mov     al, cl
    and     al, 0x0F
    call    .print_nibble

    popa
    ret

.print_nibble:
    cmp     al, 10
    jb      .digit
    add     al, 'A' - 10
    jmp     .out
.digit:
    add     al, '0'
.out:
    mov     ah, 0x0E
    int     0x10
    ret

; ============================================================================
; GDT (Global Descriptor Table)
; ============================================================================

align 16
gdt_start:
    ; Null descriptor
    dq 0

    ; Code segment: 0x08
    dw 0xFFFF                   ; Limit low
    dw 0                        ; Base low
    db 0                        ; Base middle
    db 10011010b                ; Access: present, ring 0, code, readable
    db 11001111b                ; Granularity: 4KB, 32-bit, limit high
    db 0                        ; Base high

    ; Data segment: 0x10
    dw 0xFFFF
    dw 0
    db 0
    db 10010010b                ; Access: present, ring 0, data, writable
    db 11001111b
    db 0

gdt_end:

gdt_descriptor:
    dw gdt_end - gdt_start - 1  ; Size
    dd gdt_start                ; Address

; ============================================================================
; 32-bit Protected Mode Entry
; ============================================================================

[BITS 32]

protected_mode_entry:
    ; Set up segment registers
    mov     ax, 0x10            ; Data segment
    mov     ds, ax
    mov     es, ax
    mov     fs, ax
    mov     gs, ax
    mov     ss, ax
    mov     esp, 0x90000        ; Stack below EBDA

    ; Copy kernel from low memory (0x20000) to 1MB (0x100000)
    mov     esi, 0x20000        ; Source: where we loaded it
    mov     edi, 0x100000       ; Destination: 1MB
    mov     ecx, (KERNEL_SECTORS * 512) / 4  ; Dword count
    cld
    rep movsd

    ; Build boot info structure at 0x500
    mov     edi, 0x500

    ; Magic: 'RUST' (0x54535552)
    mov     dword [edi], 0x54535552     ; 'RUST' magic
    add     edi, 4

    ; E820 map location
    mov     dword [edi], E820_BASE
    add     edi, 4

    ; VESA info
    movzx   eax, byte [vesa_enabled]
    mov     [edi], eax
    add     edi, 4

    mov     eax, [vesa_framebuffer]
    mov     [edi], eax
    add     edi, 4

    movzx   eax, word [vesa_width]
    mov     [edi], eax
    add     edi, 4

    movzx   eax, word [vesa_height]
    mov     [edi], eax
    add     edi, 4

    movzx   eax, byte [vesa_bpp]
    mov     [edi], eax
    add     edi, 4

    movzx   eax, word [vesa_pitch]
    mov     [edi], eax
    add     edi, 4

    ; Jump to kernel!
    ; Debug: Write '!' to top-left corner of VGA text buffer
    mov     byte [0xB8000], '!'
    mov     byte [0xB8001], 0x4F      ; White on red - visible!
    
    mov     eax, 0x500          ; Boot info pointer
    jmp     0x100000            ; Jump to kernel at 1MB

; ============================================================================
; Data (accessible from both 16 and 32-bit modes)
; ============================================================================

[BITS 16]

boot_drive:         db 0
vesa_enabled:       db 0
vesa_mode:          dw 0
vesa_framebuffer:   dd 0
vesa_width:         dw 0
vesa_height:        dw 0
vesa_bpp:           db 0
vesa_pitch:         dw 0

; Messages
msg_stage2:         db 13, 10
                    db '========================================', 13, 10
                    db '    RUSTACEAN OS - Stage 2 Loader', 13, 10
                    db '========================================', 13, 10, 0
msg_e820:           db '  [....] Querying memory map', 0
msg_a20:            db '  [....] Enabling A20 line', 0
msg_vesa:           db '  [....] Setting up VESA', 0
msg_kernel:         db '  [....] Loading kernel', 0
msg_pmode:          db '  [....] Entering protected mode', 13, 10, 0
msg_ok:             db 13, '  [ OK ]', 13, 10, 0
msg_e820_fail:      db 13, '  [FAIL] E820 query failed!', 13, 10, 0
msg_a20_fail:       db 13, '  [FAIL] Could not enable A20!', 13, 10, 0
msg_vesa_fallback:  db 13, '  [WARN] VESA unavailable, using VGA text', 13, 10, 0
msg_kernel_fail:    db 13, '  [FAIL] Could not load kernel!', 13, 10, 0

; Pad to sector boundary
times 16384 - ($ - $$) db 0     ; 32 sectors = 16KB
