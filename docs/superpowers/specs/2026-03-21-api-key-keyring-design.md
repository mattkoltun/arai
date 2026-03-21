# OpenAI API Key Configuration with Keyring Storage

## Overview

The OpenAI API key moves from plain-text storage in `config.yaml` to OS-native credential storage via the `keyring` crate (macOS Keychain, Linux Secret Service). A new wizard step appears after model setup on first launch if no key is configured. The key is also manageable from the Setup tab in settings, displayed as a masked preview with a "Change" button.

## Wizard Flow

### Two-Step Wizard

After model setup completes (download or browse), the app checks if an API key exists (keyring or env var). If not, a second wizard screen appears before entering the main app.

**Screen layout:**
- Title: "OpenAI API Key"
- Subtitle: "Enter your API key to enable text processing. You can get one at platform.openai.com"
- A text input for pasting the key
- "Save & Continue" button (disabled until input is non-empty)
- "Skip for now" link/button — enters main app without a key

**When accessed from settings** (via "Change API Key" button), the screen shows "Save" and "Cancel" instead of "Save & Continue" and "Skip for now".

**Validation:** Format-only check (non-empty, starts with `sk-` to cover `sk-proj-`, `sk-svcacct-`, etc.). No network validation — the agent already handles auth errors at call time with retry logic.

### Phase State Machine Update

The `AppPhase` enum gains a new variant:

```rust
enum AppPhase {
    Setup,        // Model setup wizard
    SetupApiKey,  // API key wizard step
    Main,         // Normal operation
}
```

**Transition logic on launch:**
1. Load config
2. If model path doesn't exist → `AppPhase::Setup`
3. Else if no API key (keyring or env) → `AppPhase::SetupApiKey`
4. Else → `AppPhase::Main`

**Transition from Setup → SetupApiKey:**
After model download/browse completes, check for API key. If missing, transition to `SetupApiKey`. If present, go directly to `Main`.

**Transition from SetupApiKey → Main:**
On "Save & Continue" (key saved to keyring) or "Skip for now".

**Transition from Main → SetupApiKey (settings):**
The "Change API Key" button in settings transitions to `SetupApiKey` with `wizard_from_settings: true`. The "Cancel" button returns to `Main`. The existing `wizard_from_settings` flag is safely reused since only one wizard phase can be active at a time — the flag simply controls whether a "Back/Cancel" button is shown.

## Keyring Integration

### Crate

`keyring = "3"` — cross-platform credential storage. Uses:
- macOS: Keychain
- Linux: Secret Service (via D-Bus)

### Identifiers

- **Service name:** `"arai"`
- **Account/user:** `"openai_api_key"`

### Operations

```rust
// Read
keyring::Entry::new("arai", "openai_api_key")?.get_password()

// Write
keyring::Entry::new("arai", "openai_api_key")?.set_password(&key)

// Delete
keyring::Entry::new("arai", "openai_api_key")?.delete_credential()
```

### New Module: `keyring_store.rs`

A thin wrapper around keyring operations that handles errors gracefully:

```rust
pub fn get_api_key() -> Option<String>
pub fn set_api_key(key: &str) -> Result<(), String>
pub fn delete_api_key() -> Result<(), String>
```

All functions log errors but don't panic. `get_api_key()` returns `None` on any keyring error (missing credential, no keyring service, etc.), allowing graceful fallback.

## Key Resolution Order

1. `OPENAI_API_KEY` env var (highest priority, for CI/scripting)
2. Keyring credential
3. Config file value (migration fallback — only non-empty if keyring write failed during migration)
4. Empty (no key configured)

The `resolve_api_key()` function checks these sources in order and returns the first non-empty value. If the env var is set, the keyring is not consulted.

## Config Migration

On `Config::load()`, if the `open_api_key` field in the YAML file is non-empty:
1. Store the key in keyring via `keyring_store::set_api_key()`
2. Clear the `open_api_key` field in config
3. Save the config file (removing the key from plain text)
4. Log: `"Migrated API key from config file to keyring"`

If keyring storage fails during migration, the key remains in the config file and a warning is logged. The app continues to work — the resolution function falls back to the config value.

## Config Changes

### `Config` struct

The `open_api_key` field remains in the `Config` struct for runtime use (holds the resolved key from env/keyring). However, it is **never written back to the config file** after the keyring migration:

- **Deserialization:** `open_api_key` is still read from YAML (for migration). After migration it will be empty in the file.
- **Serialization:** `Config::save()` always writes `open_api_key: None` in `FileConfig`, regardless of the runtime value. This prevents the resolved key from leaking back to disk on every config save (prompt changes, device changes, etc.). The `open_api_key` field in `FileConfig` keeps `#[serde(skip_serializing_if = "Option::is_none")]` (already present for other optional fields).
- **Loading:** After merging config layers, the loader calls `resolve_api_key()` which checks env var → keyring → config value (migration fallback) → empty.

### `from_partial()` changes

After building the config, call `resolve_api_key()` to populate `open_api_key` from the correct source. The raw YAML value (from `PartialConfig`) is checked first for migration: if non-empty, migrate it to keyring, then resolve normally.

### `Config::save()` changes

The critical change: `Config::save()` must set `open_api_key: None` (not `Some(self.open_api_key.clone())`) when building `FileConfig`. This ensures the runtime-resolved key is never written to disk, even when `save()` is called for unrelated config changes (prompts, input device, etc.).

## Settings UI (Setup Tab)

A new "API Key" card in the Setup tab, positioned between the Microphone card and the Keyboard Shortcut card.

### States

**Key configured (from keyring):**
- Label: "API Key"
- Masked display: `sk-...7xQ3` (first 3 + last 4 chars of the key)
- "Change API Key" button (ghost style, like "Change Model")

