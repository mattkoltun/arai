use log::LevelFilter;
use simplelog::{Config as SimpleConfig, WriteLogger};
use std::fs::OpenOptions;
use std::path::PathBuf;

#[derive(Debug)]
pub enum LoggerInitError {
    Io(std::io::Error),
    SetLogger(log::SetLoggerError),
}

impl std::fmt::Display for LoggerInitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoggerInitError::Io(err) => write!(f, "log file error: {err}"),
            LoggerInitError::SetLogger(err) => write!(f, "logger setup error: {err}"),
        }
    }
}

impl std::error::Error for LoggerInitError {}

impl From<std::io::Error> for LoggerInitError {
    fn from(err: std::io::Error) -> Self {
        LoggerInitError::Io(err)
    }
}

impl From<log::SetLoggerError> for LoggerInitError {
    fn from(err: log::SetLoggerError) -> Self {
        LoggerInitError::SetLogger(err)
    }
}

#[derive(Clone, Debug)]
pub struct LogConfig {
    pub level: LevelFilter,
    pub path: PathBuf,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            level: LevelFilter::Debug,
            path: default_log_path(),
        }
    }
}

/// Returns a user-writable log path appropriate for the platform.
/// - macOS: `~/Library/Logs/arai.log`
/// - Linux/other: `~/.local/share/arai/arai.log`
fn default_log_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    if cfg!(target_os = "macos") {
        PathBuf::from(home).join("Library/Logs/arai.log")
    } else {
        PathBuf::from(home).join(".local/share/arai/arai.log")
    }
}

pub fn init_with_config(config: LogConfig) -> Result<(), LoggerInitError> {
    if let Some(parent) = config.path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&config.path)?;
    WriteLogger::init(config.level, SimpleConfig::default(), file)?;
    Ok(())
}

pub fn parse_level(value: &str) -> Option<LevelFilter> {
    match value.trim().to_ascii_lowercase().as_str() {
        "trace" | "verbose" => Some(LevelFilter::Trace),
        "debug" => Some(LevelFilter::Debug),
        "info" => Some(LevelFilter::Info),
        "warn" | "warning" => Some(LevelFilter::Warn),
        "error" => Some(LevelFilter::Error),
        "off" => Some(LevelFilter::Off),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_common_aliases() {
        assert_eq!(parse_level("verbose"), Some(LevelFilter::Trace));
        assert_eq!(parse_level(" warning "), Some(LevelFilter::Warn));
        assert_eq!(parse_level("OFF"), Some(LevelFilter::Off));
    }

    #[test]
    fn rejects_unknown_level() {
        assert_eq!(parse_level("loud"), None);
    }
}
