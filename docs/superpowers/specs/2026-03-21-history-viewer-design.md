# History Viewer

## Overview

A history view accessible from the bottom bar that displays the 50 most recent copy+hide entries. Each entry shows its text and timestamp with a copy button. Copying an entry switches back to the main view and copies the text to the clipboard.

## Navigation

A history icon button (Material Icons `\u{E889}` — history clock) is added to the bottom bar between the copy button and the settings button. Like the settings button, it is disabled when `busy` (listening/processing/reconciling). Clicking it opens the history view.

A `history_open: bool` flag on `UiRuntime` controls the view switch. Within the `AppPhase::Main` branch of `view()`:

```rust
if state.config_open {
    view_config(...)
} else if state.history_open {
    view_history(...)
} else {
    view_main(...)
}
```

## Data Loading

### `history::load_recent(limit: usize) -> Vec<HistoryRecord>`

New public function in `src/history.rs`. Called synchronously in the `Message::OpenHistory` handler.

1. Read the history directory (`~/.local/share/arai/history/`)
2. Filter `*.json` files, parse numeric IDs from filenames
3. Sort IDs descending (newest first)
4. Take the first `limit` entries
5. Read and parse each JSON file into a `HistoryRecord`
6. Skip files that fail to parse — log with `warn!("Failed to parse history file {}: {}", path, err)`
7. Return the collected `Vec<HistoryRecord>`

If the directory doesn't exist or is empty, returns an empty vec.

### `HistoryRecord` Changes

The existing `HistoryRecord` struct gains `pub` visibility, `Deserialize`, and `Clone`. Add `use serde::Deserialize;` to the imports.

```rust
#[derive(Clone, Serialize, Deserialize)]
pub struct HistoryRecord {
    pub text: String,
    pub timestamp: String,
    pub prompt: String,
}
```

## UI State

New fields on `UiRuntime`:

```rust
history_open: bool,
history_entries: Vec<HistoryRecord>,
```

Initialized to `false` and empty vec.

## Messages

```rust
Message::OpenHistory    // Load entries, set history_open = true
Message::CloseHistory   // Set history_open = false, clear entries
Message::CopyHistoryEntry(usize)  // Copy entry text, close history view
```

### Message Handlers

**OpenHistory:**
```
state.history_entries = history::load_recent(50);
state.history_open = true;
```

**CloseHistory:**
```
state.history_open = false;
state.history_entries.clear();
```

**CopyHistoryEntry(index):**
```
if index >= state.history_entries.len() {
    return Task::none();
}
let text = state.history_entries[index].text.clone();
state.history_open = false;
state.history_entries.clear();
clipboard::write(text)
```

Note: does NOT call `hide_app()` — just copies and returns to main view.

## View Layout

### `view_history(state) -> Element<Message>`

New function, follows the same chrome pattern as `view_main`:

```
container {
  top_bar {
    close_btn (icon \u{E5CD}, size 20, padding 6, ghost_btn style, Message::CloseHistory)
  }
  body {
    if entries.is_empty() {
      centered muted text: "No history yet."
    } else {
      scrollable (height Fill) {
        column (spacing 12, padding [0, 14]) of entry rows
      }
    }
  }
}
```

### Entry Row

Each entry is a horizontal row:

```
row (spacing 8, align center) {
  column (Fill) {
    text (first 200 chars of entry.text + "..." if truncated, size 13, TEXT_COLOR)
    text (entry.timestamp displayed as-is in ISO 8601 format, size 11, MUTED)
  }
  copy_btn (icon \u{E14D}, size 16, icon_btn style, Message::CopyHistoryEntry(index))
}
```

Text truncation is done by character count (first 200 characters), not by visual line clamping. If the text exceeds 200 characters, append "...".

The scrollable container fills the available height.

## Bottom Bar Change

The button group in `view_main()` changes from:

```rust
row![mic_btn, send_btn, copy_btn, settings_btn]
```

To:

```rust
row![mic_btn, send_btn, copy_btn, history_btn, settings_btn]
```

The history button uses `icon('\u{E889}', 22.0)` with `icon_btn` style. Disabled when `busy`, same as settings button: `.on_press_maybe((!busy).then_some(Message::OpenHistory))`.

## Changes to Existing Code

### `src/history.rs`
- Add `use serde::Deserialize;` to imports
- Make `HistoryRecord` pub, add `Deserialize` and `Clone` derives
- Add `pub fn load_recent(limit: usize) -> Vec<HistoryRecord>`
- Reuse `history_dir()` and filename parsing logic from `scan_next_id()`

### `src/ui.rs`
- Import `HistoryRecord` from `crate::history`
- Add `history_open: bool` and `history_entries: Vec<HistoryRecord>` state fields
- Initialize in `boot()` closure
- Add `OpenHistory`, `CloseHistory`, `CopyHistoryEntry(usize)` message variants
- Add message handlers in `update()`
- Add `view_history()` function
- Add history button to bottom bar in `view_main()`
- Update view switch logic in `AppPhase::Main` branch to include history view

## Error Handling

- Directory missing or unreadable: return empty vec, no error shown
- Individual file parse failure: log warning, skip entry
- Out-of-bounds index in `CopyHistoryEntry`: guard with bounds check, return `Task::none()`
