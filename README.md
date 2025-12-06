# Rustacean OS

A Plan 9-style GUI operating system built in Rust with EventChains architecture.

## Project Structure

```
rustacean-os/
├── boot/
│   ├── boot.asm         # Stage 1 bootloader (512 bytes)
│   └── stage2.asm       # Stage 2 bootloader (16KB, VESA setup)
├── kernel/
│   ├── Cargo.toml
│   ├── linker.ld        # Kernel linker script
│   └── src/
│       ├── main.rs      # Kernel entry point
│       ├── boot_info.rs # Boot info parsing
│       ├── arch/x86/    # x86 architecture (GDT, IDT, PIC, PIT)
│       ├── mm/          # Memory management (intrusive lists, PMM)
│       ├── sched/       # Scheduler (priority round-robin)
│       ├── event_chains/ # no_std EventChains implementation
│       ├── syscall/     # System call interface
│       ├── drivers/     # VGA/VESA and keyboard drivers
│       └── fs/          # Filesystem (exFAT support planned)
├── i686-rustacean.json  # Custom target specification
├── Dockerfile           # Docker build environment
├── build.sh             # Host build script
├── docker-build.sh      # Container build script
├── Makefile             # Build system
└── output/              # Build outputs (created by Docker)
```

## Quick Start with Docker (Recommended)

The easiest way to build Rustacean OS is using Docker:

```bash
# Build the OS
./build.sh

# Build and run in QEMU
./build.sh --run

# Open a shell in the build container
./build.sh --shell

# Force a clean rebuild
./build.sh --no-cache
```

### Manual Docker Commands

```bash
# Build the Docker image
docker build -t rustacean-builder .

# Run the build (outputs go to ./output/)
docker run --rm -v $(pwd)/output:/output rustacean-builder

# Interactive shell for debugging
docker run --rm -it -v $(pwd)/output:/output rustacean-builder /bin/bash
```

## Architecture

The kernel uses a three-tier EventChain separation:

1. **GUI Layer EventChains** - Input validation, theming, UI events
2. **Window Manager EventChain** - Focus, Z-order, damage tracking
3. **Kernel EventChain** - Audit, permissions, syscall processing

Core primitives (memory manager, scheduler) use **intrusive linked lists** instead of EventChains for raw performance.

## Building Locally (Without Docker)

### Prerequisites

- Rust nightly toolchain with rust-src
- NASM assembler
- QEMU for testing

### Install Dependencies

```bash
# Install Rust nightly
rustup install nightly
rustup component add rust-src --toolchain nightly

# Ubuntu/Debian
sudo apt install nasm qemu-system-x86

# macOS
brew install nasm qemu
```

### Build

```bash
# Build bootloader and kernel
make

# Or step by step:
make bootloader   # Assemble boot.asm and stage2.asm
make kernel       # Build Rust kernel
make image        # Create bootable disk image
```

### Run in QEMU

```bash
# With VESA graphics
make run

# With VGA text mode (fallback)
make run-text

# Debug mode (serial console)
make debug
```

## Output Files

After building, the `output/` directory contains:

| File | Description |
|------|-------------|
| `boot.bin` | Stage 1 bootloader (512 bytes) |
| `stage2.bin` | Stage 2 bootloader (16KB) |
| `kernel.bin` | Kernel binary |
| `rustacean.img` | Bootable 1.44MB floppy image |

## Memory Layout

```
0x00000000 - 0x000004FF  BIOS/Boot data
0x00000500 - 0x000005FF  Boot Info Structure
0x00001000 - 0x00001FFF  E820 Memory Map
0x00007C00 - 0x00007DFF  Stage 1 Bootloader
0x00007E00 - 0x0000BDFF  Stage 2 Bootloader
0x00090000 - 0x0009FFFF  Stack (grows down)
0x000A0000 - 0x000BFFFF  VGA Memory
0x000C0000 - 0x000FFFFF  ROM Area
0x00100000 - ...         Kernel (loaded at 1MB)
```

## Boot Info Structure (at 0x500)

```
Offset  Size  Field
0x00    4     Magic ('RUST' = 0x54535552)
0x04    4     E820 map address
0x08    4     VESA enabled (0 or 1)
0x0C    4     Framebuffer address
0x10    4     Screen width
0x14    4     Screen height
0x18    4     Bits per pixel
0x1C    4     Pitch (bytes per scanline)
```

## Target Hardware

- CPU: i686 (Pentium 3 or later)
- RAM: 256MB minimum
- Display: VESA 2.0 compatible or VGA text

## License

MIT

https://github.com/user-attachments/assets/bebd238f-5e7a-467d-9219-d2e0188ea4e2




