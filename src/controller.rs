use crate::agent::Agent;
use crate::app_state::AppStateHandle;
use crate::channels::{AppEventReceiver, AppEventSender, AudioChannels, UiUpdateSender};
use crate::config::TranscriberConfig;
use crate::history::History;
use crate::messages::{AppEventKind, AppEventSource, ErrorInfo, UiUpdate};
use crate::recorder::Recorder;
use crate::transcriber::Transcriber;
use log::{debug, error, info};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// Formats current UTC time as HH:MM:SS for error timestamps.
fn format_timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let hours = (secs / 3600) % 24;
    let minutes = (secs / 60) % 60;
    let seconds = secs % 60;
    format!("{hours:02}:{minutes:02}:{seconds:02}")
}

/// Builds an `ErrorInfo` by splitting the message on the first ": ".
/// The part before becomes the title, the part after becomes the detail.
fn build_error_info(source_name: &str, message: &str) -> ErrorInfo {
    let (title, detail) = match message.split_once(": ") {
        Some((t, d)) => (t.to_string(), d.to_string()),
        None => (format!("{source_name} error"), message.to_string()),
    };
    ErrorInfo {
        source: source_name.to_string(),
        title,
        detail,
        timestamp: format_timestamp(),
    }
}

pub struct Controller {
    recorder: Recorder,
    transcriber: Transcriber,
    app_event_tx: AppEventSender,
    app_event_rx: AppEventReceiver,
    agent: Agent,
    app_state: AppStateHandle,
    ui_update_tx: UiUpdateSender,
    shutting_down: Arc<AtomicBool>,
    history: History,
}

