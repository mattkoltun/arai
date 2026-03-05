use global_hotkey::hotkey::HotKey;
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use log::{error, info, warn};

/// Wraps the `global-hotkey` crate to register a single system-wide hotkey
/// for toggling transcription. The manager must be created on the main thread
/// (macOS requirement). Events are polled via [`poll_event`] from an iced
/// subscription tick.
pub struct HotkeyHandle {
    _manager: GlobalHotKeyManager,
    hotkey_id: u32,
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
            _manager: manager,
            hotkey_id: hotkey.id(),
        })
    }

    /// Returns `true` if the registered hotkey was pressed since the last poll.
    /// Call this from the iced tick subscription.
    pub fn poll_event(&self) -> bool {
        let receiver = GlobalHotKeyEvent::receiver();
        while let Ok(event) = receiver.try_recv() {
            if event.id == self.hotkey_id && event.state == HotKeyState::Pressed {
                return true;
            }
        }
        false
    }
}
