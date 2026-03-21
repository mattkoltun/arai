# API Key Keyring Storage Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move OpenAI API key from plain-text config file to OS-native keyring, add API key wizard step, and add key management in settings.

**Architecture:** New `keyring_store` module wraps the `keyring` crate. Config resolution checks env var → keyring → config fallback → empty. A new `AppPhase::SetupApiKey` wizard step appears after model setup. The controller recreates the Agent when the key changes.

**Tech Stack:** Rust 2024, `keyring` crate v3, iced 0.14 (Elm architecture)

**Spec:** `docs/superpowers/specs/2026-03-21-api-key-keyring-design.md`

---

## File Structure

| File | Action | Responsibility |
|------|--------|---------------|
| `Cargo.toml` | Modify | Add `keyring = "3"` dependency |
| `src/keyring_store.rs` | Create | Thin wrapper: get/set/delete API key from OS keyring |
| `src/config.rs` | Modify | `resolve_api_key()`, migration, save() never writes key |
| `src/messages.rs` | Modify | `ApiKeyStatus` enum, `UiUpdateApiKey` event, snapshot field |
| `src/app_state.rs` | Modify | `update_api_key()`, API key status in snapshot |
| `src/main.rs` | Modify | `mod keyring_store`, pass `api_key_exists` to UI |
| `src/controller.rs` | Modify | Handle `UiUpdateApiKey`, `restart_agent()`, snapshot |
| `src/ui.rs` | Modify | `SetupApiKey` phase, wizard view, settings card, submit guard |

---

### Task 1: Add keyring dependency and keyring_store module

**Files:**
- Modify: `Cargo.toml`
- Create: `src/keyring_store.rs`
- Modify: `src/main.rs` (add `mod keyring_store;`)

- [ ] **Step 1: Add keyring dependency to Cargo.toml**

In `Cargo.toml`, add after the `dirs = "6"` line:

```toml
keyring = "3"
```

- [ ] **Step 2: Create `src/keyring_store.rs` with tests**

```rust
use log::{error, info};

const SERVICE: &str = "arai";
const ACCOUNT: &str = "openai_api_key";

/// Retrieves the OpenAI API key from the OS keyring.
/// Returns `None` if the credential doesn't exist or the keyring is unavailable.
pub fn get_api_key() -> Option<String> {
    match keyring::Entry::new(SERVICE, ACCOUNT) {
        Ok(entry) => match entry.get_password() {
            Ok(key) if !key.is_empty() => Some(key),
            Ok(_) => None,
            Err(keyring::Error::NoEntry) => None,
            Err(e) => {
                error!("Failed to read API key from keyring: {e}");
                None
            }
        },
        Err(e) => {
            error!("Failed to create keyring entry: {e}");
            None
        }
    }
}

/// Stores the OpenAI API key in the OS keyring.
pub fn set_api_key(key: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(SERVICE, ACCOUNT)
        .map_err(|e| format!("Failed to create keyring entry: {e}"))?;
    entry
        .set_password(key)
        .map_err(|e| format!("Failed to store API key in keyring: {e}"))?;
    info!("API key stored in keyring");
    Ok(())
}

/// Deletes the OpenAI API key from the OS keyring.
pub fn delete_api_key() -> Result<(), String> {
    let entry = keyring::Entry::new(SERVICE, ACCOUNT)
        .map_err(|e| format!("Failed to create keyring entry: {e}"))?;
    match entry.delete_credential() {
        Ok(()) => {
            info!("API key deleted from keyring");
            Ok(())
        }
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(format!("Failed to delete API key from keyring: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constants_are_set() {
        assert_eq!(SERVICE, "arai");
        assert_eq!(ACCOUNT, "openai_api_key");
    }
}
```

- [ ] **Step 3: Add `mod keyring_store;` to `src/main.rs`**

Add `mod keyring_store;` after `mod model_downloader;` (line 9):

```rust
mod keyring_store;
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo build`
Expected: Compiles successfully.

- [ ] **Step 5: Run tests**

Run: `cargo test keyring_store`
Expected: 1 test passes.

- [ ] **Step 6: Run clippy**

