//! WLED: reads a WLED controller's inputs (pots on analog pins, touch pads,
//! buttons and switches) over its JSON API, via the `wled_pots` crate.
//! Polling runs on `wled_pots`' own threads; each tick the module just
//! copies the latest reading, so the render loop never waits on the network.

use crate::modules::param::Param;
use crate::ModuleChrome;
use std::net::SocketAddr;
use std::time::{Duration, Instant};
use wled_pots::{DeviceReading, Pin};

/// Analog readings are 12-bit on an ESP32; an ESP8266 tops out at 1023
/// (set the box's Raw max to that).
pub const RAW_MAX: f32 = 4095.0;
/// An analog input that moved within this long shows as active.
const ACTIVITY: Duration = Duration::from_millis(350);

/// WLED's button types (wled00/const.h).
pub fn button_type_name(t: Option<u8>) -> &'static str {
    match t {
        Some(2) => "Push",
        Some(3) => "Push inv.",
        Some(4) => "Switch",
        Some(5) => "PIR",
        Some(6) => "Touch",
        Some(7) => "Analog",
        Some(8) => "Analog inv.",
        Some(9) => "Touch switch",
        _ => "Input",
    }
}

/// A device's connection state, for the module and the WLED panel.
#[derive(Debug, Clone, PartialEq)]
pub enum Link {
    Connecting,
    Online { age: Duration },
    /// Reachable before, but nothing new for a while.
    Stale { age: Duration },
    Error(String),
}

impl Link {
    pub fn of(reading: &DeviceReading, poll: Duration) -> Link {
        match (&reading.last_error, reading.last_success) {
            (Some(err), _) => Link::Error(err.clone()),
            (None, None) => Link::Connecting,
            (None, Some(at)) => {
                let age = at.elapsed();
                if age > poll * 4 + Duration::from_secs(1) {
                    Link::Stale { age }
                } else {
                    Link::Online { age }
                }
            }
        }
    }

    pub fn text(&self) -> String {
        match self {
            Link::Connecting => "connecting\u{2026}".into(),
            Link::Online { age } => format!("online \u{00B7} {} ms ago", age.as_millis()),
            Link::Stale { age } => format!("no data for {:.0} s", age.as_secs_f32()),
            Link::Error(err) => err.trim_start_matches("WLED ").to_string(),
        }
    }
}

/// "wled-living._wled._tcp.local." -> "wled-living"
pub fn short_name(name: &str) -> &str {
    name.split("._wled").next().unwrap_or(name)
}

/// One input as the module shows it.
#[derive(Debug, Clone)]
pub struct InputView {
    pub gpio: u8,
    pub kind: &'static str,
    /// 0..1.
    pub value: f32,
    /// What the device reported (the raw reading, or the button state).
    pub raw: i32,
    pub active: bool,
}

/// The pins worth showing: buttons/switches and anything with a live reading.
fn is_input(pin: &Pin) -> bool {
    pin.raw.is_some() || pin.button_type.is_some_and(|t| t >= 2)
}

pub struct WledState {
    pub chrome: ModuleChrome,
    pub device: Option<SocketAddr>,
    /// The GPIO whose value goes out of the module.
    pub pin: Option<u8>,
    /// Full scale of analog readings.
    pub raw_max: Param,
    pub dropdown_open: bool,
    pub inputs: Vec<InputView>,
    pub link: Option<Link>,
    /// When each GPIO's value last moved, for the activity lights.
    changed: Vec<(u8, f32, Instant)>,
}

impl WledState {
    pub fn new() -> Self {
        WledState {
            chrome: ModuleChrome::new(),
            device: None,
            pin: None,
            raw_max: Param::ranged(RAW_MAX, 1.0, RAW_MAX),
            dropdown_open: false,
            inputs: Vec::new(),
            link: None,
            changed: Vec::new(),
        }
    }

    /// The selected input's value, if the device has reported it.
    pub fn output(&self) -> Option<f32> {
        let pin = self.pin?;
        self.inputs.iter().find(|i| i.gpio == pin).map(|i| i.value)
    }

