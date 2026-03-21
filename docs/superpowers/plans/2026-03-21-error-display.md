# Error Display Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Display errors from all sources (Recorder, Transcriber, Agent) in the UI via a warning button and error detail view.

**Architecture:** Controller builds `ErrorInfo` from error events and sends `UiUpdate::ErrorOccurred` to the UI. The UI shows a warning button in the bottom bar; clicking it swaps the editor for an error detail view. Dismissing clears the error.

**Tech Stack:** Rust 2024, iced 0.14 (Elm architecture)

**Spec:** `docs/superpowers/specs/2026-03-21-error-display-design.md`

---

## File Structure

| File | Action | Responsibility |
|------|--------|---------------|
| `src/messages.rs` | Modify | Add `ErrorInfo` struct, `UiUpdate::ErrorOccurred` variant |
| `src/controller.rs` | Modify | Add `build_error_info()` + `format_timestamp()`, send errors to UI |
| `src/ui.rs` | Modify | Add state fields, messages, warning button, error detail view |

---

### Task 1: Add ErrorInfo struct and UiUpdate variant

**Files:**
- Modify: `src/messages.rs`

- [ ] **Step 1: Add `ErrorInfo` struct**

In `src/messages.rs`, add after the `ApiKeyStatus` enum (after line 56):

```rust
/// Structured error information for display in the UI.
#[derive(Clone, Debug)]
pub struct ErrorInfo {
    /// Which component produced the error ("Recorder", "Transcriber", "Agent").
    pub source: String,
    /// Short summary extracted from before the first ": " in the error message.
    pub title: String,
    /// Full error detail extracted from after the first ": ".
    pub detail: String,
    /// Human-readable UTC timestamp (e.g., "14:32:05").
    pub timestamp: String,
}
```

- [ ] **Step 2: Add `UiUpdate::ErrorOccurred` variant**

In the `UiUpdate` enum (around line 42), add before the closing brace:

```rust
    /// An error occurred in a component — display to user.
    ErrorOccurred(ErrorInfo),
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo build`
Expected: Compiles (warning about unused `ErrorOccurred` is fine).

- [ ] **Step 4: Commit**

```bash
git add src/messages.rs
git commit -m "feat: add ErrorInfo struct and UiUpdate::ErrorOccurred variant"
```

---

### Task 2: Wire controller to send ErrorOccurred for all error sources

**Files:**
- Modify: `src/controller.rs`

- [ ] **Step 1: Add `ErrorInfo` to imports**

At `src/controller.rs:5`, update the import:

```rust
use crate::messages::{AppEvent, AppEventKind, AppEventSource, ErrorInfo, UiUpdate};
```

- [ ] **Step 2: Add `format_timestamp()` helper**

Add after the imports (before `pub struct ShutdownHandle` at line 13):

```rust
/// Formats current UTC time as HH:MM:SS for error timestamps.
fn format_timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let hours = (secs / 3600) % 24;
    let minutes = (secs / 60) % 60;
    let seconds = secs % 60;
    format!("{hours:02}:{minutes:02}:{seconds:02}")
}
```

- [ ] **Step 3: Add `build_error_info()` helper**

Add after `format_timestamp()`:

```rust
/// Builds an `ErrorInfo` by splitting the message on the first ": ".
/// The part before becomes the title, the part after becomes the detail.
fn build_error_info(source_name: &str, message: &str) -> ErrorInfo {
    let (title, detail) = match message.split_once(": ") {
        Some((t, d)) => (t.to_string(), d.to_string()),
        None => (format!("{source_name} error"), message.to_string()),
    };
    ErrorInfo {
        source: source_name.to_string(),
        title,
        detail,
        timestamp: format_timestamp(),
    }
}
```

Note: takes `source_name: &str` (not `&AppEventSource`) because the match at line 205 destructures `(event.source, event.kind)` — both values are moved, so `event.source` is no longer available. The source is known from the pattern arm.

- [ ] **Step 4: Wire Recorder error arm**

At line 215-218, replace:

```rust
                (AppEventSource::Recorder, AppEventKind::Error(message)) => {
                    error!("Recorder event: {message}");
                    // TODO: implement recorder error handling (e.g., restart recorder or update UI)
                }
```

With:

```rust
                (AppEventSource::Recorder, AppEventKind::Error(message)) => {
                    error!("Recorder event: {message}");
                    let info = build_error_info("Recorder", &message);
                    let _ = self.ui_update_tx.send(UiUpdate::ErrorOccurred(info));
                }
```

- [ ] **Step 5: Wire Transcriber error arm**

At line 219-221, replace:

