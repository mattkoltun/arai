#!/usr/bin/env bash
set -euo pipefail

# Build script for Arai — cross-compiles for macOS, Linux, and Windows.
#
# Usage:
#   ./scripts/build.sh              # build all targets
#   ./scripts/build.sh macos        # macOS only (aarch64 + x86_64 + universal)
#   ./scripts/build.sh linux        # Linux only (x86_64 + aarch64 + armv7)
#   ./scripts/build.sh windows      # Windows only (x86_64 + aarch64)
#
# Prerequisites:
#   rustup target add \
#       aarch64-apple-darwin x86_64-apple-darwin \
#       x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu armv7-unknown-linux-gnueabihf \
#       x86_64-pc-windows-gnu aarch64-pc-windows-gnu
#
#   # Linux cross-compilation (from macOS):
#   brew tap messense/macos-cross-toolchains
#   brew install x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu arm-unknown-linux-gnueabihf
#
#   # Windows cross-compilation (from macOS):
#   brew install mingw-w64

PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT_DIR="$PROJECT_ROOT/dist"
APP_NAME="arai"

mkdir -p "$OUT_DIR"

build_target() {
    local target="$1"
    local label="$2"

    echo "==> Building $label ($target)"

    # Set linker for cross-compilation targets
    case "$target" in
        x86_64-unknown-linux-gnu)
            export CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER="x86_64-unknown-linux-gnu-gcc"
            export CC_x86_64_unknown_linux_gnu="x86_64-unknown-linux-gnu-gcc"
            ;;
        aarch64-unknown-linux-gnu)
            export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER="aarch64-unknown-linux-gnu-gcc"
            export CC_aarch64_unknown_linux_gnu="aarch64-unknown-linux-gnu-gcc"
            ;;
        armv7-unknown-linux-gnueabihf)
            export CARGO_TARGET_ARMV7_UNKNOWN_LINUX_GNUEABIHF_LINKER="arm-unknown-linux-gnueabihf-gcc"
            export CC_armv7_unknown_linux_gnueabihf="arm-unknown-linux-gnueabihf-gcc"
            ;;
        x86_64-pc-windows-gnu)
            export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER="x86_64-w64-mingw32-gcc"
            export CC_x86_64_pc_windows_gnu="x86_64-w64-mingw32-gcc"
            ;;
        aarch64-pc-windows-gnu)
            export CARGO_TARGET_AARCH64_PC_WINDOWS_GNU_LINKER="aarch64-w64-mingw32-gcc"
            export CC_aarch64_pc_windows_gnu="aarch64-w64-mingw32-gcc"
            ;;
    esac

    cargo build --release --target "$target" --manifest-path "$PROJECT_ROOT/Cargo.toml"

    local src_dir="$PROJECT_ROOT/target/$target/release"
    local ext=""
    [[ "$target" == *"windows"* ]] && ext=".exe"

    local dest="$OUT_DIR/${APP_NAME}-${target}${ext}"
    cp "$src_dir/${APP_NAME}${ext}" "$dest"
    echo "    -> $dest"
}

build_macos() {
    build_target "aarch64-apple-darwin" "macOS ARM64 (Apple Silicon)"
    build_target "x86_64-apple-darwin" "macOS x86_64 (Intel)"

    # Create universal binary
    echo "==> Creating macOS universal binary"
    lipo -create \
        "$OUT_DIR/${APP_NAME}-aarch64-apple-darwin" \
        "$OUT_DIR/${APP_NAME}-x86_64-apple-darwin" \
        -output "$OUT_DIR/${APP_NAME}-macos-universal"
    echo "    -> $OUT_DIR/${APP_NAME}-macos-universal"
}

build_linux() {
    build_target "x86_64-unknown-linux-gnu" "Linux x86_64 (AMD64)"
    build_target "aarch64-unknown-linux-gnu" "Linux ARM64 (AArch64)"
    build_target "armv7-unknown-linux-gnueabihf" "Linux ARMv7 (32-bit)"
}

build_windows() {
    build_target "x86_64-pc-windows-gnu" "Windows x86_64 (AMD64)"
    build_target "aarch64-pc-windows-gnu" "Windows ARM64"
}

filter="${1:-all}"

case "$filter" in
    macos)   build_macos ;;
    linux)   build_linux ;;
    windows) build_windows ;;
    all)
        build_macos
        build_linux
        build_windows
        ;;
    *)
        echo "Usage: $0 [macos|linux|windows|all]"
        exit 1
        ;;
esac

echo ""
echo "Build artifacts in $OUT_DIR:"
ls -lh "$OUT_DIR"
