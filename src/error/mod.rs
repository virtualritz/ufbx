//! Error handling and reporting
//!
//! This module provides comprehensive error handling for ufbx operations,
//! including detailed error types, stack traces, and context information.
//!
//! # Error System
//!
//! The error system provides:
//! - Specific error types for different failure modes
//! - Error stack traces (up to 8 frames) when enabled
//! - Additional context information (e.g., filenames, sizes)
//! - UTF-8 safe error messages
//!
//! # Example
//!
//! ```rust
//! use ufbx::error::{Error, ErrorType, Result};
//!
//! fn load_file(path: &str) -> Result<()> {
//!     if path.is_empty() {
//!         return Err(Error::new(ErrorType::FileNotFound)
//!             .with_info("Empty path provided"));
//!     }
//!     Ok(())
//! }
//! ```

use std::fmt;
use std::error::Error as StdError;

/// Maximum depth of the error stack trace
pub const ERROR_STACK_MAX_DEPTH: usize = 8;

/// Maximum length of additional error information
pub const ERROR_INFO_LENGTH: usize = 256;

/// Error types corresponding to different failure modes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum ErrorType {
    /// No error, operation successful
    None = 0,

    /// Unspecified error, likely caused by invalid FBX file
    Unknown,

    /// File not found
    FileNotFound,

    /// File is empty
    EmptyFile,

    /// External file referenced in FBX not found
    ExternalFileNotFound,

    /// Out of memory (allocator returned NULL/None)
    OutOfMemory,

    /// Memory limit from `AllocatorOpts::memory_limit` exhausted
    MemoryLimit,

    /// Allocation limit from `AllocatorOpts::allocation_limit` exhausted
    AllocationLimit,

    /// File ended abruptly during parsing
    TruncatedFile,

    /// IO read error
    Io,

    /// User cancelled the loading operation
    Cancelled,

    /// Could not detect file format from data or filename
    UnrecognizedFileFormat,

    /// Options struct not initialized to zero
    UninitializedOptions,

    /// Vertex streams in `generate_indices()` are empty
    ZeroVertexSize,

    /// Truncated vertex stream passed to `generate_indices()`
    TruncatedVertexStream,

    /// Invalid UTF-8 encountered when loading with abort-on-error mode
    InvalidUtf8,

    /// Required feature has been compiled out
    FeatureDisabled,

    /// Attempting to tessellate invalid NURBS geometry
    BadNurbs,

    /// Out of bounds index when loading with abort-on-error mode
    BadIndex,

    /// Node deeper than `LoadOpts::node_depth_limit` in hierarchy
    NodeDepthLimit,

    /// Error parsing ASCII array in thread
    ThreadedAsciiParse,

    /// Unsafe options specified without `allow_unsafe`
    UnsafeOptions,

    /// Duplicated override property in animation creation
    DuplicateOverride,

    /// Unsupported file format version
    UnsupportedVersion,
}

impl ErrorType {
    /// Get the human-readable description for this error type
    pub fn description(&self) -> &'static str {
        match self {
            ErrorType::None => "No error",
            ErrorType::Unknown => "Unknown error",
            ErrorType::FileNotFound => "File not found",
            ErrorType::EmptyFile => "Empty file",
            ErrorType::ExternalFileNotFound => "External file not found",
            ErrorType::OutOfMemory => "Out of memory",
            ErrorType::MemoryLimit => "Memory limit exceeded",
            ErrorType::AllocationLimit => "Allocation limit exceeded",
            ErrorType::TruncatedFile => "Truncated file",
            ErrorType::Io => "IO error",
            ErrorType::Cancelled => "Cancelled",
            ErrorType::UnrecognizedFileFormat => "Unrecognized file format",
            ErrorType::UninitializedOptions => "Uninitialized options",
            ErrorType::ZeroVertexSize => "Zero vertex size",
            ErrorType::TruncatedVertexStream => "Truncated vertex stream",
            ErrorType::InvalidUtf8 => "Invalid UTF-8",
            ErrorType::FeatureDisabled => "Feature disabled",
            ErrorType::BadNurbs => "Bad NURBS geometry",
            ErrorType::BadIndex => "Bad index",
            ErrorType::NodeDepthLimit => "Node depth limit exceeded",
            ErrorType::ThreadedAsciiParse => "Threaded ASCII parse error",
            ErrorType::UnsafeOptions => "Unsafe options",
            ErrorType::DuplicateOverride => "Duplicate override",
            ErrorType::UnsupportedVersion => "Unsupported version",
        }
    }
}

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

/// Comprehensive error information with stack trace support
#[derive(Debug, Clone)]
pub struct Error {
    /// Type of error that occurred
    pub error_type: ErrorType,

    /// Error stack trace (when error stack feature is enabled)
    pub stack: Vec<ErrorFrame>,

