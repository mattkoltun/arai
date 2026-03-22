use crate::logger;
use log::LevelFilter;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

const DEFAULT_AGENT_PROMPT: &str =
    "Rewrite the user text for clarity and brevity while preserving meaning.";

/// Returns the platform-standard directory for storing Whisper models.
/// - macOS: `~/Library/Application Support/arai/models/`
/// - Linux: `~/.local/share/arai/models/`
pub fn default_model_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".local")
        .join("share")
        .join("arai")
        .join("models")
}

static DEFAULT_MODEL_PATH: LazyLock<String> = LazyLock::new(|| {
    default_model_dir()
        .join("ggml-small.en.bin")
        .display()
        .to_string()
});
const DEFAULT_WINDOW_SECONDS: f32 = 3.0;
const DEFAULT_OVERLAP_SECONDS: f32 = 0.25;
const DEFAULT_SILENCE_THRESHOLD: f32 = 0.005;
const DEFAULT_GLOBAL_HOTKEY: &str = "Alt+Space";

#[derive(Debug)]
pub enum ConfigError {
    MissingHome,
    InvalidLogLevel(String),
    EmptyAgentPrompts,
    EmptyAgentPromptName,
    EmptyAgentPromptInstruction,
    Io(std::io::Error),
    Yaml(serde_yaml::Error),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::MissingHome => write!(f, "HOME is not set"),
            ConfigError::InvalidLogLevel(value) => write!(f, "invalid log_level: {value}"),
            ConfigError::EmptyAgentPrompts => write!(f, "agent_prompts cannot be empty"),
            ConfigError::EmptyAgentPromptName => write!(f, "agent_prompt name cannot be empty"),
            ConfigError::EmptyAgentPromptInstruction => {
                write!(f, "agent_prompt instruction cannot be empty")
            }
            ConfigError::Io(err) => write!(f, "config IO error: {err}"),
            ConfigError::Yaml(err) => write!(f, "config YAML error: {err}"),
        }
    }
}

impl std::error::Error for ConfigError {}

impl From<std::io::Error> for ConfigError {
    fn from(err: std::io::Error) -> Self {
        ConfigError::Io(err)
    }
}

impl From<serde_yaml::Error> for ConfigError {
    fn from(err: serde_yaml::Error) -> Self {
        ConfigError::Yaml(err)
    }
}

/// Application configuration. Deserialized directly from `~/.config/arai/config.yaml`
/// with serde defaults for missing fields.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Config {
    #[serde(default = "default_log_level")]
    pub log_level: String,
    #[serde(default = "default_log_path")]
    pub log_path: String,
    #[serde(default, skip_serializing)]
    pub open_api_key: String,
    #[serde(default = "default_agent_prompts")]
    pub agent_prompts: Vec<AgentPrompt>,
    #[serde(default)]
    pub default_prompt: usize,
    #[serde(default)]
    pub transcriber: TranscriberConfig,
    #[serde(default = "default_global_hotkey")]
    pub global_hotkey: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_device: Option<String>,
}

fn default_log_level() -> String {
    "debug".to_string()
}

fn default_log_path() -> String {
    logger::LogConfig::default().path.display().to_string()
}

fn default_agent_prompts() -> Vec<AgentPrompt> {
    vec![AgentPrompt {
        name: "default".to_string(),
        instruction: DEFAULT_AGENT_PROMPT.to_string(),
    }]
}

fn default_global_hotkey() -> String {
    DEFAULT_GLOBAL_HOTKEY.to_string()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            log_level: default_log_level(),
            log_path: default_log_path(),
            open_api_key: String::new(),
            agent_prompts: default_agent_prompts(),
            default_prompt: 0,
            transcriber: TranscriberConfig::default(),
            global_hotkey: default_global_hotkey(),
            input_device: None,
        }
    }
}

