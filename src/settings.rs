use serde::{Deserialize, Serialize};

pub const SETTINGS_PATH: &str = "settings.json";

/// App preferences, kept apart from compositions (they belong to the user,
/// not to a patch) and saved whenever a slider is released or a toggle flipped.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Multiplier on mouse-wheel / trackpad scrolling of the canvas.
    pub scroll_speed: f32,
    /// Multiplier on dragging the canvas (middle button / Shift+left).
    pub drag_speed: f32,
    /// Frame-rate readout in the top-left corner.
    pub show_fps: bool,
    /// On-screen debug log (Settings → Debug).
    pub debug_log: bool,
    /// Take a global lock around every camera frame grab (Settings → Debug);
    /// see `camera::FRAME_LOCK`.
    pub serialize_camera_grabs: bool,
}

impl Settings {
    pub const SPEED_MIN: f32 = 0.05;
    pub const SPEED_MAX: f32 = 2.0;
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            scroll_speed: 0.3,
            drag_speed: 1.0,
            show_fps: false,
            debug_log: false,
            serialize_camera_grabs: false,
        }
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
                ..s
            })
            .unwrap_or_default()
    }

    pub fn save(&self) {
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(SETTINGS_PATH, json);
        }
    }

    /// Pushes the settings that live outside `Model` (the debug log and
    /// the camera threads read global flags) to where they're read.
    pub fn apply_globals(&self) {
        crate::debug_log::set_enabled(self.debug_log);
        crate::modules::sources::camera::set_serialize_grabs(self.serialize_camera_grabs);
    }
}