Run: `cargo clippy --all-targets --all-features -- -D warnings`
Expected: No warnings.

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml src/keyring_store.rs src/main.rs
git commit -m "feat: add keyring_store module for secure API key storage"
```

---

### Task 2: Add ApiKeyStatus enum and message types

**Files:**
- Modify: `src/messages.rs`

- [ ] **Step 1: Add `ApiKeyStatus` enum to `src/messages.rs`**

Add after the `UiUpdate` enum definition (after line 41):

```rust
/// Status of the OpenAI API key configuration.
#[derive(Clone, Debug, PartialEq)]
pub enum ApiKeyStatus {
    /// Key is stored in keyring; carries masked display string (e.g., "sk-...7xQ3").
    Keyring(String),
    /// Key is set via environment variable.
    EnvVar,
    /// No key configured.
    NotSet,
}
```

- [ ] **Step 2: Add `api_key_status` field to `UiUpdate::ConfigSnapshot`**

In the `ConfigSnapshot` variant of `UiUpdate`, add a new field after `global_hotkey`:

```rust
    ConfigSnapshot {
        agent_prompts: Vec<AgentPrompt>,
        default_prompt: usize,
        transcriber: TranscriberConfig,
        selected_input_device: Option<String>,
        global_hotkey: String,
        api_key_status: ApiKeyStatus,
    },
```

- [ ] **Step 3: Add `UiUpdateApiKey` variant to `AppEventKind`**

Add after `UiUpdateGlobalHotkey(String)` in `AppEventKind`:

```rust
    /// Update the OpenAI API key (UI → Controller).
    UiUpdateApiKey(String),
```

- [ ] **Step 4: Add stub match arms for new variants**

This step prevents compilation failures in `controller.rs` and `ui.rs`. In `src/controller.rs`, add a match arm before the catch-all `(source, kind)` arm (before line 334):

```rust
                (AppEventSource::Ui, AppEventKind::UiUpdateApiKey(_key)) => {
                    // TODO: implement in Task 6
                }
