use iced::theme::Palette;
use iced::widget::{button, container, overlay, pick_list, text_editor, text_input};
use iced::{Background, Border, Color, Theme};

/// Semantic color palette used throughout the UI.
#[derive(Clone, Copy, Debug)]
pub struct AppPalette {
    pub bg: Color,
    pub surface: Color,
    pub surface_hover: Color,
    pub muted: Color,
    pub text: Color,
    pub accent: Color,
    pub accent_hover: Color,
    pub accent_pressed: Color,
    pub accent_faint: Color,
    pub green: Color,
    pub green_hover: Color,
    pub green_pressed: Color,
    pub red: Color,
    pub disabled: Color,
    pub border: Color,
    pub selection: Color,
    pub history_card_bg: Color,
    pub history_card_border: Color,
}

/// Catppuccin Frappe (dark theme).
pub const FRAPPE: AppPalette = AppPalette {
    bg: Color::from_rgb(0.180, 0.192, 0.251),           // #2E3140 Base
    surface: Color::from_rgb(0.208, 0.220, 0.286),       // #353848 Surface0
    surface_hover: Color::from_rgb(0.247, 0.259, 0.325), // #3F4253 Surface1
    muted: Color::from_rgb(0.455, 0.475, 0.557),         // #74798E Overlay0
    text: Color::from_rgb(0.780, 0.796, 0.871),          // #C7CBDE Text
    accent: Color::from_rgb(0.949, 0.392, 0.580),        // #F26494 Pink (Catppuccin pink)
    accent_hover: Color::from_rgb(0.969, 0.478, 0.647),  // #F87AA5
    accent_pressed: Color::from_rgb(0.831, 0.294, 0.471), // #D44B78
    accent_faint: Color::from_rgba(0.949, 0.392, 0.580, 0.12),
    green: Color::from_rgb(0.651, 0.820, 0.404),         // #A6D167 Green
    green_hover: Color::from_rgb(0.741, 0.890, 0.500),
    green_pressed: Color::from_rgb(0.541, 0.710, 0.294),
    red: Color::from_rgb(0.906, 0.298, 0.392),           // #E74C64 Red
    disabled: Color::from_rgb(0.282, 0.294, 0.361),      // #484B5C
    border: Color::from_rgb(0.282, 0.294, 0.361),        // #484B5C
    selection: Color::from_rgba(0.949, 0.392, 0.580, 0.30),
    history_card_bg: Color::from_rgb(0.231, 0.243, 0.310),
    history_card_border: Color::from_rgba(1.0, 1.0, 1.0, 0.04),
};

/// Catppuccin Latte (light theme).
pub const LATTE: AppPalette = AppPalette {
    bg: Color::from_rgb(0.937, 0.929, 0.914),            // #EFEDE9 Base
    surface: Color::from_rgb(0.898, 0.890, 0.875),       // #E5E3DF Surface0
    surface_hover: Color::from_rgb(0.859, 0.851, 0.835), // #DBD9D5 Surface1
    muted: Color::from_rgb(0.537, 0.525, 0.506),         // #898681 Overlay0
    text: Color::from_rgb(0.282, 0.271, 0.259),          // #484542 Text
    accent: Color::from_rgb(0.918, 0.286, 0.506),        // #EA4981 Pink
    accent_hover: Color::from_rgb(0.949, 0.380, 0.576),  // #F26193
    accent_pressed: Color::from_rgb(0.788, 0.188, 0.408), // #C93068
    accent_faint: Color::from_rgba(0.918, 0.286, 0.506, 0.12),
    green: Color::from_rgb(0.251, 0.584, 0.133),         // #409522 Green
    green_hover: Color::from_rgb(0.341, 0.674, 0.223),
    green_pressed: Color::from_rgb(0.161, 0.494, 0.043),
    red: Color::from_rgb(0.827, 0.204, 0.267),           // #D33444 Red
    disabled: Color::from_rgb(0.737, 0.729, 0.714),      // #BCBAB6
    border: Color::from_rgb(0.800, 0.792, 0.776),        // #CCCAC6
    selection: Color::from_rgba(0.918, 0.286, 0.506, 0.25),
    history_card_bg: Color::from_rgb(0.918, 0.910, 0.894),
    history_card_border: Color::from_rgba(0.0, 0.0, 0.0, 0.06),
};

impl AppPalette {
    /// Build an iced Theme from this palette.
    pub fn iced_theme(&self) -> Theme {
        Theme::custom(
            "Arai".to_string(),
            Palette {
                background: self.bg,
                text: self.text,
                primary: self.accent,
                success: self.green,
                warning: Color::from_rgb(0.976, 0.659, 0.145),
                danger: self.red,
            },
        )
    }

