use global_hotkey::hotkey::HotKey;
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use log::{error, info, warn};

/// Wraps the `global-hotkey` crate to register a single system-wide hotkey
/// for toggling transcription. The manager must be created on the main thread
/// (macOS requirement). An iced subscription delivers events to
/// [`matches_press`](Self::matches_press).
pub struct HotkeyHandle {
    manager: GlobalHotKeyManager,
    hotkey_id: u32,
    hotkey_str: String,
}

impl HotkeyHandle {
    /// Parse the hotkey string (e.g. `"Ctrl+Shift+A"`) and register it globally.
    /// Must be called on the main thread before the iced event loop starts.
    pub fn register(hotkey_str: &str) -> Option<Self> {
        let hotkey: HotKey = match hotkey_str.parse() {
            Ok(hk) => hk,
            Err(err) => {
                error!("Failed to parse global hotkey '{hotkey_str}': {err}");
                return None;
            }
        };

        let manager = match GlobalHotKeyManager::new() {
            Ok(m) => m,
            Err(err) => {
                warn!("Failed to create global hotkey manager: {err}");
                return None;
            }
        };

        if let Err(err) = manager.register(hotkey) {
            error!("Failed to register global hotkey '{hotkey_str}': {err}");
            return None;
        }

        info!("Global hotkey registered: {hotkey_str}");
        Some(Self {
            manager,
            hotkey_id: hotkey.id(),
            hotkey_str: hotkey_str.to_string(),
        })
    }

    /// Unregisters the current hotkey and registers a new one. Returns `true`
    /// if the new hotkey was successfully registered.
    pub fn re_register(&mut self, new_hotkey_str: &str) -> bool {
        let new_hotkey: HotKey = match new_hotkey_str.parse() {
            Ok(hk) => hk,
            Err(err) => {
                error!("Failed to parse new hotkey '{new_hotkey_str}': {err}");
                return false;
            }
        };

        // Unregister old hotkey (best-effort).
        if let Ok(old_hotkey) = self.hotkey_str.parse::<HotKey>() {
            let _ = self.manager.unregister(old_hotkey);
        }

        if let Err(err) = self.manager.register(new_hotkey) {
            error!("Failed to register new hotkey '{new_hotkey_str}': {err}");
            // Try to restore the old one.
            if let Ok(old_hotkey) = self.hotkey_str.parse::<HotKey>() {
                let _ = self.manager.register(old_hotkey);
            }
            return false;
        }

        info!(
            "Global hotkey changed: {} -> {}",
            self.hotkey_str, new_hotkey_str
        );
        self.hotkey_id = new_hotkey.id();
        self.hotkey_str = new_hotkey_str.to_string();
        true
    }

    /// Returns `true` when `event` is a press of the currently registered hotkey.
    pub fn matches_press(&self, event: &GlobalHotKeyEvent) -> bool {
        is_matching_press(self.hotkey_id, event)
    }
}

fn is_matching_press(hotkey_id: u32, event: &GlobalHotKeyEvent) -> bool {
    event.id == hotkey_id && event.state == HotKeyState::Pressed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_matching_pressed_event() {
        let event = GlobalHotKeyEvent {
            id: 42,
            state: HotKeyState::Pressed,
        };

        assert!(is_matching_press(42, &event));
    }

    #[test]
    fn rejects_matching_released_event() {
        let event = GlobalHotKeyEvent {
            id: 42,
            state: HotKeyState::Released,
        };

        assert!(!is_matching_press(42, &event));
    }

    #[test]
    fn rejects_pressed_event_for_different_hotkey() {
        let event = GlobalHotKeyEvent {
            id: 7,
            state: HotKeyState::Pressed,
        };

        assert!(!is_matching_press(42, &event));
    }
}
