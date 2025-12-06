//! Synaptics PS/2 TouchPad Driver
//!
//! Driver for Synaptics touchpads commonly found in laptops like the Compaq Armada E500.
//! These touchpads require special initialization sequences beyond standard PS/2 mouse
//! protocol to function properly.
//!
//! The Synaptics touchpad operates in two modes:
//! - **Relative mode**: Standard PS/2 mouse compatible (3-byte packets)
//! - **Absolute mode**: Enhanced mode with position, pressure, finger width (6-byte packets)
//!
//! This driver implements the "magic knock" sequence to identify Synaptics hardware
//! and enables absolute mode for proper touchpad functionality.

use crate::arch::x86::io::{inb, outb};

// =============================================================================
// PS/2 Controller Constants
// =============================================================================

/// PS/2 data port
const PS2_DATA: u16 = 0x60;
/// PS/2 status/command port
const PS2_STATUS: u16 = 0x64;
const PS2_COMMAND: u16 = 0x64;

/// Status register bits
const PS2_STATUS_OUTPUT_FULL: u8 = 0x01;
const PS2_STATUS_INPUT_FULL: u8 = 0x02;
const PS2_STATUS_AUX_DATA: u8 = 0x20;

/// PS/2 controller commands
const PS2_CMD_READ_CONFIG: u8 = 0x20;
const PS2_CMD_WRITE_CONFIG: u8 = 0x60;
const PS2_CMD_DISABLE_AUX: u8 = 0xA7;
const PS2_CMD_ENABLE_AUX: u8 = 0xA8;
const PS2_CMD_TEST_AUX: u8 = 0xA9;
const PS2_CMD_WRITE_AUX: u8 = 0xD4;

/// PS/2 mouse commands
const MOUSE_CMD_RESET: u8 = 0xFF;
const MOUSE_CMD_RESEND: u8 = 0xFE;
const MOUSE_CMD_SET_DEFAULTS: u8 = 0xF6;
const MOUSE_CMD_DISABLE: u8 = 0xF5;
const MOUSE_CMD_ENABLE: u8 = 0xF4;
const MOUSE_CMD_SET_SAMPLE_RATE: u8 = 0xF3;
const MOUSE_CMD_GET_ID: u8 = 0xF2;
const MOUSE_CMD_SET_REMOTE: u8 = 0xF0;
const MOUSE_CMD_SET_WRAP: u8 = 0xEE;
const MOUSE_CMD_RESET_WRAP: u8 = 0xEC;
const MOUSE_CMD_READ_DATA: u8 = 0xEB;
const MOUSE_CMD_SET_STREAM: u8 = 0xEA;
const MOUSE_CMD_STATUS_REQUEST: u8 = 0xE9;
const MOUSE_CMD_SET_RESOLUTION: u8 = 0xE8;
const MOUSE_CMD_SET_SCALING_2_1: u8 = 0xE7;
const MOUSE_CMD_SET_SCALING_1_1: u8 = 0xE6;

/// Mouse responses
const MOUSE_ACK: u8 = 0xFA;
const MOUSE_RESEND: u8 = 0xFE;
const MOUSE_ERROR: u8 = 0xFC;
const MOUSE_BAT_OK: u8 = 0xAA;

// =============================================================================
// Synaptics-Specific Constants
// =============================================================================

/// Synaptics identification magic sequence sample rates
/// Send SET_SAMPLE_RATE with these values in order to trigger identification
const SYNAPTICS_MAGIC_RATES: [u8; 4] = [200, 100, 80, 0];

/// Synaptics model ID query (via special sequence)
const SYNAPTICS_MODEL_ID_RATES: [u8; 3] = [200, 200, 200];

/// Synaptics capabilities query
const SYNAPTICS_CAP_RATES: [u8; 3] = [200, 100, 100];

/// Synaptics extended capabilities query
const SYNAPTICS_EXT_CAP_RATES: [u8; 3] = [200, 200, 100];

/// Synaptics modes
mod synaptics_mode {
    pub const ABSOLUTE: u8 = 0x80;
    pub const HIGH_RATE: u8 = 0x40;
    pub const SLEEP: u8 = 0x08;
    pub const GESTURE: u8 = 0x04;
    pub const FOUR_BYTE_PACKETS: u8 = 0x02;  // W mode
    pub const WMODE: u8 = 0x01;
}

