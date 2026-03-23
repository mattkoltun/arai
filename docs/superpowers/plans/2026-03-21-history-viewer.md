# History Viewer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a history viewer that displays the 50 most recent copy+hide entries with copy buttons, accessible from the bottom bar.

**Architecture:** Add `load_recent()` to the existing history module for reading JSON files. Add history view to the UI with open/close/copy messages, a scrollable entry list, and a history icon button in the bottom bar. Loading happens synchronously in the UI update handler.

**Tech Stack:** Rust 2024, iced 0.14, serde/serde_json

**Spec:** `docs/superpowers/specs/2026-03-21-history-viewer-design.md`

---

## File Structure

| File | Action | Responsibility |
|------|--------|---------------|
| `src/history.rs` | Modify | Make `HistoryRecord` pub, add `Deserialize`/`Clone`, add `load_recent()` |
| `src/ui.rs` | Modify | Add state fields, messages, handlers, `view_history()`, history button, view switch |

---

### Task 1: Make HistoryRecord public and add load_recent()

**Files:**
- Modify: `src/history.rs:1-21` (imports, struct), add `load_recent()` after `Drop` impl (line 103)

- [ ] **Step 1: Update imports and HistoryRecord**

In `src/history.rs`, change line 1 from:

```rust
use log::{error, info};
```

To:

```rust
use log::{error, info, warn};
```

Then change line 2 from:

```rust
use serde::Serialize;
```

To:

```rust
use serde::{Deserialize, Serialize};
```

Then change lines 15-21 from:

```rust
/// JSON structure written to each history file.
#[derive(Serialize)]
struct HistoryRecord {
    text: String,
    timestamp: String,
    prompt: String,
}
```

To:

```rust
/// JSON structure written to each history file.
#[derive(Clone, Serialize, Deserialize)]
pub struct HistoryRecord {
    pub text: String,
    pub timestamp: String,
    pub prompt: String,
}
```

- [ ] **Step 2: Add `load_recent()` function**

Add after the `Drop` impl (after line 103), before `fn history_dir()`:

```rust
/// Loads the most recent `limit` history entries, newest first.
///
/// Scans the history directory for JSON files, sorts by ID descending,
/// reads and parses each file. Files that fail to parse are skipped.
pub fn load_recent(limit: usize) -> Vec<HistoryRecord> {
    load_recent_from(&history_dir(), limit)
}

/// Internal implementation that accepts a directory path (testable).
fn load_recent_from(dir: &PathBuf, limit: usize) -> Vec<HistoryRecord> {
    let mut ids: Vec<u64> = fs::read_dir(dir)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let name = entry.file_name();
            let name = name.to_str()?;
            let stem = name.strip_suffix(".json")?;
            stem.parse::<u64>().ok()
        })
        .collect();

    ids.sort_unstable_by(|a, b| b.cmp(a));
    ids.truncate(limit);

    ids.into_iter()
        .filter_map(|id| {
            let path = dir.join(format_filename(id));
            let content = match fs::read_to_string(&path) {
                Ok(c) => c,
                Err(e) => {
                    warn!("Failed to read history file {}: {e}", path.display());
                    return None;
                }
            };
            match serde_json::from_str::<HistoryRecord>(&content) {
                Ok(record) => Some(record),
                Err(e) => {
                    warn!("Failed to parse history file {}: {e}", path.display());
                    None
                }
            }
        })
        .collect()
}
```

- [ ] **Step 3: Add tests for `load_recent()`**

Add inside the existing `#[cfg(test)] mod tests` block (before the closing `}`):

