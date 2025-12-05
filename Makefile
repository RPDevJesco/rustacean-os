# Rustacean OS Makefile
#
# Builds bootloader and kernel, creates bootable disk image

# Tools
NASM := nasm
CARGO := cargo
DD := dd
CAT := cat

# Directories
BOOT_DIR := boot
KERNEL_DIR := kernel
BUILD_DIR := build
TARGET_DIR := $(KERNEL_DIR)/target/i686-rustacean/release

# Output files
BOOT_BIN := $(BUILD_DIR)/boot.bin
STAGE2_BIN := $(BUILD_DIR)/stage2.bin
KERNEL_BIN := $(BUILD_DIR)/kernel.bin
OS_IMG := $(BUILD_DIR)/rustacean.img

# Target specification
TARGET_JSON := i686-rustacean.json

.PHONY: all clean bootloader kernel image run debug

all: image

# Create build directory
$(BUILD_DIR):
	mkdir -p $(BUILD_DIR)

# Assemble stage 1 bootloader
$(BOOT_BIN): $(BOOT_DIR)/boot.asm | $(BUILD_DIR)
	$(NASM) -f bin -o $@ $<

# Assemble stage 2 bootloader
$(STAGE2_BIN): $(BOOT_DIR)/stage2.asm | $(BUILD_DIR)
	$(NASM) -f bin -o $@ $<

# Build kernel
$(KERNEL_BIN): FORCE | $(BUILD_DIR)
	cd $(KERNEL_DIR) && $(CARGO) build --release --target ../$(TARGET_JSON) -Zbuild-std=core,alloc -Zbuild-std-features=compiler-builtins-mem
	cp $(TARGET_DIR)/rustacean-kernel $@

# Combine into disk image
# Layout:
#   Sector 0:      boot.bin (512 bytes)
#   Sectors 1-32:  stage2.bin (16KB)
#   Sectors 33+:   kernel.bin (padded to 64KB)
$(OS_IMG): $(BOOT_BIN) $(STAGE2_BIN) $(KERNEL_BIN)
	# Create empty 1.44MB floppy image
	$(DD) if=/dev/zero of=$@ bs=512 count=2880 2>/dev/null
	# Write boot sector
	$(DD) if=$(BOOT_BIN) of=$@ bs=512 count=1 conv=notrunc 2>/dev/null
	# Write stage 2 (sectors 1-32)
	$(DD) if=$(STAGE2_BIN) of=$@ bs=512 seek=1 conv=notrunc 2>/dev/null
	# Write kernel (sectors 33+)
	$(DD) if=$(KERNEL_BIN) of=$@ bs=512 seek=33 conv=notrunc 2>/dev/null
	@echo "Created $(OS_IMG)"

bootloader: $(BOOT_BIN) $(STAGE2_BIN)
	@echo "Bootloader built"

kernel: $(KERNEL_BIN)
	@echo "Kernel built"

image: $(OS_IMG)
	@echo "Disk image ready: $(OS_IMG)"

# Run in QEMU
run: $(OS_IMG)
	qemu-system-i386 -fda $< -boot a -m 256M

# Run with QEMU debug (no graphics, serial to stdout)
debug: $(OS_IMG)
	qemu-system-i386 -fda $< -boot a -m 256M -nographic -serial mon:stdio

# Run with VGA text mode (skip VESA)
run-text: $(BUILD_DIR)/rustacean-text.img
	qemu-system-i386 -fda $< -boot a -m 256M

$(BUILD_DIR)/rustacean-text.img: $(BOOT_BIN) $(BUILD_DIR)/stage2-text.bin $(KERNEL_BIN)
	$(DD) if=/dev/zero of=$@ bs=512 count=2880 2>/dev/null
	$(DD) if=$(BOOT_BIN) of=$@ bs=512 count=1 conv=notrunc 2>/dev/null
	$(DD) if=$(BUILD_DIR)/stage2-text.bin of=$@ bs=512 seek=1 conv=notrunc 2>/dev/null
	$(DD) if=$(KERNEL_BIN) of=$@ bs=512 seek=33 conv=notrunc 2>/dev/null

$(BUILD_DIR)/stage2-text.bin: $(BOOT_DIR)/stage2.asm | $(BUILD_DIR)
	$(NASM) -f bin -DSKIP_VESA -o $@ $<

clean:
	rm -rf $(BUILD_DIR)
	cd $(KERNEL_DIR) && $(CARGO) clean

# Force rebuild
FORCE:

# Help
help:
	@echo "Rustacean OS Build System"
	@echo ""
	@echo "Targets:"
	@echo "  all        - Build everything (default)"
	@echo "  bootloader - Build boot.bin and stage2.bin"
	@echo "  kernel     - Build kernel.bin"
	@echo "  image      - Create bootable disk image"
	@echo "  run        - Run in QEMU with VESA graphics"
	@echo "  run-text   - Run in QEMU with VGA text mode"
	@echo "  debug      - Run in QEMU with serial output"
	@echo "  clean      - Remove build artifacts"
