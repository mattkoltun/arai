use crate::logger;
use log::LevelFilter;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const DEFAULT_AGENT_PROMPT: &str =
    "Rewrite the user text for clarity and brevity while preserving meaning.";
const DEFAULT_MODEL_PATH: &str = "models/ggml-small.en.bin";
const DEFAULT_WINDOW_SECONDS: f32 = 3.0;
const DEFAULT_OVERLAP_SECONDS: f32 = 0.25;
const DEFAULT_SILENCE_THRESHOLD: f32 = 0.005;
const DEFAULT_GLOBAL_HOTKEY: &str = "CmdOrCtrl+Shift+A";

#[derive(Debug)]
pub enum ConfigError {
    MissingHome,
    InvalidLogLevel(String),
    MissingApiKey,
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
            ConfigError::MissingApiKey => write!(f, "open_api_key is missing"),
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

#[derive(Clone, Debug)]
pub struct Config {
    pub log_level: LevelFilter,
    pub log_path: PathBuf,
    pub open_api_key: String,
    pub agent_prompts: Vec<AgentPrompt>,
    pub default_prompt: usize,
    pub transcriber: TranscriberConfig,
    pub global_hotkey: String,
}

impl Config {
    pub fn load() -> Result<Self, ConfigError> {
        let default_layer = PartialConfig::default_layer();
        let file_layer = PartialConfig::from_file(config_path()?)?;
        let env_layer = PartialConfig::from_env()?;

        let merged = default_layer.merge(file_layer).merge(env_layer);
        from_partial(merged)
    }

    pub fn save(&self) -> Result<(), ConfigError> {
        let path = config_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let file_config = FileConfig {
            log_level: Some(self.log_level.to_string().to_lowercase()),
            log_path: Some(self.log_path.display().to_string()),
            open_api_key: Some(self.open_api_key.clone()),
            agent_prompts: Some(self.agent_prompts.clone()),
            default_prompt: Some(self.default_prompt),
            transcriber: Some(self.transcriber.clone()),
            global_hotkey: Some(self.global_hotkey.clone()),
        };
        let yaml = serde_yaml::to_string(&file_config)?;
        std::fs::write(&path, yaml)?;
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
    pub model_path: String,
    pub window_seconds: f32,
    pub overlap_seconds: f32,
    #[serde(default = "default_silence_threshold")]
    pub silence_threshold: f32,
}

fn default_silence_threshold() -> f32 {
    DEFAULT_SILENCE_THRESHOLD
}

impl Default for TranscriberConfig {
    fn default() -> Self {
        Self {
            model_path: DEFAULT_MODEL_PATH.to_string(),
            window_seconds: DEFAULT_WINDOW_SECONDS,
            overlap_seconds: DEFAULT_OVERLAP_SECONDS,
            silence_threshold: DEFAULT_SILENCE_THRESHOLD,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Default)]
struct PartialConfig {
    log_level: Option<String>,
    log_path: Option<String>,
    open_api_key: Option<String>,
    agent_prompts: Option<Vec<AgentPrompt>>,
    default_prompt: Option<usize>,
    transcriber: Option<TranscriberConfig>,
    global_hotkey: Option<String>,
}

#[derive(Serialize)]
struct FileConfig {
    log_level: Option<String>,
    log_path: Option<String>,
    open_api_key: Option<String>,
    agent_prompts: Option<Vec<AgentPrompt>>,
    default_prompt: Option<usize>,
    transcriber: Option<TranscriberConfig>,
    global_hotkey: Option<String>,
}

impl PartialConfig {
    fn default_layer() -> Self {
        Self {
            log_level: Some("debug".to_string()),
            log_path: Some("/var/log/arai.log".to_string()),
            open_api_key: None,
            agent_prompts: Some(vec![AgentPrompt {
                name: "default".to_string(),
                instruction: DEFAULT_AGENT_PROMPT.to_string(),
            }]),
            default_prompt: Some(0),
            transcriber: Some(TranscriberConfig::default()),
            global_hotkey: Some(DEFAULT_GLOBAL_HOTKEY.to_string()),
        }
    }

    fn from_file(path: PathBuf) -> Result<Self, ConfigError> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let contents = std::fs::read_to_string(&path)?;
        let layer = serde_yaml::from_str(&contents)?;
        Ok(layer)
    }

    fn from_env() -> Result<Self, ConfigError> {
        let log_level = std::env::var("ARAI_LOG_LEVEL").ok();
        let log_path = std::env::var("ARAI_LOG_PATH").ok();
        let open_api_key = std::env::var("OPENAI_API_KEY").ok();
        Ok(Self {
            log_level,
            log_path,
            open_api_key,
            agent_prompts: None,
            default_prompt: None,
            transcriber: None,
            global_hotkey: None,
        })
    }