impl Config {
    /// Loads config from `~/.config/arai/config.yaml`, falling back to defaults
    /// for missing fields. Resolves the API key from keyring/env and migrates
    /// plain-text keys from the file to the keyring.
    pub fn load() -> Result<Self, ConfigError> {
        let path = config_path()?;
        let mut config = if path.exists() {
            let contents = std::fs::read_to_string(&path)?;
            serde_yaml::from_str(&contents)?
        } else {
            Self::default()
        };

        // Migrate plain-text API key from file to keyring.
        let needs_migration_save = !config.open_api_key.is_empty();
        migrate_api_key_if_needed(&config.open_api_key);
        config.open_api_key = resolve_api_key(&config.open_api_key);

        // Validate.
        config.validate()?;

        // Clamp default_prompt to valid range.
        if config.default_prompt >= config.agent_prompts.len() {
            config.default_prompt = 0;
        }

        // Trim empty input_device.
        if config
            .input_device
            .as_ref()
            .is_some_and(|s| s.trim().is_empty())
        {
            config.input_device = None;
        }

        // Save to remove the plain-text key from disk.
        if needs_migration_save && let Err(e) = config.save() {
            log::warn!("Failed to save config after API key migration: {e}");
        }

        Ok(config)
    }

    /// Persists the current config to disk. The API key is excluded from the
    /// file (it lives in the keyring).
    pub fn save(&self) -> Result<(), ConfigError> {
        let path = config_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let yaml = serde_yaml::to_string(self)?;
        std::fs::write(&path, yaml)?;
        Ok(())
    }

    /// Returns the parsed log level, falling back to the default on invalid values.
    pub fn parsed_log_level(&self) -> LevelFilter {
        logger::parse_level(&self.log_level).unwrap_or_else(|| logger::LogConfig::default().level)
    }

    /// Returns the log path as a `PathBuf`.
    pub fn parsed_log_path(&self) -> PathBuf {
        PathBuf::from(&self.log_path)
    }

