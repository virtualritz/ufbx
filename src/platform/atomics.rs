//! Atomic operations
//!
//! This module provides thread-safe atomic counter operations,
//! corresponding to the C atomic counter implementation in ufbx.c lines 631-716.

use std::sync::atomic::{AtomicUsize, Ordering};

/// Thread-safe atomic counter
///
/// Wraps `AtomicUsize` to provide an interface compatible with the C version.
#[derive(Debug)]
pub struct AtomicCounter {
    value: AtomicUsize,
}

impl AtomicCounter {
    /// Create a new atomic counter initialized to zero
    #[inline]
    pub const fn new() -> Self {
        Self {
            value: AtomicUsize::new(0),
        }
    }

    /// Initialize the counter to zero
    ///
    /// This is a no-op in Rust since the counter is already initialized.
    #[inline]
    pub fn init(&self) {
        self.value.store(0, Ordering::SeqCst);
    }

    /// Free the counter
    ///
    /// This is a no-op in Rust due to automatic memory management.
    #[inline]
    pub fn free(&self) {
        // No-op in Rust
    }

    /// Increment the counter and return the previous value
    ///
    /// Uses sequentially consistent ordering to match the C implementation.
    #[inline]
    pub fn inc(&self) -> usize {
        self.value.fetch_add(1, Ordering::SeqCst)
    }

    /// Decrement the counter and return the previous value
    ///
    /// Uses sequentially consistent ordering to match the C implementation.
    #[inline]
    pub fn dec(&self) -> usize {
        self.value.fetch_sub(1, Ordering::SeqCst)
    }

    /// Load the current value of the counter
    ///
    /// Uses acquire ordering to ensure proper synchronization.
    #[inline]
    pub fn load(&self) -> usize {
        self.value.load(Ordering::Acquire)
    }

    /// Store a value to the counter
    #[inline]
    pub fn store(&self, value: usize) {
        self.value.store(value, Ordering::SeqCst);
    }

    /// Get a mutable reference to the inner value (not thread-safe)
    #[inline]
    pub fn get_mut(&mut self) -> &mut usize {
        self.value.get_mut()
    }
}

impl Default for AtomicCounter {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for AtomicCounter {
    fn clone(&self) -> Self {
        Self {
            value: AtomicUsize::new(self.load()),
        }
    }
}

/// Check if thread-safe operations are available
///
/// In Rust with std, atomics are always available.
pub const THREAD_SAFE: bool = true;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn test_atomic_counter() {
        let counter = AtomicCounter::new();
        assert_eq!(counter.load(), 0);

        let prev = counter.inc();
        assert_eq!(prev, 0);
        assert_eq!(counter.load(), 1);

        let prev = counter.inc();
        assert_eq!(prev, 1);
        assert_eq!(counter.load(), 2);

        let prev = counter.dec();
        assert_eq!(prev, 2);
        assert_eq!(counter.load(), 1);
    }

    #[test]
    fn test_atomic_counter_init() {
        let counter = AtomicCounter::new();
        counter.store(42);
        assert_eq!(counter.load(), 42);

        counter.init();
        assert_eq!(counter.load(), 0);
    }

    #[test]
    fn test_thread_safety() {
        let counter = Arc::new(AtomicCounter::new());
        let mut handles = vec![];

        // Spawn 10 threads, each incrementing 100 times
        for _ in 0..10 {
            let counter_clone = Arc::clone(&counter);
            let handle = thread::spawn(move || {
                for _ in 0..100 {
                    counter_clone.inc();
                }
            });
            handles.push(handle);
        }

        // Wait for all threads to complete
        for handle in handles {
            handle.join().unwrap();
        }

        assert_eq!(counter.load(), 1000);
    }

    #[test]
    fn test_clone() {
        let counter1 = AtomicCounter::new();
        counter1.store(42);

        let counter2 = counter1.clone();
        assert_eq!(counter2.load(), 42);

        counter1.inc();
        assert_eq!(counter1.load(), 43);
        assert_eq!(counter2.load(), 42); // Clone is independent
    }

    #[test]
    fn test_thread_safe_constant() {
        assert!(THREAD_SAFE);
    }
}