    fn merge(self, other: PartialConfig) -> PartialConfig {
        PartialConfig {
            log_level: other.log_level.or(self.log_level),
            log_path: other.log_path.or(self.log_path),
            open_api_key: other.open_api_key.or(self.open_api_key),
            agent_prompts: other.agent_prompts.or(self.agent_prompts),
            default_prompt: other.default_prompt.or(self.default_prompt),
            transcriber: other.transcriber.or(self.transcriber),
            global_hotkey: other.global_hotkey.or(self.global_hotkey),
        }
    }
}

fn config_path() -> Result<PathBuf, ConfigError> {
    let home = std::env::var("HOME").map_err(|_| ConfigError::MissingHome)?;
    Ok(Path::new(&home).join(".config/arai/config.yaml"))
}

fn from_partial(partial: PartialConfig) -> Result<Config, ConfigError> {
    let log_level = match partial.log_level {
        Some(value) => logger::parse_level(&value).ok_or(ConfigError::InvalidLogLevel(value))?,
        None => logger::LogConfig::default().level,
    };
    let log_path = partial
        .log_path
        .map(PathBuf::from)
        .unwrap_or_else(|| logger::LogConfig::default().path);

    let open_api_key = partial.open_api_key.unwrap_or_default();
    if open_api_key.trim().is_empty() {
        return Err(ConfigError::MissingApiKey);
    }

    let agent_prompts = partial.agent_prompts.unwrap_or_default();
    if agent_prompts.is_empty() {
        return Err(ConfigError::EmptyAgentPrompts);
    }
    for prompt in &agent_prompts {
        if prompt.name.trim().is_empty() {
            return Err(ConfigError::EmptyAgentPromptName);
        }
        if prompt.instruction.trim().is_empty() {
            return Err(ConfigError::EmptyAgentPromptInstruction);
        }
    }

    let default_prompt = partial.default_prompt.unwrap_or(0);
    let default_prompt = if default_prompt < agent_prompts.len() {
        default_prompt
    } else {
        0
    };

    let transcriber = partial.transcriber.unwrap_or_default();
    let global_hotkey = partial
        .global_hotkey
        .unwrap_or_else(|| DEFAULT_GLOBAL_HOTKEY.to_string());

    Ok(Config {
        log_level,
        log_path,
        open_api_key,
        agent_prompts,
        default_prompt,
        transcriber,
        global_hotkey,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_partial() -> PartialConfig {
        PartialConfig {
            log_level: Some("info".to_string()),
            log_path: Some("/tmp/arai-test.log".to_string()),
            open_api_key: Some("test-key".to_string()),
            agent_prompts: Some(vec![AgentPrompt {
                name: "default".to_string(),
                instruction: "rewrite".to_string(),
            }]),
            default_prompt: Some(0),
            transcriber: Some(TranscriberConfig::default()),
            global_hotkey: Some("CmdOrCtrl+Shift+A".to_string()),
        }
    }

    #[test]
    fn builds_config_from_valid_partial() {
        let cfg = from_partial(valid_partial()).expect("valid config should parse");
        assert_eq!(cfg.log_level, LevelFilter::Info);
        assert_eq!(cfg.log_path, PathBuf::from("/tmp/arai-test.log"));
        assert_eq!(cfg.open_api_key, "test-key");
        assert_eq!(cfg.agent_prompts[0].instruction, "rewrite");
    }

    #[test]
    fn rejects_missing_api_key() {
        let mut partial = valid_partial();
        partial.open_api_key = Some("   ".to_string());
        assert!(matches!(
            from_partial(partial),
            Err(ConfigError::MissingApiKey)
        ));
    }

    #[test]
    fn rejects_invalid_prompt_name_or_instruction() {
        let mut bad_name = valid_partial();
        bad_name.agent_prompts = Some(vec![AgentPrompt {
            name: " ".to_string(),
            instruction: "ok".to_string(),
        }]);
        assert!(matches!(
            from_partial(bad_name),
            Err(ConfigError::EmptyAgentPromptName)
        ));

        let mut bad_instruction = valid_partial();
        bad_instruction.agent_prompts = Some(vec![AgentPrompt {
            name: "default".to_string(),
            instruction: " ".to_string(),
        }]);
        assert!(matches!(
            from_partial(bad_instruction),
            Err(ConfigError::EmptyAgentPromptInstruction)
        ));
    }
}