impl Controller {
    /// Creates a Controller that uses the provided `shutdown_flag` to signal
    /// when the run loop should exit.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        recorder: Recorder,
        transcriber: Transcriber,
        app_event_tx: AppEventSender,
        app_event_rx: AppEventReceiver,
        agent: Agent,
        app_state: AppStateHandle,
        ui_update_tx: UiUpdateSender,
        shutdown_flag: Arc<AtomicBool>,
    ) -> Self {
        Self {
            recorder,
            transcriber,
            app_event_tx,
            app_event_rx,
            agent,
            app_state,
            ui_update_tx,
            shutting_down: shutdown_flag,
            history: History::new(),
        }
    }

    fn start_listening(&mut self) {
        info!("Controller starting recorder");
        self.transcriber.reset_drain();
        let _ = self.recorder.start();
    }

    fn stop_listening(&self) {
        info!("Controller signaling recorder to stop");
        // Tell the transcriber to drain buffered chunks without inference.
        // The full audio is saved to WAV for reconciliation anyway.
        self.transcriber.drain_without_inference();
        self.recorder.stop_signal();
    }

    fn process_text(&self, text: String) {
        debug!("Controller processing text");
        let _ = self
            .ui_update_tx
            .send(UiUpdate::AgentResponseReceived(text));
    }

    fn submit_text(&self, instruction: String, text: String) {
        debug!(
            "Controller submitting text with instruction: {}",
            &instruction[..instruction.len().min(80)]
        );
        self.agent.submit(instruction, text);
    }

    /// Appends a transcription chunk to the accumulated text, deduplicating
    /// any overlapping suffix/prefix caused by the transcriber's sliding window.
    fn append_transcription(accumulated: &mut String, text: &str) {
        if accumulated.is_empty() {
            accumulated.push_str(text);
            return;
        }

        // Find the longest suffix of `accumulated` that matches a prefix of
        // `text` (case-insensitive, word-boundary aligned) and skip it.
        let new_text = strip_overlap(accumulated, text);
        if !new_text.is_empty() {
            if !accumulated.ends_with(' ') {
                accumulated.push(' ');
            }
            accumulated.push_str(new_text);
        }
    }

    /// Spawns a background thread to reconcile the full recording.
    fn start_reconciliation(&self, path: String) {
        info!("Starting reconciliation from {path}");
        let _ = self.ui_update_tx.send(UiUpdate::ReconciliationStarted);
        self.transcriber.reconcile_file(path);
    }

    /// Stops the current transcriber and starts a new one with updated config.
    /// Creates a fresh audio channel and wires it to the recorder.
    fn restart_transcriber(&mut self, config: TranscriberConfig) {
        info!("Restarting transcriber with new config");
        self.transcriber.stop();
        // Drop the old transcriber to join its worker thread.
        let old = std::mem::replace(&mut self.transcriber, {
            let AudioChannels { audio_tx, audio_rx } = AudioChannels::new();
            self.recorder.set_audio_tx(audio_tx);
            Transcriber::new(audio_rx, self.app_event_tx.clone(), config)
        });
        drop(old);
        if let Err(e) = self.transcriber.start() {
            error!("Failed to restart transcriber: {e}");
        }
    }

    /// Drops the current Agent and creates a new one with the given API key.
    fn restart_agent(&mut self, api_key: String) {
        info!("Restarting agent with new API key");
        let old = std::mem::replace(
            &mut self.agent,
            Agent::new(self.app_event_tx.clone(), api_key),
        );
        drop(old);
    }

    /// Sends a `ConfigSnapshot` to the UI so it has the current config state.
    fn send_config_snapshot(&self) {
        let snapshot = self.app_state.snapshot();
        let _ = self.ui_update_tx.send(UiUpdate::ConfigSnapshot {
            agent_prompts: snapshot.agent_prompts,
            default_prompt: snapshot.default_prompt,
            transcriber: snapshot.transcriber,
            selected_input_device: snapshot.input_device,
            global_hotkey: snapshot.global_hotkey,
            api_key_status: snapshot.api_key_status,
            theme_mode: snapshot.theme_mode,
        });
    }

    /// Runs the Controller event loop, consuming `self`. The loop exits when
    /// the associated [`ShutdownHandle`] signals shutdown.
    pub fn run(mut self) {
        let mut accumulated_transcription = String::new();
        // Text that existed before the current recording session started.
        let mut pre_recording_text = String::new();
        let mut reconciling = false;
        // Both conditions must be true before reconciliation can start:
        // the streaming transcriber must finish its backlog AND the
        // recorder must finish writing the WAV file.
        let mut wav_path_ready: Option<String> = None;
        let mut streaming_drained = false;

        // Send initial config snapshot so the UI has config before any changes.
        self.send_config_snapshot();

        while !self.shutting_down.load(Ordering::SeqCst) {
            let event = match self.app_event_rx.recv_timeout(Duration::from_millis(10)) {
                Ok(event) => event,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            };

            match (event.source, event.kind) {
                (AppEventSource::Recorder, AppEventKind::Stopped(wav_path)) => {
                    info!("Recorder stopped, joining handle");
                    self.recorder.join_handle();
                    let file_size_bytes = wav_path
                        .as_deref()
                        .and_then(|path| std::fs::metadata(path).ok())
                        .map(|meta| meta.len());
                    let _ = self
                        .ui_update_tx
                        .send(UiUpdate::RecordingFinished { file_size_bytes });
                    wav_path_ready = wav_path;
                    if streaming_drained && let Some(path) = wav_path_ready.take() {
                        reconciling = true;
                        self.start_reconciliation(path);
                    }
                }
                (AppEventSource::Recorder, AppEventKind::Error(message)) => {
                    error!("Recorder event: {message}");
                    let info = build_error_info("Recorder", &message);
                    let _ = self.ui_update_tx.send(UiUpdate::ErrorOccurred(info));
                }
                (AppEventSource::Transcriber, AppEventKind::Error(message)) => {
                    error!("Transcriber event: {message}");
                    let info = build_error_info("Transcriber", &message);
                    let _ = self.ui_update_tx.send(UiUpdate::ErrorOccurred(info));
                }
                (AppEventSource::Transcriber, AppEventKind::Transcription(text)) => {
                    debug!("Controller received transcript");
                    Self::append_transcription(&mut accumulated_transcription, &text);
                    if !reconciling {
                        let _ = self.ui_update_tx.send(UiUpdate::TranscriptionUpdated(
                            accumulated_transcription.clone(),
                        ));
                    }
                }
                (AppEventSource::Transcriber, AppEventKind::StreamingDrained) => {
                    info!("Controller received streaming drained signal");
                    streaming_drained = true;
                    if let Some(path) = wav_path_ready.take() {
                        reconciling = true;
                        self.start_reconciliation(path);
                    }
                }
                (AppEventSource::Transcriber, AppEventKind::ReconciliationComplete(text)) => {
                    reconciling = false;
                    info!("Reconciliation complete ({} chars)", text.len());
                    if !text.is_empty() {
                        // Reconciliation only covers audio from this session.
                        // Prepend any text that existed before recording started.
                        accumulated_transcription = if pre_recording_text.is_empty() {
                            text
                        } else {
                            let mut combined = pre_recording_text.clone();
                            if !combined.ends_with(' ') {
                                combined.push(' ');
                            }
                            combined.push_str(&text);
                            combined
                        };
                    }
                    let _ = self.ui_update_tx.send(UiUpdate::ReconciliationComplete(
                        accumulated_transcription.clone(),
                    ));
                }
                (AppEventSource::Agent, AppEventKind::Error(message)) => {
                    error!("Agent event: {message}");
                    let info = build_error_info("Agent", &message);
                    let _ = self.ui_update_tx.send(UiUpdate::ErrorOccurred(info));
                    let _ = self.ui_update_tx.send(UiUpdate::ProcessingFailed(message));
                }
                (AppEventSource::Agent, AppEventKind::AgentResponse(text)) => {
                    self.process_text(text);
                }
                (AppEventSource::Ui, AppEventKind::UiStartListening(text)) => {
                    pre_recording_text = text.clone();
                    accumulated_transcription = text;
                    streaming_drained = false;
                    wav_path_ready = None;
                    self.start_listening();
                }
                (AppEventSource::Ui, AppEventKind::UiStopListening) => {
                    self.stop_listening();
                }
                (AppEventSource::Ui, AppEventKind::UiSubmitText { text, instruction }) => {
                    self.submit_text(instruction, text);
                }
                (AppEventSource::Ui, AppEventKind::UiShutdown) => {
                    self.shutting_down.store(true, Ordering::SeqCst);
                }
                (
                    AppEventSource::Ui,
                    AppEventKind::UiUpdatePrompts {
                        prompts,
                        default_prompt,
                    },
                ) => {
                    info!("Controller updating agent prompts");
                    self.app_state.update_prompts(prompts, default_prompt);
                    self.send_config_snapshot();
                }
                (AppEventSource::Ui, AppEventKind::UiUpdateTranscriber(transcriber_config)) => {
                    info!("Controller updating transcriber config");
                    let old = self.app_state.transcriber_config();
                    let needs_restart = old.use_gpu != transcriber_config.use_gpu
                        || old.flash_attn != transcriber_config.flash_attn
                        || old.no_timestamps != transcriber_config.no_timestamps
                        || old.model_path != transcriber_config.model_path;
                    self.app_state
                        .update_transcriber(transcriber_config.clone());
                    if needs_restart {
                        self.restart_transcriber(transcriber_config);
                    }
                    self.send_config_snapshot();
                }
                (AppEventSource::Ui, AppEventKind::UiUpdateInputDevice(device)) => {
                    info!("Controller updating input device: {:?}", device);
                    self.app_state.update_input_device(device.clone());
                    self.recorder.set_input_device(device);
                    self.send_config_snapshot();
                }
                (AppEventSource::Ui, AppEventKind::UiUpdateGlobalHotkey(hotkey)) => {
                    info!("Controller updating global hotkey: {hotkey}");
                    self.app_state.update_global_hotkey(hotkey);
                    self.send_config_snapshot();
                }
                (AppEventSource::Ui, AppEventKind::UiUpdateThemeMode(mode)) => {
                    info!("Controller updating theme mode: {mode:?}");
                    self.app_state.update_theme_mode(mode);
                    self.send_config_snapshot();
                }
                (AppEventSource::Ui, AppEventKind::UiUpdateApiKey(key)) => {
                    info!("Controller updating API key");
                    if let Err(e) = crate::keyring_store::set_api_key(&key) {
                        error!("Failed to save API key to keyring: {e}");
                    }
                    self.app_state.update_api_key(key.clone());
                    self.restart_agent(key);
                    self.send_config_snapshot();
                }
                (AppEventSource::Ui, AppEventKind::UiCopied { text, prompt }) => {
                    debug!("Saving copy to history");
                    self.history.save(text, prompt);
                }
                (_, AppEventKind::ModelDownloadProgress(downloaded, total)) => {
                    let _ = self
                        .ui_update_tx
                        .send(UiUpdate::ModelDownloadProgress(downloaded, total));
                }
                (_, AppEventKind::ModelDownloadComplete(path)) => {
                    info!("Model download complete: {}", path.display());
                    let path_str = path.display().to_string();
                    self.app_state.update_transcriber(TranscriberConfig {
                        model_path: path_str,
                        ..self.app_state.transcriber_config()
                    });
                    self.restart_transcriber(self.app_state.transcriber_config());
                    self.send_config_snapshot();
                    let _ = self
                        .ui_update_tx
                        .send(UiUpdate::ModelDownloadComplete(path));
                }
                (_, AppEventKind::ModelDownloadFailed(err)) => {
                    error!("Model download failed: {err}");
                    let _ = self.ui_update_tx.send(UiUpdate::ModelDownloadFailed(err));
                }
                (_, AppEventKind::ModelDownloadCancelled) => {
                    info!("Model download cancelled");
                    let _ = self.ui_update_tx.send(UiUpdate::ModelDownloadCancelled);
                }
                (source, kind) => {
                    let _ = (source, kind);
                    // TODO: handle other app events
                }
            }
        }

        info!("Controller shutting down");
        drop(self.ui_update_tx);
        let mut recorder = self.recorder;
        let transcriber = self.transcriber;
        transcriber.stop();
        recorder.stop();
        drop(recorder);
        drop(transcriber);
    }
}

