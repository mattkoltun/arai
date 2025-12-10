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
- Run `cargo fmt` before commits; treat clippy warnings as errors.

## Testing Guidelines
- Write unit tests alongside code in the same file using `#[cfg(test)]`; use `tests/` for integration tests that exercise binaries end-to-end.
- Name tests with behavior-first labels (e.g., `handles_empty_input`, `parses_config_file`).
- Aim for meaningful coverage of edge cases and error paths; avoid hidden global state in tests.

## Commit & Pull Request Guidelines
- Use clear, present-tense commit messages; Conventional Commits are preferred (`feat: add config loader`, `fix: handle empty args`).
- Keep commits focused and minimal; include formatting/lint changes with related code changes when possible.
- PRs should describe what changed, why, and how to verify (commands run, screenshots if user-facing behavior changes).
- Link issues when available and call out breaking changes explicitly.

## Security & Configuration Tips
- Do not commit secrets or tokens; use environment variables or `.env` files excluded via `.gitignore`.
- Consider running `cargo audit` locally before release to catch vulnerable dependencies.