/// Synaptics capability bits
mod synaptics_cap {
    pub const EXTENDED: u32 = 1 << 23;
    pub const MIDDLE_BUTTON: u32 = 1 << 18;
    pub const PASS_THROUGH: u32 = 1 << 7;
    pub const SLEEP: u32 = 1 << 4;
    pub const FOUR_BUTTON: u32 = 1 << 3;
    pub const BALLISTICS: u32 = 1 << 2;
    pub const MULTI_FINGER: u32 = 1 << 1;
    pub const PALM_DETECT: u32 = 1 << 0;
}

// =============================================================================
// Touchpad State
// =============================================================================

/// Touchpad hardware information
#[derive(Debug, Clone, Copy)]
pub struct SynapticsInfo {
    /// Firmware version
    pub firmware_id: u32,
    /// Model ID
    pub model_id: u32,
    /// Capabilities bitmap
    pub capabilities: u32,
    /// Extended capabilities
    pub ext_capabilities: u32,
    /// Has extended capabilities?
    pub has_extended: bool,
    /// Supports multi-finger detection?
    pub multi_finger: bool,
    /// Supports palm detection?
    pub palm_detect: bool,
    /// Has middle button?
    pub middle_button: bool,
    /// Has pass-through port (for TrackPoint)?
    pub pass_through: bool,
}

impl SynapticsInfo {
    const fn empty() -> Self {
        Self {
            firmware_id: 0,
            model_id: 0,
            capabilities: 0,
            ext_capabilities: 0,
            has_extended: false,
            multi_finger: false,
            palm_detect: false,
            middle_button: false,
            pass_through: false,
        }
    }
}

/// Finger state from touchpad
#[derive(Debug, Clone, Copy, Default)]
pub struct FingerState {
    /// X position (0-6143 typically)
    pub x: u16,
    /// Y position (0-6143 typically)
    pub y: u16,
    /// Pressure (Z value, 0-255)
    pub pressure: u8,
    /// Finger width (W value, 0-15)
    pub width: u8,
    /// Left button pressed
    pub left: bool,
    /// Right button pressed
    pub right: bool,
    /// Middle button pressed
    pub middle: bool,
    /// Finger is touching the pad
    pub finger_down: bool,
}

/// TouchPad driver state
pub struct SynapticsTouchpad {
    /// Has the driver been initialized?
    pub is_initialized: bool,
    /// Is this a Synaptics device?
    is_synaptics: bool,
    /// Currently in absolute mode?
    absolute_mode: bool,
    /// Hardware information
    info: SynapticsInfo,
    /// Packet buffer (6 bytes for absolute mode)
    packet: [u8; 6],
    /// Current byte in packet
    packet_idx: usize,
    /// Expected packet size (3 for relative, 6 for absolute)
    packet_size: usize,
    /// Last known finger state
    finger: FingerState,
    /// Previous finger state (for gesture detection)
    prev_finger: FingerState,
    /// Screen dimensions for coordinate scaling
    screen_width: u32,
    screen_height: u32,
    /// Scaled cursor position
    cursor_x: i32,
    cursor_y: i32,
    /// Tap detection state
    tap_counter: u32,  // Simple counter instead of PIT ticks
    tap_start_x: u16,
    tap_start_y: u16,
    was_finger_down: bool,
    /// Tap-to-click enabled
    tap_enabled: bool,
    /// Sensitivity multiplier (fixed point, 256 = 1.0)
    sensitivity: u32,
}

impl SynapticsTouchpad {
    /// Create a new touchpad driver instance
    pub const fn new() -> Self {
        Self {
            is_initialized: false,
            is_synaptics: false,
            absolute_mode: false,
            info: SynapticsInfo::empty(),
            packet: [0; 6],
            packet_idx: 0,
            packet_size: 3,
            finger: FingerState {
                x: 0, y: 0, pressure: 0, width: 0,
                left: false, right: false, middle: false,
                finger_down: false,
            },
            prev_finger: FingerState {
                x: 0, y: 0, pressure: 0, width: 0,
                left: false, right: false, middle: false,
                finger_down: false,
            },
            screen_width: 800,
            screen_height: 600,
            cursor_x: 400,
            cursor_y: 300,
            tap_counter: 0,
            tap_start_x: 0,
            tap_start_y: 0,
            was_finger_down: false,
            tap_enabled: true,
            sensitivity: 256,
        }
    }

