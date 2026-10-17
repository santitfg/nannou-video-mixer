use crate::modules::param::Param;
use crate::ModuleChrome;
use serde::{Deserialize, Serialize};

pub const TIME_MAX: f32 = 5.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LagMode {
    /// Exponential: covers ~63% of the way to the target every `time`.
    #[default]
    Smooth,
    /// Slew limiter: moves at most a full 0..1 swing per `time`.
    Linear,
}

impl LagMode {
    pub const ALL: [LagMode; 2] = [LagMode::Smooth, LagMode::Linear];

    pub fn name(self) -> &'static str {
        match self {
            LagMode::Smooth => "Smooth",
            LagMode::Linear => "Linear",
        }
    }
}

/// LAG: follows its number input slowly, smoothing out jumps (an LFO's
/// square wave, a random step, a twitchy pot). It has state, so it's
/// advanced once per tick by `step` rather than evaluated on demand.
pub struct LagState {
    pub chrome: ModuleChrome,
    /// Seconds to catch up.
    pub time: Param,
    pub mode: LagMode,
    pub dropdown_open: bool,
    current: Option<f32>,
}

impl LagState {
    pub fn new() -> Self {
        LagState {
            chrome: ModuleChrome::new(),
            time: Param::ranged(0.5, 0.0, TIME_MAX),
            mode: LagMode::Smooth,
            dropdown_open: false,
            current: None,
        }
    }

    /// The smoothed value, once an input has been seen.
    pub fn output(&self) -> Option<f32> {
        self.current
    }

    /// Moves `dt` seconds towards `target`; no input (`None`) forgets the
    /// state so the next input is picked up immediately.
    pub fn step(&mut self, target: Option<f32>, dt: f32) {
        let Some(target) = target else {
            self.current = None;
            return;
        };
        let current = self.current.unwrap_or(target);
        let time = self.time.value.max(0.0);
        let next = if time <= f32::EPSILON {
            target
        } else {
            match self.mode {
                LagMode::Smooth => current + (target - current) * (1.0 - (-dt / time).exp()),
                LagMode::Linear => {
                    let max_step = dt / time;
                    current + (target - current).clamp(-max_step, max_step)
                }
            }
        };
        self.current = Some(next);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(mode: LagMode, steps: usize) -> Vec<f32> {
        let mut lag = LagState::new();
        lag.mode = mode;
        lag.step(Some(0.0), 0.0);
        (0..steps)
            .map(|_| {
                lag.step(Some(1.0), 1.0 / 60.0);
                lag.output().unwrap()
            })
            .collect()
    }

    #[test]
    fn first_input_is_taken_as_is() {
        let mut lag = LagState::new();
        lag.step(Some(0.7), 0.016);
        assert_eq!(lag.output(), Some(0.7));
    }

    #[test]
    fn smooth_converges_without_overshoot() {
        let values = run(LagMode::Smooth, 600);
        assert!(values.windows(2).all(|w| w[1] >= w[0] && w[1] <= 1.0));
        assert!(values[29] > 0.6 && values[29] < 0.66, "after 0.5 s: {}", values[29]); // ~63%
        assert!(values.last().unwrap() > &0.999);
    }

    #[test]
    fn linear_moves_at_constant_rate() {
        let values = run(LagMode::Linear, 60);
        assert!((values[14] - 0.5).abs() < 0.01, "half way after 0.25 s: {}", values[14]);
        assert_eq!(*values.last().unwrap(), 1.0);
    }

    #[test]
    fn zero_time_is_instant_and_no_input_resets() {
        let mut lag = LagState::new();
        lag.time.set(0.0);
        lag.step(Some(0.2), 0.016);
        lag.step(Some(0.9), 0.016);
        assert_eq!(lag.output(), Some(0.9));
        lag.step(None, 0.016);
        assert_eq!(lag.output(), None);
    }
}