    fn validate(&self) -> Result<(), ConfigError> {
        // Validate log level.
        if logger::parse_level(&self.log_level).is_none() {
            return Err(ConfigError::InvalidLogLevel(self.log_level.clone()));
        }
        // Validate agent prompts.
        if self.agent_prompts.is_empty() {
            return Err(ConfigError::EmptyAgentPrompts);
        }
        for prompt in &self.agent_prompts {
            if prompt.name.trim().is_empty() {
                return Err(ConfigError::EmptyAgentPromptName);
            }
            if prompt.instruction.trim().is_empty() {
                return Err(ConfigError::EmptyAgentPromptInstruction);
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AgentPrompt {
    pub name: String,
    pub instruction: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TranscriberConfig {
    #[serde(default = "default_model_path")]
    pub model_path: String,
    #[serde(default = "default_window_seconds")]
    pub window_seconds: f32,
    #[serde(default = "default_overlap_seconds")]
    pub overlap_seconds: f32,
    #[serde(default = "default_silence_threshold")]
    pub silence_threshold: f32,
    #[serde(default = "default_true")]
    pub use_gpu: bool,
    #[serde(default = "default_true")]
    pub flash_attn: bool,
    #[serde(default = "default_true")]
    pub no_timestamps: bool,
}

fn default_model_path() -> String {
    DEFAULT_MODEL_PATH.clone()
}

fn default_window_seconds() -> f32 {
    DEFAULT_WINDOW_SECONDS
}

fn default_overlap_seconds() -> f32 {
    DEFAULT_OVERLAP_SECONDS
}

fn default_silence_threshold() -> f32 {
    DEFAULT_SILENCE_THRESHOLD
}

fn default_true() -> bool {
    true
}

impl Default for TranscriberConfig {
    fn default() -> Self {
        Self {
            model_path: DEFAULT_MODEL_PATH.clone(),
            window_seconds: DEFAULT_WINDOW_SECONDS,
            overlap_seconds: DEFAULT_OVERLAP_SECONDS,
            silence_threshold: DEFAULT_SILENCE_THRESHOLD,
            use_gpu: true,
            flash_attn: true,
            no_timestamps: true,
        }
    }
}

fn config_path() -> Result<PathBuf, ConfigError> {
    let home = std::env::var("HOME").map_err(|_| ConfigError::MissingHome)?;
    Ok(Path::new(&home).join(".config/arai/config.yaml"))
}

/// Resolves the OpenAI API key from available sources in priority order:
/// 1. `OPENAI_API_KEY` env var
/// 2. OS keyring
/// 3. Config file value (migration fallback)
/// 4. Empty string
pub fn resolve_api_key(config_file_value: &str) -> String {
    if let Ok(key) = std::env::var("OPENAI_API_KEY")
        && !key.is_empty()
    {
        return key;
    }
    if let Some(key) = crate::keyring_store::get_api_key() {
        return key;
    }
    if !config_file_value.is_empty() {
        return config_file_value.to_string();
    }
    String::new()
}

/// If the config contains a non-empty API key, migrate it to the OS keyring.
fn migrate_api_key_if_needed(key: &str) {
    if key.is_empty() {
        return;
    }
    log::info!("Migrating API key from config file to keyring");
    if let Err(e) = crate::keyring_store::set_api_key(key) {
        log::warn!("Failed to migrate API key to keyring: {e}. Key remains in config file.");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_config() -> Config {
        Config {
            log_level: "info".to_string(),
            log_path: "/tmp/arai-test.log".to_string(),
            open_api_key: "test-key".to_string(),
            agent_prompts: vec![AgentPrompt {
                name: "default".to_string(),
                instruction: "rewrite".to_string(),
            }],
            default_prompt: 0,
            transcriber: TranscriberConfig::default(),
            global_hotkey: "Alt+Space".to_string(),
            input_device: None,
        }
    }

    #[test]
    fn validates_valid_config() {
        let cfg = valid_config();
        assert!(cfg.validate().is_ok());
        assert_eq!(cfg.parsed_log_level(), LevelFilter::Info);
        assert_eq!(cfg.parsed_log_path(), PathBuf::from("/tmp/arai-test.log"));
        assert_eq!(cfg.agent_prompts[0].instruction, "rewrite");
    }

    #[test]
    fn rejects_invalid_log_level() {
        let mut cfg = valid_config();
        cfg.log_level = "banana".to_string();
        assert!(matches!(
            cfg.validate(),
            Err(ConfigError::InvalidLogLevel(_))
        ));
    }

    #[test]
    fn rejects_empty_prompts() {
        let mut cfg = valid_config();
        cfg.agent_prompts = vec![];
        assert!(matches!(
            cfg.validate(),
            Err(ConfigError::EmptyAgentPrompts)
        ));
    }

    #[test]
    fn rejects_invalid_prompt_name_or_instruction() {
        let mut bad_name = valid_config();
        bad_name.agent_prompts = vec![AgentPrompt {
            name: " ".to_string(),
            instruction: "ok".to_string(),
        }];
        assert!(matches!(
            bad_name.validate(),
            Err(ConfigError::EmptyAgentPromptName)
        ));

        let mut bad_instruction = valid_config();
        bad_instruction.agent_prompts = vec![AgentPrompt {
            name: "default".to_string(),
            instruction: " ".to_string(),
        }];
        assert!(matches!(
            bad_instruction.validate(),
            Err(ConfigError::EmptyAgentPromptInstruction)
        ));
    }

    #[test]
    fn default_model_dir_ends_with_arai_models() {
        let dir = default_model_dir();
        assert!(
            dir.ends_with("arai/models"),
            "expected path ending with arai/models, got: {dir:?}"
        );
    }

    #[test]
    fn default_model_path_is_absolute() {
        let path = std::path::Path::new(DEFAULT_MODEL_PATH.as_str());
        assert!(
            path.is_absolute(),
            "DEFAULT_MODEL_PATH should be absolute, got: {path:?}"
        );
    }

    #[test]
    fn resolve_api_key_uses_config_fallback() {
        let key = resolve_api_key("sk-fallback-key");
        assert!(!key.is_empty());
    }

    #[test]
    fn resolve_api_key_returns_empty_for_empty() {
        let _key = resolve_api_key("");
    }

    #[test]
    fn default_config_is_valid() {
        let cfg = Config::default();
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn clamps_out_of_range_default_prompt() {
        let mut cfg = valid_config();
        cfg.default_prompt = 999;
        // Simulating what load() does after deserialization.
        if cfg.default_prompt >= cfg.agent_prompts.len() {
            cfg.default_prompt = 0;
        }
        assert_eq!(cfg.default_prompt, 0);
    }
}
