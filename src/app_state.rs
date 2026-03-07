use crate::config::{AgentPrompt, Config, TranscriberConfig};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, Default)]
pub struct AppStateSnapshot {
    pub agent_prompts: Vec<AgentPrompt>,
    pub default_prompt: usize,
    pub transcriber: TranscriberConfig,
}

pub struct AppState {
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
            agent_prompts: Mutex::new(prompts),
            default_prompt: Mutex::new(default),
            transcriber: Mutex::new(transcriber),
            config: Mutex::new(config),
        })
    }

    pub fn snapshot(&self) -> AppStateSnapshot {
        let prompts = self
            .agent_prompts
            .lock()
            .expect("agent_prompts mutex poisoned")
            .clone();
        let default = *self
            .default_prompt
            .lock()
            .expect("default_prompt mutex poisoned");
        let transcriber = self
            .transcriber
            .lock()
            .expect("transcriber mutex poisoned")
            .clone();
        AppStateSnapshot {
            agent_prompts: prompts,
            default_prompt: default,
            transcriber,
        }
    }

    pub fn update_prompts(&self, prompts: Vec<AgentPrompt>, default: usize) {
        *self
            .agent_prompts
            .lock()
            .expect("agent_prompts mutex poisoned") = prompts.clone();
        let clamped = if default < prompts.len() { default } else { 0 };
        *self
            .default_prompt
            .lock()
            .expect("default_prompt mutex poisoned") = clamped;
        let mut cfg = self.config.lock().expect("config mutex poisoned");
        cfg.agent_prompts = prompts;
        cfg.default_prompt = clamped;
        if let Err(e) = cfg.save() {
            log::error!("Failed to save config: {e}");
        }
    }

    pub fn update_transcriber(&self, transcriber_config: TranscriberConfig) {
        *self.transcriber.lock().expect("transcriber mutex poisoned") = transcriber_config.clone();
        let mut cfg = self.config.lock().expect("config mutex poisoned");
        cfg.transcriber = transcriber_config;
        if let Err(e) = cfg.save() {
            log::error!("Failed to save config: {e}");
        }
    }

    pub fn agent_instruction(&self) -> String {
        let prompts = self
            .agent_prompts
            .lock()
            .expect("agent_prompts mutex poisoned");
        let default = *self
            .default_prompt
            .lock()
            .expect("default_prompt mutex poisoned");
        prompts
            .get(default)
            .map(|a| a.instruction.clone())
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
            global_hotkey: "CmdOrCtrl+Shift+A".to_string(),
        }
    }

    #[test]
    fn snapshot_returns_configured_prompts() {
        let state = AppState::new(test_config());
        let snapshot = state.snapshot();
        assert_eq!(snapshot.agent_prompts.len(), 1);
        assert_eq!(snapshot.agent_prompts[0].name, "default");
    }
}
