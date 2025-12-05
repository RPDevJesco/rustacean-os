//! Desktop / Window Manager
//!
//! Manages windows, mouse cursor, and desktop events.

use super::{Window, Framebuffer, Color, Rect, Point, theme, font, GuiEvent, MouseButton};

/// Maximum number of windows
const MAX_WINDOWS: usize = 32;

/// Mouse cursor bitmap (16x16)
const CURSOR_WIDTH: u32 = 16;
const CURSOR_HEIGHT: u32 = 16;

static CURSOR_BITMAP: [u16; 16] = [
    0b1000000000000000,
    0b1100000000000000,
    0b1110000000000000,
    0b1111000000000000,
    0b1111100000000000,
    0b1111110000000000,
    0b1111111000000000,
    0b1111111100000000,
    0b1111111110000000,
    0b1111111111000000,
    0b1111110000000000,
    0b1101111000000000,
    0b1000111100000000,
    0b0000011110000000,
    0b0000011110000000,
    0b0000001100000000,
];

static CURSOR_MASK: [u16; 16] = [
    0b1100000000000000,
    0b1110000000000000,
    0b1111000000000000,
    0b1111100000000000,
    0b1111110000000000,
    0b1111111000000000,
    0b1111111100000000,
    0b1111111110000000,
    0b1111111111000000,
    0b1111111111100000,
    0b1111111111000000,
    0b1111111110000000,
    0b1101111111000000,
    0b1000111111000000,
    0b0000111111000000,
    0b0000011110000000,
];

/// Desktop state
pub struct Desktop {
    /// All windows
    windows: [Option<Window>; MAX_WINDOWS],
    /// Window Z-order (indices into windows array, front to back)
    z_order: [usize; MAX_WINDOWS],
    /// Number of windows
    window_count: usize,
    /// Currently focused window index
    focused: Option<usize>,
    /// Mouse position
    mouse_x: i32,
    mouse_y: i32,
    /// Mouse buttons state
    mouse_buttons: u8,
    /// Window being dragged
    dragging: Option<usize>,
    /// Drag offset from window corner
    drag_offset: Point,
    /// Screen dimensions
    screen_width: u32,
    screen_height: u32,
    /// Next window ID
    next_id: u32,
    /// Desktop needs full redraw
    dirty: bool,
    /// Saved pixels under cursor
    cursor_save: [Color; 256], // 16x16
    cursor_save_x: i32,
    cursor_save_y: i32,
}

impl Desktop {
    /// Create a new desktop
    pub fn new(screen_width: u32, screen_height: u32) -> Self {
        const NONE_WINDOW: Option<Window> = None;
        
        Self {
            windows: [NONE_WINDOW; MAX_WINDOWS],
            z_order: [0; MAX_WINDOWS],
            window_count: 0,
            focused: None,
            mouse_x: (screen_width / 2) as i32,
            mouse_y: (screen_height / 2) as i32,
            mouse_buttons: 0,
            dragging: None,
            drag_offset: Point::new(0, 0),
            screen_width,
            screen_height,
            next_id: 1,
            dirty: true,
            cursor_save: [Color::BLACK; 256],
            cursor_save_x: -1,
            cursor_save_y: -1,
        }
    }
    
    /// Create a new window
    pub fn create_window(&mut self, title: &str, x: i32, y: i32, width: u32, height: u32) -> Option<u32> {
        // Find free slot
        let slot = self.windows.iter().position(|w| w.is_none())?;
        
        let id = self.next_id;
        self.next_id += 1;
        
        let mut window = Window::new(id, title, x, y, width, height);
        window.flags.focused = self.window_count == 0;
        
        self.windows[slot] = Some(window);
        self.z_order[self.window_count] = slot;
        self.window_count += 1;
        
        if self.focused.is_none() {
            self.focused = Some(slot);
        }
        
        self.dirty = true;
        Some(id)
    }
    
