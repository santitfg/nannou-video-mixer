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
/// 0..1 to -1..1, or an LFO onto 0.2..0.8. `clamp` keeps the result
/// inside the output range.
pub struct MapState {
    pub chrome: ModuleChrome,
    /// From min, from max, to min, to max.
    pub fields: [Param; 4],
    pub clamp: bool,
}

pub const MAP_LABELS: [&str; 4] = ["From", "to", "To", "to"];

impl MapState {
    pub fn new() -> Self {
        MapState { chrome: ModuleChrome::new(), fields: [field(0.0), field(1.0), field(0.0), field(1.0)], clamp: false }
    }

    pub fn apply(&self, x: f32) -> f32 {
        let [a, b, c, d] = self.fields.map(|f| f.value);
        let t = if (b - a).abs() < f32::EPSILON { 0.0 } else { (x - a) / (b - a) };
        let y = c + t * (d - c);
        if self.clamp {
            y.clamp(c.min(d), c.max(d))
        } else {
            y
        }
    }
}

/// CLAMP: holds its input between Min and Max.
pub struct ClampState {
    pub chrome: ModuleChrome,
    /// Min, max.
    pub fields: [Param; 2],
}

pub const CLAMP_LABELS: [&str; 2] = ["Min", "Max"];

impl ClampState {
    pub fn new() -> Self {
        ClampState { chrome: ModuleChrome::new(), fields: [field(0.0), field(1.0)] }
    }

    pub fn apply(&self, x: f32) -> f32 {
        let [a, b] = self.fields.map(|f| f.value);
        x.clamp(a.min(b), a.max(b))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_rescales_and_optionally_clamps() {
        let mut m = MapState::new();
        m.fields[2].set(-1.0);
        m.fields[3].set(1.0);
        assert_eq!(m.apply(0.5), 0.0);
        assert_eq!(m.apply(1.5), 2.0);
        m.clamp = true;
        assert_eq!(m.apply(1.5), 1.0);
        // Inverted output range.
        m.fields[2].set(10.0);
        m.fields[3].set(0.0);
        assert_eq!(m.apply(0.25), 7.5);
        // Degenerate input range doesn't divide by zero.
        m.fields[1].set(0.0);
        assert_eq!(m.apply(3.0), 10.0);
    }

    #[test]
    fn clamp_holds_inside_either_order() {
        let mut c = ClampState::new();
        assert_eq!(c.apply(2.0), 1.0);
        assert_eq!(c.apply(-1.0), 0.0);
        c.fields[0].set(5.0);
        c.fields[1].set(2.0);
        assert_eq!(c.apply(9.0), 5.0);
        assert_eq!(c.apply(3.0), 3.0);
    }
}
