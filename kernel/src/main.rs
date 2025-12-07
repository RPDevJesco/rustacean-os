//! Rustacean OS Kernel Entry Point
//!
//! This is where the bootloader hands off control to the kernel.
//! We're in 32-bit protected mode with a flat memory model.
//!
//! # EventChains Architecture
//!
//! Rustacean OS uses EventChains for three distinct purposes:
//!
//! 1. **Driver EventChain** (boot time) - Fault-tolerant driver initialization
//!    with graceful degradation when optional drivers fail.
//!
//! 2. **Kernel EventChain** (runtime) - Syscall processing with permission
//!    checking, audit logging, and error handling middleware.
//!
//! 3. **Window Manager EventChain** (runtime) - Discrete window lifecycle
//!    events (create, destroy, focus, z-order) with policy enforcement.
//!
//! Hot paths (mouse tracking, frame rendering, scheduler) stay outside
//! EventChains for performance.

#![no_std]
#![no_main]
#![allow(dead_code)]

extern crate alloc;

// Core kernel modules
mod boot_info;
mod arch;
mod mm;
mod sched;
mod event_chains;
mod syscall;
mod drivers;
mod fs;
mod gui;

use boot_info::BootInfo;
use drivers::vga;
use arch::x86::{gdt, idt};

use core::fmt::Write;
use core::arch::global_asm;

// Panic handler - required for no_std
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    // Try to print panic info if we have a console
    if let Some(writer) = unsafe { drivers::vga::WRITER.as_mut() } {
        let _ = writeln!(writer, "\n!!! KERNEL PANIC !!!");
        let _ = writeln!(writer, "{}", info);
    }

    // Halt the CPU
    loop {
        unsafe {
            core::arch::asm!("cli; hlt");
        }
    }
}

// Assembly entry point - uses global_asm! which is stable
global_asm!(
    ".section .text.boot",
    ".global _start",
    "_start:",
    // Debug: Write 'K' to VGA to prove we got here
    "    mov byte ptr [0xB8002], 0x4B",   // 'K'
    "    mov byte ptr [0xB8003], 0x2F",   // Green on white
    "    push eax",           // EAX contains boot_info pointer from bootloader
    // Debug: Write '1'
    "    mov byte ptr [0xB8004], 0x31",   // '1'
    "    mov byte ptr [0xB8005], 0x2F",
    "    call kernel_main",   // Call Rust entry point
    "2:",
    "    cli",
    "    hlt",
    "    jmp 2b",
);

