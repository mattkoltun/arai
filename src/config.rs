use crate::logger;
use log::LevelFilter;
use serde::Deserialize;
use std::path::{Path, PathBuf};

const DEFAULT_AGENT_PROMPT: &str =
    "Rewrite the user text for clarity and brevity while preserving meaning.";

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
}

impl Config {
    pub fn load() -> Result<Self, ConfigError> {
        let default_layer = PartialConfig::default_layer();
        let file_layer = PartialConfig::from_file(config_path()?)?;
        let env_layer = PartialConfig::from_env()?;

        let merged = default_layer.merge(file_layer).merge(env_layer);
        from_partial(merged)
    }

    pub fn agent_instruction(&self) -> String {
        self.agent_prompts
            .first()
            .map(|prompt| prompt.instruction.clone())
            .unwrap_or_default()
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct AgentPrompt {
    pub name: String,
    pub instruction: String,
}

#[derive(Clone, Debug, Deserialize, Default)]
struct PartialConfig {
    log_level: Option<String>,
    log_path: Option<String>,
    open_api_key: Option<String>,
    agent_prompts: Option<Vec<AgentPrompt>>,
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
        let open_api_key = std::env::var("ARAI_OPENAI_API_KEY").ok();
        Ok(Self {
            log_level,
            log_path,
            open_api_key,
            agent_prompts: None,
        })
    }

    fn merge(self, other: PartialConfig) -> PartialConfig {
        PartialConfig {
            log_level: other.log_level.or(self.log_level),
            log_path: other.log_path.or(self.log_path),
            open_api_key: other.open_api_key.or(self.open_api_key),
            agent_prompts: other.agent_prompts.or(self.agent_prompts),
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

    Ok(Config {
        log_level,
        log_path,
        open_api_key,
        agent_prompts,
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
        }
    }

    #[test]
    fn builds_config_from_valid_partial() {
        let cfg = from_partial(valid_partial()).expect("valid config should parse");
        assert_eq!(cfg.log_level, LevelFilter::Info);
        assert_eq!(cfg.log_path, PathBuf::from("/tmp/arai-test.log"));
        assert_eq!(cfg.open_api_key, "test-key");
        assert_eq!(cfg.agent_instruction(), "rewrite");
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
