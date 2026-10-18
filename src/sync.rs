//! Mutex helpers that never panic or block the render loop on a worker.

use std::sync::{Mutex, MutexGuard, TryLockError};

/// Locks, recovering the data if a panicking thread poisoned the mutex.
pub fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Locks only if nobody holds it right now (poison is ignored).
pub fn try_lock<T>(m: &Mutex<T>) -> Option<MutexGuard<'_, T>> {
    match m.try_lock() {
        Ok(g) => Some(g),
        Err(TryLockError::Poisoned(e)) => Some(e.into_inner()),
        Err(TryLockError::WouldBlock) => None,
    }
}