    /// Additional context information (e.g., filename, size)
    /// This is a UTF-8 validated string
    pub info: String,
}

impl Error {
    /// Create a new error of the given type
    pub fn new(error_type: ErrorType) -> Self {
        Self {
            error_type,
            stack: Vec::new(),
            info: String::new(),
        }
    }

    /// Add additional information to the error
    pub fn with_info(mut self, info: impl Into<String>) -> Self {
        let info_str = info.into();
        self.info = Self::clean_utf8(&info_str, ERROR_INFO_LENGTH);
        self
    }

    /// Add a stack frame to the error
    pub fn push_frame(&mut self, frame: ErrorFrame) {
        if self.stack.len() < ERROR_STACK_MAX_DEPTH {
            self.stack.push(frame);
        }
    }

    /// Clear the error state
    pub fn clear(&mut self) {
        self.error_type = ErrorType::None;
        self.stack.clear();
        self.info.clear();
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

    /// Get the error description
    pub fn description(&self) -> &str {
        self.error_type.description()
    }

    /// Check if this represents an actual error (not None)
    pub fn is_err(&self) -> bool {
        self.error_type != ErrorType::None
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.error_type.description())?;

        if !self.info.is_empty() {
            write!(f, ": {}", self.info)?;
        }

        if !self.stack.is_empty() {
            writeln!(f)?;
            writeln!(f, "Stack trace:")?;
            for (i, frame) in self.stack.iter().enumerate() {
                writeln!(
                    f,
                    "  #{}: {} at {}:{}",
                    i,
                    frame.description,
                    frame.function,
                    frame.source_line
                )?;
            }
        }

        Ok(())
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        None
    }
}

/// Result type alias for ufbx operations
pub type Result<T> = std::result::Result<T, Error>;

/// Helper macro to create an error with source location
#[macro_export]
macro_rules! ufbx_error {
    ($error_type:expr) => {{
        let mut err = $crate::error::Error::new($error_type);
        err.push_frame($crate::error::ErrorFrame::new(
            line!(),
            module_path!(),
            stringify!($error_type),
        ));
        err
    }};
    ($error_type:expr, $msg:expr) => {{
        let mut err = $crate::error::Error::new($error_type);
        err.push_frame($crate::error::ErrorFrame::new(
            line!(),
            module_path!(),
            $msg,
        ));
        err
    }};
}

/// Helper macro to check a condition and return an error if it fails
#[macro_export]
macro_rules! ufbx_check {
    ($cond:expr, $error_type:expr) => {
        if !($cond) {
            return Err($crate::ufbx_error!($error_type, stringify!($cond)));
        }
    };
    ($cond:expr, $error_type:expr, $msg:expr) => {
        if !($cond) {
            return Err($crate::ufbx_error!($error_type, $msg));
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_type_descriptions() {
        assert_eq!(ErrorType::None.description(), "No error");
        assert_eq!(ErrorType::FileNotFound.description(), "File not found");
        assert_eq!(ErrorType::OutOfMemory.description(), "Out of memory");
    }

    #[test]
    fn test_error_creation() {
        let err = Error::new(ErrorType::FileNotFound)
            .with_info("test.fbx");

        assert_eq!(err.error_type, ErrorType::FileNotFound);
        assert_eq!(err.info, "test.fbx");
        assert!(err.is_err());
    }

    #[test]
    fn test_error_stack() {
        let mut err = Error::new(ErrorType::Unknown);

        for i in 0..10 {
            err.push_frame(ErrorFrame::new(
                i as u32,
                format!("function_{}", i),
                format!("description_{}", i),
            ));
        }

        // Should only keep up to ERROR_STACK_MAX_DEPTH frames
        assert_eq!(err.stack.len(), ERROR_STACK_MAX_DEPTH);
    }

    #[test]
    fn test_error_display() {
        let mut err = Error::new(ErrorType::FileNotFound)
            .with_info("missing.fbx");

        err.push_frame(ErrorFrame::new(
            42,
            "load_file",
            "Failed to open file",
        ));

        let display = format!("{}", err);
        assert!(display.contains("File not found"));
        assert!(display.contains("missing.fbx"));
        assert!(display.contains("Stack trace"));
    }

    #[test]
    fn test_utf8_cleaning() {
        let long_str = "a".repeat(300);
        let cleaned = Error::clean_utf8(&long_str, ERROR_INFO_LENGTH);
        assert!(cleaned.len() <= ERROR_INFO_LENGTH);
    }

    #[test]
    fn test_error_clear() {
        let mut err = Error::new(ErrorType::OutOfMemory)
            .with_info("allocation failed");
        err.push_frame(ErrorFrame::new(10, "allocate", "failed"));

        err.clear();

        assert_eq!(err.error_type, ErrorType::None);
        assert!(err.stack.is_empty());
        assert!(err.info.is_empty());
        assert!(!err.is_err());
    }
}
