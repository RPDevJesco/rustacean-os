//! Hardware Drivers
//!
//! Device drivers for Rustacean OS.
//! Drivers can integrate with the kernel EventChain for event-driven I/O.

pub mod vga;
pub mod keyboard;
pub mod mouse;
pub mod ati_rage;
pub mod synaptics;

// Re-export common driver functions
pub use ati_rage::AtiRage;
pub use synaptics::SynapticsTouchpad;
