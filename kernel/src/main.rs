//! Rustacean OS Kernel Entry Point
//!
//! This is where the bootloader hands off control to the kernel.
//! We're in 32-bit protected mode with a flat memory model.

#![no_std]
#![no_main]
#![allow(dead_code)]

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
    
    // Verify boot magic
    if !boot_info.verify_magic() {
        // Debug: Write 'X' - magic failed
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
    
    // Initialize VGA/VESA display
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
    let _ = writeln!(writer, "    A Plan 9-style GUI Operating System");
    let _ = writeln!(writer, "========================================");
    let _ = writeln!(writer, "");
    
    // Display boot info
    let _ = writeln!(writer, "[BOOT] Magic verified: 0x{:08X}", boot_info.magic);
    let _ = writeln!(writer, "[BOOT] Display: {}x{} @ {}bpp", 
        boot_info.screen_width,
        boot_info.screen_height,
        boot_info.bits_per_pixel
    );
    let _ = writeln!(writer, "[BOOT] Framebuffer: 0x{:08X}", boot_info.framebuffer_addr);
    let _ = writeln!(writer, "[BOOT] E820 map at: 0x{:08X}", boot_info.e820_map_addr);
    
    // Initialize GDT (our own, replacing bootloader's)
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
    
    let _ = writeln!(writer, "");
    let _ = writeln!(writer, "[READY] Rustacean OS kernel initialized!");
    let _ = writeln!(writer, "[READY] EventChains architecture active.");
    
    // If we have VESA graphics, start the GUI
    if boot_info.vesa_enabled && boot_info.screen_width > 0 {
        let _ = writeln!(writer, "[GUI  ] Starting graphical interface...");
        
        // Small delay to show messages
        for _ in 0..50000000u32 {
            unsafe { core::arch::asm!("nop"); }
        }
        
        run_gui(
            boot_info.framebuffer_addr,
            boot_info.screen_width,
            boot_info.screen_height,
            boot_info.bits_per_pixel / 8, // bytes per pixel
            boot_info.pitch,
        );
    } else {
        let _ = writeln!(writer, "[TEXT] Running in text mode - no GUI available");
        let _ = writeln!(writer, "");
        let _ = writeln!(writer, "Press any key to test keyboard...");
        
        // Simple text mode loop - just show we're alive
        loop {
            unsafe { core::arch::asm!("hlt"); }
        }
    }
}

