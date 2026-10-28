//! What a number cable carries: one value, or an array of them.
//!
//! A WLED controller reports all its inputs at once, so its cable carries
//! them as an array. The math modules (Map, Mult, Lag) work on either: fed
//! an array, they process every element at once and send an array on. A
//! Select splits an array back into single values (or packs singles up).

#[derive(Debug, Clone, PartialEq)]
pub enum Signal {
    One(f32),
    Many(Vec<f32>),
}

impl Signal {
    /// The value, if this is a single one.
    pub fn as_one(&self) -> Option<f32> {
        match self {
            Signal::One(x) => Some(*x),
            Signal::Many(_) => None,
        }
    }

    pub fn is_many(&self) -> bool {
        matches!(self, Signal::Many(_))
    }

    /// Every value, in order (a single value is a list of one).
    pub fn values(&self) -> &[f32] {
        match self {
            Signal::One(x) => std::slice::from_ref(x),
            Signal::Many(v) => v,
        }
    }

    pub fn map(&self, f: impl Fn(f32) -> f32) -> Signal {
        match self {
            Signal::One(x) => Signal::One(f(*x)),
            Signal::Many(v) => Signal::Many(v.iter().map(|x| f(*x)).collect()),
        }
    }

    /// Element by element. A single value pairs with every element of an
    /// array; two arrays pair up to the shorter one's length.
    pub fn zip_with(&self, other: &Signal, f: impl Fn(f32, f32) -> f32) -> Signal {
        match (self, other) {
            (Signal::One(a), Signal::One(b)) => Signal::One(f(*a, *b)),
            (Signal::One(a), Signal::Many(v)) => Signal::Many(v.iter().map(|b| f(*a, *b)).collect()),
            (Signal::Many(v), Signal::One(b)) => Signal::Many(v.iter().map(|a| f(*a, *b)).collect()),
            (Signal::Many(a), Signal::Many(b)) => Signal::Many(a.iter().zip(b).map(|(a, b)| f(*a, *b)).collect()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_values_broadcast_over_arrays() {
        let arr = Signal::Many(vec![0.5, 1.0]);
        assert_eq!(arr.zip_with(&Signal::One(2.0), |a, b| a * b), Signal::Many(vec![1.0, 2.0]));
        assert_eq!(Signal::One(2.0).zip_with(&arr, |a, b| a * b), Signal::Many(vec![1.0, 2.0]));
        assert_eq!(Signal::One(2.0).zip_with(&Signal::One(3.0), |a, b| a * b), Signal::One(6.0));
    }

    #[test]
    fn arrays_pair_up_to_the_shorter() {
        let a = Signal::Many(vec![1.0, 2.0, 3.0]);
        let b = Signal::Many(vec![10.0, 20.0]);
        assert_eq!(a.zip_with(&b, |a, b| a + b), Signal::Many(vec![11.0, 22.0]));
        assert_eq!(a.map(|x| -x).values(), [-1.0, -2.0, -3.0]);
        assert_eq!(Signal::One(4.0).values(), [4.0]);
    }
}
