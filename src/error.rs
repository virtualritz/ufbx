//! Error handling and reporting
//!
//! This module provides comprehensive error handling for ufbx operations using
//! the `thiserror` crate for idiomatic Rust error handling.
//!
//! # Error System
//!
//! The error system provides:
//! - Specific error types using `thiserror` for automatic trait implementations
//! - Error stack traces (up to 8 frames) when enabled
//! - Additional context information (e.g., filenames, sizes)
//! - UTF-8 safe error messages
//! - Proper error source chains
//!
//! # Example
//!
//! ```rust
//! use ufbx::error::{Error, Result};
//!
//! fn load_file(path: &str) -> Result<()> {
//!     if path.is_empty() {
//!         return Err(Error::FileNotFound {
//!             path: "".to_string()
//!         });
//!     }
//!     Ok(())
//! }
//! ```

use thiserror::Error as ThisError;
use std::fmt;

/// Maximum depth of the error stack trace
pub const ERROR_STACK_MAX_DEPTH: usize = 8;

/// Maximum length of additional error information
pub const ERROR_INFO_LENGTH: usize = 256;

/// A single frame in the error stack trace
#[derive(Debug, Clone)]
pub struct ErrorFrame {
    /// Source line number where the error occurred
    pub source_line: u32,

    /// Function name where the error occurred
    pub function: String,

    /// Description of what failed at this frame
    pub description: String,
}

impl ErrorFrame {
    /// Create a new error frame
    pub fn new(source_line: u32, function: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            source_line,
            function: function.into(),
            description: description.into(),
        }
    }
}

impl fmt::Display for ErrorFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at {}:{}", self.description, self.function, self.source_line)
    }
}

/// Comprehensive error type for all ufbx operations
///
/// This enum uses `thiserror` to automatically derive `Display` and `std::error::Error`
/// implementations. Each variant corresponds to a specific failure mode from the C API.
#[derive(ThisError, Debug)]
pub enum Error {
    /// File not found at the specified path
    #[error("File not found: {path}")]
    FileNotFound {
        /// Path to the file that was not found
        path: String,
    },

    /// File exists but is empty (0 bytes)
    #[error("Empty file: {path}")]
    EmptyFile {
        /// Path to the empty file
        path: String,
    },

    /// External file referenced in FBX not found
    ///
    /// This occurs when loading with `load_external_files` enabled and a referenced
    /// texture, geometry cache, or other external file cannot be found.
    #[error("External file not found: {path}")]
    ExternalFileNotFound {
        /// Path to the external file that was not found
        path: String,
    },

    /// Out of memory - allocator returned NULL/None
    #[error("Out of memory: failed to allocate {size} bytes")]
    OutOfMemory {
        /// Size of the allocation that failed
        size: usize,
    },

    /// Memory limit from allocator options exhausted
    #[error("Memory limit exceeded: {used} bytes used, {limit} bytes limit")]
    MemoryLimit {
        /// Bytes currently allocated
        used: usize,
        /// Maximum allowed bytes
        limit: usize,
    },

    /// Allocation limit from allocator options exhausted
    #[error("Allocation limit exceeded: {count} allocations, {limit} limit")]
    AllocationLimit {
        /// Number of allocations made
        count: usize,
        /// Maximum allowed allocations
        limit: usize,
    },

    /// File ended abruptly during parsing
    #[error("Truncated file: unexpected end of file at offset {offset}")]
    TruncatedFile {
        /// Byte offset where truncation was detected
        offset: u64,
    },

    /// IO read error from stream
    #[error("IO error: {message}")]
    Io {
        /// Error message from the IO operation
        message: String,
    },

    /// User cancelled the loading operation via progress callback
    #[error("Operation cancelled by user")]
    Cancelled,

    /// Could not detect file format from data or filename
    #[error("Unrecognized file format: {filename}")]
    UnrecognizedFileFormat {
        /// Filename that was being analyzed
        filename: String,
    },

    /// Options struct not initialized to zero
    #[error("Uninitialized options: options struct must be zero-initialized")]
    UninitializedOptions,

    /// Vertex streams in `generate_indices()` are empty
    #[error("Zero vertex size: vertex streams cannot be empty")]
    ZeroVertexSize,

