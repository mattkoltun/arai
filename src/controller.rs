use crate::agent::Agent;
use crate::app_state::AppStateHandle;
use crate::channels::{AppEventReceiver, AppEventSender, UiUpdateSender};
use crate::config::TranscriberConfig;
use crate::messages::{AppEvent, AppEventKind, AppEventSource, UiUpdate};
use crate::recorder::Recorder;
use crate::transcriber::Transcriber;
use log::{debug, error, info};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// Lightweight handle that allows external code (e.g. main) to signal shutdown
/// without holding the entire Controller behind an Arc.
pub struct ShutdownHandle {
    flag: Arc<AtomicBool>,
}

impl ShutdownHandle {
    /// Signals the Controller's run loop to stop.
    pub fn shutdown(&self) {
        info!("Controller shutdown requested");
        self.flag.store(true, Ordering::SeqCst);
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
}

impl Controller {
    /// Creates a Controller and a [`ShutdownHandle`] that can trigger graceful
    /// shutdown from another thread.
    pub fn new(
        recorder: Recorder,
        transcriber: Transcriber,
        app_event_tx: AppEventSender,
        app_event_rx: AppEventReceiver,
        agent: Agent,
        app_state: AppStateHandle,
        ui_update_tx: UiUpdateSender,
    ) -> (Self, ShutdownHandle) {
        let flag = Arc::new(AtomicBool::new(false));
        let handle = ShutdownHandle { flag: flag.clone() };
        let controller = Self {
            recorder,
            transcriber,
            app_event_tx,
            app_event_rx,
            agent,
            app_state,
            ui_update_tx,
            shutting_down: flag,
        };
        (controller, handle)
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

    fn submit_text(&self, text: String) {
        let instruction = self.app_state.agent_instruction();
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
        let transcriber_config = self.app_state.transcriber_config();
        let tx = self.app_event_tx.clone();
        std::thread::spawn(move || {
            let result = crate::transcriber::transcribe_wav_file(&transcriber_config, &path);
            match result {
                Ok(text) => {
                    let _ = tx.send(AppEvent {
                        source: AppEventSource::Transcriber,
                        kind: AppEventKind::ReconciliationComplete(text),
                    });
                }
                Err(e) => {
                    error!("Reconciliation failed: {e}");
                    let _ = tx.send(AppEvent {
                        source: AppEventSource::Transcriber,
                        kind: AppEventKind::ReconciliationComplete(String::new()),
                    });
                }
            }
            if let Err(e) = std::fs::remove_file(&path) {
                log::warn!("Failed to remove recording {path}: {e}");
            }
        });
    }

    /// Stops the current transcriber and starts a new one with updated config.
    /// Creates a fresh audio channel and wires it to the recorder.
    fn restart_transcriber(&mut self, config: TranscriberConfig) {
        info!("Restarting transcriber with new config");
        self.transcriber.stop();
        // Drop the old transcriber to join its worker thread.
        let old = std::mem::replace(&mut self.transcriber, {
            let (audio_tx, audio_rx) = std::sync::mpsc::channel();
            self.recorder.set_audio_tx(audio_tx);
            Transcriber::new(audio_rx, self.app_event_tx.clone(), config)
        });
        drop(old);
        if let Err(e) = self.transcriber.start() {
            error!("Failed to restart transcriber: {e}");
        }
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
                    wav_path_ready = wav_path;
                    if streaming_drained && let Some(path) = wav_path_ready.take() {
                        reconciling = true;
                        self.start_reconciliation(path);
                    }
                }
                (AppEventSource::Recorder, AppEventKind::Error(message)) => {
                    error!("Recorder event: {message}");
                    // TODO: implement recorder error handling (e.g., restart recorder or update UI)
                }
                (AppEventSource::Transcriber, AppEventKind::Error(message)) => {
                    error!("Transcriber event: {message}");
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
                (AppEventSource::Ui, AppEventKind::UiSubmitText(text)) => {
                    self.submit_text(text);
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
}
