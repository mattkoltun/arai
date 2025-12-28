use crate::channels::AppEventSender;
use crate::messages::{AppEvent, AppEventKind, AppEventSource};
use log::debug;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::thread;
use std::time::Duration;

const AGENT_PROMPT: &str =
    "Rewrite the user text for clarity and brevity while preserving meaning.";
const OPENAI_MODEL: &str = "gpt-4o-mini";

#[derive(Clone)]
pub struct Agent {
    app_event_tx: AppEventSender,
    api_key: String,
}

impl Agent {
    pub fn new(app_event_tx: AppEventSender, api_key: String) -> Self {
        Self {
            app_event_tx,
            api_key,
        }
    }

    pub fn submit(&self, text: String) {
        let app_event_tx = self.app_event_tx.clone();
        let api_key = self.api_key.clone();
        thread::spawn(move || match call_openai(&api_key, text) {
            Ok(response) => {
                let _ = app_event_tx.send(AppEvent {
                    source: AppEventSource::Agent,
                    kind: AppEventKind::AgentResponse(response),
                });
            }
            Err(err) => {
                let _ = app_event_tx.send(AppEvent {
                    source: AppEventSource::Agent,
                    kind: AppEventKind::Error(format!("Agent request failed: {err}")),
                });
            }
        });
    }
}

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    temperature: f32,
}

#[derive(Serialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatResponseMessage,
}

#[derive(Deserialize)]
struct ChatResponseMessage {
    content: String,
}

fn call_openai(api_key: &str, text: String) -> Result<String, reqwest::Error> {
    debug!("Agent submitting text to OpenAI");
    let client = Client::builder().timeout(Duration::from_secs(60)).build()?;

    let request = ChatRequest {
        model: OPENAI_MODEL.to_string(),
        messages: vec![
            ChatMessage {
                role: "system".to_string(),
                content: AGENT_PROMPT.to_string(),
            },
            ChatMessage {
                role: "user".to_string(),
                content: text,
            },
        ],
        temperature: 0.2,
    };

    let response: ChatResponse = client
        .post("https://api.openai.com/v1/chat/completions")
        .bearer_auth(api_key)
        .json(&request)
        .send()?
        .error_for_status()?
        .json()?;

    Ok(response
        .choices
        .into_iter()
        .next()
        .map(|choice| choice.message.content)
        .unwrap_or_default()
        .trim()
        .to_string())
}