    /// Truncated vertex stream passed to `generate_indices()`
    #[error("Truncated vertex stream: expected {expected} vertices, got {actual}")]
    TruncatedVertexStream {
        /// Expected number of vertices
        expected: usize,
        /// Actual number of vertices in stream
        actual: usize,
    },

    /// Invalid UTF-8 encountered when loading with abort-on-error mode
    #[error("Invalid UTF-8 at offset {offset}")]
    InvalidUtf8 {
        /// Byte offset of the invalid UTF-8 sequence
        offset: usize,
    },

    /// Required feature has been compiled out
    #[error("Feature disabled: {feature}")]
    FeatureDisabled {
        /// Name of the disabled feature
        feature: String,
    },

    /// Attempting to tessellate invalid NURBS geometry
    #[error("Bad NURBS geometry: {reason}")]
    BadNurbs {
        /// Reason why the NURBS is invalid
        reason: String,
    },

    /// Out of bounds index when loading with abort-on-error mode
    #[error("Bad index: {index} out of bounds (max: {max})")]
    BadIndex {
        /// The out-of-bounds index
        index: usize,
        /// Maximum valid index
        max: usize,
    },

    /// Node deeper than `node_depth_limit` in hierarchy
    #[error("Node depth limit exceeded: depth {depth}, limit {limit}")]
    NodeDepthLimit {
        /// Actual node depth
        depth: usize,
        /// Maximum allowed depth
        limit: usize,
    },

    /// Error parsing ASCII array in thread
    #[error("Threaded ASCII parse error: {message}")]
    ThreadedAsciiParse {
        /// Description of the parse error
        message: String,
    },

    /// Unsafe options specified without `allow_unsafe`
    #[error("Unsafe options: {option} requires allow_unsafe=true")]
    UnsafeOptions {
        /// Name of the unsafe option
        option: String,
    },

    /// Duplicated override property in animation creation
    #[error("Duplicate override: property '{property}' already overridden")]
    DuplicateOverride {
        /// Name of the duplicated property
        property: String,
    },

    /// Unsupported file format version
    #[error("Unsupported version: FBX version {version}")]
    UnsupportedVersion {
        /// Version number that is unsupported
        version: u32,
    },

    /// Unspecified error, likely caused by invalid FBX file
    ///
    /// This is a catch-all for errors that don't fit other categories.
    /// It includes an error stack for debugging.
    #[error("Unknown error: {description}")]
    Unknown {
        /// Human-readable description
        description: String,
        /// Error stack trace (when available)
        #[source]
        stack: Option<Box<ErrorStack>>,
    },
}

/// Error stack information for complex error scenarios
///
/// This type is used with the `Unknown` error variant to provide
/// detailed stack traces for debugging complex failures.
#[derive(Debug, Clone)]
pub struct ErrorStack {
    /// Stack frames (up to ERROR_STACK_MAX_DEPTH)
    pub frames: Vec<ErrorFrame>,

    /// Additional context information (UTF-8 validated)
    pub info: String,
}

impl ErrorStack {
    /// Create a new empty error stack
    pub fn new() -> Self {
        Self {
            frames: Vec::new(),
            info: String::new(),
        }
    }

    /// Add a frame to the stack (respects max depth)
    pub fn push_frame(&mut self, frame: ErrorFrame) {
        if self.frames.len() < ERROR_STACK_MAX_DEPTH {
            self.frames.push(frame);
        }
    }

    /// Set additional context information
    pub fn set_info(&mut self, info: impl Into<String>) {
        let info_str = info.into();
        self.info = clean_utf8(&info_str, ERROR_INFO_LENGTH);
    }
}

impl fmt::Display for ErrorStack {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !self.info.is_empty() {
            writeln!(f, "Info: {}", self.info)?;
        }
        if !self.frames.is_empty() {
            writeln!(f, "Stack trace:")?;
            for (i, frame) in self.frames.iter().enumerate() {
                writeln!(f, "  #{}: {}", i, frame)?;
            }
        }
        Ok(())
    }
}

impl std::error::Error for ErrorStack {}

impl Default for ErrorStack {
    fn default() -> Self {
        Self::new()
    }
}

impl Error {
    /// Create an unknown error with stack information
    pub fn unknown(description: impl Into<String>) -> Self {
        Error::Unknown {
            description: description.into(),
            stack: None,
        }
    }

