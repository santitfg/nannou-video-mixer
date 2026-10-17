//! A number box's value plus how it treats what's wired into it.
//!
//! Number signals travel along cables normalized to 0..1 (an LFO swings
//! 0..1, a Number sends where its value sits within its own range). Each
//! receiving box maps that 0..1 onto its useful range `min..max`, and may
//! clamp the result to that range. Drawing always uses `norm`, which is
//! bounded, so a slider never spills out of its box.

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Param {
    pub value: f32,
    pub min: f32,
    pub max: f32,
    pub clamp: bool,
}

impl Param {
    /// The default box: 0..1, clamped.
    pub fn unit(value: f32) -> Self {
        Param::ranged(value, 0.0, 1.0)
    }

    pub fn ranged(value: f32, min: f32, max: f32) -> Self {
        let mut p = Param { value, min, max, clamp: true };
        p.value = p.limit(value);
        p
    }

    fn span(&self) -> f32 {
        self.max - self.min
    }

    /// `v` held inside the range if this box clamps; untouched otherwise.
    pub fn limit(&self, v: f32) -> f32 {
        if self.clamp {
            v.clamp(self.min.min(self.max), self.min.max(self.max))
        } else {
            v
        }
    }

    /// The value this box uses: the wired signal (0..1) mapped onto the
    /// range, or its own value when nothing's plugged in.
    pub fn resolve(&self, input: Option<f32>) -> f32 {
        match input {
            Some(x) => self.limit(self.min + x * self.span()),
            None => self.value,
        }
    }

    /// Where `v` sits in the range, 0..1, for drawing. Always bounded.
    pub fn norm(&self, v: f32) -> f32 {
        if self.span().abs() < f32::EPSILON {
            return 0.0;
        }
        ((v - self.min) / self.span()).clamp(0.0, 1.0)
    }

    /// What a source box (Number) sends down its cable: its value as a
    /// fraction of its range. Unbounded when the box doesn't clamp.
    pub fn output_norm(&self) -> f32 {
        if self.span().abs() < f32::EPSILON {
            return 0.0;
        }
        (self.value - self.min) / self.span()
    }

    /// Moves the value by `dt` of the full range (drag-to-adjust).
    pub fn nudge(&mut self, dt: f32) {
        self.value = self.limit(self.value + dt * self.span());
    }

    pub fn set(&mut self, v: f32) {
        if v.is_finite() {
            self.value = self.limit(v);
        }
    }

    /// New range bounds; the value is re-limited to fit.
    pub fn set_range(&mut self, min: f32, max: f32) {
        if min.is_finite() && max.is_finite() {
            self.min = min;
            self.max = max;
            self.value = self.limit(self.value);
        }
    }

    pub fn set_clamp(&mut self, clamp: bool) {
        self.clamp = clamp;
        self.value = self.limit(self.value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_input_onto_range() {
        let p = Param::ranged(0.0, 2.0, 10.0);
        assert_eq!(p.resolve(Some(0.0)), 2.0);
        assert_eq!(p.resolve(Some(0.5)), 6.0);
        assert_eq!(p.resolve(Some(1.0)), 10.0);
        assert_eq!(p.resolve(None), 2.0);
    }

    #[test]
    fn clamps_only_when_asked() {
        let mut p = Param::unit(0.5);
        assert_eq!(p.resolve(Some(5.0)), 1.0);
        p.set_clamp(false);
        assert_eq!(p.resolve(Some(5.0)), 5.0);
        p.set(3.0);
        assert_eq!(p.value, 3.0);
    }

    #[test]
    fn norm_is_always_bounded() {
        let mut p = Param::unit(0.0);
        p.set_clamp(false);
        assert_eq!(p.norm(5.0), 1.0);
        assert_eq!(p.norm(-5.0), 0.0);
        assert_eq!(p.norm(0.25), 0.25);
    }

    #[test]
    fn degenerate_and_inverted_ranges() {
        let p = Param::ranged(3.0, 3.0, 3.0);
        assert_eq!(p.norm(3.0), 0.0);
        assert_eq!(p.output_norm(), 0.0);
        let q = Param::ranged(0.5, 1.0, 0.0); // inverted
        assert_eq!(q.resolve(Some(1.0)), 0.0);
        assert_eq!(q.limit(2.0), 1.0);
    }

    #[test]
    fn number_sends_its_fraction() {
        let p = Param::ranged(5.0, 0.0, 10.0);
        assert_eq!(p.output_norm(), 0.5);
    }

    #[test]
    fn ignores_non_finite_input() {
        let mut p = Param::unit(0.3);
        p.set(f32::NAN);
        assert_eq!(p.value, 0.3);
    }
}
