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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appends_with_spaces_between_chunks() {
        let state = AppState::default();
        state.append_transcription("hello");
        state.append_transcription("world");

        assert_eq!(state.snapshot().transcribed_text, "hello world");
    }

    #[test]
    fn does_not_add_extra_space_when_existing_chunk_already_ends_with_space() {
        let state = AppState::default();
        state.append_transcription("hello ");
        state.append_transcription("world");

        assert_eq!(state.snapshot().transcribed_text, "hello world");
    }
}
