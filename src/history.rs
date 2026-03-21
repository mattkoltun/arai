use log::{error, info, warn};
use serde::{Deserialize, Serialize};
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
#[derive(Clone, Serialize, Deserialize)]
pub struct HistoryRecord {
    pub text: String,
    pub timestamp: String,
    pub prompt: String,
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
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

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

    let (year, month, day) = days_to_ymd(days);

    format!("{year:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}Z")
}

/// Converts days since Unix epoch to (year, month, day).
fn days_to_ymd(mut days: u64) -> (u64, u64, u64) {
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
    (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
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
        let (year, month, day) = days_to_ymd(20533);
        assert!((2025..=2027).contains(&year));
        assert!((1..=12).contains(&month));
        assert!((1..=31).contains(&day));
    }

    #[test]
    fn history_save_writes_file() {
        let dir = std::env::temp_dir().join("arai_test_history_save");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

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
}
