#!/usr/bin/env bash
set -euo pipefail

# Release script for Arai — builds, bundles, and packages for distribution.
#
# Usage:
#   ./scripts/release.sh [release-macos|release-linux]
#
# Configurable targets:
#   MACOS_TARGETS="aarch64-apple-darwin:arm64,x86_64-apple-darwin:x86_64"
#   LINUX_TARGETS="x86_64-unknown-linux-gnu:x86_64,aarch64-unknown-linux-gnu:arm64"
#
# Prerequisites:
#   cargo install cargo-bundle
#   brew install create-dmg
#
#   # Cross-compilation targets:
#   rustup target add aarch64-apple-darwin x86_64-apple-darwin \
#     x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu

PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DIST_DIR="$PROJECT_ROOT/dist"
APP_NAME="arai"
APP_DISPLAY_NAME="Arai"
VERSION=$(grep '^version' "$PROJECT_ROOT/Cargo.toml" | head -1 | sed 's/.*"\(.*\)"/\1/')

MACOS_TARGETS="${MACOS_TARGETS:-aarch64-apple-darwin:arm64,x86_64-apple-darwin:x86_64}"
LINUX_TARGETS="${LINUX_TARGETS:-x86_64-unknown-linux-gnu:x86_64,aarch64-unknown-linux-gnu:arm64}"

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

target_specs_lines() {
    local specs="$1"
    printf '%s\n' "$specs" | tr ',' '\n'
}

target_path() {
    local target="$1"
    echo "$PROJECT_ROOT/target/$target/release/$APP_NAME"
}

archive_binary() {
    local binary_path="$1"
    local archive_base="$2"

    tar -czf "${archive_base}.tar.gz" -C "$(dirname "$binary_path")" "$APP_NAME"
    tar -cf "${archive_base}.tar" -C "$(dirname "$binary_path")" "$APP_NAME"
    zip -j "${archive_base}.zip" "$binary_path" >/dev/null

    echo "    -> ${archive_base}.tar.gz"
    echo "    -> ${archive_base}.tar"
    echo "    -> ${archive_base}.zip"
}

# ── macOS ────────────────────────────────────────────────────────────

release_macos() {
    echo ""
    echo "── macOS ──────────────────────────────────"
    check_command "cargo-bundle" "Install with: cargo install cargo-bundle"
    check_command "create-dmg" "Install with: brew install create-dmg"

    local bundle_target=""
    local universal_target="universal-apple-darwin"
    local universal_bin="$PROJECT_ROOT/target/$universal_target/release/$APP_NAME"
    local lipo_inputs=()
    local build_count=0

    while IFS= read -r spec; do
        [[ -z "$spec" ]] && continue
        IFS=':' read -r target label <<< "$spec"
        if [[ -z "$target" || -z "$label" ]]; then
            echo "Error: Invalid macOS target spec '$spec'. Expected target:label."
            exit 1
        fi
        build_binary "$target" "macOS ${label}"
        lipo_inputs+=("$(target_path "$target")")
        if [[ -z "$bundle_target" ]]; then
            bundle_target="$target"
        fi
        build_count=$((build_count + 1))
    done < <(target_specs_lines "$MACOS_TARGETS")

    if [[ $build_count -lt 2 ]]; then
        echo "Error: MACOS_TARGETS must contain at least two target:label entries."
        exit 1
    fi

    echo ""
    echo "==> Creating universal binary"
    mkdir -p "$(dirname "$universal_bin")"
    lipo -create "${lipo_inputs[@]}" -output "$universal_bin"

    echo ""
    echo "==> Creating .app bundle"
    cargo bundle --release --target "$bundle_target"

    local app_bundle="$PROJECT_ROOT/target/$bundle_target/release/bundle/osx/${APP_DISPLAY_NAME}.app"

    cp "$universal_bin" "$app_bundle/Contents/MacOS/$APP_NAME"
    echo "    -> $app_bundle (universal binary)"

    echo ""
    echo "==> Code signing .app bundle (ad-hoc)"
    codesign --force --deep --sign - "$app_bundle"
    echo "    -> Signed"

    echo ""
    echo "==> Creating .dmg installer"
    local dmg_path="$DIST_DIR/${APP_DISPLAY_NAME}-${VERSION}-macos-universal.dmg"

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

    hdiutil detach "/Volumes/$APP_DISPLAY_NAME" 2>/dev/null || true

    if [[ -f "$dmg_path" ]]; then
        echo "    -> $dmg_path"
    else
        echo "    !! DMG creation failed, copying .app instead"
        cp -r "$app_bundle" "$DIST_DIR/"
    fi

    local app_zip="$DIST_DIR/${APP_DISPLAY_NAME}-${VERSION}-macos-universal.app.zip"
    (
        cd "$(dirname "$app_bundle")"
        zip -r "$app_zip" "${APP_DISPLAY_NAME}.app" >/dev/null
    )
    echo "    -> $app_zip"

    while IFS= read -r spec; do
        [[ -z "$spec" ]] && continue
        IFS=':' read -r target label <<< "$spec"
        archive_binary "$(target_path "$target")" "$DIST_DIR/${APP_NAME}-${VERSION}-macos-${label}"
    done < <(target_specs_lines "$MACOS_TARGETS")
    archive_binary "$universal_bin" "$DIST_DIR/${APP_NAME}-${VERSION}-macos-universal"
}

# ── Linux ────────────────────────────────────────────────────────────

release_linux() {
    echo ""
    echo "── Linux ──────────────────────────────────"
    local build_count=0

    while IFS= read -r spec; do
        [[ -z "$spec" ]] && continue
        IFS=':' read -r target label <<< "$spec"
        if [[ -z "$target" || -z "$label" ]]; then
            echo "Error: Invalid Linux target spec '$spec'. Expected target:label."
            exit 1
        fi
        build_binary "$target" "Linux ${label}"
        archive_binary "$(target_path "$target")" "$DIST_DIR/${APP_NAME}-${VERSION}-linux-${label}"
        build_count=$((build_count + 1))
    done < <(target_specs_lines "$LINUX_TARGETS")

    if [[ $build_count -eq 0 ]]; then
        echo "Error: LINUX_TARGETS must contain at least one target:label entry."
        exit 1
    fi
}

usage() {
    cat <<EOF
Usage: ./scripts/release.sh [release-macos|release-linux]

Environment overrides:
  MACOS_TARGETS=target:label,target:label
  LINUX_TARGETS=target:label,target:label
EOF
}

# ── Main ─────────────────────────────────────────────────────────────

command="${1:-release-macos}"

case "$command" in
    release-macos)
        release_macos
        ;;
    release-linux)
        release_linux
        ;;
    -h|--help|help)
        usage
        exit 0
        ;;
    *)
        echo "Error: Unknown command '$command'."
        usage
        exit 1
        ;;
esac

echo ""
echo "=========================================="
echo "Release artifacts:"
ls -lh "$DIST_DIR"