    /// Close a window by ID
    pub fn close_window(&mut self, id: u32) {
        if let Some(slot) = self.find_window_slot(id) {
            self.windows[slot] = None;
            
            // Remove from z_order
            if let Some(z_pos) = self.z_order[..self.window_count].iter().position(|&s| s == slot) {
                for i in z_pos..self.window_count - 1 {
                    self.z_order[i] = self.z_order[i + 1];
                }
                self.window_count -= 1;
            }
            
            // Update focus
            if self.focused == Some(slot) {
                self.focused = if self.window_count > 0 {
                    Some(self.z_order[0])
                } else {
                    None
                };
            }
            
            self.dirty = true;
        }
    }
    
    /// Get a window by ID
    pub fn get_window(&mut self, id: u32) -> Option<&mut Window> {
        self.find_window_slot(id)
            .and_then(|slot| self.windows[slot].as_mut())
    }
    
    /// Find window slot by ID
    fn find_window_slot(&self, id: u32) -> Option<usize> {
        self.windows.iter().position(|w| {
            w.as_ref().map_or(false, |win| win.id == id)
        })
    }
    
    /// Find window at screen position (top-most first)
    fn window_at(&self, x: i32, y: i32) -> Option<usize> {
        for i in 0..self.window_count {
            let slot = self.z_order[i];
            if let Some(ref window) = self.windows[slot] {
                if window.flags.visible && window.contains(x, y) {
                    return Some(slot);
                }
            }
        }
        None
    }
    
    /// Bring window to front
    fn bring_to_front(&mut self, slot: usize) {
        if let Some(z_pos) = self.z_order[..self.window_count].iter().position(|&s| s == slot) {
            // Shift others back
            for i in (1..=z_pos).rev() {
                self.z_order[i] = self.z_order[i - 1];
            }
            self.z_order[0] = slot;
        }
        
        // Update focus
        if let Some(old_focused) = self.focused {
            if let Some(ref mut win) = self.windows[old_focused] {
                win.flags.focused = false;
            }
        }
        
        if let Some(ref mut win) = self.windows[slot] {
            win.flags.focused = true;
        }
        
        self.focused = Some(slot);
        self.dirty = true;
    }
    
    /// Handle mouse movement
    pub fn handle_mouse_move(&mut self, x: i32, y: i32) {
        self.mouse_x = x.max(0).min(self.screen_width as i32 - 1);
        self.mouse_y = y.max(0).min(self.screen_height as i32 - 1);
        
        // Handle dragging
        if let Some(slot) = self.dragging {
            if let Some(ref mut window) = self.windows[slot] {
                let new_x = self.mouse_x - self.drag_offset.x;
                let new_y = self.mouse_y - self.drag_offset.y;
                window.move_to(new_x, new_y);
                self.dirty = true;
            }
        }
    }
    
    /// Handle mouse button
    pub fn handle_mouse_button(&mut self, button: MouseButton, pressed: bool) {
        let bit = match button {
            MouseButton::Left => 1,
            MouseButton::Middle => 2,
            MouseButton::Right => 4,
        };
        
        if pressed {
            self.mouse_buttons |= bit;
            
            // Check what we clicked on
            if let Some(slot) = self.window_at(self.mouse_x, self.mouse_y) {
                // Bring to front if not already focused
                if self.focused != Some(slot) {
                    self.bring_to_front(slot);
                }
                
                // Check if clicking title bar (start drag)
                if let Some(ref window) = self.windows[slot] {
                    if button == MouseButton::Left && window.in_title_bar(self.mouse_x, self.mouse_y) {
                        self.dragging = Some(slot);
                        self.drag_offset = Point::new(
                            self.mouse_x - window.bounds.x,
                            self.mouse_y - window.bounds.y,
                        );
                    }
                }
            }
        } else {
            self.mouse_buttons &= !bit;
            
            // Stop dragging
            if button == MouseButton::Left {
                self.dragging = None;
            }
        }
    }
    
