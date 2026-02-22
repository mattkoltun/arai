# Development

This guide covers local setup, running, building, and testing ARAI.

## Prerequisites

- Rust stable toolchain (`cargo`, `rustc`)
- Whisper model file (default path: `models/ggml-small.en.bin`)
- OpenAI API key

## Setup

1. Clone the repository.
2. Create config file:

```bash
mkdir -p ~/.config/arai
cat > ~/.config/arai/config.yaml <<'YAML'
log_level: debug
log_path: /tmp/arai.log
open_api_key: YOUR_OPENAI_API_KEY
agent_prompts:
  - name: default
    instruction: Rewrite the user text for clarity and brevity while preserving meaning.
YAML
```

You can also provide the key via env var:

```bash
export ARAI_OPENAI_API_KEY=YOUR_OPENAI_API_KEY
```

## Run

```bash
cargo run
```

## Build

```bash
cargo build
cargo build --release
```

## Tests

Run all tests:

```bash
cargo test
```

Run module-specific tests (non-UI / non-recorder):

```bash
cargo test agent::tests
cargo test app_state::tests
cargo test config::tests
cargo test logger::tests
cargo test transcriber::tests
cargo test stdin_listener::tests
```

## Lint/Format

```bash
cargo fmt
cargo clippy --all-targets --all-features -- -D warnings
```
