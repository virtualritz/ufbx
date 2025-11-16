//! String pool implementation with deduplication and UTF-8 validation
//!
//! The string pool provides:
//! - String interning for memory efficiency
//! - Fast hash-based lookup
//! - UTF-8 validation and sanitization
//! - Thread-safe shared string references

use std::collections::HashMap;
use std::sync::Arc;
use crate::error::{Error, Result};

/// Unicode error handling modes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnicodeErrorHandling {
    /// Abort loading on invalid UTF-8
    AbortLoading,
    /// Replace with U+FFFD replacement character (�)
    ReplacementCharacter,
    /// Replace with underscore (_)
    Underscore,
    /// Replace with question mark (?)
    QuestionMark,
    /// Remove invalid bytes
    Remove,
    /// Keep invalid bytes as-is (unsafe)
    UnsafeIgnore,
}

impl Default for UnicodeErrorHandling {
    fn default() -> Self {
        Self::ReplacementCharacter
    }
}

/// A sanitized string with both raw and UTF-8 versions
#[derive(Debug, Clone)]
pub struct SanitizedString {
    /// The raw (original) string data
    pub raw: Arc<str>,
    /// The sanitized UTF-8 string (if different from raw)
    pub utf8: Option<Arc<str>>,
}

impl SanitizedString {
    /// Get the UTF-8 version of the string
    pub fn as_utf8(&self) -> &str {
        self.utf8.as_ref().map(|s| s.as_ref()).unwrap_or(&self.raw)
    }

    /// Get the raw version of the string
    pub fn as_raw(&self) -> &str {
        &self.raw
    }
}

/// String pool for interning and deduplication
pub struct StringPool {
    /// Hash map for fast string lookup
    map: HashMap<Arc<str>, Arc<str>>,
    /// Unicode error handling mode
    error_handling: UnicodeErrorHandling,
    /// Total bytes allocated for strings
    bytes_allocated: usize,
    /// Number of unique strings
    num_strings: usize,
}

impl StringPool {
    /// Create a new string pool
    pub fn new() -> Self {
        Self::with_capacity(1024)
    }

