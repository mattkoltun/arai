use crate::channels::AppEventSender;
use crate::messages::{AppEvent, AppEventKind, AppEventSource};
use log::{debug, warn};
use reqwest::StatusCode;
use reqwest::blocking::Client;
use serde_json::{Value, json};
use std::thread;
use std::time::Duration;

const OPENAI_MODEL: &str = "gpt-4o-mini";
const REQUEST_TIMEOUT_SECS: u64 = 60;
const MAX_RETRY_BACKOFF_SECS: u64 = 30;
const MAX_RETRIES: u32 = 5;

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

    pub fn submit(&self, instructions: String, text: String) {
        let app_event_tx = self.app_event_tx.clone();
        let api_key = self.api_key.clone();
        thread::spawn(
            move || match call_openai_with_retry(&api_key, instructions, text) {
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
            },
        );
    }
}

fn call_openai_with_retry(
    api_key: &str,
    instructions: String,
    text: String,
) -> Result<String, reqwest::Error> {
    debug!("Agent submitting text to OpenAI");
    let client = Client::builder()
        .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
        .build()?;

    let request = json!({
        "model": OPENAI_MODEL,
        "instructions": instructions,
        "input": text,
        "temperature": 0.2
    });

    let mut attempt = 0_u32;
    loop {
        attempt = attempt.saturating_add(1);

        match call_openai_once(&client, api_key, &request) {
            Ok(response) => return Ok(response),
            Err(err) => {
                if !is_retryable_error(&err) || attempt >= MAX_RETRIES {
                    return Err(err);
                }

                let delay = retry_delay(attempt);
                warn!(
                    "OpenAI request failed (attempt {}/{}): {}. Retrying in {}s",
                    attempt,
                    MAX_RETRIES,
                    err,
                    delay.as_secs()
                );
                thread::sleep(delay);
            }
        }
    }
}

fn call_openai_once(
    client: &Client,
    api_key: &str,
    request: &Value,
) -> Result<String, reqwest::Error> {
    let response: Value = client
        .post("https://api.openai.com/v1/responses")
        .bearer_auth(api_key)
        .json(request)
        .send()?
        .error_for_status()?
        .json()?;

    Ok(extract_output_text(&response))
}

fn is_retryable_error(err: &reqwest::Error) -> bool {
    if err.is_timeout() || err.is_connect() {
        return true;
    }

    matches!(
        err.status(),
        Some(StatusCode::TOO_MANY_REQUESTS)
            | Some(StatusCode::INTERNAL_SERVER_ERROR)
            | Some(StatusCode::BAD_GATEWAY)
            | Some(StatusCode::SERVICE_UNAVAILABLE)
            | Some(StatusCode::GATEWAY_TIMEOUT)
    )
}

fn retry_delay(attempt: u32) -> Duration {
    let exp = 2_u64.saturating_pow(attempt.saturating_sub(1).min(10));
    Duration::from_secs(exp.min(MAX_RETRY_BACKOFF_SECS))
}

fn extract_output_text(response: &Value) -> String {
    if let Some(output) = response.get("output").and_then(|v| v.as_array()) {
        for item in output {
            if let Some(content) = item.get("content").and_then(|v| v.as_array()) {
                for block in content {
                    if block.get("type").and_then(|v| v.as_str()) == Some("output_text")
                        && let Some(text) = block.get("text").and_then(|v| v.as_str())
                    {
                        return text.trim().to_string();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_output_from_nested_blocks() {
        let response = json!({
            "output": [{"content": [{"type": "output_text", "text": " hello world "}]}]
        });

        assert_eq!(extract_output_text(&response), "hello world");
    }

    #[test]
    fn falls_back_to_output_text_field() {
        let response = json!({"output_text": "  fallback text  "});
        assert_eq!(extract_output_text(&response), "fallback text");
    }

    #[test]
    fn retry_delay_grows_and_caps_at_thirty_seconds() {
        assert_eq!(retry_delay(1), Duration::from_secs(1));
        assert_eq!(retry_delay(2), Duration::from_secs(2));
        assert_eq!(retry_delay(5), Duration::from_secs(16));
        assert_eq!(retry_delay(6), Duration::from_secs(30));
        assert_eq!(retry_delay(12), Duration::from_secs(30));
    }
}
