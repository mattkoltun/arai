# Linux Cross-Platform Support Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make ARAI compile and run on desktop Linux with CPU-only Whisper inference.

**Architecture:** Move platform-specific dependency features into target-conditional Cargo.toml sections. Gate macOS-only UI behavior (NSApplication hide/show, GPU toggle, System theme) behind `#[cfg(target_os = "macos")]` and provide Linux alternatives using iced's window APIs. Add Linux CI job.

**Tech Stack:** Rust, Cargo target-conditional dependencies, iced window management, GitHub Actions

**Spec:** `docs/superpowers/specs/2026-03-23-linux-support-design.md`

---

## File Structure

| File | Action | Responsibility |
|---|---|---|
| `Cargo.toml` | Modify | Move `whisper-rs` and `keyring` to target-specific sections |
| `src/ui.rs` | Modify | Platform-gate hide/show, theme selector, GPU toggle |
| `.github/workflows/rust.yml` | Modify | Add Linux CI job |

---

### Task 1: Platform-Conditional Dependencies in Cargo.toml

**Files:**
- Modify: `Cargo.toml:6-20,32-34`

- [ ] **Step 1: Remove `whisper-rs` and `keyring` from `[dependencies]`**

Remove lines 8 and 20 from `Cargo.toml`:
```toml
# DELETE these two lines from [dependencies]:
whisper-rs = { version = "0.16", features = ["metal"] }
keyring = { version = "3", features = ["apple-native"] }
```

- [ ] **Step 2: Add macOS target dependencies for whisper-rs and keyring**

Add `whisper-rs` and `keyring` to the existing `[target.'cfg(target_os = "macos")'.dependencies]` section (lines 32-34), so it becomes:

```toml
[target.'cfg(target_os = "macos")'.dependencies]
whisper-rs = { version = "0.16", features = ["metal"] }
keyring = { version = "3", features = ["apple-native"] }
objc2-app-kit = "0.3"
objc2 = "0.6"
```

- [ ] **Step 3: Add Linux target dependencies section**

Add a new section after the macOS one:

```toml
[target.'cfg(target_os = "linux")'.dependencies]
whisper-rs = "0.16"
keyring = { version = "3", features = ["sync-secret-service", "crypto-rust"] }
```

- [ ] **Step 4: Verify macOS build still works**

Run: `cargo check`
Expected: Compiles without errors on macOS.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "feat: platform-conditional dependencies for Linux support

Move whisper-rs and keyring to target-specific Cargo.toml sections.
macOS: whisper-rs with metal, keyring with apple-native.
Linux: whisper-rs CPU-only, keyring with sync-secret-service."
```

---

### Task 2: Linux hide_app() / show_app() via iced Window APIs

**Files:**
- Modify: `src/ui.rs:22-48` (hide_app/show_app definitions)
- Modify: `src/ui.rs:512-513` (hotkey call site)
- Modify: `src/ui.rs:680` (copy call site)
- Modify: `src/ui.rs:1107-1109` (Cmd+W call site)

- [ ] **Step 1: Remove the non-macOS no-op stubs**

Delete lines 44-48 in `src/ui.rs`:
```rust
// DELETE these lines:
#[cfg(not(target_os = "macos"))]
fn hide_app() {}