/// Main kernel initialization
///
/// Called from _start with boot_info pointer on stack
#[no_mangle]
extern "C" fn kernel_main(boot_info_ptr: u32) -> ! {
    // Debug: Write '2' to VGA - we made it into Rust!
    unsafe {
        let vga = 0xB8006 as *mut u8;
        vga.write_volatile(b'2');
        vga.add(1).write_volatile(0x2F); // Green on white
    }

    // Parse boot info from bootloader
    let boot_info = unsafe { BootInfo::from_ptr(boot_info_ptr as *const u8) };

    // Debug: Write '3' - boot info parsed
    unsafe {
        let vga = 0xB8008 as *mut u8;
        vga.write_volatile(b'3');
        vga.add(1).write_volatile(0x2F);
    }

    // Initialize heap allocator (enables Box, Vec, String)
    unsafe {
        mm::heap::init();
    }

    // Verify boot magic
    if !boot_info.verify_magic() {
        unsafe {
            let vga = 0xB800A as *mut u8;
            vga.write_volatile(b'X');
            vga.add(1).write_volatile(0x4F); // White on red
        }
        loop {
            unsafe { core::arch::asm!("cli; hlt"); }
        }
    }

    // Debug: Write '4' - magic verified
    unsafe {
        let vga = 0xB800A as *mut u8;
        vga.write_volatile(b'4');
        vga.add(1).write_volatile(0x2F);
    }

    // Initialize VGA/VESA display for boot messages
    unsafe {
        if boot_info.vesa_enabled {
            vga::init_framebuffer(
                boot_info.framebuffer_addr,
                boot_info.screen_width,
                boot_info.screen_height,
                boot_info.bits_per_pixel,
                boot_info.pitch,
            );
        } else {
            vga::init_text_mode();
        }
    }

    // Now we can print!
    let writer = unsafe { vga::WRITER.as_mut().unwrap() };

    let _ = writeln!(writer, "");
    let _ = writeln!(writer, "========================================");
    let _ = writeln!(writer, "    RUSTACEAN OS v0.1.0");
    let _ = writeln!(writer, "    EventChains Architecture");
    let _ = writeln!(writer, "========================================");
    let _ = writeln!(writer, "");

    // Display boot info
    let _ = writeln!(writer, "[BOOT] Display: {}x{} @ {}bpp",
                     boot_info.screen_width,
                     boot_info.screen_height,
                     boot_info.bits_per_pixel
    );
    let _ = writeln!(writer, "[BOOT] Framebuffer: 0x{:08X}", boot_info.framebuffer_addr);

    // Initialize GDT
    let _ = write!(writer, "[INIT] Loading GDT...");
    gdt::init();
    let _ = writeln!(writer, " OK");

    // Initialize IDT
    let _ = write!(writer, "[INIT] Loading IDT...");
    idt::init();
    let _ = writeln!(writer, " OK");

    // Parse E820 memory map and initialize memory manager
    let _ = write!(writer, "[INIT] Parsing E820 memory map...");
    let mem_info = mm::init(boot_info.e820_map_addr);
    let _ = writeln!(writer, " OK");
    let _ = writeln!(writer, "[MEM ] Total: {} KB, Usable: {} KB",
                     mem_info.total_kb,
                     mem_info.usable_kb
    );

    // Enable interrupts
    let _ = write!(writer, "[INIT] Enabling interrupts...");
    unsafe { core::arch::asm!("sti"); }
    let _ = writeln!(writer, " OK");

    // If we have VESA graphics, start the GUI
    if boot_info.vesa_enabled && boot_info.screen_width > 0 {
        let _ = writeln!(writer, "");
        let _ = writeln!(writer, "[DRV ] Initializing drivers via EventChain...");

        // Use Driver EventChain for fault-tolerant initialization
        let drv_result = drivers::init_all_drivers(
            boot_info.framebuffer_addr,
            boot_info.screen_width,
            boot_info.screen_height,
            boot_info.bits_per_pixel / 8,
            boot_info.pitch,
        );

        // Report driver initialization results
        let _ = writeln!(writer, "[DRV ] GPU: {}", drv_result.gpu_type_str());
        let _ = writeln!(writer, "[DRV ] Input: {}", drv_result.input_type_str());
        let _ = writeln!(writer, "[DRV ] Hardware cursor: {}",
                         if drv_result.hw_cursor { "yes" } else { "no" });

        // Report any failures (non-fatal in BestEffort mode)
        if drv_result.failure_count > 0 {
            let _ = writeln!(writer, "[DRV ] Failures (non-fatal):");
            for i in 0..drv_result.failure_count {
                if let Some(name) = drv_result.failures[i] {
                    let _ = writeln!(writer, "[DRV ]   - {}", name);
                }
            }
        }

        let _ = writeln!(writer, "");
        let _ = writeln!(writer, "[READY] Rustacean OS kernel initialized!");
        let _ = writeln!(writer, "[READY] EventChains: Driver, Kernel, WindowManager");
        let _ = writeln!(writer, "[GUI  ] Starting graphical interface...");

        // Small delay to show messages
        for _ in 0..50000000u32 {
            unsafe { core::arch::asm!("nop"); }
        }

        run_gui(drv_result);
    } else {
        let _ = writeln!(writer, "[TEXT] Running in text mode - no GUI available");
        loop {
            unsafe { core::arch::asm!("hlt"); }
        }
    }
}

// =============================================================================
// Back buffer for double buffering (in BSS section - regular RAM)
// =============================================================================
static mut BACK_BUFFER_DATA: [u8; 800 * 600 * 4] = [0u8; 800 * 600 * 4];

