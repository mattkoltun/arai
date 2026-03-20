#!/usr/bin/env bash
set -euo pipefail

# Release script for Arai — builds, bundles, and packages for distribution.
#
# Usage:
#   ./scripts/release.sh              # release all platforms
#   ./scripts/release.sh macos        # macOS only (.app + .dmg + universal binary)
#   ./scripts/release.sh linux        # Linux only (tarballs per architecture)
#   ./scripts/release.sh windows      # Windows only (zip per architecture)
#
# Prerequisites:
#   cargo install cargo-bundle
#   brew install create-dmg           # macOS packaging
#
#   # Cross-compilation targets:
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
DIST_DIR="$PROJECT_ROOT/dist"
APP_NAME="arai"
APP_DISPLAY_NAME="Arai"
VERSION=$(grep '^version' "$PROJECT_ROOT/Cargo.toml" | head -1 | sed 's/.*"\(.*\)"/\1/')

echo "Building $APP_DISPLAY_NAME v$VERSION"
echo "=========================================="

rm -rf "$DIST_DIR"
mkdir -p "$DIST_DIR"

# ── Helpers ──────────────────────────────────────────────────────────

check_command() {
    if ! command -v "$1" &>/dev/null; then
        echo "Error: $1 is required but not installed."
        echo "  $2"
        exit 1
    fi
}

set_cross_linker() {
    local target="$1"
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
}

build_binary() {
    local target="$1"
    local label="$2"

    echo ""
    echo "==> Building $label ($target)"
    set_cross_linker "$target"
    cargo build --release --target "$target" --manifest-path "$PROJECT_ROOT/Cargo.toml"
}

# ── macOS ────────────────────────────────────────────────────────────

release_macos() {
    echo ""
    echo "── macOS ──────────────────────────────────"
    check_command "cargo-bundle" "Install with: cargo install cargo-bundle"
    check_command "create-dmg" "Install with: brew install create-dmg"

    # 1. Build both architectures
    build_binary "aarch64-apple-darwin" "macOS ARM64 (Apple Silicon)"
    build_binary "x86_64-apple-darwin" "macOS x86_64 (Intel)"

    # 2. Create universal binary
    echo ""
    echo "==> Creating universal binary"
    local universal_bin="$PROJECT_ROOT/target/universal-apple-darwin/release/$APP_NAME"
    mkdir -p "$(dirname "$universal_bin")"
    lipo -create \
        "$PROJECT_ROOT/target/aarch64-apple-darwin/release/$APP_NAME" \
        "$PROJECT_ROOT/target/x86_64-apple-darwin/release/$APP_NAME" \
        -output "$universal_bin"

    # 3. Create .app bundle using cargo-bundle (builds for host arch)
    echo ""
    echo "==> Creating .app bundle"
    cargo bundle --release --target aarch64-apple-darwin

    local app_bundle="$PROJECT_ROOT/target/aarch64-apple-darwin/release/bundle/osx/${APP_DISPLAY_NAME}.app"

    # Replace the binary in the bundle with the universal one
    cp "$universal_bin" "$app_bundle/Contents/MacOS/$APP_NAME"
    echo "    -> $app_bundle (universal binary)"

    # 4. Create .dmg
    echo ""
    echo "==> Creating .dmg installer"
    local dmg_path="$DIST_DIR/${APP_DISPLAY_NAME}-${VERSION}-macos-universal.dmg"

    # create-dmg returns non-zero if it can't set the icon, which is fine
    local volicon_args=()
    local logo_icns="$PROJECT_ROOT/assets/images/logo.icns"
    if [[ -f "$logo_icns" ]]; then
        volicon_args=(--volicon "$logo_icns")
    fi

    create-dmg \
        --volname "$APP_DISPLAY_NAME" \
        "${volicon_args[@]}" \
        --window-pos 200 120 \
        --window-size 600 400 \
        --icon-size 100 \
        --icon "${APP_DISPLAY_NAME}.app" 175 190 \
        --app-drop-link 425 190 \
        --no-internet-enable \
        "$dmg_path" \
        "$app_bundle" || true

    # Eject any volumes left mounted by create-dmg.
    hdiutil detach "/Volumes/$APP_DISPLAY_NAME" 2>/dev/null || true

    if [[ -f "$dmg_path" ]]; then
        echo "    -> $dmg_path"
    else
        echo "    !! DMG creation failed, copying .app instead"
        cp -r "$app_bundle" "$DIST_DIR/"
    fi

    # 5. Also output standalone binaries as tarballs
    for arch in aarch64 x86_64; do
        local tarball="$DIST_DIR/${APP_NAME}-${VERSION}-macos-${arch}.tar.gz"
        tar -czf "$tarball" -C "$PROJECT_ROOT/target/${arch}-apple-darwin/release" "$APP_NAME"
        echo "    -> $tarball"
    done
}

# ── Linux ────────────────────────────────────────────────────────────

release_linux() {
    echo ""
    echo "── Linux ──────────────────────────────────"

    local targets=(
        "x86_64-unknown-linux-gnu:linux-amd64"
        "aarch64-unknown-linux-gnu:linux-arm64"
        "armv7-unknown-linux-gnueabihf:linux-armv7"
    )

    for entry in "${targets[@]}"; do
        local target="${entry%%:*}"
        local label="${entry##*:}"

        build_binary "$target" "Linux $label"

        local tarball="$DIST_DIR/${APP_NAME}-${VERSION}-${label}.tar.gz"
        tar -czf "$tarball" -C "$PROJECT_ROOT/target/$target/release" "$APP_NAME"
        echo "    -> $tarball"
    done
}

# ── Windows ──────────────────────────────────────────────────────────

release_windows() {
    echo ""
    echo "── Windows ────────────────────────────────"

    local targets=(
        "x86_64-pc-windows-gnu:windows-amd64"
        "aarch64-pc-windows-gnu:windows-arm64"
    )

    for entry in "${targets[@]}"; do
        local target="${entry%%:*}"
        local label="${entry##*:}"

        build_binary "$target" "Windows $label"

        local zipfile="$DIST_DIR/${APP_NAME}-${VERSION}-${label}.zip"
        (cd "$PROJECT_ROOT/target/$target/release" && zip -q "$zipfile" "${APP_NAME}.exe")
        echo "    -> $zipfile"
    done
}

# ── Main ─────────────────────────────────────────────────────────────

filter="${1:-all}"

case "$filter" in
    macos)   release_macos ;;
    linux)   release_linux ;;
    windows) release_windows ;;
    all)
        release_macos
        release_linux
        release_windows
        ;;
    *)
        echo "Usage: $0 [macos|linux|windows|all]"
        exit 1
        ;;
esac

echo ""
echo "=========================================="
echo "Release artifacts:"
ls -lh "$DIST_DIR"
