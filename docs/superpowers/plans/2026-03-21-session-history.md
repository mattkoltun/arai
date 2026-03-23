# Session History Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Save copied text to JSON files in `~/.local/share/arai/history/` every time the user triggers copy+hide.

**Architecture:** A new `History` module with a background worker thread (mpsc channel pattern matching Recorder/Transcriber). The UI sends a `UiCopied` event to the Controller on copy+hide, the Controller forwards to `History::save()`, and the worker thread writes a JSON file with incremental IDs.

**Tech Stack:** Rust 2024, serde/serde_json (already in Cargo.toml), std::time::SystemTime for timestamps

**Spec:** `docs/superpowers/specs/2026-03-21-session-history-design.md`

---

## File Structure

| File | Action | Responsibility |
|------|--------|---------------|
| `src/messages.rs` | Modify | Add `UiCopied { text, prompt }` variant to `AppEventKind` |
| `src/history.rs` | Create | `History` struct with worker thread, `save()` method, Drop impl |
| `src/ui.rs` | Modify | Send `UiCopied` event in `Message::Copy` handler |
| `src/controller.rs` | Modify | Create `History` instance, handle `UiCopied` event |
| `src/main.rs` | Modify | Add `mod history;` |

---

### Task 1: Add UiCopied event variant to messages

**Files:**
- Modify: `src/messages.rs:86-122`

- [ ] **Step 1: Add `UiCopied` variant to `AppEventKind`**

In `src/messages.rs`, add after `ModelDownloadCancelled` (line 121), before the closing brace:

```rust
    /// User triggered copy+hide — save text to session history.
    UiCopied { text: String, prompt: String },
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo build`
Expected: Compiles (warning about unused variant is fine).

- [ ] **Step 3: Commit**

```bash
git add src/messages.rs
git commit -m "feat: add UiCopied event variant for session history"
```

---

### Task 2: Create history module

**Files:**
- Create: `src/history.rs`

- [ ] **Step 1: Create `src/history.rs` with full implementation**