**Key configured (from env var):**
- Label: "API Key"
- Text: "Set via environment variable" (muted)
- "Change API Key" button is disabled (env var takes precedence)

**No key configured:**
- Label: "API Key"
- Text: "Not configured" (muted, red-ish)
- "Set API Key" button

### Edit Flow

Clicking "Change API Key" or "Set API Key" transitions to `AppPhase::SetupApiKey` with `wizard_from_settings: true`, reusing the same wizard screen. On save, the key is stored in keyring and the app returns to settings. On cancel, returns to settings with no changes.

## Submit Without API Key

When the user skips API key setup and later tries to submit text, the Submit button is disabled and the status line shows "API key required — configure in Settings". This check uses the runtime `open_api_key` field: if empty, submission is blocked at the UI level. The agent is never called with an empty key.

## Agent Restart on Key Change

When the API key changes (via wizard or settings), the controller must recreate the `Agent` because it captures the key in its worker thread closure. The flow:

1. UI sends `AppEventKind::UiUpdateApiKey(String)` to controller
2. Controller updates `app_state.open_api_key` and saves to keyring
3. Controller drops old `Agent` and creates new one with the new key
4. Controller sends config snapshot to UI

### `restart_agent()` method

Similar to the existing `restart_transcriber()` pattern:

```rust
fn restart_agent(&mut self, api_key: String) {
    // Drop old Agent — its Drop impl stops the worker thread
    let old = std::mem::replace(
        &mut self.agent,
        Agent::new(self.app_event_tx.clone(), api_key),
    );
    drop(old);
}
```

### New AppState method

```rust
pub fn update_api_key(&self, key: String) {
    let mut inner = self.inner.lock().expect("...");
    inner.config.open_api_key = key;
    // Note: key is saved to keyring, NOT to config file
}
```

The keyring write happens in the controller (or keyring_store module), not in app_state, since app_state only manages config file persistence.

## Changes to Existing Code

### `Cargo.toml`
- Add `keyring = "3"`

### `src/keyring_store.rs` (new)
- `get_api_key() -> Option<String>`
- `set_api_key(key: &str) -> Result<(), String>`
- `delete_api_key() -> Result<(), String>`
- Internal helper for creating `keyring::Entry`

### `src/config.rs`
- Add `resolve_api_key()` function: env var → keyring → config file value (migration fallback) → empty
- Run migration in `Config::load()` if YAML has non-empty key
- Ensure `Config::save()` sets `open_api_key: None` in `FileConfig` (never serialize the runtime-resolved key)
- Remove `open_api_key` from `PartialConfig::from_env()` (env var is handled by `resolve_api_key()`)

### `src/messages.rs`
- Add `AppEventKind::UiUpdateApiKey(String)` — UI tells controller the key changed
- Add `ApiKeyStatus` enum (shared type used by snapshot, UI, and controller):
  ```rust
  #[derive(Clone, Debug)]
  pub enum ApiKeyStatus {
      /// Key is stored in keyring; carries masked display string (e.g., "sk-...7xQ3")
      Keyring(String),
      /// Key is set via environment variable
      EnvVar,
      /// No key configured
      NotSet,
  }
  ```
- Add `api_key_status: ApiKeyStatus` field to `UiUpdate::ConfigSnapshot`

### `src/ui.rs`
- Add `AppPhase::SetupApiKey` variant
- Add wizard API key state fields: `wizard_api_key_input: String`, `wizard_api_key_error: Option<String>`
- Add `Message` variants: `WizardApiKeyChanged(String)`, `WizardApiKeySave`, `WizardApiKeySkip`, `OpenApiKeyFromSettings`
- Add `view_wizard_api_key()` function
- Update `view()` to route `AppPhase::SetupApiKey`
- Update `view_setup_tab()` to add API key card
- Update model wizard completion to check for API key before going to Main
- Add config state fields: `config_api_key_status: ApiKeyStatus` enum (Keyring(masked), EnvVar, NotSet)

### `src/controller.rs`
- Handle `AppEventKind::UiUpdateApiKey` — save to keyring, recreate Agent, send snapshot
- Update `send_config_snapshot()` to include API key status
- Add `restart_agent()` method

### `src/app_state.rs`
- Add `update_api_key()` method (updates runtime `Config.open_api_key`, does NOT call `Config::save()`)
- Add `api_key_status: ApiKeyStatus` field to `AppStateSnapshot`, computed from runtime state:
  - If `OPENAI_API_KEY` env var is set → `ApiKeyStatus::EnvVar`
  - Else if `open_api_key` is non-empty → `ApiKeyStatus::Keyring(masked)` where masked = first 3 + "..." + last 4 chars
  - Else → `ApiKeyStatus::NotSet`

### `src/main.rs`
- Add `mod keyring_store;` declaration
- Add `api_key_exists` check after config load
- Pass both `model_exists` and `api_key_exists` to `Ui::new()`

## Error Handling

- **Keyring unavailable** (e.g., no Secret Service on headless Linux): `get_api_key()` returns `None`, `set_api_key()` returns `Err`. The wizard shows an error message suggesting the user set the `OPENAI_API_KEY` env var instead. The app remains functional — just can't store the key persistently via keyring.
- **Invalid key format**: Wizard shows inline error "API key should start with sk-"
- **Migration failure**: Key stays in config file, warning logged. App works normally.

## Known Limitations

- No network validation of the API key during setup. Invalid keys will produce errors when the agent tries to call OpenAI.
- Keyring may prompt for system authentication (e.g., macOS Keychain access dialog) on first use. This is expected OS behavior.
- On Linux without a Secret Service provider, keyring storage won't work. The env var fallback is the recommended path for headless/minimal setups.
