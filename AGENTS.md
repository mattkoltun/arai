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
- `transcriber.rs` — Resamples to 16kHz mono, 3s windows with 0.25s overlap, runs Whisper model with anti-hallucination params
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

# Repository Guidelines

## Project Structure & Module Organization
- Binary entry point is `src/main.rs`; keep core logic in small modules under `src/` (e.g., `src/lib.rs` or feature folders) and import into `main.rs`.
- Place integration tests in `tests/` and fixtures under `tests/fixtures/` if needed; Rust build artifacts live in `target/` and should not be checked in.
- Add scripts or examples under `examples/` when demonstrating usage.

## Build, Test, and Development Commands
- `cargo fmt` — format all Rust sources per rustfmt defaults.
- `cargo clippy --all-targets --all-features -D warnings` — lint and fail on warnings.
- `cargo test` — run unit and integration tests.
- `cargo run` — build and execute the binary in debug mode.
- `cargo build --release` — produce an optimized binary in `target/release/`.

## Coding Style & Naming Conventions
- Follow Rust defaults: 4-space indentation, snake_case for modules/functions/variables, CamelCase for types and traits, and SCREAMING_SNAKE_CASE for constants.
- Prefer small, testable functions; keep `main.rs` thin and move reusable logic to a library module.
- Add `///` doc comments to all public structs, impl blocks, and functions. Update existing doc comments when changing behavior.
- Run `cargo fmt` before commits; treat clippy warnings as errors.

## Testing Guidelines
- Write unit tests alongside code in the same file using `#[cfg(test)]`; use `tests/` for integration tests that exercise binaries end-to-end.
- Name tests with behavior-first labels (e.g., `handles_empty_input`, `parses_config_file`).
- Aim for meaningful coverage of edge cases and error paths; avoid hidden global state in tests.
- For fast targeted checks, run module tests directly:
  - `cargo test agent::tests`
  - `cargo test app_state::tests`
  - `cargo test config::tests`
  - `cargo test logger::tests`
  - `cargo test transcriber::tests`
  - `cargo test stdin_listener::tests`
- UI layout/components and microphone-recorder specific behavior are out of scope for routine unit tests in this project.

## Commit & Pull Request Guidelines
- Use clear, present-tense commit messages; Conventional Commits are preferred (`feat: add config loader`, `fix: handle empty args`).
- Keep commits focused and minimal; include formatting/lint changes with related code changes when possible.
- PRs should describe what changed, why, and how to verify (commands run, screenshots if user-facing behavior changes).
- Link issues when available and call out breaking changes explicitly.

## Security & Configuration Tips
- Do not commit secrets or tokens; use environment variables or `.env` files excluded via `.gitignore`.
- Consider running `cargo audit` locally before release to catch vulnerable dependencies.
