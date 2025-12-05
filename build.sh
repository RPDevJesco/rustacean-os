#!/bin/bash
#
# Rustacean OS Docker Build Script
#
# This script builds Rustacean OS inside a Docker container
# and copies the output files to ./output/
#
# Usage: ./build.sh [options]
#
# Options:
#   --no-cache    Force rebuild without Docker cache
#   --run         Run the built image in QEMU after building
#   --shell       Open a shell in the build container instead of building
#

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

IMAGE_NAME="rustacean-builder"
OUTPUT_DIR="$SCRIPT_DIR/output"

# Parse arguments
NO_CACHE=""
RUN_AFTER=""
SHELL_MODE=""

for arg in "$@"; do
    case $arg in
        --no-cache)
            NO_CACHE="--no-cache"
            ;;
        --run)
            RUN_AFTER="yes"
            ;;
        --shell)
            SHELL_MODE="yes"
            ;;
        --help|-h)
            echo "Rustacean OS Docker Build Script"
            echo ""
            echo "Usage: $0 [options]"
            echo ""
            echo "Options:"
            echo "  --no-cache    Force rebuild without Docker cache"
            echo "  --run         Run the built image in QEMU after building"
            echo "  --shell       Open a shell in the build container"
            echo "  --help, -h    Show this help message"
            exit 0
            ;;
    esac
done

# Create output directory
mkdir -p "$OUTPUT_DIR"

echo "========================================"
echo "  Rustacean OS Docker Builder"
echo "========================================"
echo ""

# Build Docker image
echo "[Docker] Building image '$IMAGE_NAME'..."
docker build $NO_CACHE -t "$IMAGE_NAME" .

if [ "$SHELL_MODE" = "yes" ]; then
    echo ""
    echo "[Docker] Opening shell in container..."
    docker run --rm -it \
        -v "$OUTPUT_DIR:/output" \
        "$IMAGE_NAME" \
        /bin/bash
    exit 0
fi

echo ""
echo "[Docker] Running build..."
docker run --rm \
    -v "$OUTPUT_DIR:/output" \
    "$IMAGE_NAME"

echo ""
echo "========================================"
echo "  Output Files"
echo "========================================"
ls -la "$OUTPUT_DIR/"

if [ "$RUN_AFTER" = "yes" ]; then
    echo ""
    echo "[QEMU] Starting Rustacean OS..."
    
    # Check if QEMU is available on host
    if command -v qemu-system-i386 &> /dev/null; then
        qemu-system-i386 \
            -fda "$OUTPUT_DIR/rustacean.img" \
            -boot a \
            -m 256M
    else
        echo "QEMU not found on host. Running in container..."
        docker run --rm -it \
            -v "$OUTPUT_DIR:/output" \
            -e DISPLAY="$DISPLAY" \
            -v /tmp/.X11-unix:/tmp/.X11-unix \
            "$IMAGE_NAME" \
            qemu-system-i386 -fda /output/rustacean.img -boot a -m 256M -nographic -serial mon:stdio
    fi
fi

echo ""
echo "Done! Output files are in: $OUTPUT_DIR/"