    /// Create an unknown error with a stack
    pub fn unknown_with_stack(description: impl Into<String>, stack: ErrorStack) -> Self {
        Error::Unknown {
            description: description.into(),
            stack: Some(Box::new(stack)),
        }
    }

    /// Create a file not found error
    pub fn file_not_found(path: impl Into<String>) -> Self {
        Error::FileNotFound { path: path.into() }
    }

    /// Create an IO error from a source error
    pub fn io(message: impl Into<String>) -> Self {
        Error::Io {
            message: message.into(),
        }
    }

    /// Create an IO error from a std::io::Error
    pub fn from_io_error(err: std::io::Error) -> Self {
        Error::Io {
            message: err.to_string(),
        }
    }

    /// Get a static string description of the error type
    ///
    /// This matches the C API's error descriptions and is useful for
    /// consistent error reporting across language boundaries.
    pub fn type_description(&self) -> &'static str {
        match self {
            Error::FileNotFound { .. } => "File not found",
            Error::EmptyFile { .. } => "Empty file",
            Error::ExternalFileNotFound { .. } => "External file not found",
            Error::OutOfMemory { .. } => "Out of memory",
            Error::MemoryLimit { .. } => "Memory limit exceeded",
            Error::AllocationLimit { .. } => "Allocation limit exceeded",
            Error::TruncatedFile { .. } => "Truncated file",
            Error::Io { .. } => "IO error",
            Error::Cancelled => "Cancelled",
            Error::UnrecognizedFileFormat { .. } => "Unrecognized file format",
            Error::UninitializedOptions => "Uninitialized options",
            Error::ZeroVertexSize => "Zero vertex size",
            Error::TruncatedVertexStream { .. } => "Truncated vertex stream",
            Error::InvalidUtf8 { .. } => "Invalid UTF-8",
            Error::FeatureDisabled { .. } => "Feature disabled",
            Error::BadNurbs { .. } => "Bad NURBS geometry",
            Error::BadIndex { .. } => "Bad index",
            Error::NodeDepthLimit { .. } => "Node depth limit exceeded",
            Error::ThreadedAsciiParse { .. } => "Threaded ASCII parse error",
            Error::UnsafeOptions { .. } => "Unsafe options",
            Error::DuplicateOverride { .. } => "Duplicate override",
            Error::UnsupportedVersion { .. } => "Unsupported version",
            Error::Unknown { .. } => "Unknown error",
        }
    }

    /// Convert to an error code matching the C API
    ///
    /// This is useful for FFI compatibility.
    pub fn to_error_code(&self) -> u32 {
        match self {
            Error::FileNotFound { .. } => 2,
            Error::EmptyFile { .. } => 3,
            Error::ExternalFileNotFound { .. } => 4,
            Error::OutOfMemory { .. } => 5,
            Error::MemoryLimit { .. } => 6,
            Error::AllocationLimit { .. } => 7,
            Error::TruncatedFile { .. } => 8,
            Error::Io { .. } => 9,
            Error::Cancelled => 10,
            Error::UnrecognizedFileFormat { .. } => 11,
            Error::UninitializedOptions => 12,
            Error::ZeroVertexSize => 13,
            Error::TruncatedVertexStream { .. } => 14,
            Error::InvalidUtf8 { .. } => 15,
            Error::FeatureDisabled { .. } => 16,
            Error::BadNurbs { .. } => 17,
            Error::BadIndex { .. } => 18,
            Error::NodeDepthLimit { .. } => 19,
            Error::ThreadedAsciiParse { .. } => 20,
            Error::UnsafeOptions { .. } => 21,
            Error::DuplicateOverride { .. } => 22,
            Error::UnsupportedVersion { .. } => 23,
            Error::Unknown { .. } => 1,
        }
    }
}

/// Clean a string to ensure valid UTF-8, replacing invalid sequences with '?'
/// and truncating to max_len bytes
fn clean_utf8(s: &str, max_len: usize) -> String {
    let mut result = String::with_capacity(max_len.min(s.len()));
    let mut bytes_used = 0;

    for ch in s.chars() {
        let char_len = ch.len_utf8();
        if bytes_used + char_len > max_len {
            break;
        }

        // Replace control characters (except newline and tab) with '?'
        if ch.is_ascii_control() && ch != '\n' && ch != '\t' {
            result.push('?');
            bytes_used += 1;
        } else {
            result.push(ch);
            bytes_used += char_len;
        }
    }

    result
}

