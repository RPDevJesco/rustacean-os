//! Boot Information Parser
//!
//! Parses the boot info structure created by stage2 bootloader at 0x500.
//!
//! # Boot Info Structure Layout (at 0x500)
//!
//! ```text
//! Offset  Size  Field
//! 0x00    4     Magic ('RUST' = 0x54535552)
//! 0x04    4     E820 map address
//! 0x08    4     VESA enabled (0 or 1)
//! 0x0C    4     Framebuffer address
//! 0x10    4     Screen width
//! 0x14    4     Screen height
//! 0x18    4     Bits per pixel
//! 0x1C    4     Pitch (bytes per scanline)
//! ```

/// Magic value: 'RUST' in little-endian
pub const BOOT_MAGIC: u32 = 0x54535552;

/// Boot information passed from bootloader to kernel
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct BootInfo {
    /// Magic number ('RUST' = 0x54535552)
    pub magic: u32,
    /// Address of E820 memory map
    pub e820_map_addr: u32,
    /// Whether VESA mode is enabled (vs VGA text)
    pub vesa_enabled: bool,
    /// Physical address of framebuffer
    pub framebuffer_addr: u32,
    /// Screen width in pixels (or columns for text mode)
    pub screen_width: u32,
    /// Screen height in pixels (or rows for text mode)
    pub screen_height: u32,
    /// Bits per pixel (or 16 for text mode = 2 bytes per cell)
    pub bits_per_pixel: u32,
    /// Pitch: bytes per scanline
    pub pitch: u32,
}

impl BootInfo {
    /// Parse boot info from raw pointer
    ///
    /// # Safety
    ///
    /// The pointer must point to a valid boot info structure
    /// created by the stage2 bootloader.
    pub unsafe fn from_ptr(ptr: *const u8) -> Self {
        let data = ptr as *const u32;
        
        Self {
            magic: *data.offset(0),
            e820_map_addr: *data.offset(1),
            vesa_enabled: *data.offset(2) != 0,
            framebuffer_addr: *data.offset(3),
            screen_width: *data.offset(4),
            screen_height: *data.offset(5),
            bits_per_pixel: *data.offset(6),
            pitch: *data.offset(7),
        }
    }
    
    /// Verify the boot magic is correct
    pub fn verify_magic(&self) -> bool {
        self.magic == BOOT_MAGIC
    }
}

/// E820 Memory Map Entry
#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct E820Entry {
    /// Base address of memory region
    pub base: u64,
    /// Length of memory region in bytes
    pub length: u64,
    /// Type of memory region
    pub region_type: u32,
    /// ACPI 3.0 extended attributes (may be 0)
    pub acpi_attrs: u32,
}

/// E820 memory region types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum E820Type {
    /// Usable RAM
    Usable = 1,
    /// Reserved by system
    Reserved = 2,
    /// ACPI reclaimable
    AcpiReclaimable = 3,
    /// ACPI NVS (non-volatile storage)
    AcpiNvs = 4,
    /// Bad memory
    BadMemory = 5,
}

impl E820Entry {
    /// Get the memory type
    pub fn memory_type(&self) -> E820Type {
        match self.region_type {
            1 => E820Type::Usable,
            2 => E820Type::Reserved,
            3 => E820Type::AcpiReclaimable,
            4 => E820Type::AcpiNvs,
            5 => E820Type::BadMemory,
            _ => E820Type::Reserved, // Treat unknown as reserved
        }
    }
    
    /// Check if this region is usable RAM
    pub fn is_usable(&self) -> bool {
        self.region_type == 1
    }
    
    /// Get end address of this region
    pub fn end(&self) -> u64 {
        self.base + self.length
    }
}

/// E820 Memory Map
pub struct E820Map {
    /// Pointer to entry array
    entries_ptr: *const E820Entry,
    /// Number of entries
    count: usize,
}

impl E820Map {
    /// Parse E820 map from address
    ///
    /// # Safety
    ///
    /// The address must point to a valid E820 map structure
    /// created by the stage2 bootloader.
    pub unsafe fn from_addr(addr: u32) -> Self {
        let count_ptr = addr as *const u16;
        let count = *count_ptr as usize;
        let entries_ptr = (addr + 4) as *const E820Entry;
        
        Self { entries_ptr, count }
    }
    
    /// Get number of entries
    pub fn len(&self) -> usize {
        self.count
    }
    
    /// Check if map is empty
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
    
    /// Get entry by index
    pub fn get(&self, index: usize) -> Option<E820Entry> {
        if index < self.count {
            unsafe {
                Some(*self.entries_ptr.add(index))
            }
        } else {
            None
        }
    }
    
    /// Iterate over all entries
    pub fn iter(&self) -> E820Iterator {
        E820Iterator {
            map: self,
            index: 0,
        }
    }
    
    /// Calculate total usable memory in bytes
    pub fn total_usable_memory(&self) -> u64 {
        self.iter()
            .filter(|e| e.is_usable())
            .map(|e| e.length)
            .sum()
    }
    
    /// Calculate total memory (all types) in bytes
    pub fn total_memory(&self) -> u64 {
        self.iter()
            .map(|e| e.end())
            .max()
            .unwrap_or(0)
    }
}

/// Iterator over E820 entries
pub struct E820Iterator<'a> {
    map: &'a E820Map,
    index: usize,
}

impl<'a> Iterator for E820Iterator<'a> {
    type Item = E820Entry;
    
    fn next(&mut self) -> Option<Self::Item> {
        let entry = self.map.get(self.index);
        if entry.is_some() {
            self.index += 1;
        }
        entry
    }
}
