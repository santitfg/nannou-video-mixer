use crate::modules::signal::Signal;
use crate::ModuleChrome;

/// SELECT: takes apart an array (a WLED's inputs, or anything a Map or Lag
/// did to them), or packs single values up into one. Its input is a pill
/// that takes any number of cables; whatever is plugged in is laid end to
/// end as one list, one row per element.
///
/// Clicking a row picks it. Split sends each picked element out of its own
/// port, at its row; Combine sends the picked ones on as one array (all of
/// them if none is picked, which makes it a plain packer).
pub struct SelectState {
    pub chrome: ModuleChrome,
    /// Which elements are picked, by position.
    pub selected: Vec<bool>,
    pub combine: bool,
    /// The latest input, refreshed every tick: drives the rows (and the
    /// module's height). Evaluation reads the live input instead.
    pub values: Vec<f32>,
    pub labels: Vec<String>,
    /// Set by loading an old composition, whose WLED cable carried one
    /// GPIO: the element labelled with it is picked once it shows up, and
    /// that cable moves to its port.
    pub pending_gpio: Option<u8>,
}

impl SelectState {
    pub fn new() -> Self {
        SelectState {
            chrome: ModuleChrome::new(),
            selected: Vec::new(),
            combine: false,
            values: Vec::new(),
            labels: Vec::new(),
            pending_gpio: None,
        }
    }

    pub fn is_selected(&self, element: usize) -> bool {
        self.selected.get(element).copied().unwrap_or(false)
    }

    pub fn toggle(&mut self, element: usize) {
        if self.selected.len() <= element {
            self.selected.resize(element + 1, false);
        }
        self.selected[element] = !self.selected[element];
    }

    /// What output `output` sends, given the input laid out as one list.
    /// Split: output n is element n (if picked). Combine: output 0 only.
    pub fn apply(&self, input: &[f32], output: usize) -> Option<Signal> {
        if self.combine {
            if output != 0 {
                return None;
            }
            let picked: Vec<f32> =
                input.iter().enumerate().filter(|(i, _)| self.is_selected(*i)).map(|(_, x)| *x).collect();
            return Some(Signal::Many(if picked.is_empty() { input.to_vec() } else { picked }));
        }
        self.is_selected(output).then(|| input.get(output).copied()).flatten().map(Signal::One)
    }

    /// The elements whose ports are showing (Split mode), in order.
    pub fn output_elements(&self) -> Vec<usize> {
        if self.combine {
            return vec![0];
        }
        (0..self.values.len()).filter(|&i| self.is_selected(i)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_sends_each_picked_element_on_its_own_port() {
        let mut s = SelectState::new();
        s.values = vec![0.1, 0.2, 0.3];
        s.toggle(0);
        s.toggle(2);
        let input = [0.1, 0.2, 0.3];
        assert_eq!(s.apply(&input, 0), Some(Signal::One(0.1)));
        assert_eq!(s.apply(&input, 1), None, "not picked");
        assert_eq!(s.apply(&input, 2), Some(Signal::One(0.3)));
        assert_eq!(s.output_elements(), [0, 2]);
    }

    #[test]
    fn combine_sends_the_picked_ones_or_everything() {
        let mut s = SelectState::new();
        s.combine = true;
        let input = [0.1, 0.2, 0.3];
        assert_eq!(s.apply(&input, 0), Some(Signal::Many(vec![0.1, 0.2, 0.3])), "packs all");
        s.toggle(1);
        s.toggle(2);
        assert_eq!(s.apply(&input, 0), Some(Signal::Many(vec![0.2, 0.3])));
        assert_eq!(s.apply(&input, 1), None);
    }
}