/// Returns the portion of `new_text` that doesn't overlap with the tail of
/// `existing`. Compares word sequences case-insensitively to handle slight
/// Whisper variations. If no overlap is found, returns the full `new_text`.
fn strip_overlap<'a>(existing: &str, new_text: &'a str) -> &'a str {
    let existing_words: Vec<&str> = existing.split_whitespace().collect();
    let new_words: Vec<&str> = new_text.split_whitespace().collect();

    if existing_words.is_empty() || new_words.is_empty() {
        return new_text;
    }

    // Try matching progressively longer prefixes of new_words against the
    // tail of existing_words. Find the longest overlap.
    let max_check = existing_words.len().min(new_words.len());
    let mut best_overlap = 0;

    for overlap_len in 1..=max_check {
        let existing_tail = &existing_words[existing_words.len() - overlap_len..];
        let new_prefix = &new_words[..overlap_len];

        if existing_tail
            .iter()
            .zip(new_prefix.iter())
            .all(|(a, b)| a.eq_ignore_ascii_case(b))
        {
            best_overlap = overlap_len;
        }
    }

    if best_overlap == 0 {
        return new_text;
    }

    // Find the byte offset in new_text after skipping `best_overlap` words.
    let mut offset = 0;
    for _ in 0..best_overlap {
        // Skip whitespace
        while offset < new_text.len() && new_text.as_bytes()[offset].is_ascii_whitespace() {
            offset += 1;
        }
        // Skip word
        while offset < new_text.len() && !new_text.as_bytes()[offset].is_ascii_whitespace() {
            offset += 1;
        }
    }
    // Skip leading whitespace of remainder
    while offset < new_text.len() && new_text.as_bytes()[offset].is_ascii_whitespace() {
        offset += 1;
    }
    &new_text[offset..]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_overlap_appends_full_text() {
        assert_eq!(strip_overlap("hello world", "foo bar"), "foo bar");
    }

    #[test]
    fn strips_simple_overlap() {
        assert_eq!(
            strip_overlap("interesting to see.", "interesting to see. because I hope"),
            "because I hope"
        );
    }

    #[test]
    fn strips_partial_tail_overlap() {
        assert_eq!(
            strip_overlap(
                "because I hope because the other",
                "because the other part was very annoying."
            ),
            "part was very annoying."
        );
    }

    #[test]
    fn strips_single_word_overlap() {
        assert_eq!(
            strip_overlap(
                "It's still kind of happening.",
                "happening. Some parts are still happening."
            ),
            "Some parts are still happening."
        );
    }

    #[test]
    fn empty_existing_returns_full_text() {
        assert_eq!(strip_overlap("", "new text"), "new text");
    }

    #[test]
    fn full_overlap_returns_empty() {
        assert_eq!(strip_overlap("hello world", "hello world"), "");
    }

    #[test]
    fn build_error_info_splits_on_colon() {
        let info = super::build_error_info("Agent", "Agent request failed: connection timeout");
        assert_eq!(info.source, "Agent");
        assert_eq!(info.title, "Agent request failed");
        assert_eq!(info.detail, "connection timeout");
        assert!(!info.timestamp.is_empty());
    }

    #[test]
    fn build_error_info_no_colon_uses_source_as_title() {
        let info = super::build_error_info("Recorder", "something went wrong");
        assert_eq!(info.source, "Recorder");
        assert_eq!(info.title, "Recorder error");
        assert_eq!(info.detail, "something went wrong");
    }

    #[test]
    fn build_error_info_multiple_colons_splits_on_first() {
        let info = super::build_error_info("Transcriber", "Model error: path: /foo/bar not found");
        assert_eq!(info.title, "Model error");
        assert_eq!(info.detail, "path: /foo/bar not found");
    }
}
