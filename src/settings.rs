use crate::ui::Palette;
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
    /// Show an Exit button (and the cursor) when the mouse moves in
    /// fullscreen. Off: fullscreen stays clean; leave with a click or key.
    pub show_exit_button: bool,
    pub palette: Palette,
    /// Shader files pinned to the sidebar (Effects), in pin order.
    pub favorite_shaders: Vec<String>,
    /// WLED controllers to reconnect to on start (found by a scan or added by IP).
    pub wled_devices: Vec<WledDevice>,
    /// How often each WLED controller is polled.
    pub wled_poll_ms: u64,
    /// On-screen debug log (Settings → Debug).
    pub debug_log: bool,
    /// Take a global lock around every camera frame grab (Settings → Debug);
    /// see `camera::FRAME_LOCK`.
    pub serialize_camera_grabs: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WledDevice {
    /// "192.168.1.50:80"
    pub addr: String,
    pub name: String,
}

impl Settings {
    /// The WLED panel's poll-rate choices.
    pub const WLED_POLL_CHOICES: [u64; 5] = [20, 50, 100, 250, 500];
    pub const SPEED_MIN: f32 = 0.05;
    pub const SPEED_MAX: f32 = 2.0;
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            scroll_speed: 0.3,
            drag_speed: 1.0,
            show_fps: false,
            show_exit_button: false,
            palette: Palette::Dark,
            favorite_shaders: Vec::new(),
            wled_devices: Vec::new(),
            wled_poll_ms: 50,
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
