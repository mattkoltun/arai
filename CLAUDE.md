# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

ARAI is a voice-first prompt and writing assistant in Rust. It captures microphone audio, transcribes it locally via Whisper, then transforms the text through OpenAI's API. The result is polished text ready for use in agent prompts, emails, messages, etc.

## Build & Development Commands

```bash
cargo build                # debug build
cargo build --release      # optimized build
cargo run                  # build and run
cargo fmt                  # format all sources
cargo clippy --all-targets --all-features -- -D warnings  # lint (warnings = errors)
cargo test                 # run all tests
cargo test <module>::tests # run specific module tests, e.g. cargo test agent::tests
```

Available test modules: `agent::tests`, `app_state::tests`, `config::tests`, `logger::tests`, `transcriber::tests`, `stdin_listener::tests`. UI and recorder modules are out of scope for unit tests.

## Architecture

Multi-threaded event-driven architecture with mpsc channel message passing. All modules live flat under `src/`.

**Data flow:**
```
Recorder (cpal) → AudioChunk → Transcriber (whisper) → text → Agent (OpenAI) → polished text
                                                                      ↕
                                              Controller (event loop, 10ms poll)
                                                                      ↕
                                                               UI (iced, Elm-style)
```

**Key modules:**
- `main.rs` — Entry point; wires up config, logger, channels, and spawns threads
- `recorder.rs` — Audio capture via cpal (F32/I16/U16 formats), streams chunks over channel
- `transcriber.rs` — Resamples to 16kHz mono, 2s windows with 0.25s overlap, runs Whisper model
- `agent.rs` — OpenAI gpt-4o-mini calls with exponential backoff retry (429s, 5xx, timeouts)
- `controller.rs` — Central event loop bridging all components via AppEvent channel
- `ui.rs` — Iced GUI (480x620, Tokyo Night theme) with text editor, Listen/Submit/Copy buttons
- `config.rs` — Three-layer config merge: defaults → `~/.config/arai/config.yaml` → env vars (`ARAI_LOG_LEVEL`, `ARAI_LOG_PATH`, `ARAI_OPENAI_API_KEY`)
- `app_state.rs` — Shared mutable state (Arc<Mutex<>>)
- `messages.rs` — Event types (AudioChunk, AppEvent, AppEventKind)
- `channels.rs` — Type aliases for mpsc channels

**Whisper model:** `models/ggml-small.en.bin` (487 MB, gitignored)

## Coding Conventions

- Rust 2024 edition, 4-space indent
- snake_case (functions/modules), CamelCase (types/traits), SCREAMING_SNAKE_CASE (constants)
- Run `cargo fmt` before commits; clippy warnings are errors
- Conventional Commits: `feat:`, `fix:`, `test:`, `docs:`, etc.
- Unit tests go in `#[cfg(test)]` modules alongside code; name with behavior-first labels (e.g., `handles_empty_input`)
- Keep `main.rs` thin; move logic to dedicated modules

## Configuration

Config file at `~/.config/arai/config.yaml`. API key is required (via file or `ARAI_OPENAI_API_KEY` env var). Agent prompts list cannot be empty.
