use crate::modules::param::Param;
use crate::ModuleChrome;

/// A typed-in number box (no range of its own: dragging moves 1.0 per box
/// width, clicking types an exact value).
fn field(value: f32) -> Param {
    let mut p = Param::unit(value);
    p.set_clamp(false);
    p
}

/// MAP: rescales its input from one range to another, e.g. a pot's
/// 0..1 to -1..1, or an LFO onto 0.2..0.8. With both ends of the output
/// range equal to the input's it is a plain clamp.
///
/// The input first gets `bias` added (calibrating a sensor's rest value to
/// zero). `auto` learns the From range from what the input does; `center`
/// scales either side of zero separately, so a rest point at zero lands on
/// the middle of the output range whatever the extremes are.
pub struct MapState {
    pub chrome: ModuleChrome,
    /// From min, from max, to min, to max, bias.
    pub fields: [Param; 5],
    /// `clamp` keeps the result inside the output range.
    pub clamp: bool,
    /// Learn the From range from the (biased) input.
    pub auto: bool,
    pub center: bool,
    /// Lowest and highest biased input seen since `auto` was switched on.
    seen: Option<(f32, f32)>,
}

pub const MAP_LABELS: [&str; 5] = ["From", "to", "To", "to", "Bias"];
pub const BIAS: usize = 4;

impl MapState {
    pub fn new() -> Self {
        MapState {
            chrome: ModuleChrome::new(),
            fields: [field(0.0), field(1.0), field(0.0), field(1.0), field(0.0)],
            clamp: false,
            auto: false,
            center: false,
            seen: None,
        }
    }

    pub fn apply(&self, x: f32) -> f32 {
        let [a, b, c, d, bias] = self.fields.map(|f| f.value);
        let x = x + bias;
        let t = if self.center && a < 0.0 && b > 0.0 {
            if x >= 0.0 { 0.5 + 0.5 * x / b } else { 0.5 - 0.5 * x / a }
        } else if (b - a).abs() < f32::EPSILON {
            0.0
        } else {
            (x - a) / (b - a)
        };
        let y = c + t * (d - c);
        if self.clamp {
            y.clamp(c.min(d), c.max(d))
        } else {
            y
        }
    }

    /// Autorange: widens From to cover every input seen so far.
    pub fn learn(&mut self, input: Option<f32>) {
        if !self.auto {
            return;
        }
        let Some(x) = input.map(|x| x + self.fields[BIAS].value) else { return };
        let (lo, hi) = self.seen.map_or((x, x), |(lo, hi)| (lo.min(x), hi.max(x)));
        self.seen = Some((lo, hi));
        self.fields[0].set(lo);
        self.fields[1].set(hi);
    }

    /// Forgets what autorange has learned.
    pub fn relearn(&mut self) {
        self.seen = None;
    }

    /// Calibrates: the current input becomes zero.
    pub fn calibrate(&mut self, input: f32) {
        self.fields[BIAS].set(-input);
        self.relearn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_behaviour_is_a_map_onto_itself() {
        let mut m = MapState::new();
        m.fields[0].set(0.2);
        m.fields[1].set(0.8);
        m.fields[2].set(0.2);
        m.fields[3].set(0.8);
        m.clamp = true;
        assert_eq!(m.apply(0.5), 0.5);
        assert_eq!(m.apply(2.0), 0.8);
        assert_eq!(m.apply(-1.0), 0.2);
    }

    #[test]
    fn autorange_learns_and_center_splits_at_zero() {
        let mut m = MapState::new();
        m.auto = true;
        m.center = true;
        m.calibrate(0.5);
        for x in [0.5, 0.0, 1.5] {
            m.learn(Some(x));
        }
        assert_eq!((m.fields[0].value, m.fields[1].value), (-0.5, 1.0));
        assert_eq!(m.apply(0.5), 0.5);
        assert_eq!(m.apply(1.5), 1.0);
        assert_eq!(m.apply(0.0), 0.0);
    }
}
