# Changelog

All notable source code changes in this repository should be documented in this file.

The format is based on Keep a Changelog and uses an `Unreleased` section that should be updated alongside every source code change.

## [Unreleased]

### Added
- Added `CHANGELOG.md` and documented the requirement to record source code changes here.
- Added Linux support, including platform-conditional dependencies, Linux-friendly paths, Linux build CI, and Linux-specific UI/platform behavior adjustments.

### Changed
- Added and tuned a configurable silence threshold for transcription gating so quiet or silent input is skipped more reliably during live capture.
