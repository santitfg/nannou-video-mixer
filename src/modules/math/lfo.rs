use crate::modules::param::Param;
use crate::ModuleChrome;
use serde::{Deserialize, Serialize};

pub const FREQ_MAX: f32 = 10.0;

/// The LFO's shape. Every wave runs 0..1 over one cycle (before centering).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Wave {
    #[default]
    Sine,
    Triangle,
    RampUp,
    RampDown,
    Square,
    /// Sample & hold: a new random value every cycle (`freq` times a second).
    Random,
    /// Smooth random: eases from one random value to the next each cycle.
    Noise,
}

impl Wave {
    pub const ALL: [Wave; 7] = [
        Wave::Sine,
        Wave::Triangle,
        Wave::RampUp,
        Wave::RampDown,
        Wave::Square,
        Wave::Random,
        Wave::Noise,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Wave::Sine => "Sine",
            Wave::Triangle => "Triangle",
            Wave::RampUp => "Ramp up",
            Wave::RampDown => "Ramp down",
            Wave::Square => "Square",
            Wave::Random => "Random (S&H)",
            Wave::Noise => "Noise",
        }
    }
}

/// LFO: oscillates at `freq` Hz, no inputs. Output is 0..1, or -1..1 when
/// `centered` (swinging around zero instead of around 0.5). The frequency
/// box works like any number box (drag, click to type, right-click range).
pub struct LfoState {
    pub chrome: ModuleChrome,
    pub freq: Param,
    pub wave: Wave,
    pub centered: bool,
    pub dropdown_open: bool,
}

impl LfoState {
    pub fn new() -> Self {
        LfoState {
            chrome: ModuleChrome::new(),
            freq: Param::ranged(1.0, 0.0, FREQ_MAX),
            wave: Wave::Sine,
            centered: false,
            dropdown_open: false,
        }
    }

    /// The output at time `now`. Stateless, so it can be evaluated anywhere
    /// (graph, UI readouts) and always agrees; `seed` (the module id) keeps
    /// two Random/Noise LFOs from producing the same sequence.
    pub fn sample(&self, now: f32, seed: u64) -> f32 {
        let x = self.freq.value * now;
        let phase = x.rem_euclid(1.0);
        let cycle = x.floor() as i64;
        let v = match self.wave {
            Wave::Sine => ((2.0 * std::f32::consts::PI * phase).sin() + 1.0) / 2.0,
            Wave::Triangle => {
                if phase < 0.5 {
                    2.0 * phase
                } else {
                    2.0 - 2.0 * phase
                }
            }
            Wave::RampUp => phase,
            Wave::RampDown => 1.0 - phase,
            Wave::Square => {
                if phase < 0.5 {
                    1.0
                } else {
                    0.0
                }
            }
            Wave::Random => hash01(seed, cycle),
            Wave::Noise => {
                let (a, b) = (hash01(seed, cycle), hash01(seed, cycle + 1));
                let s = phase * phase * (3.0 - 2.0 * phase); // smoothstep
                a + (b - a) * s
            }
        };
        if self.centered {
            v * 2.0 - 1.0
        } else {
            v
        }
    }
}

/// A repeatable pseudo-random value in 0..1 for cycle `n` (splitmix64).
fn hash01(seed: u64, n: i64) -> f32 {
    let mut z = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (n as u64).wrapping_add(0xD1B5_4A32_D192_ED03);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    (z >> 40) as f32 / (1u64 << 24) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lfo(wave: Wave) -> LfoState {
        let mut l = LfoState::new();
        l.wave = wave;
        l
    }

    #[test]
    fn shapes_at_known_phases() {
        assert!((lfo(Wave::Sine).sample(0.25, 0) - 1.0).abs() < 1e-5);
        assert!((lfo(Wave::Triangle).sample(0.5, 0) - 1.0).abs() < 1e-5);
        assert!((lfo(Wave::RampUp).sample(0.25, 0) - 0.25).abs() < 1e-5);
        assert!((lfo(Wave::RampDown).sample(0.25, 0) - 0.75).abs() < 1e-5);
        assert_eq!(lfo(Wave::Square).sample(0.25, 0), 1.0);
        assert_eq!(lfo(Wave::Square).sample(0.75, 0), 0.0);
    }

    #[test]
    fn every_wave_stays_in_range() {
        for wave in Wave::ALL {
            for centered in [false, true] {
                let mut l = lfo(wave);
                l.centered = centered;
                let (lo, hi) = if centered { (-1.0, 1.0) } else { (0.0, 1.0) };
                for i in 0..500 {
                    let v = l.sample(i as f32 * 0.037, 7);
                    assert!((lo..=hi).contains(&v), "{wave:?} centered={centered}: {v}");
                }
            }
        }
    }

    #[test]
    fn random_holds_within_a_cycle_and_changes_between() {
        let l = lfo(Wave::Random);
        assert_eq!(l.sample(0.1, 3), l.sample(0.9, 3));
        let distinct: std::collections::HashSet<u32> = (0..20).map(|c| l.sample(c as f32 + 0.5, 3).to_bits()).collect();
        assert!(distinct.len() > 15);
    }

    #[test]
    fn noise_is_continuous_across_cycles() {
        let l = lfo(Wave::Noise);
        assert!((l.sample(0.9999, 1) - l.sample(1.0001, 1)).abs() < 0.01);
    }

    #[test]
    fn seeds_give_different_sequences() {
        let l = lfo(Wave::Random);
        assert_ne!(l.sample(0.5, 1), l.sample(0.5, 2));
    }
}
