# Development

This guide covers local setup, running, building, and testing Arai.

## Prerequisites

- Rust stable toolchain (`cargo`, `rustc`) -- install via [rustup](https://rustup.rs/)
- OpenAI API key
- macOS (primary target; uses Metal for GPU acceleration and Keychain for key storage)

## Setup

1. Clone the repository:

```bash
git clone https://github.com/mkoltun/arai.git
cd arai
```

2. Set your OpenAI API key via one of:

   - **Environment variable:** `export OPENAI_API_KEY=sk-...`
   - **In-app settings:** enter it through the UI on first launch
   - **Config file:** `~/.config/arai/config.yaml` (will be migrated to Keychain automatically)

3. On first launch, Arai will prompt you to download a Whisper model.

## Run

```bash
cargo run
```

On first launch, Arai will prompt you to download a Whisper model and enter your OpenAI API key if they are not already configured.

## Build

```bash
cargo build           # debug build
cargo build --release # optimized build
```

To build a macOS app bundle (requires [cargo-bundle](https://github.com/burtonageo/cargo-bundle)):

```bash
cargo bundle --release
```

## Tests

Run all tests:

```bash
cargo test
```

Run module-specific tests:

```bash
cargo test agent::tests
cargo test app_state::tests
cargo test config::tests
cargo test logger::tests
cargo test transcriber::tests
cargo test stdin_listener::tests
```

UI and recorder modules are out of scope for unit tests.

## Lint / Format

```bash
cargo fmt
cargo clippy --all-targets --all-features -- -D warnings
```

Clippy warnings are treated as errors.

## Project Structure

All modules live flat under `src/`. See the main [README](./README.md) for configuration details and keybindings.