```rust
                (AppEventSource::Transcriber, AppEventKind::Error(message)) => {
                    error!("Transcriber event: {message}");
                }
```

With:

```rust
                (AppEventSource::Transcriber, AppEventKind::Error(message)) => {
                    error!("Transcriber event: {message}");
                    let info = build_error_info("Transcriber", &message);
                    let _ = self.ui_update_tx.send(UiUpdate::ErrorOccurred(info));
                }
```

- [ ] **Step 6: Wire Agent error arm**

At line 260-263, replace:

```rust
                (AppEventSource::Agent, AppEventKind::Error(message)) => {
                    error!("Agent event: {message}");
                    let _ = self.ui_update_tx.send(UiUpdate::ProcessingFailed(message));
                }
```

With:

```rust
                (AppEventSource::Agent, AppEventKind::Error(message)) => {
                    error!("Agent event: {message}");
                    let info = build_error_info("Agent", &message);
                    let _ = self.ui_update_tx.send(UiUpdate::ErrorOccurred(info));
                    let _ = self.ui_update_tx.send(UiUpdate::ProcessingFailed(message));
                }
```

- [ ] **Step 7: Write tests for `build_error_info`**

The file already has `#[cfg(test)] mod tests` with existing tests. Add these inside the existing test module:

```rust
    #[test]
    fn build_error_info_splits_on_colon() {
        let info = super::build_error_info("Agent", "Agent request failed: connection timeout");
        assert_eq!(info.source, "Agent");
        assert_eq!(info.title, "Agent request failed");
        assert_eq!(info.detail, "connection timeout");
        assert!(!info.timestamp.is_empty());
    }

    #[test]
    fn build_error_info_no_colon_uses_source_as_title() {
        let info = super::build_error_info("Recorder", "something went wrong");
        assert_eq!(info.source, "Recorder");
        assert_eq!(info.title, "Recorder error");
        assert_eq!(info.detail, "something went wrong");
    }

    #[test]
    fn build_error_info_multiple_colons_splits_on_first() {
        let info = super::build_error_info("Transcriber", "Model error: path: /foo/bar not found");
        assert_eq!(info.title, "Model error");
        assert_eq!(info.detail, "path: /foo/bar not found");
    }
```

- [ ] **Step 8: Run tests**

Run: `cargo test controller::tests`
Expected: All tests pass (existing + 3 new).

- [ ] **Step 9: Run clippy**

Run: `cargo clippy --all-targets --all-features -- -D warnings`
Expected: Clean.

- [ ] **Step 10: Commit**

```bash
git add src/controller.rs
git commit -m "feat: send ErrorOccurred to UI for all error sources"
```

---

### Task 3: Add UI state, messages, and error handlers

**Files:**
- Modify: `src/ui.rs`

- [ ] **Step 1: Add `ErrorInfo` import**

At line 4, update the import:

```rust
use crate::messages::{ApiKeyStatus, AppEvent, AppEventKind, AppEventSource, ErrorInfo, UiUpdate};
```

- [ ] **Step 2: Add Message variants**

In the `Message` enum (around line 665), add before `Shutdown`:

```rust
    ShowErrorDetail,
    DismissError,
```

- [ ] **Step 3: Add state fields to `UiRuntime`**

The `UiRuntime` struct fields are defined starting around line 558. Add after `config_api_key_status: ApiKeyStatus` (around line 615):

```rust
    /// The most recent error, if any. Cleared when the user dismisses it.
    last_error: Option<ErrorInfo>,
    /// Whether the error detail view is currently shown.
    showing_error_detail: bool,
```

- [ ] **Step 4: Initialize state fields**

In the `boot()` closure (around line 538), add after `config_api_key_status: ApiKeyStatus::NotSet,`:

```rust
                    last_error: None,
                    showing_error_detail: false,
```

- [ ] **Step 5: Handle `UiUpdate::ErrorOccurred` in poll loop**

In the `UiUpdate` match block in `update()`, add after the `ProcessingFailed` handler (after line 792):

```rust
                UiUpdate::ErrorOccurred(error_info) => {
                    state.last_error = Some(error_info);
                }
```

- [ ] **Step 6: Handle `ShowErrorDetail` and `DismissError` messages**

In the `update()` function's main match, add after the `DragWindow` handler:

```rust
        Message::ShowErrorDetail => {
            state.showing_error_detail = true;
            Task::none()
        }
        Message::DismissError => {
            state.last_error = None;
            state.showing_error_detail = false;
            Task::none()
        }
```

- [ ] **Step 7: Verify it compiles**

Run: `cargo build`
Expected: Compiles (warnings about unused variants are fine at this point).

- [ ] **Step 8: Commit**