    /// Takes in the device's latest reading (`None`: no device / not tracked).
    pub fn refresh(&mut self, reading: Option<&DeviceReading>, poll: Duration) {
        let Some(reading) = reading else {
            self.inputs.clear();
            self.link = None;
            return;
        };
        self.link = Some(Link::of(reading, poll));
        let max = self.raw_max.value.max(1.0);
        let now = Instant::now();
        self.inputs = reading
            .pins
            .iter()
            .filter(|p| is_input(p))
            .map(|p| {
                let value = match p.raw {
                    Some(raw) => {
                        let v = (raw as f32 / max).clamp(0.0, 1.0);
                        if p.button_type == Some(8) { 1.0 - v } else { v }
                    }
                    None => (p.state.unwrap_or(0) != 0) as u8 as f32,
                };
                // Remember when it last moved (beyond pot jitter).
                match self.changed.iter_mut().find(|(g, _, _)| *g == p.gpio) {
                    Some(entry) if (entry.1 - value).abs() > 0.01 => *entry = (p.gpio, value, now),
                    Some(_) => {}
                    None => self.changed.push((p.gpio, value, now - ACTIVITY)),
                }
                let moved = self.changed.iter().find(|(g, _, _)| *g == p.gpio).is_some_and(|(_, _, at)| at.elapsed() < ACTIVITY);
                let pressed = p.raw.is_none() && value > 0.5;
                InputView {
                    gpio: p.gpio,
                    kind: button_type_name(p.button_type),
                    value,
                    raw: p.raw.map_or(p.state.unwrap_or(0), i32::from),
                    active: moved || pressed,
                }
            })
            .collect();
        // First reading: pick the first pot (or any input) automatically.
        if self.pin.is_none() {
            self.pin = self
                .inputs
                .iter()
                .find(|i| i.kind.starts_with("Analog"))
                .or(self.inputs.first())
                .map(|i| i.gpio);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reading(json: &str) -> DeviceReading {
        DeviceReading {
            pins: wled_pots::PinsResponse::parse(json).unwrap().pins,
            last_success: Some(Instant::now()),
            last_error: None,
        }
    }

    const PINS: &str = r#"{"pins": [
        {"p": 16, "c": 0, "a": true, "o": 10, "n": "LED", "m": 1, "s": 1},
        {"p": 0, "c": 0, "a": true, "o": 2, "n": "Button", "m": 0, "t": 2, "s": 1},
        {"p": 32, "c": 1, "a": true, "o": 2, "n": "Button", "m": 0, "t": 7, "s": 0, "r": 2048},
        {"p": 33, "c": 1, "a": true, "o": 2, "n": "Button", "m": 0, "t": 8, "s": 1, "r": 1024}
    ]}"#;

    #[test]
    fn reads_inputs_and_picks_the_first_pot() {
        let mut w = WledState::new();
        w.refresh(Some(&reading(PINS)), Duration::from_millis(50));
        assert_eq!(w.inputs.len(), 3, "the LED pin isn't an input");
        assert_eq!(w.pin, Some(32));
        assert!((w.output().unwrap() - 0.5).abs() < 0.01);
        let button = w.inputs.iter().find(|i| i.gpio == 0).unwrap();
        assert_eq!((button.value, button.active), (1.0, true));
        let inverted = w.inputs.iter().find(|i| i.gpio == 33).unwrap();
        assert!((inverted.value - 0.75).abs() < 0.01);
        assert!(matches!(w.link, Some(Link::Online { .. })));
    }

    #[test]
    fn raw_max_rescales_for_10_bit_boards() {
        let mut w = WledState::new();
        w.raw_max.set(1023.0);
        w.refresh(Some(&reading(PINS)), Duration::from_millis(50));
        assert_eq!(w.output(), Some(1.0)); // 2048 is past a 10-bit full scale
    }

    #[test]
    fn link_states() {
        let poll = Duration::from_millis(50);
        let mut r = DeviceReading::default();
        assert_eq!(Link::of(&r, poll), Link::Connecting);
        r.last_error = Some("WLED HTTP error: timeout".into());
        assert!(matches!(Link::of(&r, poll), Link::Error(_)));
        r.last_error = None;
        r.last_success = Some(Instant::now() - Duration::from_secs(5));
        assert!(matches!(Link::of(&r, poll), Link::Stale { .. }));
        assert_eq!(short_name("wled-sala._wled._tcp.local."), "wled-sala");
    }
}