    // ── Style: icon button — fully transparent, icon glows on hover ──
    pub fn icon_btn(&self, status: button::Status) -> button::Style {
        let text_color = match status {
            button::Status::Hovered => self.accent,
            button::Status::Pressed => self.accent_pressed,
            button::Status::Disabled => self.disabled,
            _ => self.muted,
        };
        transparent_btn(text_color)
    }

    // Icon button when "active" (e.g. listening) — glows green
    pub fn icon_btn_active(&self, status: button::Status) -> button::Style {
        let text_color = match status {
            button::Status::Hovered => self.green_hover,
            button::Status::Pressed => self.green_pressed,
            _ => self.green,
        };
        transparent_btn(text_color)
    }

    pub fn icon_btn_danger(&self, status: button::Status) -> button::Style {
        let text_color = match status {
            button::Status::Hovered => self.accent,
            button::Status::Pressed => self.accent_pressed,
            _ => self.red,
        };
        transparent_btn(text_color)
    }

    // ── Style: containers ──────────────────────────────────────────
    pub fn bg_container(&self) -> container::Style {
        container::Style {
            text_color: None,
            background: Some(Background::Color(self.bg)),
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: 0.0.into(),
            },
            shadow: Default::default(),
            snap: false,
        }
    }

    pub fn surface_container(&self) -> container::Style {
        container::Style {
            text_color: None,
            background: Some(Background::Color(self.surface)),
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: 10.0.into(),
            },
            shadow: Default::default(),
            snap: false,
        }
    }

    // ── Style: primary filled button (Save) ──────────────────────
    pub fn primary_btn(&self, status: button::Status) -> button::Style {
        let bg = match status {
            button::Status::Hovered => self.accent_hover,
            button::Status::Pressed => self.accent_pressed,
            _ => self.accent,
        };
        button::Style {
            text_color: Color::WHITE,
            background: Some(Background::Color(bg)),
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: 8.0.into(),
            },
            shadow: Default::default(),
            snap: false,
        }
    }

    // Carousel chip — selected state
    pub fn carousel_chip_active(&self, status: button::Status) -> button::Style {
        let bg = match status {
            button::Status::Hovered => with_alpha(self.accent, 0.18),
            button::Status::Pressed => with_alpha(self.accent, 0.28),
            _ => with_alpha(self.accent, 0.12),
        };
        button::Style {
            text_color: self.accent,
            background: Some(Background::Color(bg)),
            border: Border {
                color: with_alpha(self.accent, 0.3),
                width: 1.0,
                radius: 14.0.into(),
            },
            shadow: Default::default(),
            snap: false,
        }
    }

    // Carousel chip — inactive state
    pub fn carousel_chip_inactive(&self, status: button::Status) -> button::Style {
        let (bg, text_color) = match status {
            button::Status::Hovered => (self.surface_hover, self.text),
            button::Status::Pressed => (self.surface, self.text),
            _ => (Color::TRANSPARENT, self.muted),
        };
        button::Style {
            text_color,
            background: Some(Background::Color(bg)),
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: 14.0.into(),
            },
            shadow: Default::default(),
            snap: false,
        }
    }

    // History entry card
    pub fn history_card(&self) -> container::Style {
        container::Style {
            text_color: None,
            background: Some(Background::Color(self.history_card_bg)),
            border: Border {
                color: self.history_card_border,
                width: 1.0,
                radius: 12.0.into(),
            },
            shadow: Default::default(),
            snap: false,
        }
    }

    // Ghost button for config items
    pub fn ghost_btn(&self, status: button::Status) -> button::Style {
        let (bg, text_color) = match status {
            button::Status::Hovered => (self.accent_faint, self.text),
            button::Status::Pressed => (with_alpha(self.accent, 0.22), self.text),
            button::Status::Disabled => (Color::TRANSPARENT, self.disabled),
            _ => (Color::TRANSPARENT, self.muted),
        };
        button::Style {
            text_color,
            background: Some(Background::Color(bg)),
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: 8.0.into(),
            },
            shadow: Default::default(),
            snap: false,
        }
    }

    pub fn hotkey_input(&self, status: button::Status) -> button::Style {
        let bg = match status {
            button::Status::Hovered => self.surface_hover,
            _ => self.surface,
        };
        button::Style {
            text_color: self.text,
            background: Some(Background::Color(bg)),
            border: Border {
                color: self.border,
                width: 1.0,
                radius: 8.0.into(),
            },
            shadow: Default::default(),
            snap: false,
        }
    }

    pub fn hotkey_input_active(&self) -> button::Style {
        button::Style {
            text_color: self.accent,
            background: Some(Background::Color(self.accent_faint)),
            border: Border {
                color: self.accent,
                width: 1.5,
                radius: 8.0.into(),
            },
            shadow: Default::default(),
            snap: false,
        }
    }

    // ── Style: tab buttons ────────────────────────────────────────
    pub fn tab_btn_active(&self, status: button::Status) -> button::Style {
        let bg = match status {
            button::Status::Hovered => self.accent_hover,
            button::Status::Pressed => self.accent_pressed,
            _ => self.accent,
        };
        button::Style {
            text_color: Color::WHITE,
            background: Some(Background::Color(bg)),
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: 6.0.into(),
            },
            shadow: Default::default(),
            snap: false,
        }
    }

    pub fn tab_btn_inactive(&self, status: button::Status) -> button::Style {
        let (bg, text_color) = match status {
            button::Status::Hovered => (with_alpha(self.accent, 0.10), self.text),
            button::Status::Pressed => (with_alpha(self.accent, 0.20), self.text),
            _ => (Color::TRANSPARENT, self.muted),
        };
        button::Style {
            text_color,
            background: Some(Background::Color(bg)),
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: 6.0.into(),
            },
            shadow: Default::default(),
            snap: false,
        }
    }

    // ── Style: text input / editor ────────────────────────────────
    pub fn borderless_input(&self, status: text_input::Status) -> text_input::Style {
        let focused = matches!(status, text_input::Status::Focused { .. });
        text_input::Style {
            background: Background::Color(self.surface),
            border: Border {
                color: if focused { self.accent } else { Color::TRANSPARENT },
                width: if focused { 1.0 } else { 0.0 },
                radius: 8.0.into(),
            },
            icon: self.muted,
            placeholder: self.muted,
            value: self.text,
            selection: self.selection,
        }
    }

    pub fn styled_pick_list(&self, status: pick_list::Status) -> pick_list::Style {
        let opened = matches!(status, pick_list::Status::Opened { .. });
        let border_color = match status {
            pick_list::Status::Opened { .. } => self.accent,
            pick_list::Status::Hovered => with_alpha(self.accent, 0.4),
            _ => Color::TRANSPARENT,
        };
        pick_list::Style {
            background: Background::Color(self.surface),
            text_color: self.text,
            placeholder_color: self.muted,
            handle_color: self.muted,
            border: Border {
                color: border_color,
                width: if opened { 1.0 } else { 0.0 },
                radius: 8.0.into(),
            },
        }
    }

    pub fn pick_list_menu(&self) -> overlay::menu::Style {
        overlay::menu::Style {
            background: Background::Color(self.surface),
            text_color: self.text,
            selected_text_color: Color::WHITE,
            selected_background: Background::Color(self.accent),
            border: Border {
                color: self.muted,
                width: 1.0,
                radius: 8.0.into(),
            },
            shadow: Default::default(),
        }
    }

    pub fn borderless_editor(&self, status: text_editor::Status) -> text_editor::Style {
        let focused = matches!(status, text_editor::Status::Focused { .. });
        text_editor::Style {
            background: Background::Color(self.surface),
            border: Border {
                color: if focused { self.accent } else { Color::TRANSPARENT },
                width: if focused { 1.0 } else { 0.0 },
                radius: 8.0.into(),
            },
            placeholder: self.muted,
            value: self.text,
            selection: self.selection,
        }
    }
}

fn transparent_btn(text_color: Color) -> button::Style {
    button::Style {
        text_color,
        background: None,
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 0.0.into(),
        },
        shadow: Default::default(),
        snap: false,
    }
}

fn with_alpha(color: Color, alpha: f32) -> Color {
    Color::from_rgba(color.r, color.g, color.b, alpha)
}

/// Detects whether macOS is currently in dark mode by reading the system
/// `AppleInterfaceStyle` preference. Returns `true` if dark mode is active.
#[cfg(target_os = "macos")]
pub fn system_is_dark() -> bool {
    std::process::Command::new("defaults")
        .args(["read", "-g", "AppleInterfaceStyle"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .is_some_and(|s| s.trim().eq_ignore_ascii_case("dark"))
}

#[cfg(not(target_os = "macos"))]
pub fn system_is_dark() -> bool {
    true
}
