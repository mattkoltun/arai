use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, Default)]
pub struct AppStateSnapshot {
    pub transcribed_text: String,
}

#[derive(Default)]
pub struct AppState {
    transcribed_text: Mutex<String>,
}

pub type AppStateHandle = Arc<AppState>;

impl AppState {
    pub fn new() -> AppStateHandle {
        Arc::new(Self::default())
    }

    pub fn snapshot(&self) -> AppStateSnapshot {
        let text = self
            .transcribed_text
            .lock()
            .map(|value| value.clone())
            .unwrap_or_default();
        AppStateSnapshot {
            transcribed_text: text,
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
}
