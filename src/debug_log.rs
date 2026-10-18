//! A tiny in-app debug log: a ring buffer of lines any thread can push to
//! (camera capture threads included), drawn on screen when enabled from
//! Settings and echoed to stderr. When disabled, `dlog!` doesn't even
//! format its arguments.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

const CAPACITY: usize = 200;

static ENABLED: AtomicBool = AtomicBool::new(false);
static LINES: Mutex<VecDeque<String>> = Mutex::new(VecDeque::new());
static START: OnceLock<Instant> = OnceLock::new();

pub fn set_enabled(on: bool) {
    START.get_or_init(Instant::now);
    ENABLED.store(on, Ordering::Relaxed);
}

pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

pub fn push(message: String) {
    let t = START.get_or_init(Instant::now).elapsed().as_secs_f32();
    let line = format!("[{t:8.2}] {message}");
    eprintln!("{line}");
    let mut lines = crate::sync::lock(&LINES);
    if lines.len() == CAPACITY {
        lines.pop_front();
    }
    lines.push_back(line);
}

/// The newest `n` lines, oldest first.
pub fn recent(n: usize) -> Vec<String> {
    let lines = crate::sync::lock(&LINES);
    lines.iter().skip(lines.len().saturating_sub(n)).cloned().collect()
}

#[macro_export]
macro_rules! dlog {
    ($($arg:tt)*) => {
        if $crate::debug_log::enabled() {
            $crate::debug_log::push(format!($($arg)*));
        }
    };
}
