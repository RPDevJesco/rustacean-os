#!/bin/bash
set -e

echo "========================================"
echo "  Rustacean OS Build System"
echo "========================================"
echo ""

echo "[1/5] Assembling bootloader..."
mkdir -p build
nasm -f bin -o build/boot.bin boot/boot.asm
nasm -f bin -o build/stage2.bin boot/stage2.asm
echo "      boot.bin: $(stat -c%s build/boot.bin) bytes"
echo "      stage2.bin: $(stat -c%s build/stage2.bin) bytes"
echo ""

echo "[2/5] Building kernel..."
cd kernel

# Show FULL cargo output
if cargo +nightly build --release --target ../i686-rustacean.json \
    -Zbuild-std=core,alloc \
    -Zbuild-std-features=compiler-builtins-mem 2>&1; then
    echo "      Kernel build successful!"
else
    echo ""
    echo "      ERROR: Kernel build failed!"
    echo ""
fi

cd ..

# Find the kernel binary
echo ""
echo "      Searching for kernel binary..."
find kernel/target -type f -name "rustacean*" ! -name "*.d" ! -name "*.rlib" 2>/dev/null || true

KERNEL_BIN=""
if [ -f "kernel/target/i686-rustacean/release/rustacean-kernel" ]; then
    KERNEL_BIN="kernel/target/i686-rustacean/release/rustacean-kernel"
elif [ -f "kernel/target/i686-rustacean/release/rustacean_kernel" ]; then
    KERNEL_BIN="kernel/target/i686-rustacean/release/rustacean_kernel"
fi

if [ -n "$KERNEL_BIN" ]; then
    # Convert ELF to flat binary - THIS IS CRITICAL!
    # The bootloader jumps directly to 0x100000, so we need raw machine code
    echo "      Converting ELF to flat binary..."
    objcopy -O binary "$KERNEL_BIN" build/kernel.bin
    echo "      kernel.bin: $(stat -c%s build/kernel.bin) bytes"
else
    echo "      WARNING: Kernel binary not found!"
    echo "      Listing kernel/target contents:"
    ls -laR kernel/target/i686-rustacean/release/ 2>/dev/null | head -50 || echo "      Directory not found"
fi
echo ""

echo "[3/5] Creating floppy image..."
# Create 1.44MB floppy image
dd if=/dev/zero of=build/rustacean.img bs=512 count=2880 2>/dev/null
# Write boot sector (sector 0)
dd if=build/boot.bin of=build/rustacean.img bs=512 count=1 conv=notrunc 2>/dev/null
# Write stage2 (sectors 1-32)
dd if=build/stage2.bin of=build/rustacean.img bs=512 seek=1 conv=notrunc 2>/dev/null
# Write kernel (sectors 33+) if it exists
if [ -f build/kernel.bin ]; then
    dd if=build/kernel.bin of=build/rustacean.img bs=512 seek=33 conv=notrunc 2>/dev/null
fi
echo "      rustacean.img: $(stat -c%s build/rustacean.img) bytes"
echo ""

echo "[4/5] Creating bootable ISO (El Torito with floppy emulation)..."
# Create ISO directory structure
mkdir -p build/iso

# Copy the floppy image to ISO root
cp build/rustacean.img build/iso/

# Create bootable ISO using El Torito floppy emulation
# The -b option with a 1.44MB image triggers floppy emulation automatically
genisoimage -o build/rustacean.iso \
    -R -J \
    -V "RUSTACEAN_OS" \
    -b rustacean.img \
    -hide rustacean.img \
    build/iso 2>&1 || {
    
    echo "      genisoimage failed, trying xorriso..."
    xorriso -as mkisofs \
        -R -J \
        -V "RUSTACEAN_OS" \
        -b rustacean.img \
        -hide rustacean.img \
        -o build/rustacean.iso \
        build/iso 2>&1 || echo "      ISO creation had issues"
}

if [ -f build/rustacean.iso ]; then
    echo "      rustacean.iso: $(stat -c%s build/rustacean.iso) bytes"
else
    echo "      WARNING: ISO creation failed!"
fi
echo ""

echo "[5/5] Copying to output..."
cp -v build/*.bin /output/ 2>/dev/null || true
cp -v build/*.img /output/ 2>/dev/null || true
cp -v build/*.iso /output/ 2>/dev/null || true
echo ""

echo "========================================"
echo "  Build Complete!"
echo "========================================"
echo ""
echo "Output files:"
ls -la /output/
echo ""
echo "========================================" 
echo "  CD/DVD Burning Instructions"
echo "========================================"
echo ""
echo "Use rustacean.iso to burn to CD/DVD."
echo ""
echo "Windows: Right-click rustacean.iso -> 'Burn disc image'"
echo "Linux:   wodim -v dev=/dev/sr0 rustacean.iso"
echo "macOS:   hdiutil burn rustacean.iso"
echo ""
echo "Test in QEMU:"
echo "  qemu-system-i386 -cdrom rustacean.iso -m 256M"