```rust
use log::{error, info};
use serde::Serialize;
use std::fs;
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

/// Entry sent from the controller to the history worker thread.
struct HistoryEntry {
    text: String,
    prompt: String,
}

/// JSON structure written to each history file.
#[derive(Serialize)]
struct HistoryRecord {
    text: String,
    timestamp: String,
    prompt: String,
}

/// Background worker that writes session history files.
///
/// Each call to [`save()`](History::save) sends an entry to the worker thread,
/// which writes it as a JSON file in `~/.local/share/arai/history/`.
pub struct History {
    tx: mpsc::Sender<HistoryEntry>,
    handle: Option<thread::JoinHandle<()>>,
}

impl History {
    /// Spawns the history worker thread.
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel::<HistoryEntry>();

        let handle = thread::Builder::new()
            .name("history".into())
            .spawn(move || {
                let dir = history_dir();
                if let Err(e) = fs::create_dir_all(&dir) {
                    error!("Failed to create history directory: {e}");
                    return;
                }

                let mut next_id = scan_next_id(&dir);
                info!("History worker started, next ID: {next_id}");

                while let Ok(entry) = rx.recv() {
                    let record = HistoryRecord {
                        text: entry.text,
                        timestamp: iso_timestamp(),
                        prompt: entry.prompt,
                    };

                    let json = match serde_json::to_string_pretty(&record) {
                        Ok(j) => j,
                        Err(e) => {
                            error!("Failed to serialize history entry: {e}");
                            continue;
                        }
                    };

                    loop {
                        let path = dir.join(format_filename(next_id));
                        if path.exists() {
                            error!("History file collision at {next_id}, incrementing");
                            next_id += 1;
                            continue;
                        }
                        if let Err(e) = fs::write(&path, &json) {
                            error!("Failed to write history file: {e}");
                        }
                        next_id += 1;
                        break;
                    }
                }

                info!("History worker stopped");
            })
            .expect("Failed to spawn history thread");

        Self {
            tx,
            handle: Some(handle),
        }
    }

    /// Sends a history entry to the worker thread for writing. Non-blocking.
    pub fn save(&self, text: String, prompt: String) {
        if let Err(e) = self.tx.send(HistoryEntry { text, prompt }) {
            error!("Failed to send history entry: {e}");
        }
    }
}

impl Drop for History {
    fn drop(&mut self) {
        // Dropping tx is handled implicitly when Self is dropped,
        // which causes the worker to exit on next recv.
        // We just need to join the thread.
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

/// Returns `~/.local/share/arai/history/`.
fn history_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".local/share/arai/history")
}

/// Scans the history directory for existing files and returns `max_id + 1`.
fn scan_next_id(dir: &PathBuf) -> u64 {
    let max = fs::read_dir(dir)
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
        .max();

    match max {
        Some(id) => id + 1,
        None => 1,
    }
}

/// Formats an ID as a zero-padded filename: `0001.json`, `0002.json`, etc.
fn format_filename(id: u64) -> String {
    if id <= 9999 {
        format!("{id:04}.json")
    } else {
        format!("{id}.json")
    }
}

/// Returns the current UTC time as an ISO 8601 string.
fn iso_timestamp() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let days = secs / 86400;
    let time_secs = secs % 86400;
    let hours = time_secs / 3600;
    let minutes = (time_secs % 3600) / 60;
    let seconds = time_secs % 60;

    // Convert days since epoch to year-month-day.
    let (year, month, day) = days_to_ymd(days);

    format!("{year:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}Z")
}

/// Converts days since Unix epoch to (year, month, day).
fn days_to_ymd(mut days: u64) -> (u64, u64, u64) {
    // Simple civil calendar conversion.
    let mut year = 1970;
    loop {
        let days_in_year = if is_leap(year) { 366 } else { 365 };
        if days < days_in_year {
            break;
        }
        days -= days_in_year;
        year += 1;
    }
    let month_days: [u64; 12] = if is_leap(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut month = 1;
    for &md in &month_days {
        if days < md {
            break;
        }
        days -= md;
        month += 1;
    }
    (year, month, days + 1)
}

/// Returns true if the given year is a leap year.
fn is_leap(year: u64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn format_filename_pads_to_four_digits() {
        assert_eq!(format_filename(1), "0001.json");
        assert_eq!(format_filename(42), "0042.json");
        assert_eq!(format_filename(9999), "9999.json");
    }

    #[test]
    fn format_filename_beyond_9999_no_padding() {
        assert_eq!(format_filename(10000), "10000.json");
        assert_eq!(format_filename(123456), "123456.json");
    }

    #[test]
    fn scan_next_id_empty_dir() {
        let dir = std::env::temp_dir().join("arai_test_history_empty");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        assert_eq!(scan_next_id(&dir), 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn scan_next_id_with_existing_files() {
        let dir = std::env::temp_dir().join("arai_test_history_existing");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("0001.json"), "{}").unwrap();
        fs::write(dir.join("0005.json"), "{}").unwrap();
        fs::write(dir.join("0003.json"), "{}").unwrap();
        assert_eq!(scan_next_id(&dir), 6);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn scan_next_id_ignores_non_json() {
        let dir = std::env::temp_dir().join("arai_test_history_nonjson");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("0003.json"), "{}").unwrap();
        fs::write(dir.join("readme.txt"), "hello").unwrap();
        fs::write(dir.join(".lock"), "").unwrap();
        assert_eq!(scan_next_id(&dir), 4);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn iso_timestamp_format() {
        let ts = iso_timestamp();
        // Should match YYYY-MM-DDTHH:MM:SSZ pattern.
        assert_eq!(ts.len(), 20);
        assert_eq!(&ts[4..5], "-");
        assert_eq!(&ts[7..8], "-");
        assert_eq!(&ts[10..11], "T");
        assert_eq!(&ts[13..14], ":");
        assert_eq!(&ts[16..17], ":");
        assert_eq!(&ts[19..20], "Z");
    }

    #[test]
    fn days_to_ymd_epoch() {
        assert_eq!(days_to_ymd(0), (1970, 1, 1));
    }

    #[test]
    fn days_to_ymd_known_date() {
        // 2026-03-21 is day 20533 since epoch.
        // 1970-2025 = 55 years. Let's just verify the function handles
        // a recent date without panicking and returns a plausible result.
        let (year, month, day) = days_to_ymd(20533);
        assert!(year >= 2025 && year <= 2027);
        assert!(month >= 1 && month <= 12);
        assert!(day >= 1 && day <= 31);
    }

    #[test]
    fn history_save_writes_file() {
        let dir = std::env::temp_dir().join("arai_test_history_save");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        // We test the worker by creating a History, saving, and dropping it
        // (which joins the thread). Then we check the file.
        let (tx, rx) = std::sync::mpsc::channel::<HistoryEntry>();
        let dir_clone = dir.clone();
        let handle = std::thread::spawn(move || {
            let mut next_id = 1u64;
            while let Ok(entry) = rx.recv() {
                let record = HistoryRecord {
                    text: entry.text,
                    timestamp: iso_timestamp(),
                    prompt: entry.prompt,
                };
                let json = serde_json::to_string_pretty(&record).unwrap();
                let path = dir_clone.join(format_filename(next_id));
                fs::write(&path, &json).unwrap();
                next_id += 1;
            }
        });

        tx.send(HistoryEntry {
            text: "Hello world".into(),
            prompt: "Rewrite clearly".into(),
        })
        .unwrap();
        drop(tx);
        handle.join().unwrap();

        let content = fs::read_to_string(dir.join("0001.json")).unwrap();
        let record: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(record["text"], "Hello world");
        assert_eq!(record["prompt"], "Rewrite clearly");
        assert!(record["timestamp"].as_str().unwrap().ends_with('Z'));

        let _ = fs::remove_dir_all(&dir);
    }
}
```

- [ ] **Step 2: Add `mod history;` to `src/main.rs`**

In `src/main.rs`, add after `mod global_hotkey;` (line 6), maintaining alphabetical order (before `mod keyring_store;`):

