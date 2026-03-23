#!/usr/bin/env bash
set -euo pipefail

# Release script for Arai — builds, bundles, and packages for distribution.
# Currently macOS-only; Linux and Windows are not yet supported.
#
# Usage:
#   ./scripts/release.sh
#
# Prerequisites:
#   cargo install cargo-bundle
#   brew install create-dmg
#
#   # Cross-compilation targets:
#   rustup target add aarch64-apple-darwin x86_64-apple-darwin

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

build_binary() {
    local target="$1"
    local label="$2"

    echo ""
    echo "==> Building $label ($target)"
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

    # Ad-hoc sign the bundle so macOS can persist permission grants (microphone etc.)
    echo ""
    echo "==> Code signing .app bundle (ad-hoc)"
    codesign --force --deep --sign - "$app_bundle"
    echo "    -> Signed"

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

# ── Main ─────────────────────────────────────────────────────────────

release_macos

echo ""
echo "=========================================="
echo "Release artifacts:"
ls -lh "$DIST_DIR"
