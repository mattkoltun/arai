# Changelog

All notable source code changes in this repository should be documented in this file.

The format is based on Keep a Changelog and uses an `Unreleased` section that should be updated alongside every source code change.

## [Unreleased]

## [0.18.0] - 2026-03-25

## [0.18.0] - 2026-03-25

### Added
- Added `CHANGELOG.md` and documented the requirement to record source code changes here.
- Added Linux support, including platform-conditional dependencies, Linux-friendly paths, Linux build CI, and Linux-specific UI/platform behavior adjustments.
- Added automated release workflows: `release-prep` (version bump + changelog via workflow_dispatch), `auto-tag` (tags on merged release PRs), and origin verification in the release build.

### Changed
- Added and tuned a configurable silence threshold for transcription gating so quiet or silent input is skipped more reliably during live capture.
- Refactored application startup around a dedicated app bootstrap flow so config loading, channel creation, and controller/UI assembly are more standardized.
- Wrapped OpenAI instructions and editable input in explicit, separate sections so the model treats the submitted text only as source material to format, not as instructions to follow.
- Replaced the input footer character count with a token estimate and updated the recording metadata display to show persistent duration plus saved audio file size after recording stops.
- Reworked reconciliation so the controller routes saved recordings back through the existing transcriber worker, allowing Whisper model reuse instead of loading a separate reconciliation context per recording.
- Removed the reconciliation WAV round-trip by keeping finalized recordings in memory, passing them through the controller, and reconciling directly from PCM instead of writing and re-reading a temp file.

### Fixed
- Fixed Linux CI failures by aligning platform-specific UI code with conditional compilation so Linux builds pass with `-D warnings`.