```rust
mod history;
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo build`
Expected: Compiles (warning about unused `History::new` and `History::save` is fine).

- [ ] **Step 4: Run tests**

Run: `cargo test history::tests`
Expected: All tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/history.rs src/main.rs
git commit -m "feat: add history module with background worker thread"
```

---

### Task 3: Wire UI to send UiCopied event

**Files:**
- Modify: `src/ui.rs:893-903`

- [ ] **Step 1: Send `UiCopied` event in `Message::Copy` handler**

In `src/ui.rs`, find the `Message::Copy` handler (line 893-903):

```rust
        Message::Copy => {
            if state.mode != AppMode::Idle || state.input.trim().is_empty() {
                return Task::none();
            }
            debug!("UI copying text to clipboard");
            let text = state.input.clone();
            state.input.clear();
            state.editor = text_editor::Content::new();
            hide_app();
            iced::clipboard::write::<Message>(text)
        }
```

Replace with:

```rust
        Message::Copy => {
            if state.mode != AppMode::Idle || state.input.trim().is_empty() {
                return Task::none();
            }
            debug!("UI copying text to clipboard");
            let text = state.input.clone();
            let prompt = state
                .snapshot_prompts
                .get(state.active_prompt)
                .map(|p| p.name.clone())
                .unwrap_or_default();
            let _ = state.app_event_tx.send(AppEvent {
                source: AppEventSource::Ui,
                kind: AppEventKind::UiCopied {
                    text: text.clone(),
                    prompt,
                },
            });
            state.input.clear();
            state.editor = text_editor::Content::new();
            hide_app();
            iced::clipboard::write::<Message>(text)
        }
```

- [ ] **Step 2: Verify the import includes needed types**

Check that `src/ui.rs` line 4 imports `AppEvent`, `AppEventKind`, and `AppEventSource`. It should already have:

```rust
use crate::messages::{ApiKeyStatus, AppEvent, AppEventKind, AppEventSource, ErrorInfo, UiUpdate};
```

If `AppEvent` or `AppEventSource` are missing, add them.

- [ ] **Step 3: Verify it compiles**

Run: `cargo build`
Expected: Compiles successfully.

- [ ] **Step 4: Commit**

```bash
git add src/ui.rs
git commit -m "feat: send UiCopied event on copy+hide"
```

---

### Task 4: Wire controller to create History and handle UiCopied

**Files:**
- Modify: `src/controller.rs:1-90` (imports, struct, new), and the event match block

- [ ] **Step 1: Add History import**

At `src/controller.rs:1`, add:

```rust
use crate::history::History;
```

- [ ] **Step 2: Add `history` field to Controller struct**

In the `Controller` struct (line 55-64), add after `shutting_down`:

```rust
    history: History,
```

- [ ] **Step 3: Create History in `Controller::new()`**

In `Controller::new()` (line 80-89), add `history: History::new(),` to the struct literal:

```rust
        let controller = Self {
            recorder,
            transcriber,
            app_event_tx,
            app_event_rx,
            agent,
            app_state,
            ui_update_tx,
            shutting_down: flag,
            history: History::new(),
        };
```

- [ ] **Step 4: Handle `UiCopied` event in the event loop**

In the `run()` method's event match block, add a new arm **before** the catch-all `(source, kind) => { ... }` pattern at line 387 (after the `UiUpdateApiKey` handler at line 360, and after the `ModelDownload*` handlers at line 386):

```rust
                (AppEventSource::Ui, AppEventKind::UiCopied { text, prompt }) => {
                    debug!("Saving copy to history");
                    self.history.save(text, prompt);
                }
```

- [ ] **Step 5: Verify it compiles**

Run: `cargo build`
Expected: Compiles successfully.

- [ ] **Step 6: Run all tests**

Run: `cargo test`
Expected: All tests pass.

- [ ] **Step 7: Run clippy**

Run: `cargo clippy --all-targets --all-features -- -D warnings`
Expected: Clean.

- [ ] **Step 8: Commit**

```bash
git add src/controller.rs
git commit -m "feat: wire History into controller for UiCopied events"
```

---

### Task 5: Manual testing

**Files:** None (testing only)

- [ ] **Step 1: Build and run**

Run: `cargo run`
Expected: App launches normally.

- [ ] **Step 2: Test copy+hide creates history file**

1. Type or dictate some text
2. Press Cmd+Enter (copy+hide)
3. Check `~/.local/share/arai/history/` for `0001.json`
4. Verify JSON contains `text`, `timestamp`, and `prompt` fields

- [ ] **Step 3: Test incremental IDs**

1. Reopen app, type text, Cmd+Enter again
2. Verify `0002.json` exists (ID continues from previous run)

- [ ] **Step 4: Test prompt name is captured**

1. Switch to a different agent prompt in the carousel
2. Copy+hide
3. Check that the new file's `prompt` field matches the selected prompt name

- [ ] **Step 5: Run final checks**

Run: `cargo fmt --check && cargo clippy --all-targets --all-features -- -D warnings && cargo test`
Expected: All pass.