    /// Set screen dimensions for coordinate scaling
    pub fn set_screen_size(&mut self, width: u32, height: u32) {
        self.screen_width = width;
        self.screen_height = height;
        self.cursor_x = (width / 2) as i32;
        self.cursor_y = (height / 2) as i32;
    }

    /// Initialize the touchpad
    pub fn init(&mut self) -> Result<(), &'static str> {
        // Reset the PS/2 auxiliary device
        self.ps2_reset()?;

        // Try to identify as Synaptics
        if self.identify_synaptics()? {
            self.is_synaptics = true;

            // Query capabilities
            self.query_capabilities()?;

            // Enable absolute mode
            self.set_absolute_mode(true)?;

            // Enable the device
            self.ps2_send_command(MOUSE_CMD_ENABLE)?;
        } else {
            // Fall back to standard PS/2 mouse mode
            self.is_synaptics = false;
            self.absolute_mode = false;
            self.packet_size = 3;

            // Standard mouse initialization
            self.ps2_send_command(MOUSE_CMD_SET_DEFAULTS)?;
            self.ps2_send_command(MOUSE_CMD_ENABLE)?;
        }

        self.is_initialized = true;
        Ok(())
    }

    /// Reset the PS/2 auxiliary device
    fn ps2_reset(&mut self) -> Result<(), &'static str> {
        // Send reset command
        self.ps2_write_aux(MOUSE_CMD_RESET)?;

        // Wait for BAT completion (0xAA) and device ID
        let bat = self.ps2_read_timeout(500)?;
        if bat != MOUSE_BAT_OK {
            return Err("TouchPad BAT failed");
        }

        // Read device ID (should be 0x00 for standard mouse)
        let _id = self.ps2_read_timeout(100).unwrap_or(0);

        Ok(())
    }

    /// Identify if this is a Synaptics touchpad
    fn identify_synaptics(&mut self) -> Result<bool, &'static str> {
        // The Synaptics "magic knock" sequence:
        // 1. Set scaling 1:1
        // 2. Set scaling 1:1
        // 3. Set scaling 1:1
        // 4. Request status
        // If Synaptics, the status response contains the firmware ID

        self.ps2_send_command(MOUSE_CMD_SET_SCALING_1_1)?;
        self.ps2_send_command(MOUSE_CMD_SET_SCALING_1_1)?;
        self.ps2_send_command(MOUSE_CMD_SET_SCALING_1_1)?;

        // Request status - Synaptics returns special info in status bytes
        self.ps2_write_aux(MOUSE_CMD_STATUS_REQUEST)?;

        let status1 = self.ps2_read_timeout(100)?;
        let status2 = self.ps2_read_timeout(100)?;
        let status3 = self.ps2_read_timeout(100)?;

        // Synaptics returns major version in byte 2, minor in byte 1
        // Standard mouse returns button/resolution info
        // Synaptics has bit 6 of byte 1 set (0x47 pattern)

        if status2 == 0x47 {
            // This is a Synaptics touchpad!
            self.info.firmware_id = ((status1 as u32) << 16) |
                ((status2 as u32) << 8) |
                (status3 as u32);
            return Ok(true);
        }

        // Alternative identification: try the special sample rate sequence
        // E8 00, E8 00, E8 00, E8 00, E9 (read identify)
        self.ps2_send_command(MOUSE_CMD_SET_RESOLUTION)?;
        self.ps2_write_aux(0)?;
        self.ps2_send_command(MOUSE_CMD_SET_RESOLUTION)?;
        self.ps2_write_aux(0)?;
        self.ps2_send_command(MOUSE_CMD_SET_RESOLUTION)?;
        self.ps2_write_aux(0)?;
        self.ps2_send_command(MOUSE_CMD_SET_RESOLUTION)?;
        self.ps2_write_aux(0)?;

        self.ps2_write_aux(MOUSE_CMD_STATUS_REQUEST)?;

        let id1 = self.ps2_read_timeout(100).unwrap_or(0);
        let id2 = self.ps2_read_timeout(100).unwrap_or(0);
        let id3 = self.ps2_read_timeout(100).unwrap_or(0);

        // Check for Synaptics signature
        if id2 == 0x47 {
            self.info.firmware_id = ((id1 as u32) << 16) |
                ((id2 as u32) << 8) |
                (id3 as u32);
            return Ok(true);
        }

        Ok(false)
    }

    /// Query touchpad capabilities
    fn query_capabilities(&mut self) -> Result<(), &'static str> {
        // Query model ID
        self.ps2_send_command(MOUSE_CMD_SET_RESOLUTION)?;
        self.ps2_write_aux(0)?;
        self.ps2_send_command(MOUSE_CMD_SET_RESOLUTION)?;
        self.ps2_write_aux(0)?;
        self.ps2_send_command(MOUSE_CMD_SET_RESOLUTION)?;
        self.ps2_write_aux(0)?;
        self.ps2_send_command(MOUSE_CMD_SET_RESOLUTION)?;
        self.ps2_write_aux(3)?;  // 3 = query model ID

        self.ps2_write_aux(MOUSE_CMD_STATUS_REQUEST)?;

        let m1 = self.ps2_read_timeout(100).unwrap_or(0);
        let m2 = self.ps2_read_timeout(100).unwrap_or(0);
        let m3 = self.ps2_read_timeout(100).unwrap_or(0);

        self.info.model_id = ((m1 as u32) << 16) | ((m2 as u32) << 8) | (m3 as u32);

        // Query capabilities
        self.ps2_send_command(MOUSE_CMD_SET_RESOLUTION)?;
        self.ps2_write_aux(0)?;
        self.ps2_send_command(MOUSE_CMD_SET_RESOLUTION)?;
        self.ps2_write_aux(0)?;
        self.ps2_send_command(MOUSE_CMD_SET_RESOLUTION)?;
        self.ps2_write_aux(0)?;
        self.ps2_send_command(MOUSE_CMD_SET_RESOLUTION)?;
        self.ps2_write_aux(2)?;  // 2 = query capabilities

        self.ps2_write_aux(MOUSE_CMD_STATUS_REQUEST)?;

        let c1 = self.ps2_read_timeout(100).unwrap_or(0);
        let c2 = self.ps2_read_timeout(100).unwrap_or(0);
        let c3 = self.ps2_read_timeout(100).unwrap_or(0);

        self.info.capabilities = ((c1 as u32) << 16) | ((c2 as u32) << 8) | (c3 as u32);

        // Parse capability bits
        self.info.has_extended = (self.info.capabilities & synaptics_cap::EXTENDED) != 0;
        self.info.middle_button = (self.info.capabilities & synaptics_cap::MIDDLE_BUTTON) != 0;
        self.info.palm_detect = (self.info.capabilities & synaptics_cap::PALM_DETECT) != 0;
        self.info.multi_finger = (self.info.capabilities & synaptics_cap::MULTI_FINGER) != 0;
        self.info.pass_through = (self.info.capabilities & synaptics_cap::PASS_THROUGH) != 0;

        Ok(())
    }

    /// Set absolute or relative mode
    fn set_absolute_mode(&mut self, absolute: bool) -> Result<(), &'static str> {
        let mode = if absolute {
            synaptics_mode::ABSOLUTE | synaptics_mode::HIGH_RATE
        } else {
            0
        };

        // Set mode via special sequence
        self.ps2_send_command(MOUSE_CMD_SET_RESOLUTION)?;
        self.ps2_write_aux((mode >> 6) & 0x03)?;
        self.ps2_send_command(MOUSE_CMD_SET_RESOLUTION)?;
        self.ps2_write_aux((mode >> 4) & 0x03)?;
        self.ps2_send_command(MOUSE_CMD_SET_RESOLUTION)?;
        self.ps2_write_aux((mode >> 2) & 0x03)?;
        self.ps2_send_command(MOUSE_CMD_SET_RESOLUTION)?;
        self.ps2_write_aux(mode & 0x03)?;

        // Confirm mode with sample rate command
        self.ps2_send_command(MOUSE_CMD_SET_SAMPLE_RATE)?;
        self.ps2_write_aux(20)?;  // 20 = set mode

        self.absolute_mode = absolute;
        self.packet_size = if absolute { 6 } else { 3 };

        Ok(())
    }

    /// Process an incoming byte from the touchpad
    /// Returns true if a complete packet was processed
    pub fn process_byte(&mut self, byte: u8) -> bool {
        // Packet synchronization
        if self.packet_idx == 0 {
            // First byte must have bit 3 set for standard mouse,
            // or specific sync patterns for Synaptics absolute mode
            if self.absolute_mode {
                // In absolute mode, byte 0 has bits 7:6 = 10, byte 3 has bits 7:6 = 11
                if (byte & 0xC0) != 0x80 {
                    // Out of sync, try to resync
                    return false;
                }
            } else {
                // Standard mouse: bit 3 should always be 1
                if (byte & 0x08) == 0 {
                    return false;
                }
            }
        }

        // Middle byte in Synaptics 6-byte packet (byte 3) has sync bits
        if self.absolute_mode && self.packet_idx == 3 {
            if (byte & 0xC0) != 0xC0 {
                // Resync
                self.packet_idx = 0;
                return false;
            }
        }

        self.packet[self.packet_idx] = byte;
        self.packet_idx += 1;

        if self.packet_idx >= self.packet_size {
            // Full packet received
            self.packet_idx = 0;

            if self.absolute_mode {
                self.parse_absolute_packet();
            } else {
                self.parse_relative_packet();
            }

            self.detect_tap();
            return true;
        }

        false
    }

    /// Parse a 6-byte absolute mode packet
    fn parse_absolute_packet(&mut self) {
        // Save previous state
        self.prev_finger = self.finger;

        // Packet format (6 bytes):
        // Byte 0: 1 W[2] W[1] 0   W[0] 1   R   L
        // Byte 1: Y[11:8]                  X[11:8]
        // Byte 2: Z[7:0] (pressure)
        // Byte 3: 1 1   Y[7]  X[7] 0  W[3] R   L
        // Byte 4: X[6:0]                   X[0]
        // Byte 5: Y[6:0]                   Y[0]

        let p0 = self.packet[0];
        let p1 = self.packet[1];
        let p2 = self.packet[2];
        let p3 = self.packet[3];
        let p4 = self.packet[4];
        let p5 = self.packet[5];

        // Extract W value (finger width)
        let w = ((p0 & 0x30) >> 2) | ((p0 & 0x04) >> 1) | ((p3 & 0x04) >> 2);

        // Extract X coordinate (12 bits)
        let x = ((p1 as u16 & 0x0F) << 8) | ((p3 as u16 & 0x10) << 4) | (p4 as u16);

        // Extract Y coordinate (12 bits)
        let y = ((p1 as u16 & 0xF0) << 4) | ((p3 as u16 & 0x20) << 3) | (p5 as u16);

        // Extract pressure (Z)
        let z = p2;

        // Extract buttons
        let left = (p0 & 0x01) != 0 || (p3 & 0x01) != 0;
        let right = (p0 & 0x02) != 0 || (p3 & 0x02) != 0;

        // Update state
        self.finger.x = x;
        self.finger.y = y;
        self.finger.pressure = z;
        self.finger.width = w;
        self.finger.left = left;
        self.finger.right = right;
        self.finger.finger_down = z > 25;  // Pressure threshold

        // Update cursor position if finger is down
        if self.finger.finger_down {
            self.update_cursor();
        }
    }

    /// Parse a 3-byte relative mode packet
    fn parse_relative_packet(&mut self) {
        let flags = self.packet[0];
        let mut dx = self.packet[1] as i32;
        let mut dy = self.packet[2] as i32;

        // Sign extend
        if flags & 0x10 != 0 {
            dx -= 256;
        }
        if flags & 0x20 != 0 {
            dy -= 256;
        }

        // Overflow check
        if flags & 0x40 != 0 { dx = 0; }
        if flags & 0x80 != 0 { dy = 0; }

        // Update buttons
        self.finger.left = (flags & 0x01) != 0;
        self.finger.right = (flags & 0x02) != 0;
        self.finger.middle = (flags & 0x04) != 0;

        // Update cursor with relative movement
        let sensitivity = self.sensitivity as i32;
        self.cursor_x += (dx * sensitivity) / 256;
        self.cursor_y -= (dy * sensitivity) / 256;  // Y is inverted

        // Clamp to screen bounds
        self.cursor_x = self.cursor_x.max(0).min(self.screen_width as i32 - 1);
        self.cursor_y = self.cursor_y.max(0).min(self.screen_height as i32 - 1);
    }

    /// Update cursor position from absolute coordinates
    fn update_cursor(&mut self) {
        if !self.finger.finger_down {
            return;
        }

        // Synaptics touchpads typically have coordinate range 0-6143
        // Scale to screen coordinates
        const TOUCHPAD_MAX_X: u32 = 6143;
        const TOUCHPAD_MAX_Y: u32 = 6143;

        let scaled_x = ((self.finger.x as u32) * self.screen_width) / TOUCHPAD_MAX_X;
        let scaled_y = ((TOUCHPAD_MAX_Y - self.finger.y as u32) * self.screen_height) / TOUCHPAD_MAX_Y;

        // Smooth cursor movement (blend with previous position)
        let alpha = 180;  // 0-255, higher = smoother
        self.cursor_x = ((self.cursor_x as u32 * alpha + scaled_x * (255 - alpha)) / 255) as i32;
        self.cursor_y = ((self.cursor_y as u32 * alpha + scaled_y * (255 - alpha)) / 255) as i32;

        // Clamp
        self.cursor_x = self.cursor_x.max(0).min(self.screen_width as i32 - 1);
        self.cursor_y = self.cursor_y.max(0).min(self.screen_height as i32 - 1);
    }

    /// Detect tap gestures (simplified - no PIT dependency)
    fn detect_tap(&mut self) {
        if !self.tap_enabled {
            return;
        }

        // Use a simple counter instead of PIT ticks
        // This gets incremented on each packet, roughly correlating with time
        self.tap_counter = self.tap_counter.wrapping_add(1);

        if self.finger.finger_down && !self.was_finger_down {
            // Finger just touched - record start
            self.tap_start_x = self.finger.x;
            self.tap_start_y = self.finger.y;
            self.tap_counter = 0;  // Reset counter
        } else if !self.finger.finger_down && self.was_finger_down {
            // Finger just lifted - check for tap
            let dx = (self.finger.x as i32 - self.tap_start_x as i32).abs();
            let dy = (self.finger.y as i32 - self.tap_start_y as i32).abs();

            // Tap criteria: short duration (low packet count), minimal movement
            // ~20 packets at 80 reports/sec = ~250ms
            if self.tap_counter < 20 && dx < 100 && dy < 100 {
                // Generate a click event
                self.finger.left = true;
            }
        }

        self.was_finger_down = self.finger.finger_down;
    }

    // =========================================================================
    // PS/2 Low-Level Communication
    // =========================================================================

    /// Wait for PS/2 controller to be ready to receive data
    fn ps2_wait_write(&self) -> Result<(), &'static str> {
        for _ in 0..100000 {
            let status = unsafe { inb(PS2_STATUS) };
            if (status & PS2_STATUS_INPUT_FULL) == 0 {
                return Ok(());
            }
        }
        Err("PS/2 write timeout")
    }

    /// Wait for PS/2 data to be available
    fn ps2_wait_read(&self) -> Result<(), &'static str> {
        for _ in 0..100000 {
            let status = unsafe { inb(PS2_STATUS) };
            if (status & PS2_STATUS_OUTPUT_FULL) != 0 {
                return Ok(());
            }
        }
        Err("PS/2 read timeout")
    }

    /// Read byte from PS/2 data port with timeout
    /// Uses iteration count for timeout instead of PIT
    fn ps2_read_timeout(&self, timeout_ms: u32) -> Result<u8, &'static str> {
        // Approximate: each iteration is ~1us on typical hardware
        // So timeout_ms * 1000 iterations
        let max_iterations = timeout_ms as usize * 1000;

        for _ in 0..max_iterations {
            let status = unsafe { inb(PS2_STATUS) };
            if (status & PS2_STATUS_OUTPUT_FULL) != 0 {
                return Ok(unsafe { inb(PS2_DATA) });
            }
            // Small delay
            for _ in 0..10 {
                unsafe { core::arch::asm!("nop"); }
            }
        }

        Err("PS/2 read timeout")
    }

    /// Write byte to auxiliary device (touchpad)
    fn ps2_write_aux(&mut self, data: u8) -> Result<(), &'static str> {
        self.ps2_wait_write()?;
        unsafe { outb(PS2_COMMAND, PS2_CMD_WRITE_AUX); }

        self.ps2_wait_write()?;
        unsafe { outb(PS2_DATA, data); }

        // Wait for ACK
        let ack = self.ps2_read_timeout(100)?;
        if ack != MOUSE_ACK && ack != MOUSE_RESEND {
            // Some commands don't return ACK, that's okay
        }

        Ok(())
    }

    /// Send a command to the touchpad and wait for ACK
    fn ps2_send_command(&mut self, cmd: u8) -> Result<(), &'static str> {
        self.ps2_write_aux(cmd)
    }

    // =========================================================================
    // Public Interface
    // =========================================================================

    /// Get cursor position
    pub fn get_position(&self) -> (i32, i32) {
        (self.cursor_x, self.cursor_y)
    }

    /// Get button state (bit 0 = left, bit 1 = right, bit 2 = middle)
    pub fn get_buttons(&self) -> u8 {
        let mut buttons = 0u8;
        if self.finger.left { buttons |= 0x01; }
        if self.finger.right { buttons |= 0x02; }
        if self.finger.middle { buttons |= 0x04; }
        buttons
    }

    /// Get finger state for advanced applications
    pub fn get_finger_state(&self) -> &FingerState {
        &self.finger
    }

    /// Check if touchpad is Synaptics
    pub fn is_synaptics(&self) -> bool {
        self.is_synaptics
    }

    /// Check if in absolute mode
    pub fn is_absolute(&self) -> bool {
        self.absolute_mode
    }

    /// Get hardware info
    pub fn info(&self) -> &SynapticsInfo {
        &self.info
    }

    /// Enable/disable tap-to-click
    pub fn set_tap_enabled(&mut self, enabled: bool) {
        self.tap_enabled = enabled;
    }

    /// Set sensitivity (256 = 1.0, 512 = 2.0, etc.)
    pub fn set_sensitivity(&mut self, sensitivity: u32) {
        self.sensitivity = sensitivity;
    }
}

