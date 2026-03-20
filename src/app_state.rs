use crate::config::{AgentPrompt, Config, TranscriberConfig};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, Default)]
pub struct AppStateSnapshot {
    pub agent_prompts: Vec<AgentPrompt>,
    pub default_prompt: usize,
    pub transcriber: TranscriberConfig,
    pub input_device: Option<String>,
    pub global_hotkey: String,
}

/// Internal state protected by a single mutex to guarantee consistent reads
/// and writes across all fields.
struct AppStateInner {
    agent_prompts: Vec<AgentPrompt>,
    default_prompt: usize,
    transcriber: TranscriberConfig,
    config: Config,
}

/// Shared application state.
///
/// All mutable fields live inside a single `Mutex<AppStateInner>` so that
/// every read or write sees a consistent view — no window where
/// `default_prompt` can point past the end of `agent_prompts`.
pub struct AppState {
    inner: Mutex<AppStateInner>,
}

pub type AppStateHandle = Arc<AppState>;

impl AppState {
    pub fn new(config: Config) -> AppStateHandle {
        let prompts = config.agent_prompts.clone();
        let default = config.default_prompt;
        let transcriber = config.transcriber.clone();
        Arc::new(Self {
            inner: Mutex::new(AppStateInner {
                agent_prompts: prompts,
                default_prompt: default,
                transcriber,
                config,
            }),
        })
    }

    pub fn snapshot(&self) -> AppStateSnapshot {
        let inner = self.inner.lock().expect("app_state mutex poisoned");
        AppStateSnapshot {
            agent_prompts: inner.agent_prompts.clone(),
            default_prompt: inner.default_prompt,
            transcriber: inner.transcriber.clone(),
            input_device: inner.config.input_device.clone(),
            global_hotkey: inner.config.global_hotkey.clone(),
        }
    }

    pub fn update_prompts(&self, prompts: Vec<AgentPrompt>, default: usize) {
        let mut inner = self.inner.lock().expect("app_state mutex poisoned");
        let clamped = if default < prompts.len() { default } else { 0 };
        inner.agent_prompts = prompts.clone();
        inner.default_prompt = clamped;
        inner.config.agent_prompts = prompts;
        inner.config.default_prompt = clamped;
        if let Err(e) = inner.config.save() {
            log::error!("Failed to save config: {e}");
        }
    }

    pub fn update_transcriber(&self, transcriber_config: TranscriberConfig) {
        let mut inner = self.inner.lock().expect("app_state mutex poisoned");
        inner.transcriber = transcriber_config.clone();
        inner.config.transcriber = transcriber_config;
        if let Err(e) = inner.config.save() {
            log::error!("Failed to save config: {e}");
        }
    }

    pub fn update_input_device(&self, device: Option<String>) {
        let mut inner = self.inner.lock().expect("app_state mutex poisoned");
        inner.config.input_device = device;
        if let Err(e) = inner.config.save() {
            log::error!("Failed to save config: {e}");
        }
    }

    pub fn update_global_hotkey(&self, hotkey: String) {
        let mut inner = self.inner.lock().expect("app_state mutex poisoned");
        inner.config.global_hotkey = hotkey;
        if let Err(e) = inner.config.save() {
            log::error!("Failed to save config: {e}");
        }
    }

    /// Returns a clone of the current transcriber configuration.
    pub fn transcriber_config(&self) -> TranscriberConfig {
        let inner = self.inner.lock().expect("app_state mutex poisoned");
        inner.transcriber.clone()
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
            input_device: None,
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
