# Linux Cross-Platform Support

## Goal

Make ARAI compile and run on desktop Linux (primarily X11) with CPU-only Whisper inference. No feature parity requirements — macOS keeps its NSApplication and Metal GPU behaviors.

## Changes

### 1. Cargo.toml — Platform-Conditional Features

Remove `whisper-rs` and `keyring` from `[dependencies]`. Add them to target-specific sections:

```toml
[target.'cfg(target_os = "macos")'.dependencies]
whisper-rs = { version = "0.16", features = ["metal"] }
keyring = { version = "3", features = ["apple-native"] }
objc2-app-kit = "0.3"
objc2 = "0.6"

[target.'cfg(target_os = "linux")'.dependencies]
whisper-rs = "0.16"
keyring = { version = "3", features = ["sync-secret-service", "crypto-rust"] }
```

On Linux, `whisper-rs` compiles with CPU-only inference (no feature flags). The `keyring` crate uses `sync-secret-service` with `crypto-rust` for D-Bus encryption, which stores credentials via the Secret Service API (persistent across reboots). Note: the `linux-native` feature uses the kernel session keyring which loses credentials on reboot — `sync-secret-service` is the correct choice.

### 2. ui.rs — hide_app() / show_app() on Linux

macOS keeps its `NSApplication::hide`/`unhide` behavior (app-level hide, returns focus to previous app).

Linux uses iced's window management APIs. The current no-op stubs (`fn hide_app() {}` / `fn show_app() {}`) already compile on Linux, but we replace them with functional behavior.

Since `window::minimize(id)` and `window::gain_focus(id)` return `Task<Message>` and require a `window::Id`, the approach is:
- Remove the `#[cfg(not(target_os = "macos"))]` no-op free functions
- At each call site, use `cfg` to branch between macOS (free function) and Linux (iced Task)
- The Linux path accesses `state.window_id` which is already available in `update()`

The three call sites:

| Call site | macOS | Linux |
|---|---|---|
| `Message::Copy` | `hide_app()` + clipboard task | `window::minimize(id)` chained with clipboard task |
| `Message::Tick` (hotkey) | `show_app()` + `window::gain_focus(id)` | `window::gain_focus(id)` only |
| `Message::KeyPressed` Cmd+W | `hide_app()` | `window::minimize(id)` |

### 3. ui.rs — Theme Selector (Linux)

Remove the "System" option from the theme selector on Linux using `#[cfg]`. Only Dark and Light are shown. This is a UX improvement — the app already compiles without it since `system_is_dark()` has a non-macOS fallback returning `true`.

If a Linux user has `theme: system` in their config YAML, the existing fallback resolves to Dark. No config-level change needed.

### 4. ui.rs — Advanced Tab (Linux)

Hide the "GPU Acceleration" toggle on Linux with `#[cfg(target_os = "macos")]`. The toggle controls whisper-rs `use_gpu` which has no GPU backend on the Linux build.

Flash Attention and Disable Timestamps toggles remain visible (CPU-relevant).

### 5. CI — Linux Build Job

Add a Linux job to `.github/workflows/rust.yml`:

- `runs-on: ubuntu-latest`
- System dependencies: `cmake`, `libasound2-dev` (ALSA for cpal), `libdbus-1-dev` (for D-Bus/secret-service keyring), `libsecret-1-dev` (for keyring), `libgtk-3-dev` (for rfd file dialogs), `libxkbcommon-dev` (for iced keyboard handling)
- Same steps: build, test, clippy, fmt check

No changes to `release.yml` (Linux release artifacts are out of scope for this work).

## Files Modified

- `Cargo.toml`
- `src/ui.rs`
- `.github/workflows/rust.yml`

## Known Risks

- **Wayland + `decorations(false)`**: The app uses `decorations(false)` with a custom drag handler. On Wayland compositors, CSD (client-side decorations) are the norm and a window without decorations may not be draggable depending on the compositor. X11 is the primary Linux target.
- **`aplay` for blip sound**: The `play_blip()` function uses `aplay` (from `alsa-utils`) on Linux. If not installed, the blip sound silently fails (logs a warning). Acceptable.
- **`global-hotkey` on Wayland**: Uses X11 `XGrabKey` APIs. On pure Wayland sessions without XWayland, the hotkey silently fails to register. The app handles `None` from `HotkeyHandle::register` gracefully.

## Out of Scope

- Linux GPU acceleration (CUDA)
- Linux system dark mode detection
- Linux release artifacts (.deb, .AppImage)
- Wayland-specific global hotkey support
- Windows support changes