```rust
    #[test]
    fn load_recent_from_returns_newest_first() {
        let dir = std::env::temp_dir().join("arai_test_load_recent");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        for (id, txt) in [(1, "first"), (3, "third"), (2, "second")] {
            let record = HistoryRecord {
                text: txt.into(),
                timestamp: "2026-01-01T00:00:00Z".into(),
                prompt: "test".into(),
            };
            let json = serde_json::to_string_pretty(&record).unwrap();
            fs::write(dir.join(format_filename(id)), &json).unwrap();
        }

        let results = load_recent_from(&dir, 10);
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].text, "third");
        assert_eq!(results[1].text, "second");
        assert_eq!(results[2].text, "first");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_recent_from_skips_invalid_json() {
        let dir = std::env::temp_dir().join("arai_test_load_recent_invalid");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let record = HistoryRecord {
            text: "valid".into(),
            timestamp: "2026-01-01T00:00:00Z".into(),
            prompt: "test".into(),
        };
        fs::write(
            dir.join("0001.json"),
            serde_json::to_string_pretty(&record).unwrap(),
        )
        .unwrap();
        fs::write(dir.join("0002.json"), "not valid json").unwrap();

        let results = load_recent_from(&dir, 10);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].text, "valid");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_recent_from_respects_limit() {
        let dir = std::env::temp_dir().join("arai_test_load_recent_limit");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        for id in 1..=5 {
            let record = HistoryRecord {
                text: format!("entry {id}"),
                timestamp: "2026-01-01T00:00:00Z".into(),
                prompt: "test".into(),
            };
            let json = serde_json::to_string_pretty(&record).unwrap();
            fs::write(dir.join(format_filename(id)), &json).unwrap();
        }

        let results = load_recent_from(&dir, 2);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].text, "entry 5");
        assert_eq!(results[1].text, "entry 4");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_recent_from_empty_dir() {
        let dir = std::env::temp_dir().join("arai_test_load_recent_empty");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let results = load_recent_from(&dir, 10);
        assert!(results.is_empty());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_recent_from_missing_dir() {
        let dir = std::env::temp_dir().join("arai_test_load_recent_missing_dir_xyz");
        let _ = fs::remove_dir_all(&dir);

        let results = load_recent_from(&dir, 10);
        assert!(results.is_empty());
    }
```

- [ ] **Step 4: Verify it compiles and tests pass**

Run: `cargo build && cargo test history::tests`
Expected: Compiles, all tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/history.rs
git commit -m "feat: add load_recent() and make HistoryRecord public"
```

---

### Task 2: Add UI state fields and message variants

**Files:**
- Modify: `src/ui.rs:4` (imports), `src/ui.rs:539-540` (state init), `src/ui.rs:560-621` (struct fields), `src/ui.rs:624-674` (Message enum)

- [ ] **Step 1: Add HistoryRecord import**

In `src/ui.rs` line 4, change:

```rust
use crate::messages::{ApiKeyStatus, AppEvent, AppEventKind, AppEventSource, ErrorInfo, UiUpdate};
```

To:

```rust
use crate::history::HistoryRecord;
use crate::messages::{ApiKeyStatus, AppEvent, AppEventKind, AppEventSource, ErrorInfo, UiUpdate};
```

- [ ] **Step 2: Add state fields to UiRuntime**

In the `UiRuntime` struct, add after `showing_error_detail: bool,` (line 621):

```rust
    /// Whether the history viewer is currently open.
    history_open: bool,
    /// Loaded history entries for the viewer (newest first).
    history_entries: Vec<HistoryRecord>,
```

- [ ] **Step 3: Initialize state fields in boot()**

In the `boot()` closure, add after `showing_error_detail: false,` (line 540):

```rust
                    history_open: false,
                    history_entries: Vec::new(),
```

- [ ] **Step 4: Add Message variants**

In the `Message` enum, add before `ShowErrorDetail` (line 668):

```rust
    OpenHistory,
    CloseHistory,
    CopyHistoryEntry(usize),
