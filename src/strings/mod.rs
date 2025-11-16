//! String interning and constant string pool
//!
//! This module provides string interning for memory efficiency and fast comparison.
//! All strings found in FBX files are interned for deduplication, and our fixed
//! internal strings are considered canonical pointers so we can compare them by address.

pub mod pool;
pub mod constants;

pub use pool::{StringPool, SanitizedString};
pub use constants::{UFBX_STRINGS, get_constant_string};

use std::cmp::Ordering;

/// String comparison utilities
#[inline]
pub fn str_equal(a: &str, b: &str) -> bool {
    a == b
}

#[inline]
pub fn str_less(a: &str, b: &str) -> bool {
    a < b
}

#[inline]
pub fn str_cmp(a: &str, b: &str) -> Ordering {
    a.cmp(b)
}

/// Check if string starts with prefix
#[inline]
pub fn starts_with(s: &str, prefix: &str) -> bool {
    s.starts_with(prefix)
}

/// Check if string ends with suffix
#[inline]
pub fn ends_with(s: &str, suffix: &str) -> bool {
    s.ends_with(suffix)
}

/// Remove prefix from string if present
pub fn remove_prefix<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    s.strip_prefix(prefix)
}

/// Remove suffix from string if present
pub fn remove_suffix<'a>(s: &'a str, suffix: &str) -> Option<&'a str> {
    s.strip_suffix(suffix)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_string_comparison() {
        assert!(str_equal("hello", "hello"));
        assert!(!str_equal("hello", "world"));
        assert!(str_less("abc", "xyz"));
        assert_eq!(str_cmp("test", "test"), Ordering::Equal);
    }

    #[test]
    fn test_prefix_suffix() {
        assert!(starts_with("HelloWorld", "Hello"));
        assert!(ends_with("HelloWorld", "World"));
        assert_eq!(remove_prefix("HelloWorld", "Hello"), Some("World"));
        assert_eq!(remove_suffix("HelloWorld", "World"), Some("Hello"));
    }
}
