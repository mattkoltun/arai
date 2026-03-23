use crate::config::{AgentPrompt, Config, ThemeMode, TranscriberConfig};
use crate::messages::ApiKeyStatus;
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, Default)]
pub struct AppStateSnapshot {
    pub agent_prompts: Vec<AgentPrompt>,
    pub default_prompt: usize,
    pub transcriber: TranscriberConfig,
    pub input_device: Option<String>,
    pub global_hotkey: String,
    #[allow(dead_code)]
    pub api_key_status: ApiKeyStatus,
    pub theme_mode: ThemeMode,
}

/// Shared application state.
///
/// Holds a single `Config` behind a mutex. All reads and writes go through
/// the config directly — no duplicated fields.
pub struct AppState {
    inner: Mutex<Config>,
}

pub type AppStateHandle = Arc<AppState>;

/// Masks an API key for display: shows first 3 + "..." + last 4 chars.
fn mask_api_key(key: &str) -> String {
    if key.len() <= 7 {
        return "sk-...".to_string();
    }
    format!("{}...{}", &key[..3], &key[key.len() - 4..])
}

/// Determines the API key status from runtime state.
fn compute_api_key_status(key: &str) -> ApiKeyStatus {
    if std::env::var("OPENAI_API_KEY")
        .ok()
        .filter(|k| !k.is_empty())
        .is_some()
    {
        ApiKeyStatus::EnvVar
    } else if !key.is_empty() {
        ApiKeyStatus::Keyring(mask_api_key(key))
    } else {
        ApiKeyStatus::NotSet
    }
}

impl AppState {
    pub fn new(config: Config) -> AppStateHandle {
        Arc::new(Self {
            inner: Mutex::new(config),
        })
    }

    pub fn snapshot(&self) -> AppStateSnapshot {
        let config = self.inner.lock().expect("app_state mutex poisoned");
        AppStateSnapshot {
            agent_prompts: config.agent_prompts.clone(),
            default_prompt: config.default_prompt,
            transcriber: config.transcriber.clone(),
            input_device: config.input_device.clone(),
            global_hotkey: config.global_hotkey.clone(),
            api_key_status: compute_api_key_status(&config.open_api_key),
            theme_mode: config.theme_mode.clone(),
        }
    }

    pub fn update_prompts(&self, prompts: Vec<AgentPrompt>, default: usize) {
        let mut config = self.inner.lock().expect("app_state mutex poisoned");
        let clamped = if default < prompts.len() { default } else { 0 };
        config.agent_prompts = prompts;
        config.default_prompt = clamped;
        if let Err(e) = config.save() {
            log::error!("Failed to save config: {e}");
        }
    }

    pub fn update_transcriber(&self, transcriber_config: TranscriberConfig) {
        let mut config = self.inner.lock().expect("app_state mutex poisoned");
        config.transcriber = transcriber_config;
        if let Err(e) = config.save() {
            log::error!("Failed to save config: {e}");
        }
    }

    pub fn update_input_device(&self, device: Option<String>) {
        let mut config = self.inner.lock().expect("app_state mutex poisoned");
        config.input_device = device;
        if let Err(e) = config.save() {
            log::error!("Failed to save config: {e}");
        }
    }

    pub fn update_global_hotkey(&self, hotkey: String) {
        let mut config = self.inner.lock().expect("app_state mutex poisoned");
        config.global_hotkey = hotkey;
        if let Err(e) = config.save() {
            log::error!("Failed to save config: {e}");
        }
    }

    pub fn update_theme_mode(&self, mode: ThemeMode) {
        let mut config = self.inner.lock().expect("app_state mutex poisoned");
        config.theme_mode = mode;
        if let Err(e) = config.save() {
            log::error!("Failed to save config: {e}");
        }
    }

    /// Updates the runtime API key. Does NOT save to config file —
    /// the key is persisted via keyring, not YAML.
    #[allow(dead_code)]
    pub fn update_api_key(&self, key: String) {
        let mut config = self.inner.lock().expect("app_state mutex poisoned");
        config.open_api_key = key;
    }

    /// Returns a clone of the current transcriber configuration.
    pub fn transcriber_config(&self) -> TranscriberConfig {
        let config = self.inner.lock().expect("app_state mutex poisoned");
        config.transcriber.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> Config {
        Config {
            log_level: "debug".to_string(),
            log_path: "/tmp/test.log".to_string(),
            open_api_key: "test-key".to_string(),
            agent_prompts: vec![AgentPrompt {
                name: "default".to_string(),
                instruction: "rewrite".to_string(),
            }],
            default_prompt: 0,
            transcriber: TranscriberConfig::default(),
            global_hotkey: "Alt+Space".to_string(),
            input_device: None,
            theme_mode: ThemeMode::default(),
        }
    }

    #[test]
    fn snapshot_returns_configured_prompts() {
        let state = AppState::new(test_config());
        let snapshot = state.snapshot();
        assert_eq!(snapshot.agent_prompts.len(), 1);
        assert_eq!(snapshot.agent_prompts[0].name, "default");
    }

    #[test]
    fn mask_api_key_shows_prefix_and_suffix() {
        assert_eq!(mask_api_key("sk-proj-abcdefghijklmnop"), "sk-...mnop");
    }

    #[test]
    fn mask_api_key_short_key_returns_placeholder() {
        assert_eq!(mask_api_key("sk-abc"), "sk-...");
    }

    #[test]
    fn update_api_key_changes_runtime_value() {
        let state = AppState::new(test_config());
        state.update_api_key("sk-new-key".to_string());
        let config = state.inner.lock().unwrap();
        assert_eq!(config.open_api_key, "sk-new-key");
    }

    #[test]
    fn snapshot_includes_api_key_status() {
        let state = AppState::new(test_config());
        let snapshot = state.snapshot();
        match snapshot.api_key_status {
            crate::messages::ApiKeyStatus::Keyring(masked) => {
                assert!(masked.contains("..."));
            }
            crate::messages::ApiKeyStatus::EnvVar => {
                // Acceptable if env var is set
            }
            crate::messages::ApiKeyStatus::NotSet => {
                panic!("Expected Keyring or EnvVar, got NotSet");
            }
        }
    }
}