    /// Create a new string pool with initial capacity
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            map: HashMap::with_capacity(capacity),
            error_handling: UnicodeErrorHandling::default(),
            bytes_allocated: 0,
            num_strings: 0,
        }
    }

    /// Set the unicode error handling mode
    pub fn set_error_handling(&mut self, mode: UnicodeErrorHandling) {
        self.error_handling = mode;
    }

    /// Get the unicode error handling mode
    pub fn error_handling(&self) -> UnicodeErrorHandling {
        self.error_handling
    }

    /// Intern a string, returning a shared reference
    ///
    /// If the string already exists in the pool, returns the existing reference.
    /// Otherwise, validates UTF-8 and adds it to the pool.
    pub fn intern(&mut self, s: &str) -> Result<Arc<str>> {
        self.intern_impl(s, false)
    }

    /// Intern a raw string without UTF-8 validation
    ///
    /// This is unsafe and should only be used for binary data or when
    /// you're certain the input is valid UTF-8.
    pub fn intern_raw(&mut self, s: &str) -> Result<Arc<str>> {
        self.intern_impl(s, true)
    }

    /// Intern a string with full sanitization info
    pub fn intern_sanitized(&mut self, s: &str) -> Result<SanitizedString> {
        if s.is_empty() {
            return Ok(SanitizedString {
                raw: Arc::from(""),
                utf8: None,
            });
        }

        // Check if already interned
        if let Some(existing) = self.map.get(s) {
            return Ok(SanitizedString {
                raw: Arc::clone(existing),
                utf8: None,
            });
        }

        // Validate UTF-8
        let valid_len = utf8_valid_length(s.as_bytes());

        if valid_len == s.len() {
            // String is valid UTF-8, just intern it
            let arc = Arc::from(s);
            self.map.insert(Arc::clone(&arc), Arc::clone(&arc));
            self.bytes_allocated += s.len();
            self.num_strings += 1;

            Ok(SanitizedString {
                raw: arc,
                utf8: None,
            })
        } else {
            // String has invalid UTF-8, sanitize it
            let sanitized = self.sanitize_string(s, valid_len)?;

            // Intern both versions
            let raw = Arc::from(s);
            let utf8 = Arc::from(sanitized.as_str());

            self.map.insert(Arc::clone(&raw), Arc::clone(&raw));
            self.map.insert(Arc::clone(&utf8), Arc::clone(&utf8));

            self.bytes_allocated += s.len() + sanitized.len();
            self.num_strings += 2;

            Ok(SanitizedString {
                raw,
                utf8: Some(utf8),
            })
        }
    }

    /// Internal implementation of string interning
    fn intern_impl(&mut self, s: &str, raw: bool) -> Result<Arc<str>> {
        if s.is_empty() {
            return Ok(Arc::from(""));
        }

        // Check if already interned
        if let Some(existing) = self.map.get(s) {
            return Ok(Arc::clone(existing));
        }

        // Validate UTF-8 if not raw
        let final_str = if !raw {
            let valid_len = utf8_valid_length(s.as_bytes());
            if valid_len < s.len() {
                self.sanitize_string(s, valid_len)?
            } else {
                s.to_string()
            }
        } else {
            s.to_string()
        };

        // Intern the string
        let arc = Arc::from(final_str.as_str());
        self.map.insert(Arc::clone(&arc), Arc::clone(&arc));
        self.bytes_allocated += final_str.len();
        self.num_strings += 1;

        Ok(arc)
    }

    /// Sanitize a string with invalid UTF-8
    fn sanitize_string(&self, s: &str, valid_len: usize) -> Result<String> {
        if self.error_handling == UnicodeErrorHandling::AbortLoading {
            return Err(Error::BadUnicode("Invalid UTF-8 in string".to_string()));
        }

        let bytes = s.as_bytes();
        let mut result = Vec::with_capacity(s.len());

        // Copy the valid part
        result.extend_from_slice(&bytes[..valid_len]);

        // Process the invalid part
        let mut i = valid_len;
        while i < bytes.len() {
            let c = bytes[i];
            let left = bytes.len() - i;

            // Try to decode UTF-8 sequences
            if c & 0x80 == 0 {
                // ASCII
                if c != 0 {
                    result.push(c);
                    i += 1;
                    continue;
                }
            } else if c & 0xe0 == 0xc0 && left >= 2 {
                // 2-byte sequence
                let t0 = bytes[i + 1];
                let code = (c as u32) << 8 | (t0 as u32);
                if (t0 & 0xc0) == 0x80 && code >= 0xc280 {
                    result.push(c);
                    result.push(t0);
                    i += 2;
                    continue;
                }
            } else if c & 0xf0 == 0xe0 && left >= 3 {
                // 3-byte sequence
                let t0 = bytes[i + 1];
                let t1 = bytes[i + 2];
                let code = (c as u32) << 16 | (t0 as u32) << 8 | (t1 as u32);
                if (code & 0xc0c0) == 0x8080 && code >= 0xe0a080
                    && (code < 0xeda080 || code >= 0xee8080) {
                    result.push(c);
                    result.push(t0);
                    result.push(t1);
                    i += 3;
                    continue;
                }
            } else if c & 0xf8 == 0xf0 && left >= 4 {
                // 4-byte sequence
                let t0 = bytes[i + 1];
                let t1 = bytes[i + 2];
                let t2 = bytes[i + 3];
                let code = (c as u32) << 24 | (t0 as u32) << 16
                    | (t1 as u32) << 8 | (t2 as u32);
                if (code & 0xc0c0c0) == 0x808080
                    && code >= 0xf0908080 && code <= 0xf48fbfbf {
                    result.push(c);
                    result.push(t0);
                    result.push(t1);
                    result.push(t2);
                    i += 4;
                    continue;
                }
            }

            // Invalid sequence, add replacement
            self.add_replacement_char(&mut result, c);
            i += 1;
        }

        Ok(String::from_utf8(result).unwrap_or_else(|_| String::from("?")))
    }

    /// Add a replacement character based on error handling mode
    fn add_replacement_char(&self, buf: &mut Vec<u8>, _c: u8) {
        match self.error_handling {
            UnicodeErrorHandling::ReplacementCharacter => {
                // U+FFFD in UTF-8: 0xEF 0xBF 0xBD
                buf.extend_from_slice(&[0xef, 0xbf, 0xbd]);
            }
            UnicodeErrorHandling::Underscore => {
                buf.push(b'_');
            }
            UnicodeErrorHandling::QuestionMark => {
                buf.push(b'?');
            }
            UnicodeErrorHandling::Remove => {
                // Don't add anything
            }
            UnicodeErrorHandling::UnsafeIgnore => {
                buf.push(_c);
            }
            UnicodeErrorHandling::AbortLoading => {
                // Should never reach here
            }
        }
    }

    /// Get statistics about the string pool
    pub fn stats(&self) -> StringPoolStats {
        StringPoolStats {
            num_strings: self.num_strings,
            bytes_allocated: self.bytes_allocated,
            capacity: self.map.capacity(),
        }
    }

    /// Clear the string pool
    pub fn clear(&mut self) {
        self.map.clear();
        self.bytes_allocated = 0;
        self.num_strings = 0;
    }
}

