use serde::{Deserialize, Serialize};

pub const SETTINGS_PATH: &str = "settings.json";

/// App preferences, kept apart from compositions (they belong to the user,
/// not to a patch) and saved whenever a slider is released.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Multiplier on mouse-wheel / trackpad scrolling of the canvas.
    pub scroll_speed: f32,
    /// Multiplier on dragging the canvas (middle button / Shift+left).
    pub drag_speed: f32,
}

impl Settings {
    pub const SPEED_MIN: f32 = 0.05;
    pub const SPEED_MAX: f32 = 2.0;
}

impl Default for Settings {
    fn default() -> Self {
        Settings { scroll_speed: 0.3, drag_speed: 1.0 }
    }
}

impl Settings {
    pub fn load() -> Self {
        std::fs::read_to_string(SETTINGS_PATH)
            .ok()
            .and_then(|json| serde_json::from_str::<Settings>(&json).ok())
            .map(|s| Settings {
                scroll_speed: s.scroll_speed.clamp(Self::SPEED_MIN, Self::SPEED_MAX),
                drag_speed: s.drag_speed.clamp(Self::SPEED_MIN, Self::SPEED_MAX),
            })
            .unwrap_or_default()
    }

    pub fn save(&self) {
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(SETTINGS_PATH, json);
        }
    }
}