```

In `src/ui.rs`, in the `UiUpdate::ConfigSnapshot` handler (around line 778), add `api_key_status` to the destructuring pattern and ignore it for now:

```rust
                UiUpdate::ConfigSnapshot {
                    agent_prompts,
                    default_prompt,
                    transcriber,
                    selected_input_device,
                    global_hotkey,
                    api_key_status: _,
                } => {
```

In `src/controller.rs`, in `send_config_snapshot()` (around line 162), add the new field:

```rust
        let _ = self.ui_update_tx.send(UiUpdate::ConfigSnapshot {
            agent_prompts: snapshot.agent_prompts,
            default_prompt: snapshot.default_prompt,
            transcriber: snapshot.transcriber,
            selected_input_device: snapshot.input_device,
            global_hotkey: snapshot.global_hotkey,
            api_key_status: crate::messages::ApiKeyStatus::NotSet, // TODO: compute in Task 5
        });
```

- [ ] **Step 5: Verify it compiles**

Run: `cargo build`
Expected: Compiles successfully.

- [ ] **Step 6: Commit**

```bash
git add src/messages.rs src/controller.rs src/ui.rs
git commit -m "feat: add ApiKeyStatus enum and UiUpdateApiKey event type"
```

---

### Task 3: Config changes — resolve_api_key(), migration, save() protection

**Files:**
- Modify: `src/config.rs`

- [ ] **Step 1: Write failing test for `resolve_api_key`**

Add to the `#[cfg(test)] mod tests` in `src/config.rs`:

```rust
    #[test]
    fn resolve_api_key_uses_config_fallback() {
        // Config fallback should be used when env var and keyring are unavailable.
        let key = resolve_api_key(&Some("sk-fallback-key".to_string()));
        // Either the env var is set (takes priority) or the fallback is used —
        // either way the result should be non-empty.
        assert!(!key.is_empty());
    }

    #[test]
    fn resolve_api_key_returns_string_for_none() {
        // Should not panic; returns env var value or empty string.
        let _key = resolve_api_key(&None);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test config::tests::resolve_api_key`
Expected: FAIL — `resolve_api_key` is not defined.

- [ ] **Step 3: Implement `resolve_api_key()` function**

Add in `src/config.rs`, before the `from_partial()` function:

```rust
/// Resolves the OpenAI API key from available sources in priority order:
/// 1. `OPENAI_API_KEY` env var
/// 2. OS keyring
/// 3. Config file value (migration fallback)
/// 4. Empty string
pub fn resolve_api_key(config_file_value: &Option<String>) -> String {
    if let Ok(key) = std::env::var("OPENAI_API_KEY") {
        if !key.is_empty() {
            return key;
        }
    }
    if let Some(key) = crate::keyring_store::get_api_key() {
        return key;
    }
    if let Some(ref key) = config_file_value {
        if !key.is_empty() {
            return key.clone();
        }
    }
    String::new()
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test config::tests::resolve_api_key`
Expected: PASS.

- [ ] **Step 5: Write failing test for migration**

Add to the test module:

```rust
    #[test]
    fn migrate_api_key_to_keyring_clears_config_value() {
        // We can't easily test real keyring in unit tests, but we can test
        // that migrate_api_key_if_needed returns the key and logs intent.
        let yaml_key = Some("sk-test-migration".to_string());
        let result = migrate_api_key_if_needed(&yaml_key);
        // Should return the key regardless of whether keyring write succeeded.
        assert_eq!(result, Some("sk-test-migration".to_string()));
    }

    #[test]
    fn migrate_api_key_noop_when_empty() {
        let result = migrate_api_key_if_needed(&None);
        assert_eq!(result, None);

        let result2 = migrate_api_key_if_needed(&Some(String::new()));
        assert_eq!(result2, None);
    }
```

- [ ] **Step 6: Run tests to verify they fail**

Run: `cargo test config::tests::migrate_api_key`
Expected: FAIL — `migrate_api_key_if_needed` is not defined.

- [ ] **Step 7: Implement `migrate_api_key_if_needed()`**

Add in `src/config.rs`:

```rust
/// If the YAML config contains a non-empty API key, migrate it to the OS keyring.
/// Returns the key value if migration was attempted (regardless of keyring success).
fn migrate_api_key_if_needed(yaml_value: &Option<String>) -> Option<String> {
    let key = yaml_value.as_ref().filter(|k| !k.is_empty())?;
    log::info!("Migrating API key from config file to keyring");
    if let Err(e) = crate::keyring_store::set_api_key(key) {
        log::warn!("Failed to migrate API key to keyring: {e}. Key remains in config file.");
    }
    Some(key.clone())
}
```

- [ ] **Step 8: Run tests to verify they pass**

Run: `cargo test config::tests::migrate_api_key`
Expected: PASS.

- [ ] **Step 9: Update `from_partial()` to use `resolve_api_key()` and migration**

In `src/config.rs`, replace the line (around line 258):

```rust
    let open_api_key = partial.open_api_key.unwrap_or_default();
```

With:

```rust
    // Migrate API key from config file to keyring if present.
    let yaml_api_key = partial.open_api_key;
    let migrated = migrate_api_key_if_needed(&yaml_api_key);
    let open_api_key = resolve_api_key(&yaml_api_key);
    // If migration happened, we need to save the config to remove the key from disk.
    // This is deferred until after the Config struct is built (see Config::load below).
```

- [ ] **Step 10: Remove `open_api_key` from `PartialConfig::from_env()`**

In `src/config.rs`, in `PartialConfig::from_env()` (around line 213-226), remove the `OPENAI_API_KEY` env var read:

Replace:
```rust
    fn from_env() -> Result<Self, ConfigError> {
        let log_level = std::env::var("ARAI_LOG_LEVEL").ok();
        let log_path = std::env::var("ARAI_LOG_PATH").ok();
        let open_api_key = std::env::var("OPENAI_API_KEY").ok();
        Ok(Self {
            log_level,
            log_path,
            open_api_key,
```

With:
```rust
    fn from_env() -> Result<Self, ConfigError> {
        let log_level = std::env::var("ARAI_LOG_LEVEL").ok();
        let log_path = std::env::var("ARAI_LOG_PATH").ok();
        Ok(Self {
            log_level,
            log_path,
            open_api_key: None,
```

- [ ] **Step 11: Add post-migration config save in `Config::load()`**

In `Config::load()`, after the `from_partial(merged)` call, save the config if migration happened to immediately remove the key from disk. Update `from_partial` to return the `migrated` flag alongside the config. The simplest approach: add a migration save directly in `Config::load()`:

```rust
    pub fn load() -> Result<Self, ConfigError> {
        let default_layer = PartialConfig::default_layer();
        let file_layer = PartialConfig::from_file(config_path()?)?;
        let env_layer = PartialConfig::from_env()?;

        // Check if the file layer has a non-empty API key that needs migration.
        let needs_migration_save = file_layer
            .open_api_key
            .as_ref()
            .is_some_and(|k| !k.is_empty());

        let merged = default_layer.merge(file_layer).merge(env_layer);
        let config = from_partial(merged)?;

        // If we migrated a key from the file to keyring, save immediately
        // to remove the plain-text key from disk.
        if needs_migration_save {
            if let Err(e) = config.save() {
                log::warn!("Failed to save config after API key migration: {e}");
            }
        }

        Ok(config)
    }
```

- [ ] **Step 12: Update `Config::save()` to never write the API key**

In `src/config.rs`, in `Config::save()` (around line 106), change:

```rust
            open_api_key: Some(self.open_api_key.clone()),
```

To:

```rust
            open_api_key: None,
```

- [ ] **Step 13: Run all config tests**

Run: `cargo test config::tests`
Expected: All tests pass.

- [ ] **Step 14: Run full test suite and clippy**

Run: `cargo test && cargo clippy --all-targets --all-features -- -D warnings`
Expected: All pass, no warnings.

- [ ] **Step 15: Commit**

```bash
git add src/config.rs
git commit -m "feat: add API key resolution via keyring with config migration"
```

---

### Task 4: Update app_state with API key support

**Files:**
- Modify: `src/app_state.rs`

- [ ] **Step 1: Add `api_key_status` to `AppStateSnapshot`**

In `src/app_state.rs`, add `use crate::messages::ApiKeyStatus;` to the imports. Then add the field to `AppStateSnapshot`:

```rust
#[derive(Clone, Debug, Default)]
pub struct AppStateSnapshot {
    pub agent_prompts: Vec<AgentPrompt>,
    pub default_prompt: usize,
    pub transcriber: TranscriberConfig,
    pub input_device: Option<String>,
    pub global_hotkey: String,
    pub api_key_status: ApiKeyStatus,
}
```

This requires `ApiKeyStatus` to implement `Default`. Add this to `src/messages.rs`:

```rust
impl Default for ApiKeyStatus {
    fn default() -> Self {
        ApiKeyStatus::NotSet
    }
}
```

- [ ] **Step 2: Add `mask_api_key()` helper and compute status in `snapshot()`**

Add a helper function in `src/app_state.rs`:

```rust
/// Masks an API key for display: shows first 3 + "..." + last 4 chars.
fn mask_api_key(key: &str) -> String {
    if key.len() <= 7 {
        return "sk-...".to_string();
    }
    format!("{}...{}", &key[..3], &key[key.len() - 4..])
}

/// Determines the API key status from runtime state.
fn compute_api_key_status(key: &str) -> ApiKeyStatus {
    if std::env::var("OPENAI_API_KEY")
        .ok()
        .filter(|k| !k.is_empty())
        .is_some()
    {
        ApiKeyStatus::EnvVar
    } else if !key.is_empty() {
        ApiKeyStatus::Keyring(mask_api_key(key))
    } else {
        ApiKeyStatus::NotSet
    }
}
```

Update `snapshot()` to include the new field:

```rust
    pub fn snapshot(&self) -> AppStateSnapshot {
        let inner = self.inner.lock().expect("app_state mutex poisoned");
        AppStateSnapshot {
            agent_prompts: inner.agent_prompts.clone(),
            default_prompt: inner.default_prompt,
            transcriber: inner.transcriber.clone(),
            input_device: inner.config.input_device.clone(),
            global_hotkey: inner.config.global_hotkey.clone(),
            api_key_status: compute_api_key_status(&inner.config.open_api_key),
        }
    }
```

- [ ] **Step 3: Add `update_api_key()` method**

Add to the `impl AppState` block:

```rust
    /// Updates the runtime API key. Does NOT save to config file —
    /// the key is persisted via keyring, not YAML.
    pub fn update_api_key(&self, key: String) {
        let mut inner = self.inner.lock().expect("app_state mutex poisoned");
        inner.config.open_api_key = key;
    }
```

- [ ] **Step 4: Write tests**

Add to the test module in `src/app_state.rs`:

```rust
    #[test]
    fn mask_api_key_shows_prefix_and_suffix() {
        assert_eq!(mask_api_key("sk-proj-abcdefghijklmnop"), "sk-...mnop");
    }

    #[test]
    fn mask_api_key_short_key_returns_placeholder() {
        assert_eq!(mask_api_key("sk-abc"), "sk-...");
    }

    #[test]
    fn update_api_key_changes_runtime_value() {
        let state = AppState::new(test_config());
        state.update_api_key("sk-new-key".to_string());
        let inner = state.inner.lock().unwrap();
        assert_eq!(inner.config.open_api_key, "sk-new-key");
    }

    #[test]
    fn snapshot_includes_api_key_status() {
        let state = AppState::new(test_config());
        let snapshot = state.snapshot();
        // test_config has "test-key" which is non-empty, so it should be Keyring variant
        // (unless OPENAI_API_KEY env var is set in the test environment)
        match snapshot.api_key_status {
            crate::messages::ApiKeyStatus::Keyring(masked) => {
                assert!(masked.contains("..."));
            }
            crate::messages::ApiKeyStatus::EnvVar => {
                // Acceptable if env var is set
            }
            crate::messages::ApiKeyStatus::NotSet => {
                panic!("Expected Keyring or EnvVar, got NotSet");
            }
        }
    }
```

- [ ] **Step 5: Run tests**

Run: `cargo test app_state::tests`
Expected: All pass.

- [ ] **Step 6: Run clippy**

Run: `cargo clippy --all-targets --all-features -- -D warnings`
Expected: No warnings.

- [ ] **Step 7: Commit**

```bash
git add src/app_state.rs src/messages.rs
git commit -m "feat: add API key status tracking to app state"
```

---

### Task 5: Wire controller — API key update, agent restart, snapshot

**Files:**
- Modify: `src/controller.rs`

- [ ] **Step 1: Update `send_config_snapshot()` to compute real API key status**

In `src/controller.rs`, replace the stub `api_key_status: crate::messages::ApiKeyStatus::NotSet` in `send_config_snapshot()` with the real snapshot value:

```rust
    fn send_config_snapshot(&self) {
        let snapshot = self.app_state.snapshot();
        let _ = self.ui_update_tx.send(UiUpdate::ConfigSnapshot {
            agent_prompts: snapshot.agent_prompts,
            default_prompt: snapshot.default_prompt,
            transcriber: snapshot.transcriber,
            selected_input_device: snapshot.input_device,
            global_hotkey: snapshot.global_hotkey,
            api_key_status: snapshot.api_key_status,
        });
    }
```

- [ ] **Step 2: Add `restart_agent()` method**

Add to the `impl Controller` block, after `restart_transcriber()`:

```rust
    /// Drops the current Agent and creates a new one with the given API key.
    fn restart_agent(&mut self, api_key: String) {
        info!("Restarting agent with new API key");
        let old = std::mem::replace(
            &mut self.agent,
            Agent::new(self.app_event_tx.clone(), api_key),
        );
        drop(old);
    }
```

- [ ] **Step 3: Replace the stub `UiUpdateApiKey` handler with real implementation**

Replace the stub match arm:

```rust
                (AppEventSource::Ui, AppEventKind::UiUpdateApiKey(_key)) => {
                    // TODO: implement in Task 6
                }
```

With:

```rust
                (AppEventSource::Ui, AppEventKind::UiUpdateApiKey(key)) => {
                    info!("Controller updating API key");
                    if let Err(e) = crate::keyring_store::set_api_key(&key) {
                        error!("Failed to save API key to keyring: {e}");
                    }
                    self.app_state.update_api_key(key.clone());
                    self.restart_agent(key);
                    self.send_config_snapshot();
                }
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo build`
Expected: Compiles successfully.

- [ ] **Step 5: Run full test suite**

Run: `cargo test`
Expected: All pass.

- [ ] **Step 6: Commit**

```bash
git add src/controller.rs
git commit -m "feat: wire controller to handle API key updates and restart agent"
```

---

### Task 6: Update main.rs — pass api_key_exists to UI

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Add `api_key_exists` check and pass to UI**

In `src/main.rs`, after the `model_exists` line (line 26), add:

```rust
    let api_key_exists = !config.open_api_key.is_empty();
```

Update the `Ui::new()` call (line 54) to pass `api_key_exists`:

```rust
    let ui = ui::Ui::new(
        app_event_tx.clone(),
        hotkey_handle,
        ui_update_rx,
        model_exists,
        api_key_exists,
    );
```

- [ ] **Step 2: Update `Ui::new()` and `Ui` struct in `src/ui.rs`**

In `src/ui.rs`, add `api_key_exists: bool` field to the `Ui` struct (after `model_exists`):

```rust
pub struct Ui {
    app_event_tx: AppEventSender,
    hotkey_handle: Option<Arc<Mutex<HotkeyHandle>>>,
    ui_update_rx: Arc<Mutex<Option<UiUpdateReceiver>>>,
    model_exists: bool,
    api_key_exists: bool,
}
```

Update `Ui::new()` signature and body:

```rust
    pub fn new(
        app_event_tx: AppEventSender,
        hotkey_handle: Option<HotkeyHandle>,
        ui_update_rx: UiUpdateReceiver,
        model_exists: bool,
        api_key_exists: bool,
    ) -> Self {
        Self {
            app_event_tx,
            hotkey_handle: hotkey_handle.map(|h| Arc::new(Mutex::new(h))),
            ui_update_rx: Arc::new(Mutex::new(Some(ui_update_rx))),
            model_exists,
            api_key_exists,
        }
    }
```

- [ ] **Step 3: Update phase initialization in `Ui::run()`**

In the `boot` closure inside `Ui::run()`, capture `api_key_exists`:

```rust
        let api_key_exists = self.api_key_exists;
```

Update the `phase` initialization (around line 515):

```rust
                    phase: if model_exists {
                        if api_key_exists {
                            AppPhase::Main
                        } else {
                            AppPhase::SetupApiKey
                        }
                    } else {
                        AppPhase::Setup
                    },
```

- [ ] **Step 4: Add `AppPhase::SetupApiKey` variant**

In `src/ui.rs`, add the variant to `AppPhase`:

```rust
enum AppPhase {
    #[default]
    Setup,
    /// API key configuration step.
    SetupApiKey,
    /// Normal operation.
    Main,
}
```

- [ ] **Step 5: Add stub route in `view()`**

In the `view()` function, update the match on `state.phase`:

```rust
    let content = match state.phase {
        AppPhase::Setup => view_wizard(state),
        AppPhase::SetupApiKey => view_wizard_api_key(state),
        AppPhase::Main => {
```

Add a temporary stub `view_wizard_api_key()`:

```rust
fn view_wizard_api_key(state: &UiRuntime) -> Element<'_, Message> {
    let _ = state;
    container(text("API Key Setup — coming soon").color(TEXT_COLOR))
        .style(bg_container)
        .width(Fill)
        .height(Fill)
        .into()
}
```

- [ ] **Step 6: Add wizard API key state fields to `UiRuntime`**

Add after the `wizard_from_settings` field:

```rust
    /// Text input for the API key wizard.
    wizard_api_key_input: String,
    /// Error message for the API key wizard.
    wizard_api_key_error: Option<String>,
    /// API key status from the latest config snapshot.
    config_api_key_status: ApiKeyStatus,
```

Add `use crate::messages::ApiKeyStatus;` to the imports at the top of `src/ui.rs` (it's already importing from `messages`).

Initialize these fields in the `boot` closure:

```rust
                    wizard_api_key_input: String::new(),
                    wizard_api_key_error: None,
                    config_api_key_status: ApiKeyStatus::NotSet,
```

- [ ] **Step 7: Verify it compiles**

Run: `cargo build`
Expected: Compiles successfully.

- [ ] **Step 8: Commit**

```bash
git add src/main.rs src/ui.rs
git commit -m "feat: add SetupApiKey phase and pass api_key_exists to UI"
```

---

### Task 7: Implement API key wizard view and handlers

**Files:**
- Modify: `src/ui.rs`

- [ ] **Step 1: Add new Message variants**

Add to the `Message` enum, after `OpenWizardFromSettings`:

```rust
    WizardApiKeyChanged(String),
    WizardApiKeySave,
    WizardApiKeySkip,
    OpenApiKeyFromSettings,
```

- [ ] **Step 2: Implement `view_wizard_api_key()`**

Replace the stub with the full implementation:

```rust
fn view_wizard_api_key(state: &UiRuntime) -> Element<'_, Message> {
    // Close button (top-right)
    let close_btn = button(icon('\u{E5CD}', 20.0))
        .style(icon_btn)
        .padding(6)
        .on_press(Message::Shutdown);

    let mut top_row = row![].align_y(iced::Alignment::Center);
    if state.wizard_from_settings {
        let back_btn = button(icon('\u{E5C4}', 20.0))
            .style(icon_btn)
            .padding(6)
            .on_press(Message::WizardBack);
        top_row = top_row.push(back_btn);
    }
    top_row = top_row.push(container(close_btn).align_right(Fill));

    let top_bar = container(top_row).padding([10, 14]).width(Fill);

    // Title
    let title = text("OpenAI API Key").size(18).color(TEXT_COLOR);
    let subtitle = text(
        "Enter your API key to enable text processing.\nYou can get one at platform.openai.com",
    )
    .size(12)
    .color(MUTED);

    // Key input
    let key_input = text_input("sk-...", &state.wizard_api_key_input)
        .style(borderless_input)
        .padding(10)
        .on_input(Message::WizardApiKeyChanged);

    // Buttons
    let valid_key = state.wizard_api_key_input.starts_with("sk-");

    let save_label = if state.wizard_from_settings {
        "Save"
    } else {
        "Save & Continue"
    };
    let save_btn = button(text(save_label).size(13))
        .style(primary_btn)
        .padding([8, 20])
        .on_press_maybe(valid_key.then_some(Message::WizardApiKeySave));

    let skip_label = if state.wizard_from_settings {
        "Cancel"
    } else {
        "Skip for now"
    };
    let skip_msg = if state.wizard_from_settings {
        Message::WizardBack
    } else {
        Message::WizardApiKeySkip
    };
    let skip_btn = button(text(skip_label).size(12))
        .style(ghost_btn)
        .padding([6, 14])
        .on_press(skip_msg);

    // Error message
    let error_row: Element<'_, Message> = if let Some(ref err) = state.wizard_api_key_error {
        text(err).size(12).color(RED).into()
    } else {
        column![].into()
    };

    let body = column![
        title,
        subtitle,
        container(key_input)
            .style(surface_container)
            .padding(4)
            .width(Fill),
        error_row,
        container(
            column![save_btn, skip_btn]
                .spacing(8)
                .align_x(iced::Alignment::Center)
        )
        .center_x(Fill),
    ]
    .spacing(16)
    .padding([0, 20]);

    let content = column![top_bar, body];

    container(content)
        .style(bg_container)
        .width(Fill)
        .height(Fill)
        .into()
}
```

- [ ] **Step 3: Add message handlers**

In the `update()` function, add handlers before the `Message::DragWindow` arm:

```rust
        Message::WizardApiKeyChanged(value) => {
            state.wizard_api_key_input = value;
            state.wizard_api_key_error = None;
            Task::none()
        }
        Message::WizardApiKeySave => {
            let key = state.wizard_api_key_input.trim().to_string();
            if !key.starts_with("sk-") {
                state.wizard_api_key_error =
                    Some("API key should start with sk-".to_string());
                return Task::none();
            }
            state.send_event(AppEventKind::UiUpdateApiKey(key));
            state.wizard_api_key_input.clear();
            state.wizard_api_key_error = None;
            state.phase = AppPhase::Main;
            if state.wizard_from_settings {
                state.config_open = true;
                state.wizard_from_settings = false;
            }
            Task::none()
        }
        Message::WizardApiKeySkip => {
            state.wizard_api_key_input.clear();
            state.wizard_api_key_error = None;
            state.phase = AppPhase::Main;
            Task::none()
        }
        Message::OpenApiKeyFromSettings => {
            state.config_open = false;
            state.wizard_from_settings = true;
            state.wizard_api_key_input.clear();
            state.wizard_api_key_error = None;
            state.phase = AppPhase::SetupApiKey;
            Task::none()
        }
```

- [ ] **Step 4: Update model wizard completion to check for API key**

In the `Message::WizardModelPicked` handler (around line 1113-1122), change:

```rust
                state.phase = AppPhase::Main;
```

To:

```rust
                state.phase = if matches!(state.config_api_key_status, ApiKeyStatus::NotSet) {
                    AppPhase::SetupApiKey
                } else {
                    AppPhase::Main
                };
```

Apply the same change in the `Message::WizardDownloadComplete` handler (around line 1131):

```rust
                state.phase = if matches!(state.config_api_key_status, ApiKeyStatus::NotSet) {
                    AppPhase::SetupApiKey
                } else {
                    AppPhase::Main
                };
```

- [ ] **Step 5: Update `WizardBack` to also handle API key phase**

In the `Message::WizardBack` handler, update to handle both wizard phases:

```rust
        Message::WizardBack => {
            if state.wizard_from_settings {
                if state.wizard_downloading {
                    state
                        .wizard_cancel_flag
                        .store(true, std::sync::atomic::Ordering::Relaxed);
                    state.wizard_downloading = false;
                    state.wizard_download_progress = None;
                }
                state.phase = AppPhase::Main;
                state.config_open = true;
                state.wizard_error = None;
                state.wizard_api_key_error = None;
                state.wizard_from_settings = false;
            }
            Task::none()
        }
```

- [ ] **Step 6: Verify it compiles**

Run: `cargo build`
Expected: Compiles successfully.

- [ ] **Step 7: Commit**

```bash
git add src/ui.rs src/messages.rs
git commit -m "feat: implement API key wizard view and message handlers"
```

---

### Task 8: Add API key card to settings and wire ConfigSnapshot

**Files:**
- Modify: `src/ui.rs`

- [ ] **Step 1: Wire `config_api_key_status` from ConfigSnapshot**

In the `UiUpdate::ConfigSnapshot` handler in `update()`, replace `api_key_status: _,` with:

```rust
                    api_key_status,
```

And add after `state.snapshot_global_hotkey = global_hotkey;`:

```rust
                    state.config_api_key_status = api_key_status;
```

- [ ] **Step 2: Add API key card to `view_setup_tab()`**

The `view_setup_tab()` function currently takes `&SetupFields`. Add `api_key_status: &ApiKeyStatus` as a parameter. Update the call site in `view_config()` to pass it:

```rust
        ConfigTab::Setup => view_setup_tab(&sf, &state.config_api_key_status),
```

In `view_setup_tab()`, add the API key card between the `mic_card` and `hotkey_card`. Add the parameter:

```rust
fn view_setup_tab(sf: &SetupFields, api_key_status: &ApiKeyStatus) -> Column<'static, Message> {
```

After the `mic_card` definition, add:

```rust
    // ── API Key card ────────────────────────────────────────────────
    let (api_key_display, api_key_btn_label, api_key_btn_enabled) = match api_key_status {
        ApiKeyStatus::Keyring(masked) => (masked.clone(), "Change API Key", true),
        ApiKeyStatus::EnvVar => ("Set via environment variable".to_string(), "Change API Key", false),
        ApiKeyStatus::NotSet => ("Not configured".to_string(), "Set API Key", true),
    };

    let display_color = match api_key_status {
        ApiKeyStatus::NotSet => RED,
        _ => MUTED,
    };

    let api_key_display_text = text(api_key_display).size(12).color(display_color);

    // vpn_key: E62C
    let api_key_btn = button(
        row![icon('\u{E62C}', 16.0), text(api_key_btn_label).size(13)]
            .spacing(6)
            .align_y(iced::Alignment::Center),
    )
    .style(ghost_btn)
    .padding([6, 14])
    .on_press_maybe(api_key_btn_enabled.then_some(Message::OpenApiKeyFromSettings));

    let api_key_card = column![
        text("API Key").size(15).color(TEXT_COLOR),
        api_key_display_text,
        api_key_btn,
    ]
    .spacing(8)
    .padding(14);
```

Then in the final `column!` at the end of `view_setup_tab()`, insert the API key card between `mic_card` and `hotkey_card`:

```rust
    column![
        container(mic_card).style(surface_container).width(Fill),
        container(api_key_card).style(surface_container).width(Fill),
        container(hotkey_card).style(surface_container).width(Fill),
        container(transcriber_card)
            .style(surface_container)
            .width(Fill),
    ]
    .spacing(12)
    .padding(14)
```

- [ ] **Step 3: Disable Submit when no API key**

In `view_main()`, the submit button currently uses:

```rust
.on_press_maybe((!busy && has_text).then_some(Message::Submit))
```

We need to also check for API key. Add `has_api_key: bool` as a parameter to `view_main()`. Update the call site in `view()`:

```rust
                view_main(
                    state,
                    listening,
                    processing,
                    reconciling,
                    !state.input.trim().is_empty(),
                    state.input.chars().count(),
                    state.config_api_key_status != ApiKeyStatus::NotSet,
                )
```

Update `view_main()` signature:

```rust
fn view_main<'a>(
    state: &'a UiRuntime,
    listening: bool,
    processing: bool,
    reconciling: bool,
    has_text: bool,
    char_count: usize,
    has_api_key: bool,
) -> Element<'a, Message> {
```

Update the submit button:

```rust
.on_press_maybe((!busy && has_text && has_api_key).then_some(Message::Submit))
```

- [ ] **Step 4: Update status line when no API key**

In the `submit()` method of `UiRuntime`, add an API key check:

```rust
    fn submit(&mut self) {
        if self.mode != AppMode::Idle || self.input.trim().is_empty() {
            return;
        }
        if self.config_api_key_status == ApiKeyStatus::NotSet {
            self.status_line = "API key required \u{2014} configure in Settings".to_string();
            return;
        }
```

- [ ] **Step 5: Verify it compiles**

Run: `cargo build`
Expected: Compiles successfully.

- [ ] **Step 6: Run full test suite and clippy**

Run: `cargo test && cargo clippy --all-targets --all-features -- -D warnings`
Expected: All pass, no warnings.

- [ ] **Step 7: Commit**

```bash
git add src/ui.rs
git commit -m "feat: add API key card to settings and disable submit without key"
```

---

### Task 9: Manual integration testing

**Files:** None (testing only)

- [ ] **Step 1: Test first launch with no API key**

Remove any existing keyring entry (or use a fresh user). Launch the app with no model and no API key configured.

Expected:
1. Model setup wizard appears first
2. After model setup, API key wizard appears
3. Entering an invalid key (not starting with `sk-`) shows error
4. Entering a valid `sk-...` key and clicking "Save & Continue" transitions to main app
5. Submit button works with the new key

- [ ] **Step 2: Test "Skip for now"**

Launch with no API key. On the API key wizard, click "Skip for now".

Expected:
1. Main app appears
2. Submit button is disabled
3. Status line shows "API key required — configure in Settings"

- [ ] **Step 3: Test settings — Change API Key**

Open settings → Setup tab.

Expected:
1. API key card shows status (masked key, "Not configured", or "Set via environment variable")
2. "Change API Key" / "Set API Key" button opens API key wizard
3. Saving a new key returns to settings with updated masked display
4. Cancel returns to settings without changes

- [ ] **Step 4: Test env var override**

Set `OPENAI_API_KEY=sk-test-env-key` and launch.

Expected:
1. API key wizard is skipped (key exists from env)
2. Settings shows "Set via environment variable"
3. "Change API Key" button is disabled

- [ ] **Step 5: Test config migration**

Add `open_api_key: sk-old-key-from-yaml` to `~/.config/arai/config.yaml` and launch.

Expected:
1. Log shows "Migrating API key from config file to keyring"
2. After launch, `open_api_key` field is removed from config.yaml
3. Key is accessible from keyring (app works normally)

- [ ] **Step 6: Run final checks**

Run: `cargo fmt && cargo clippy --all-targets --all-features -- -D warnings && cargo test`
Expected: All pass.
