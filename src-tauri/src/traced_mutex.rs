//! Traced Mutex for Lock Contention Detection
//!
//! Wraps std::sync::Mutex to automatically log lock acquisition,
//! wait times, and held durations. Helps diagnose deadlocks and
//! performance bottlenecks.

use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// Threshold for logging lock wait time (10ms)
const LOCK_WAIT_WARN_THRESHOLD: Duration = Duration::from_millis(10);

/// Threshold for logging lock held time (50ms)
const LOCK_HELD_WARN_THRESHOLD: Duration = Duration::from_millis(50);

/// Mutex wrapper that traces lock contention
pub struct TracedMutex<T> {
    inner: Mutex<T>,
    name: &'static str,
}

impl<T> TracedMutex<T> {
    /// Create a new traced mutex
    pub fn new(inner: T, name: &'static str) -> Self {
        Self {
            inner: Mutex::new(inner),
            name,
        }
    }

    /// Lock the mutex with tracing
    ///
    /// Logs:
    /// - [LOCK_WAIT] if waiting > 10ms
    /// - [LOCK_ACQUIRE] on successful acquisition
    pub fn lock_traced(&self) -> Result<TracedMutexGuard<'_, T>, String> {
        let start = Instant::now();

        // Attempt to lock
        let guard = self
            .inner
            .lock()
            .map_err(|e| format!("Mutex {} poisoned: {:?}", self.name, e))?;

        let wait_time = start.elapsed();

        // Log if wait time exceeds threshold
        if wait_time > LOCK_WAIT_WARN_THRESHOLD {
            log::warn!(
                "[LOCK_WAIT] mutex={} wait_ms={}",
                self.name,
                wait_time.as_millis()
            );
        }

        log::debug!(
            "[LOCK_ACQUIRE] mutex={} wait_ms={}",
            self.name,
            wait_time.as_micros()
        );

        Ok(TracedMutexGuard {
            guard,
            name: self.name,
            acquired_at: start,
        })
    }

    /// Get the inner value by reference (no lock)
    pub fn get_ref(&self) -> &Mutex<T> {
        &self.inner
    }
}

/// Guard for traced mutex
///
/// Automatically logs when the lock is released and how long it was held.
pub struct TracedMutexGuard<'a, T> {
    guard: MutexGuard<'a, T>,
    name: &'static str,
    acquired_at: Instant,
}

impl<'a, T> std::ops::Deref for TracedMutexGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.guard
    }
}

impl<'a, T> std::ops::DerefMut for TracedMutexGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.guard
    }
}

impl<'a, T> Drop for TracedMutexGuard<'a, T> {
    fn drop(&mut self) {
        let held_time = self.acquired_at.elapsed();

        // Log if held time exceeds threshold
        if held_time > LOCK_HELD_WARN_THRESHOLD {
            log::warn!(
                "[LOCK_HELD_LONG] mutex={} held_ms={}",
                self.name,
                held_time.as_millis()
            );
        }

        log::debug!(
            "[LOCK_RELEASE] mutex={} held_us={}",
            self.name,
            held_time.as_micros()
        );
    }
}

/// Macro to easily create traced mutex
#[macro_export]
macro_rules! traced_mutex {
    ($value:expr, $name:expr) => {
        $crate::traced_mutex::TracedMutex::new($value, $name)
    };
}