```bash
git add src/ui.rs
git commit -m "feat: add error state fields and message handlers to UI"
```

---

### Task 4: Add warning button and error detail view to view_main

**Files:**
- Modify: `src/ui.rs`

- [ ] **Step 1: Add `view_error_detail()` function**

Add a new function right after `view_main()` ends (after line 1900):

```rust
/// Renders the error detail view that replaces the editor when an error is being viewed.
fn view_error_detail(error: &ErrorInfo) -> Element<'_, Message> {
    let title = text(&error.title).size(18).color(RED);
    let source_line = text(format!("Source: {}", error.source))
        .size(12)
        .color(MUTED);
    let time_line = text(format!("Time: {}", error.timestamp))
        .size(12)
        .color(MUTED);
    let detail = text(&error.detail).size(13).color(TEXT_COLOR);

    let dismiss_btn = button(
        row![icon('\u{E5CD}', 16.0), text("Dismiss").size(13)]
            .spacing(6)
            .align_y(iced::Alignment::Center),
    )
    .style(ghost_btn)
    .padding([6, 14])
    .on_press(Message::DismissError);

    let content = column![
        title,
        source_line,
        time_line,
        container(scrollable(detail).height(Fill))
            .padding([10, 0])
            .height(Fill),
        dismiss_btn,
    ]
    .spacing(8)
    .padding(14);

    container(content)
        .style(surface_container)
        .padding(4)
        .height(Fill)
        .width(Fill)
        .into()
}
```

- [ ] **Step 2: Add warning button to the bottom bar in `view_main()`**

In `view_main()`, find the bottom bar definition (around line 1851-1855):

```rust
    let bottom_bar = column![
        container(button_group).center_x(Fill),
        container(char_count_text).padding([4, 18])
    ]
    .spacing(6);
```

Replace with:

```rust
    let mut bottom_bar = column![
        container(button_group).center_x(Fill),
        container(char_count_text).padding([4, 18])
    ]
    .spacing(6);

    if let Some(ref error) = state.last_error
        && !state.showing_error_detail
    {
        let warning_btn = button(
            row![icon('\u{E002}', 16.0), text(&error.title).size(11)]
                .spacing(4)
                .align_y(iced::Alignment::Center),
        )
        .style(icon_btn_danger)
        .padding([2, 8])
        .on_press(Message::ShowErrorDetail);
        bottom_bar = bottom_bar.push(container(warning_btn).padding([0, 14]));
    }
```

- [ ] **Step 3: Swap editor for error detail view**

In `view_main()`, find the body definition (around line 1882-1891):

```rust
    let body = column![
        prompt_carousel,
        container(editor_widget)
            .style(surface_container)
            .padding(4)
            .height(FillPortion(8)),
        container(bottom_bar).height(FillPortion(2))
    ]
    .spacing(8)
    .padding([0, 14]);
```

Replace with:

```rust
    let content_area: Element<'_, Message> = if state.showing_error_detail
        && let Some(ref error) = state.last_error
    {
        view_error_detail(error)
    } else {
        container(editor_widget)
            .style(surface_container)
            .padding(4)
            .height(Fill)
            .into()
    };

    let body = column![
        prompt_carousel,
        container(content_area).height(FillPortion(8)),
        container(bottom_bar).height(FillPortion(2))
    ]
    .spacing(8)
    .padding([0, 14]);
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo build`
Expected: Compiles successfully.

- [ ] **Step 5: Run full test suite and clippy**

Run: `cargo test && cargo clippy --all-targets --all-features -- -D warnings`
Expected: All pass, no warnings.

- [ ] **Step 6: Commit**

```bash
git add src/ui.rs
git commit -m "feat: add warning button and error detail view to main UI"
```

---

### Task 5: Manual testing

**Files:** None (testing only)

- [ ] **Step 1: Test error display with invalid API key**

Set an invalid API key (e.g., `sk-invalid`) and submit text.

Expected:
1. Agent error occurs
2. Warning button appears in bottom-left with error title (e.g., "Agent request failed")
3. Clicking the warning button shows error detail view with source, timestamp, and full error message
4. Clicking "Dismiss" returns to normal editor view

- [ ] **Step 2: Test error persistence**

Trigger an error, verify the warning button stays visible across UI interactions (opening/closing settings, toggling listen, etc.) until explicitly dismissed.

- [ ] **Step 3: Test error replacement**

Trigger two errors in sequence (e.g., submit twice with invalid key). Verify only the most recent error is shown.

- [ ] **Step 4: Run final checks**

Run: `cargo fmt --check && cargo clippy --all-targets --all-features -- -D warnings && cargo test`
Expected: All pass.