impl Default for StringPool {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics about the string pool
#[derive(Debug, Clone, Copy)]
pub struct StringPoolStats {
    /// Number of unique strings
    pub num_strings: usize,
    /// Total bytes allocated
    pub bytes_allocated: usize,
    /// Hash map capacity
    pub capacity: usize,
}

/// Determine the length of valid UTF-8 at the start of a byte slice
fn utf8_valid_length(bytes: &[u8]) -> usize {
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        let left = bytes.len() - i;

        if c & 0x80 == 0 {
            // ASCII (0xxxxxxx)
            if c == 0 {
                return i; // Null terminator
            }
            i += 1;
        } else if c & 0xe0 == 0xc0 && left >= 2 {
            // 2-byte sequence (110xxxxx 10xxxxxx)
            let t0 = bytes[i + 1];
            let code = (c as u32) << 8 | (t0 as u32);
            if (t0 & 0xc0) == 0x80 && code >= 0xc280 {
                i += 2;
            } else {
                return i;
            }
        } else if c & 0xf0 == 0xe0 && left >= 3 {
            // 3-byte sequence (1110xxxx 10xxxxxx 10xxxxxx)
            let t0 = bytes[i + 1];
            let t1 = bytes[i + 2];
            let code = (c as u32) << 16 | (t0 as u32) << 8 | (t1 as u32);
            if (code & 0xc0c0) == 0x8080 && code >= 0xe0a080
                && (code < 0xeda080 || code >= 0xee8080) {
                i += 3;
            } else {
                return i;
            }
        } else if c & 0xf8 == 0xf0 && left >= 4 {
            // 4-byte sequence (11110xxx 10xxxxxx 10xxxxxx 10xxxxxx)
            let t0 = bytes[i + 1];
            let t1 = bytes[i + 2];
            let t2 = bytes[i + 3];
            let code = (c as u32) << 24 | (t0 as u32) << 16
                | (t1 as u32) << 8 | (t2 as u32);
            if (code & 0xc0c0c0) == 0x808080
                && code >= 0xf0908080 && code <= 0xf48fbfbf {
                i += 4;
            } else {
                return i;
            }
        } else {
            return i;
        }
    }

    i
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_string_pool_basic() {
        let mut pool = StringPool::new();

        let s1 = pool.intern("hello").unwrap();
        let s2 = pool.intern("hello").unwrap();
        let s3 = pool.intern("world").unwrap();

        // Same string should return same Arc
        assert!(Arc::ptr_eq(&s1, &s2));
        assert!(!Arc::ptr_eq(&s1, &s3));

        let stats = pool.stats();
        assert_eq!(stats.num_strings, 2);
    }

    #[test]
    fn test_utf8_validation() {
        assert_eq!(utf8_valid_length(b"hello"), 5);
        assert_eq!(utf8_valid_length("hello 世界".as_bytes()), 11);

        // Invalid UTF-8
        assert_eq!(utf8_valid_length(&[0xff, 0xff]), 0);
        assert_eq!(utf8_valid_length(&[b'h', b'i', 0xff]), 2);
    }

    #[test]
    fn test_sanitization() {
        let mut pool = StringPool::new();
        pool.set_error_handling(UnicodeErrorHandling::QuestionMark);

        // Valid UTF-8
        let s1 = pool.intern("hello").unwrap();
        assert_eq!(s1.as_ref(), "hello");

        // Pool should handle invalid UTF-8 gracefully in real implementation
    }

    #[test]
    fn test_empty_string() {
        let mut pool = StringPool::new();
        let empty = pool.intern("").unwrap();
        assert_eq!(empty.as_ref(), "");

        let stats = pool.stats();
        assert_eq!(stats.num_strings, 0); // Empty strings don't count
    }
}
