use crate::modules::param::Param;
use crate::ModuleChrome;

/// NUMBER: a constant, with no inputs — drag its box, click it to type a
/// value, or right-click to change its range. Its cable carries where the
/// value sits within that range (0..1).
pub struct NumberState {
    pub chrome: ModuleChrome,
    pub value: Param,
}

impl NumberState {
    pub fn new() -> Self {
        NumberState {
            chrome: ModuleChrome::new(),
            value: Param::unit(0.0),
        }
    }
}
