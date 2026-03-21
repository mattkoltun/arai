# Error Display Feature

## Overview

Errors from all sources (Recorder, Transcriber, Agent) are currently logged but not shown to the user. This feature adds a floating warning button in the main view that appears when an error occurs. Clicking it opens an error detail view showing the source, title, detail, and timestamp. The indicator persists until the user dismisses it.

## Error Data Model

### `ErrorInfo` struct

A new struct in `messages.rs`:

```rust
#[derive(Clone, Debug)]
pub struct ErrorInfo {
    /// Which component produced the error ("Recorder", "Transcriber", "Agent").
    pub source: String,
    /// Short summary extracted from before the first ": " in the error message.
    pub title: String,
    /// Full error detail extracted from after the first ": " in the error message.
    pub detail: String,
    /// Human-readable timestamp (e.g., "14:32:05").
    pub timestamp: String,
}
```

### Title/Detail Extraction

Error messages in the codebase follow the pattern `"Title: {err}"`:
- `"Agent request failed: {err}"`
- `"Stream config error: {err}"`
- `"Build stream error: {err}"`
- `"Failed to load model: {err}"`
- `"Transcription error: {err}"`

The controller splits on the first `: ` to extract the title and detail. If no `: ` separator is found, the title defaults to `"{Source} error"` and detail is the entire message.

### `UiUpdate::ErrorOccurred`

A new variant on `UiUpdate`:

```rust
UiUpdate::ErrorOccurred(ErrorInfo)
```

### Controller Changes

All three error arms in the controller event loop send `UiUpdate::ErrorOccurred(ErrorInfo)` to the UI. The existing `ProcessingFailed` handling for Agent errors (resetting mode to Idle) remains unchanged — `ErrorOccurred` is sent in addition to it.

A helper function builds `ErrorInfo` from the source and message:

```rust
fn build_error_info(source: &AppEventSource, message: &str) -> ErrorInfo {
    let source_name = match source {
        AppEventSource::Recorder => "Recorder",
        AppEventSource::Transcriber => "Transcriber",
        AppEventSource::Agent => "Agent",
        AppEventSource::Ui => "System",
    };
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

### Timestamp

No external dependency. Use `std::time::SystemTime` with manual formatting:

```rust
fn format_timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // UTC time — acceptable for error timestamps
    let hours = (secs / 3600) % 24;
    let minutes = (secs / 60) % 60;
    let seconds = secs % 60;
    format!("{hours:02}:{minutes:02}:{seconds:02}")
}
```

### Recorder and Transcriber Error Forwarding

Currently the controller only forwards Agent errors to the UI. Recorder errors have a `// TODO` comment, and Transcriber errors are silently logged. With this change, all three send `ErrorOccurred` to the UI. The existing logging (`error!()` calls) remains. The Recorder TODO comment is removed.

## UI State

Two new fields on `UiRuntime`:

```rust
/// The most recent error, if any. Cleared when the user dismisses it.
last_error: Option<ErrorInfo>,
/// Whether the error detail view is currently shown.
showing_error_detail: bool,
```

**Initialization** in the `boot()` closure:

```rust
last_error: None,
showing_error_detail: false,
```

### New Messages

```rust
Message::ShowErrorDetail,      // User clicked the warning button
Message::DismissError,         // User clicked "Dismiss" in the detail view
```

### Message Handlers

```rust
UiUpdate::ErrorOccurred(error_info) => {
    state.last_error = Some(error_info);
    // Don't change showing_error_detail — if user is viewing a previous
    // error, the detail view updates in place with the new error.
}

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

## Warning Button

- **Position:** Bottom-left of `view_main()`. The bottom bar is currently a column with centered buttons and char count below. Add the warning button as a new row below char count, left-aligned.
- **Icon:** Material Icons warning `\u{E002}`, colored RED.
- **Label:** The error title text next to the icon, colored RED. Clipped by container width (no manual truncation).
- **Visibility:** Only rendered when `last_error.is_some() && !showing_error_detail`.
- **Action:** `Message::ShowErrorDetail` — sets `showing_error_detail = true`.

## Error Detail View

When `showing_error_detail == true`, the editor area (the `FillPortion(8)` container) in `view_main()` is replaced with the error detail view. The top bar, prompt carousel, and bottom bar remain visible.

```rust
let content_area = if state.showing_error_detail {
    view_error_detail(state)
} else {
    // existing editor widget
};
```

**`view_error_detail()` layout:**
- Title: error title in RED, size 18
- Source: "Source: {source}" in MUTED, size 12
- Timestamp: "Time: {timestamp}" in MUTED, size 12
- Separator (small vertical space)
- Detail: full error detail text in a scrollable container, TEXT_COLOR, size 13
- "Dismiss" button (ghost style) at bottom

## Behavior

1. Error occurs in any component (Recorder, Transcriber, Agent)
2. Controller builds `ErrorInfo` and sends `UiUpdate::ErrorOccurred`
3. UI sets `last_error = Some(info)`
4. Warning button appears in the bottom-left of the main view
5. User clicks warning button — `showing_error_detail = true`, detail view replaces editor
6. User clicks "Dismiss" — `last_error = None`, `showing_error_detail = false`, editor reappears
7. If a new error arrives while detail view is open, `last_error` is replaced (detail view updates in place)
8. If a new error arrives while warning button is showing, `last_error` is replaced with the new error

## Changes to Existing Code

### `src/messages.rs`
- Add `ErrorInfo` struct
- Add `UiUpdate::ErrorOccurred(ErrorInfo)` variant

### `src/controller.rs`
- Add `build_error_info()` helper function (with `format_timestamp()`)
- Recorder error arm: add `UiUpdate::ErrorOccurred` send, remove TODO comment
- Transcriber error arm: add `UiUpdate::ErrorOccurred` send
- Agent error arm: add `UiUpdate::ErrorOccurred` send (alongside existing `ProcessingFailed`)

### `src/ui.rs`
- Add `last_error: Option<ErrorInfo>` and `showing_error_detail: bool` to `UiRuntime`, initialized to `None`/`false`
- Add `Message::ShowErrorDetail` and `Message::DismissError` variants
- Handle `UiUpdate::ErrorOccurred` in the poll loop
- Handle `ShowErrorDetail` and `DismissError` in `update()`
- Add `view_error_detail()` function
- Modify `view_main()`: swap editor area for error detail when `showing_error_detail`
- Modify `view_main()`: add warning button row to bottom bar when error exists