/// Convert a standard IO error to a ufbx error
impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        use std::io::ErrorKind;

        match err.kind() {
            ErrorKind::NotFound => {
                Error::FileNotFound {
                    path: err.to_string(),
                }
            }
            ErrorKind::UnexpectedEof => {
                Error::TruncatedFile {
                    offset: 0, // Unknown offset from std error
                }
            }
            _ => {
                Error::Io {
                    message: err.to_string(),
                }
            }
        }
    }
}

/// Result type alias for ufbx operations
pub type Result<T> = std::result::Result<T, Error>;

/// Helper macro to create an error with source location
///
/// # Examples
///
/// ```rust
/// # use ufbx::ufbx_error;
/// # use ufbx::error::Error;
/// let err = ufbx_error!(Error::Cancelled);
/// ```
#[macro_export]
macro_rules! ufbx_error {
    ($error:expr) => {{
        // Just return the error as-is since thiserror handles Display
        $error
    }};
}

/// Helper macro to check a condition and return an error if it fails
///
/// # Examples
///
/// ```rust
/// # use ufbx::{ufbx_check, error::Error};
/// fn check_size(size: usize) -> ufbx::error::Result<()> {
///     ufbx_check!(size > 0, Error::ZeroVertexSize);
///     Ok(())
/// }
/// ```
#[macro_export]
macro_rules! ufbx_check {
    ($cond:expr, $error:expr) => {
        if !($cond) {
            return Err($error);
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = Error::FileNotFound {
            path: "test.fbx".to_string(),
        };
        assert_eq!(err.to_string(), "File not found: test.fbx");

        let err = Error::OutOfMemory { size: 1024 };
        assert_eq!(err.to_string(), "Out of memory: failed to allocate 1024 bytes");
    }

    #[test]
    fn test_error_codes() {
        assert_eq!(Error::file_not_found("test").to_error_code(), 2);
        assert_eq!(Error::Cancelled.to_error_code(), 10);
        assert_eq!(Error::unknown("test").to_error_code(), 1);
    }

    #[test]
    fn test_error_type_descriptions() {
        let err = Error::FileNotFound {
            path: "test.fbx".to_string(),
        };
        assert_eq!(err.type_description(), "File not found");

        let err = Error::OutOfMemory { size: 100 };
        assert_eq!(err.type_description(), "Out of memory");
    }

    #[test]
    fn test_error_stack() {
        let mut stack = ErrorStack::new();

        for i in 0..10 {
            stack.push_frame(ErrorFrame::new(
                i as u32,
                format!("function_{}", i),
                format!("description_{}", i),
            ));
        }

        // Should only keep up to ERROR_STACK_MAX_DEPTH frames
        assert_eq!(stack.frames.len(), ERROR_STACK_MAX_DEPTH);
    }

    #[test]
    fn test_error_with_stack() {
        let mut stack = ErrorStack::new();
        stack.push_frame(ErrorFrame::new(42, "load_file", "Failed to open file"));
        stack.set_info("additional context");

        let err = Error::unknown_with_stack("parsing failed", stack);
        let display = format!("{}", err);
        assert!(display.contains("Unknown error"));
        assert!(display.contains("parsing failed"));
    }

    #[test]
    fn test_utf8_cleaning() {
        let long_str = "a".repeat(300);
        let cleaned = clean_utf8(&long_str, ERROR_INFO_LENGTH);
        assert!(cleaned.len() <= ERROR_INFO_LENGTH);
    }

    #[test]
    fn test_io_error_conversion() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
        let err: Error = io_err.into();

        match err {
            Error::FileNotFound { .. } => {},
            _ => panic!("Expected FileNotFound error"),
        }
    }

    #[test]
    fn test_error_helpers() {
        let err = Error::file_not_found("missing.fbx");
        assert!(err.to_string().contains("missing.fbx"));

        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied");
        let err = Error::from_io_error(io_err);
        assert!(err.to_string().contains("access denied"));
    }

    #[test]
    fn test_check_macro() {
        fn test_fn(value: bool) -> Result<()> {
            ufbx_check!(value, Error::Cancelled);
            Ok(())
        }

        assert!(test_fn(true).is_ok());
        assert!(test_fn(false).is_err());
    }
}