/// Run the graphical user interface
fn run_gui(fb_addr: u32, width: u32, height: u32, bpp: u32, pitch: u32) -> ! {
    // Initialize framebuffer
    unsafe {
        gui::framebuffer::init(fb_addr as *mut u8, width, height, bpp, pitch);
    }
    
    // Initialize mouse (may not work on all trackpads)
    drivers::mouse::init(width, height);
    
    // Initialize desktop
    gui::desktop::init(width, height);
    
    // Get desktop and framebuffer
    let desktop = gui::desktop::get().expect("Desktop not initialized");
    let fb = gui::framebuffer::get().expect("Framebuffer not initialized");
    
    // Create some demo windows
    desktop.create_window("Welcome to Rustacean OS!", 50, 50, 450, 250);
    desktop.create_window("Terminal", 100, 150, 500, 300);
    desktop.create_window("Files", 520, 80, 300, 250);
    
    // Add welcome text to first window
    desktop.mark_dirty();
    
    // Main GUI event loop
    let mut last_mouse_x = 0i32;
    let mut last_mouse_y = 0i32;
    let mut last_buttons = 0u8;
    
    // Keyboard-controlled cursor position
    let mut kb_cursor_x = (width / 2) as i32;
    let mut kb_cursor_y = (height / 2) as i32;
    let cursor_speed = 8i32;
    
    loop {
        // Check for keyboard input (arrow keys move cursor, Enter = click)
        let scancode = unsafe { 
            // Check if key available
            let status = crate::arch::x86::io::inb(0x64);
            if status & 0x01 != 0 {
                Some(crate::arch::x86::io::inb(0x60))
            } else {
                None
            }
        };
        
        if let Some(code) = scancode {
            let pressed = code & 0x80 == 0;
            let key = code & 0x7F;
            
            if pressed {
                match key {
                    // Arrow keys (using scancodes)
                    0x48 => { // Up
                        kb_cursor_y = (kb_cursor_y - cursor_speed).max(0);
                        desktop.handle_mouse_move(kb_cursor_x, kb_cursor_y);
                    }
                    0x50 => { // Down
                        kb_cursor_y = (kb_cursor_y + cursor_speed).min(height as i32 - 1);
                        desktop.handle_mouse_move(kb_cursor_x, kb_cursor_y);
                    }
                    0x4B => { // Left
                        kb_cursor_x = (kb_cursor_x - cursor_speed).max(0);
                        desktop.handle_mouse_move(kb_cursor_x, kb_cursor_y);
                    }
                    0x4D => { // Right
                        kb_cursor_x = (kb_cursor_x + cursor_speed).min(width as i32 - 1);
                        desktop.handle_mouse_move(kb_cursor_x, kb_cursor_y);
                    }
                    0x1C => { // Enter = left click
                        desktop.handle_mouse_button(gui::MouseButton::Left, true);
                        // Small delay then release
                        for _ in 0..100000u32 { unsafe { core::arch::asm!("nop"); } }
                        desktop.handle_mouse_button(gui::MouseButton::Left, false);
                    }
                    0x39 => { // Space = left click (hold)
                        desktop.handle_mouse_button(gui::MouseButton::Left, true);
                    }
                    // WASD as alternative
                    0x11 => { // W
                        kb_cursor_y = (kb_cursor_y - cursor_speed).max(0);
                        desktop.handle_mouse_move(kb_cursor_x, kb_cursor_y);
                    }
                    0x1F => { // S
                        kb_cursor_y = (kb_cursor_y + cursor_speed).min(height as i32 - 1);
                        desktop.handle_mouse_move(kb_cursor_x, kb_cursor_y);
                    }
                    0x1E => { // A
                        kb_cursor_x = (kb_cursor_x - cursor_speed).max(0);
                        desktop.handle_mouse_move(kb_cursor_x, kb_cursor_y);
                    }
                    0x20 => { // D
                        kb_cursor_x = (kb_cursor_x + cursor_speed).min(width as i32 - 1);
                        desktop.handle_mouse_move(kb_cursor_x, kb_cursor_y);
                    }
                    _ => {}
                }
            } else {
                // Key released
                if key == 0x39 { // Space released
                    desktop.handle_mouse_button(gui::MouseButton::Left, false);
                }
            }
        }
        
        // Check for mouse updates (may work on some systems)
        let (mouse_x, mouse_y) = drivers::mouse::get_position();
        let buttons = drivers::mouse::get_buttons();
        
        // Handle mouse movement
        if mouse_x != last_mouse_x || mouse_y != last_mouse_y {
            desktop.handle_mouse_move(mouse_x, mouse_y);
            // Sync keyboard cursor with mouse
            kb_cursor_x = mouse_x;
            kb_cursor_y = mouse_y;
            last_mouse_x = mouse_x;
            last_mouse_y = mouse_y;
        }
        
        // Handle button changes
        if buttons != last_buttons {
            if (buttons & 0x01) != (last_buttons & 0x01) {
                desktop.handle_mouse_button(gui::MouseButton::Left, buttons & 0x01 != 0);
            }
            if (buttons & 0x02) != (last_buttons & 0x02) {
                desktop.handle_mouse_button(gui::MouseButton::Right, buttons & 0x02 != 0);
            }
            last_buttons = buttons;
        }
        
        // Draw the desktop
        desktop.draw(fb);
        
        // Small yield
        for _ in 0..10000u32 { unsafe { core::arch::asm!("nop"); } }
    }
}
