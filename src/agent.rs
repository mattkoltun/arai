use crate::channels::AppEventSender;
use crate::messages::{AppEvent, AppEventKind, AppEventSource};
use log::debug;
use reqwest::blocking::Client;
use serde_json::{Value, json};
use std::thread;
use std::time::Duration;

const OPENAI_MODEL: &str = "gpt-4o-mini";

#[derive(Clone)]
pub struct Agent {
    app_event_tx: AppEventSender,
    api_key: String,
}

impl Agent {
    pub fn new(app_event_tx: AppEventSender, api_key: String) -> Self {
        Self { app_event_tx, api_key }
    }

    pub fn submit(&self, instructions: String, text: String) {
        let app_event_tx = self.app_event_tx.clone();
        let api_key = self.api_key.clone();
        thread::spawn(move || match call_openai(&api_key, instructions, text) {
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

fn call_openai(api_key: &str, instructions: String, text: String) -> Result<String, reqwest::Error> {
    debug!("Agent submitting text to OpenAI");
    let client = Client::builder().timeout(Duration::from_secs(60)).build()?;

    let request = json!({
        "model": OPENAI_MODEL,
        "instructions": instructions,
        "input": text,
        "temperature": 0.2
    });

    let response: Value = client
        .post("https://api.openai.com/v1/responses")
        .bearer_auth(api_key)
        .json(&request)
        .send()?
        .error_for_status()?
        .json()?;

    Ok(extract_output_text(&response))
}

fn extract_output_text(response: &Value) -> String {
    if let Some(output) = response.get("output").and_then(|v| v.as_array()) {
        for item in output {
            if let Some(content) = item.get("content").and_then(|v| v.as_array()) {
                for block in content {
                    if block.get("type").and_then(|v| v.as_str()) == Some("output_text") {
                        if let Some(text) = block.get("text").and_then(|v| v.as_str()) {
                            return text.trim().to_string();
                        }
                    }
                }
            }
        }
    }

    response
        .get("output_text")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .trim()
        .to_string()
}