```

- [ ] **Step 5: Verify it compiles**

Run: `cargo build`
Expected: Compiles (warnings about unused variants are fine).

- [ ] **Step 6: Commit**

```bash
git add src/ui.rs
git commit -m "feat: add history viewer state fields and message variants"
```

---

### Task 3: Add message handlers

**Files:**
- Modify: `src/ui.rs` — the `update()` function's main match block

- [ ] **Step 1: Add handlers for the three history messages**

In the `update()` function, find the `Message::ShowErrorDetail` handler. Add **before** it:

```rust
        Message::OpenHistory => {
            state.history_entries = crate::history::load_recent(50);
            state.history_open = true;
            Task::none()
        }
        Message::CloseHistory => {
            state.history_open = false;
            state.history_entries.clear();
            Task::none()
        }
        Message::CopyHistoryEntry(index) => {
            if index >= state.history_entries.len() {
                return Task::none();
            }
            let text = state.history_entries[index].text.clone();
            state.history_open = false;
            state.history_entries.clear();
            iced::clipboard::write::<Message>(text)
        }
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo build`
Expected: Compiles.

- [ ] **Step 3: Commit**

```bash
git add src/ui.rs
git commit -m "feat: add history message handlers"
```

---

### Task 4: Add view_history() and wire into view switch

**Files:**
- Modify: `src/ui.rs` — add `view_history()` function, modify `view()` and `view_main()`

- [ ] **Step 1: Add `view_history()` function**

Add after `view_error_detail()` (after line 1998), before `view_config()`:

```rust
/// Renders the history viewer showing recent copy+hide entries.
fn view_history(state: &UiRuntime) -> Element<'_, Message> {
    let close_btn = button(icon('\u{E5CD}', 20.0))
        .style(icon_btn)
        .padding(6)
        .on_press(Message::CloseHistory);

    let top_bar =
        container(row![container(close_btn).align_right(Fill)].align_y(iced::Alignment::Center))
            .padding([10, 14])
            .width(Fill);

    let body: Element<'_, Message> = if state.history_entries.is_empty() {
        container(text("No history yet.").size(14).color(MUTED))
            .center_x(Fill)
            .center_y(Fill)
            .height(Fill)
            .width(Fill)
            .into()
    } else {
        let mut entries_col = column![].spacing(12);
        for (index, entry) in state.history_entries.iter().enumerate() {
            let display_text = if entry.text.chars().count() > 200 {
                let truncated: String = entry.text.chars().take(200).collect();
                format!("{truncated}...")
            } else {
                entry.text.clone()
            };
            let copy_btn = button(icon('\u{E14D}', 16.0))
                .style(icon_btn)
                .padding(6)
                .on_press(Message::CopyHistoryEntry(index));
            let entry_row = row![
                column![
                    text(display_text).size(13).color(TEXT_COLOR),
                    text(&entry.timestamp).size(11).color(MUTED),
                ]
                .spacing(4)
                .width(Fill),
                copy_btn,
            ]
            .spacing(8)
            .align_y(iced::Alignment::Center);
            entries_col = entries_col.push(entry_row);
        }
        scrollable(entries_col.padding([0, 14]))
            .height(Fill)
            .into()
    };

    let content = column![top_bar, body].height(Fill);

    container(content)
        .style(bg_container)
        .height(Fill)
        .width(Fill)
        .into()
}
```

- [ ] **Step 2: Update view switch logic**

In the `view()` function (line 1732), change the `AppPhase::Main` branch from:

```rust
        AppPhase::Main => {
            if state.config_open {
```

Find the `} else {` at line 1751 that leads to `view_main(...)`. Change:

```rust
            } else {
                let listening = state.mode == AppMode::Listening;
```

To:

```rust
            } else if state.history_open {
                view_history(state)
            } else {
                let listening = state.mode == AppMode::Listening;
```

- [ ] **Step 3: Add history button to bottom bar in `view_main()`**

In `view_main()`, add after the copy button (after line 1874):

```rust
    // history: E889
    let history_btn = button(icon('\u{E889}', 22.0))
        .style(icon_btn)
        .padding([8, 12])
        .on_press_maybe((!busy).then_some(Message::OpenHistory));
```

Then change line 1882 from:

```rust
    let button_group = row![mic_btn, send_btn, copy_btn, settings_btn]
```

To:

```rust
    let button_group = row![mic_btn, send_btn, copy_btn, history_btn, settings_btn]
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo build`
Expected: Compiles successfully.

- [ ] **Step 5: Run all tests and clippy**

Run: `cargo test && cargo clippy --all-targets --all-features -- -D warnings`
Expected: All pass, no warnings.

- [ ] **Step 6: Commit**

```bash
git add src/ui.rs
git commit -m "feat: add history viewer with view_history() and bottom bar button"
```

---

### Task 5: Manual testing

**Files:** None (testing only)

- [ ] **Step 1: Build and run**

Run: `cargo run`
Expected: App launches. New history icon (clock) visible in bottom bar between copy and settings.

- [ ] **Step 2: Test empty history**

Click the history icon with no history files present. Expected: "No history yet." centered in the view.

- [ ] **Step 3: Test with history entries**

1. Type text, Cmd+Enter to copy+hide (creates history files)
2. Reopen app, click history icon
3. Verify entries appear newest first with text and timestamp
4. Verify long text is truncated with "..."

- [ ] **Step 4: Test copy from history**

1. Open history viewer
2. Click the copy button on an entry
3. Verify: view switches back to main, text is on clipboard

- [ ] **Step 5: Test history button disabled when busy**

1. Start listening (mic button)
2. Verify history button is not clickable

- [ ] **Step 6: Run final checks**

Run: `cargo fmt --check && cargo clippy --all-targets --all-features -- -D warnings && cargo test`
Expected: All pass.