// =============================================================================
// Global Instance
// =============================================================================

/// Global touchpad instance
pub static mut TOUCHPAD: SynapticsTouchpad = SynapticsTouchpad::new();

/// Initialize the Synaptics touchpad driver
pub fn init(screen_width: u32, screen_height: u32) -> Result<(), &'static str> {
    unsafe {
        TOUCHPAD.set_screen_size(screen_width, screen_height);
        TOUCHPAD.init()
    }
}

/// Process incoming touchpad data (called from IRQ12 handler)
pub fn handle_irq() -> bool {
    let status = unsafe { inb(PS2_STATUS) };

    // Check if data is from auxiliary device (bit 5)
    if (status & PS2_STATUS_AUX_DATA) == 0 {
        return false;
    }

    let byte = unsafe { inb(PS2_DATA) };
    unsafe { TOUCHPAD.process_byte(byte) }
}

/// Get current cursor position
pub fn get_position() -> (i32, i32) {
    unsafe { TOUCHPAD.get_position() }
}

/// Get button state
pub fn get_buttons() -> u8 {
    unsafe { TOUCHPAD.get_buttons() }
}

/// Check if Synaptics hardware was detected
pub fn is_synaptics() -> bool {
    unsafe { TOUCHPAD.is_synaptics() }
}

/// Check if the driver has been initialized (regardless of Synaptics detection)
pub fn is_initialized() -> bool {
    unsafe { TOUCHPAD.is_initialized }
}

/// Process a byte that was already read from the data port
/// Used by the IDT handler which reads the byte before routing
pub fn handle_irq_byte(byte: u8) -> bool {
    unsafe { TOUCHPAD.process_byte(byte) }
}
