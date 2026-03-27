use crate::channels::{AppEventSender, LlmSender};
use crate::messages::{AppEvent, AppEventKind, AppEventSource};
use log::{debug, info};
use std::fmt::{Display, Formatter};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};

/// Error returned by an LLM connector operation.
#[derive(Debug)]
pub enum LlmError {
    Request(reqwest::Error),
    Message(String),
}

impl Display for LlmError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Request(err) => write!(f, "{err}"),
            Self::Message(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for LlmError {}

impl From<reqwest::Error> for LlmError {
    fn from(err: reqwest::Error) -> Self {
        Self::Request(err)
    }
}

impl From<String> for LlmError {
    fn from(message: String) -> Self {
        Self::Message(message)
    }
}

impl From<&str> for LlmError {
    fn from(message: &str) -> Self {
        Self::Message(message.to_string())
    }
}

/// Request sent to the background LLM worker.
pub struct LlmRequest {
    pub command: LlmCommand,
}

/// Commands handled by the LLM worker.
pub enum LlmCommand {
    SubmitText {
        model: String,
        instruction: String,
        text: String,
    },
    #[allow(dead_code)]
    ListModels,
}

/// Abstracts an LLM provider implementation behind a common connector interface.
pub trait LlmConnector: Send {
    fn provider_name(&self) -> &'static str;
    fn submit_text(
        &self,
        model: &str,
        instruction: &str,
        text: &str,
        stop: &AtomicBool,
    ) -> Result<String, LlmError>;
    fn list_models(&self, stop: &AtomicBool) -> Result<Vec<String>, LlmError>;
}

/// Runs LLM requests sequentially on a dedicated worker thread using a connector.
pub struct LlmWorker {
    tx: Option<LlmSender>,
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl LlmWorker {
    /// Creates a new LLM worker with the given provider connector.
    pub fn new(app_event_tx: AppEventSender, connector: Box<dyn LlmConnector>) -> Self {
        let (tx, rx) = std::sync::mpsc::channel::<LlmRequest>();
        let stop = Arc::new(AtomicBool::new(false));
        let stop_flag = Arc::clone(&stop);

        let handle = thread::spawn(move || {
            let provider = connector.provider_name();
            info!("LLM worker thread started for provider {provider}");
            while let Ok(request) = rx.recv() {
                if stop_flag.load(Ordering::SeqCst) {
                    break;
                }
                debug!("LLM worker processing request");
                match request.command {
                    LlmCommand::SubmitText {
                        model,
                        instruction,
                        text,
                    } => match connector.submit_text(&model, &instruction, &text, &stop_flag) {
                        Ok(response) => {
                            let _ = app_event_tx.send(AppEvent {
                                source: AppEventSource::Llm,
                                kind: AppEventKind::LlmResponse(response),
                            });
                        }
                        Err(err) => {
                            let _ = app_event_tx.send(AppEvent {
                                source: AppEventSource::Llm,
                                kind: AppEventKind::Error(format!(
                                    "{provider} submit failed: {err}"
                                )),
                            });
                        }
                    },
                    LlmCommand::ListModels => match connector.list_models(&stop_flag) {
                        Ok(models) => {
                            let _ = app_event_tx.send(AppEvent {
                                source: AppEventSource::Llm,
                                kind: AppEventKind::LlmModelsAvailable(models),
                            });
                        }
                        Err(err) => {
                            let _ = app_event_tx.send(AppEvent {
                                source: AppEventSource::Llm,
                                kind: AppEventKind::LlmModelsLoadFailed(format!(
                                    "{provider} list models failed: {err}"
                                )),
                            });
                        }
                    },
                }
            }
            info!("LLM worker thread exiting");
        });

        Self {
            tx: Some(tx),
            stop,
            handle: Some(handle),
        }
    }

    /// Submits a text processing request to the worker thread.
    pub fn submit_text(&self, model: String, instruction: String, text: String) {
        if let Some(ref tx) = self.tx {
            let _ = tx.send(LlmRequest {
                command: LlmCommand::SubmitText {
                    model,
                    instruction,
                    text,
                },
            });
        }
    }

    /// Requests the list of models available from the current provider.
    #[allow(dead_code)]
    pub fn list_models(&self) {
        if let Some(ref tx) = self.tx {
            let _ = tx.send(LlmRequest {
                command: LlmCommand::ListModels,
            });
        }
    }
}

impl Drop for LlmWorker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        self.tx.take();
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}