#[cfg(not(target_os = "macos"))]
fn show_app() {}
```

- [ ] **Step 2: Update the hotkey call site (Message::Tick)**

Replace lines 512-518 in the `Message::Tick` handler. Currently:
```rust
if hotkey_fired {
    show_app();
    if let Some(id) = state.window_id {
        window::gain_focus(id)
    } else {
        Task::none()
    }
```

Replace with:
```rust
if hotkey_fired {
    #[cfg(target_os = "macos")]
    show_app();
    if let Some(id) = state.window_id {
        window::gain_focus(id)
    } else {
        Task::none()
    }
```

The `window::gain_focus(id)` call already works on both platforms. The only change is gating `show_app()` to macOS.

- [ ] **Step 3: Update the copy call site (Message::Copy)**

Replace line 680 in the `Message::Copy` handler. Currently:
```rust
hide_app();
iced::clipboard::write::<Message>(text)
```

Replace with:
```rust
#[cfg(target_os = "macos")]
hide_app();
let clipboard_task = iced::clipboard::write::<Message>(text);
#[cfg(target_os = "macos")]
{
    clipboard_task
}
#[cfg(not(target_os = "macos"))]
{
    if let Some(id) = state.window_id {
        Task::batch([window::minimize(id, true), clipboard_task])
    } else {
        clipboard_task
    }
}
```

- [ ] **Step 4: Update the Cmd+W call site (Message::KeyPressed)**

Replace lines 1107-1109. Currently:
```rust
keyboard::Key::Character(ref c) if c.as_str() == "w" && modifiers.command() => {
    hide_app();
    Task::none()
}
```

Replace with:
```rust
keyboard::Key::Character(ref c) if c.as_str() == "w" && modifiers.command() => {
    #[cfg(target_os = "macos")]
    hide_app();
    #[cfg(not(target_os = "macos"))]
    if let Some(id) = state.window_id {
        return window::minimize(id, true);
    }
    Task::none()
}
```

- [ ] **Step 5: Verify macOS build**

Run: `cargo check`
Expected: Compiles without errors. The macOS behavior is unchanged.

- [ ] **Step 6: Commit**

```bash
git add src/ui.rs
git commit -m "feat: Linux window hide/show via iced minimize/focus

Replace no-op stubs with iced window::minimize and window::gain_focus
on Linux. macOS keeps NSApplication hide/unhide behavior."
```

---

### Task 3: Hide "System" Theme Option on Linux

**Files:**
- Modify: `src/ui.rs:2161-2165` (theme selector in view_setup_tab)

- [ ] **Step 1: Gate the System theme radio button**

Replace lines 2161-2165. Currently:
```rust
let theme_selector = row![
    theme_radio("Dark", ThemeMode::Dark),
    theme_radio("Light", ThemeMode::Light),
    theme_radio("System", ThemeMode::System),
]
.spacing(6);
```

Replace with:
```rust
let mut theme_selector = row![
    theme_radio("Dark", ThemeMode::Dark),
    theme_radio("Light", ThemeMode::Light),
]
.spacing(6);
#[cfg(target_os = "macos")]
{
    theme_selector = theme_selector.push(theme_radio("System", ThemeMode::System));
}
```

- [ ] **Step 2: Verify macOS build**

Run: `cargo check`
Expected: Compiles without errors. macOS still shows all three options.

- [ ] **Step 3: Commit**

```bash
git add src/ui.rs
git commit -m "feat: hide System theme option on Linux

Only show Dark and Light theme options on Linux since there is no
system dark mode detection. macOS keeps all three options."
```

---

### Task 4: Hide GPU Toggle on Linux

**Files:**
- Modify: `src/ui.rs:2265-2314` (view_advanced_tab function)

- [ ] **Step 1: Gate the GPU toggle and its description**

Replace the `view_advanced_tab` function body (lines 2265-2319). Currently it builds a single `gpu_card` column with GPU toggle, flash attention toggle, and no-timestamps toggle. Restructure so the GPU toggle section is macOS-only:

```rust
fn view_advanced_tab(state: &UiRuntime) -> Column<'_, Message> {
    let flash_attn_toggle = toggler(state.config_flash_attn)
        .label("Flash Attention")
        .on_toggle(Message::FlashAttnToggled)
        .text_size(13)
        .spacing(10)
        .size(20);

    let no_timestamps_toggle = toggler(state.config_no_timestamps)
        .label("Disable Timestamps")
        .on_toggle(Message::NoTimestampsToggled)
        .text_size(13)
        .spacing(10)
        .size(20);

    let mut card = column![
        text("Model Inference")
            .size(15)
            .color(current_palette().text),
    ]
    .spacing(12)
    .padding(14);

    #[cfg(target_os = "macos")]
    {
        let gpu_toggle = toggler(state.config_use_gpu)
            .label("GPU Acceleration")
            .on_toggle(Message::UseGpuToggled)
            .text_size(13)
            .spacing(10)
            .size(20);
        card = card.push(
            column![
                text("Enable Metal GPU for faster inference on Apple Silicon.")
                    .size(11)
                    .color(current_palette().muted),
                gpu_toggle,
            ]
            .spacing(6),
        );
    }

    card = card.push(
        column![
            text("Use flash attention for reduced memory and faster decoding.")
                .size(11)
                .color(current_palette().muted),
            flash_attn_toggle,
        ]
        .spacing(6),
    );

    card = card.push(
        column![
            text("Skip timestamp computation for faster output.")
                .size(11)
                .color(current_palette().muted),
            no_timestamps_toggle,
        ]
        .spacing(6),
    );

    column![container(card).style(surface_container).width(Fill)]
        .spacing(12)
        .padding(14)
}
```

- [ ] **Step 2: Verify macOS build**

Run: `cargo check`
Expected: Compiles without errors. macOS still shows all three toggles.

- [ ] **Step 3: Run clippy**

Run: `cargo clippy --all-targets --all-features -- -D warnings`
Expected: No warnings or errors.

- [ ] **Step 4: Commit**

```bash
git add src/ui.rs
git commit -m "feat: hide GPU acceleration toggle on Linux

GPU toggle is only relevant on macOS (Metal backend). Flash attention
and disable timestamps toggles remain on all platforms."
```

---

### Task 5: Add Linux CI Job

**Files:**
- Modify: `.github/workflows/rust.yml`

- [ ] **Step 1: Rename existing job and add Linux job**

Replace the entire `.github/workflows/rust.yml` with:

```yaml
name: CI

on:
  push:
    branches: [ "main" ]
  pull_request:
    branches: [ "main" ]

env:
  CARGO_TERM_COLOR: always

jobs:
  build-macos:
    runs-on: macos-latest

    steps:
    - uses: actions/checkout@v4
    - uses: dtolnay/rust-toolchain@stable
    - name: Install system dependencies
      run: brew install cmake
    - name: Build
      run: cargo build --verbose
    - name: Run tests
      run: cargo test --verbose
    - name: Clippy
      run: cargo clippy --all-targets --all-features -- -D warnings
    - name: Format check
      run: cargo fmt -- --check

  build-linux:
    runs-on: ubuntu-latest

    steps:
    - uses: actions/checkout@v4
    - uses: dtolnay/rust-toolchain@stable
    - name: Install system dependencies
      run: |
        sudo apt-get update
        sudo apt-get install -y cmake libasound2-dev libdbus-1-dev libsecret-1-dev libgtk-3-dev libxkbcommon-dev
    - name: Build
      run: cargo build --verbose
    - name: Run tests
      run: cargo test --verbose
    - name: Clippy
      run: cargo clippy --all-targets --all-features -- -D warnings
    - name: Format check
      run: cargo fmt -- --check
```

- [ ] **Step 2: Commit**

```bash
git add .github/workflows/rust.yml
git commit -m "ci: add Linux build job

Adds ubuntu-latest CI with ALSA, D-Bus, libsecret, and GTK
system dependencies for cpal, keyring, and rfd."
```

---

### Task 6: Final Verification

- [ ] **Step 1: Run full build**

Run: `cargo build`
Expected: Clean build on macOS.

- [ ] **Step 2: Run full test suite**

Run: `cargo test`
Expected: All tests pass.

- [ ] **Step 3: Run clippy**

Run: `cargo clippy --all-targets --all-features -- -D warnings`
Expected: No warnings.

- [ ] **Step 4: Run format check**

Run: `cargo fmt -- --check`
Expected: No formatting issues.