/// Run the graphical user interface
///
/// Uses:
/// - Driver EventChain results for display/input configuration
/// - Window Manager EventChain for discrete window events
/// - Direct calls for hot path (mouse tracking, rendering)
fn run_gui(drv: drivers::DriverInitResult) -> ! {
    // Create back buffer for double buffering
    let mut back_buffer = unsafe {
        gui::Framebuffer::new(
            BACK_BUFFER_DATA.as_mut_ptr(),
            drv.width,
            drv.height,
            drv.bpp,
            drv.pitch,
        )
    };

    // Initialize desktop window manager with hardware cursor support
    gui::desktop::init_with_hw_cursor(drv.width, drv.height, drv.hw_cursor);

    let desktop = gui::desktop::get().expect("Desktop not initialized");
    let fb = gui::framebuffer::get().expect("Framebuffer not initialized");

    // Create demo windows (goes through WM EventChain)
    desktop.create_window("Welcome to Rustacean OS!", 50, 50, 450, 220);
    desktop.create_terminal_window(100, 280, 400, 180);  // Heap-allocated terminal!
    desktop.create_window("Files", 470, 50, 300, 220);

    desktop.mark_dirty();

    // =========================================================================
    // Main GUI event loop (Polling Mode)
    // =========================================================================

    // Disable keyboard (IRQ1) and mouse (IRQ12) interrupts - we'll poll instead
    // This avoids race conditions between IRQ handlers and our polling loop
    unsafe {
        // Disable IRQ1 (keyboard) on master PIC
        let mask = crate::arch::x86::io::inb(0x21);
        crate::arch::x86::io::outb(0x21, mask | 0x02);  // Set bit 1

        // Disable IRQ12 (mouse) on slave PIC
        let mask = crate::arch::x86::io::inb(0xA1);
        crate::arch::x86::io::outb(0xA1, mask | 0x10);  // Set bit 4
    }

    let mut last_mouse_x = (drv.width / 2) as i32;
    let mut last_mouse_y = (drv.height / 2) as i32;
    let mut last_buttons = 0u8;

    // Keyboard-controlled cursor (fallback)
    let mut kb_cursor_x = last_mouse_x;
    let mut kb_cursor_y = last_mouse_y;
    let cursor_speed = 8i32;

    let using_synaptics = drv.is_synaptics();
    let using_ati_rage = drv.is_ati_rage();

    loop {
        // =====================================================================
        // Poll PS/2 controller - route keyboard and mouse data to drivers
        // =====================================================================
        unsafe {
            let status = crate::arch::x86::io::inb(0x64);

            // Check if output buffer has data (bit 0)
            if status & 0x01 != 0 {
                let data = crate::arch::x86::io::inb(0x60);

                // Bit 5 tells us if it's from auxiliary device (mouse/touchpad)
                if status & 0x20 == 0 {
                    // Keyboard data - process through keyboard driver
                    drivers::keyboard::KEYBOARD.process_scancode(data);
                } else {
                    // Mouse/touchpad data - route to appropriate driver
                    if using_synaptics {
                        drivers::synaptics::handle_irq_byte(data);
                    } else {
                        drivers::mouse::MOUSE.process_byte(data);
                    }
                }
            }
        }

        // =====================================================================
        // Handle keyboard input - poll driver buffer
        // =====================================================================
        while let Some(key) = drivers::keyboard::get_key() {
            use drivers::keyboard::KeyCode;

            if desktop.is_terminal_focused() {
                // Terminal input mode
                match key.keycode {
                    KeyCode::Enter => desktop.term_enter(),
                    KeyCode::Backspace => desktop.term_backspace(),
                    KeyCode::Up => {
                        kb_cursor_y = (kb_cursor_y - cursor_speed).max(0);
                        desktop.handle_mouse_move(kb_cursor_x, kb_cursor_y);
                    }
                    KeyCode::Down => {
                        kb_cursor_y = (kb_cursor_y + cursor_speed).min(drv.height as i32 - 1);
                        desktop.handle_mouse_move(kb_cursor_x, kb_cursor_y);
                    }
                    KeyCode::Left => {
                        kb_cursor_x = (kb_cursor_x - cursor_speed).max(0);
                        desktop.handle_mouse_move(kb_cursor_x, kb_cursor_y);
                    }
                    KeyCode::Right => {
                        kb_cursor_x = (kb_cursor_x + cursor_speed).min(drv.width as i32 - 1);
                        desktop.handle_mouse_move(kb_cursor_x, kb_cursor_y);
                    }
                    _ => {
                        // Send printable characters to terminal
                        if let Some(c) = key.ascii {
                            desktop.term_key_input(c);
                        }
                    }
                }
            } else {
                // Window navigation mode
                match key.keycode {
                    KeyCode::Up | KeyCode::W => {
                        kb_cursor_y = (kb_cursor_y - cursor_speed).max(0);
                        desktop.handle_mouse_move(kb_cursor_x, kb_cursor_y);
                    }
                    KeyCode::Down | KeyCode::S => {
                        kb_cursor_y = (kb_cursor_y + cursor_speed).min(drv.height as i32 - 1);
                        desktop.handle_mouse_move(kb_cursor_x, kb_cursor_y);
                    }
                    KeyCode::Left | KeyCode::A => {
                        kb_cursor_x = (kb_cursor_x - cursor_speed).max(0);
                        desktop.handle_mouse_move(kb_cursor_x, kb_cursor_y);
                    }
                    KeyCode::Right | KeyCode::D => {
                        kb_cursor_x = (kb_cursor_x + cursor_speed).min(drv.width as i32 - 1);
                        desktop.handle_mouse_move(kb_cursor_x, kb_cursor_y);
                    }
                    KeyCode::Enter => unsafe {
                        desktop.handle_mouse_button(gui::MouseButton::Left, true);
                        for _ in 0..100000u32 { core::arch::asm!("nop"); }
                        desktop.handle_mouse_button(gui::MouseButton::Left, false);
                    }
                    KeyCode::Space => {
                        desktop.handle_mouse_button(gui::MouseButton::Left, true);
                    }
                    _ => {}
                }
            }
        }

        // =====================================================================
        // Handle pointing device input (direct - hot path)
        // =====================================================================
        let (mouse_x, mouse_y, buttons) = if using_synaptics {
            let (x, y) = drivers::synaptics::get_position();
            let btns = drivers::synaptics::get_buttons();
            (x, y, btns)
        } else {
            let (x, y) = drivers::mouse::get_position();
            let btns = drivers::mouse::get_buttons();
            (x, y, btns)
        };

        if mouse_x != last_mouse_x || mouse_y != last_mouse_y {
            desktop.handle_mouse_move(mouse_x, mouse_y);
            kb_cursor_x = mouse_x;
            kb_cursor_y = mouse_y;

            if using_ati_rage {
                if let Some(gpu) = drivers::ati_rage::get() {
                    gpu.set_cursor_pos(mouse_x, mouse_y);
                }
            }

            last_mouse_x = mouse_x;
            last_mouse_y = mouse_y;
        }

        if buttons != last_buttons {
            if (buttons & 0x01) != (last_buttons & 0x01) {
                desktop.handle_mouse_button(gui::MouseButton::Left, buttons & 0x01 != 0);
            }
            if (buttons & 0x02) != (last_buttons & 0x02) {
                desktop.handle_mouse_button(gui::MouseButton::Right, buttons & 0x02 != 0);
            }
            if (buttons & 0x04) != (last_buttons & 0x04) {
                desktop.handle_mouse_button(gui::MouseButton::Middle, buttons & 0x04 != 0);
            }
            last_buttons = buttons;
        }

        // =====================================================================
        // Draw the desktop (direct - hot path, double buffered)
        // =====================================================================
        desktop.draw(&mut back_buffer, fb);

        // Small yield
        for _ in 0..10000u32 {
            unsafe { core::arch::asm!("nop"); }
        }
    }
}