    /// Handle keyboard input
    pub fn handle_key(&mut self, key: char, pressed: bool) {
        // Forward to focused window (would go through EventChain in full implementation)
        if let Some(slot) = self.focused {
            if let Some(ref mut _window) = self.windows[slot] {
                // In a full implementation, this would dispatch to the window's event handler
            }
        }
    }
    
    /// Save pixels under cursor
    fn save_cursor_area(&mut self, fb: &Framebuffer) {
        let x = self.mouse_x;
        let y = self.mouse_y;
        
        for cy in 0..CURSOR_HEIGHT as i32 {
            for cx in 0..CURSOR_WIDTH as i32 {
                let idx = (cy * CURSOR_WIDTH as i32 + cx) as usize;
                if idx < 256 {
                    self.cursor_save[idx] = fb.get_pixel(x + cx, y + cy)
                        .unwrap_or(Color::BLACK);
                }
            }
        }
        
        self.cursor_save_x = x;
        self.cursor_save_y = y;
    }
    
    /// Restore pixels under cursor
    fn restore_cursor_area(&self, fb: &mut Framebuffer) {
        if self.cursor_save_x < 0 {
            return;
        }
        
        let x = self.cursor_save_x;
        let y = self.cursor_save_y;
        
        for cy in 0..CURSOR_HEIGHT as i32 {
            for cx in 0..CURSOR_WIDTH as i32 {
                let idx = (cy * CURSOR_WIDTH as i32 + cx) as usize;
                if idx < 256 {
                    fb.set_pixel(x + cx, y + cy, self.cursor_save[idx]);
                }
            }
        }
    }
    
    /// Draw cursor at current position
    fn draw_cursor(&mut self, fb: &mut Framebuffer) {
        self.save_cursor_area(fb);
        
        let x = self.mouse_x;
        let y = self.mouse_y;
        
        for cy in 0..16i32 {
            let bitmap_row = CURSOR_BITMAP[cy as usize];
            let mask_row = CURSOR_MASK[cy as usize];
            
            for cx in 0..16i32 {
                let bit = 15 - cx;
                let mask_bit = (mask_row >> bit) & 1;
                let color_bit = (bitmap_row >> bit) & 1;
                
                if mask_bit != 0 {
                    let color = if color_bit != 0 {
                        Color::WHITE
                    } else {
                        Color::BLACK
                    };
                    fb.set_pixel(x + cx, y + cy, color);
                }
            }
        }
    }
    
    /// Draw the entire desktop
    pub fn draw(&mut self, fb: &mut Framebuffer) {
        let theme = theme::current();
        
        // Restore cursor area first
        self.restore_cursor_area(fb);
        
        if self.dirty {
            // Draw desktop background
            fb.clear(theme.desktop_bg);
            
            // Draw windows back to front
            for i in (0..self.window_count).rev() {
                let slot = self.z_order[i];
                if let Some(ref window) = self.windows[slot] {
                    if window.flags.visible {
                        window.draw(fb);
                    }
                }
            }
            
            self.dirty = false;
        }
        
        // Always draw cursor on top
        self.draw_cursor(fb);
    }
    
    /// Mark desktop as dirty
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }
    
    /// Get mouse position
    pub fn mouse_pos(&self) -> (i32, i32) {
        (self.mouse_x, self.mouse_y)
    }
    
    /// Get focused window ID
    pub fn focused_window(&self) -> Option<u32> {
        self.focused.and_then(|slot| {
            self.windows[slot].as_ref().map(|w| w.id)
        })
    }
}

// Global desktop instance
static mut DESKTOP: Option<Desktop> = None;

/// Initialize the global desktop
pub fn init(width: u32, height: u32) {
    unsafe {
        DESKTOP = Some(Desktop::new(width, height));
    }
}

/// Get the global desktop
pub fn get() -> Option<&'static mut Desktop> {
    unsafe { DESKTOP.as_mut() }
}
