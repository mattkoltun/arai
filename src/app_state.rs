use crate::config::{AgentPrompt, Config, TranscriberConfig};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, Default)]
pub struct AppStateSnapshot {
    pub transcribed_text: String,
    pub agent_prompts: Vec<AgentPrompt>,
    pub default_prompt: usize,
    pub transcriber: TranscriberConfig,
}

pub struct AppState {
    transcribed_text: Mutex<String>,
    agent_prompts: Mutex<Vec<AgentPrompt>>,
    default_prompt: Mutex<usize>,
    transcriber: Mutex<TranscriberConfig>,
    config: Mutex<Config>,
}

pub type AppStateHandle = Arc<AppState>;

impl AppState {
    pub fn new(config: Config) -> AppStateHandle {
        let prompts = config.agent_prompts.clone();
        let default = config.default_prompt;
        let transcriber = config.transcriber.clone();
        Arc::new(Self {
            transcribed_text: Mutex::new(String::new()),
            agent_prompts: Mutex::new(prompts),
            default_prompt: Mutex::new(default),
            transcriber: Mutex::new(transcriber),
            config: Mutex::new(config),
        })
    }

    pub fn snapshot(&self) -> AppStateSnapshot {
        let text = self
            .transcribed_text
            .lock()
            .map(|value| value.clone())
            .unwrap_or_default();
        let prompts = self
            .agent_prompts
            .lock()
            .map(|v| v.clone())
            .unwrap_or_default();
        let default = self.default_prompt.lock().map(|g| *g).unwrap_or(0);
        let transcriber = self
            .transcriber
            .lock()
            .map(|v| v.clone())
            .unwrap_or_default();
        AppStateSnapshot {
            transcribed_text: text,
            agent_prompts: prompts,
            default_prompt: default,
            transcriber,
        }
    }

    pub(crate) fn append_transcription(&self, text: &str) {
        if let Ok(mut value) = self.transcribed_text.lock() {
            if !value.is_empty() && !value.ends_with(' ') {
                value.push(' ');
            }
            value.push_str(text);
        }
    }

    pub fn update_prompts(&self, prompts: Vec<AgentPrompt>, default: usize) {
        if let Ok(mut p) = self.agent_prompts.lock() {
            *p = prompts.clone();
        }
        let clamped = if default < prompts.len() { default } else { 0 };
        if let Ok(mut d) = self.default_prompt.lock() {
            *d = clamped;
        }
        if let Ok(mut cfg) = self.config.lock() {
            cfg.agent_prompts = prompts;
            cfg.default_prompt = clamped;
            if let Err(e) = cfg.save() {
                log::error!("Failed to save config: {e}");
            }
        }
    }

    pub fn update_transcriber(&self, transcriber_config: TranscriberConfig) {
        if let Ok(mut t) = self.transcriber.lock() {
            *t = transcriber_config.clone();
        }
        if let Ok(mut cfg) = self.config.lock() {
            cfg.transcriber = transcriber_config;
            if let Err(e) = cfg.save() {
                log::error!("Failed to save config: {e}");
            }
        }
    }

    pub fn agent_instruction(&self) -> String {
        let prompts = self.agent_prompts.lock().ok();
        let default = self.default_prompt.lock().map(|g| *g).unwrap_or(0);
        prompts
            .and_then(|p| p.get(default).map(|a| a.instruction.clone()))
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> Config {
        Config {
            log_level: log::LevelFilter::Debug,
            log_path: std::path::PathBuf::from("/tmp/test.log"),
            open_api_key: "test-key".to_string(),
            agent_prompts: vec![AgentPrompt {
                name: "default".to_string(),
                instruction: "rewrite".to_string(),
            }],
            default_prompt: 0,
            transcriber: TranscriberConfig::default(),
        }
    }

    #[test]
    fn appends_with_spaces_between_chunks() {
        let state = AppState::new(test_config());
        state.append_transcription("hello");
        state.append_transcription("world");

        assert_eq!(state.snapshot().transcribed_text, "hello world");
    }

    #[test]
    fn does_not_add_extra_space_when_existing_chunk_already_ends_with_space() {
        let state = AppState::new(test_config());
        state.append_transcription("hello ");
        state.append_transcription("world");

        assert_eq!(state.snapshot().transcribed_text, "hello world");
    }
}
