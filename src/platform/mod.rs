//! Platform detection and utilities
//!
//! This module provides platform-specific functionality including:
//! - Compiler and platform detection
//! - Architecture detection
//! - Alignment utilities
//! - Version information
//!
//! Corresponds to ufbx.c lines 399-881.

pub mod atomics;
pub mod config;

use std::mem;

// -- Platform Detection (lines 401-440)

/// Microsoft Visual C++ compiler version (0 if not MSVC)
pub const MSC_VER: u32 = if cfg!(target_env = "msvc") {
    // Rust doesn't expose _MSC_VER directly, but we know it's MSVC
    1900 // VS 2015+ baseline
} else {
    0
};

/// GNU C compiler version (0 if not GCC)
pub const GNUC: u32 = if cfg!(target_env = "gnu") && !cfg!(target_env = "msvc") {
    // Approximate GCC version
    4
} else {
    0
};

/// Check if compiling for x64 architecture
pub const IS_X64: bool = cfg!(target_arch = "x86_64");

/// Check if compiling for x86 architecture
pub const IS_X86: bool = cfg!(target_arch = "x86");

/// Check if compiling for ARM64 architecture
pub const IS_ARM64: bool = cfg!(target_arch = "aarch64");

/// Check if compiling for WebAssembly
pub const IS_WASM: bool = cfg!(target_arch = "wasm32") || cfg!(target_arch = "wasm64");

/// Check if SSE instructions are available
pub const HAS_SSE: bool = cfg!(target_feature = "sse2") && IS_X64;

// -- Path Separator (lines 601-607)

/// Platform-specific path separator
#[cfg(target_os = "windows")]
pub const PATH_SEPARATOR: char = '\\';

#[cfg(not(target_os = "windows"))]
pub const PATH_SEPARATOR: char = '/';

// -- Endianness (lines 733-739)

/// Check if the platform is little-endian
pub const IS_LITTLE_ENDIAN: bool = cfg!(target_endian = "little");

// -- Alignment (lines 856-873)

/// Maximum alignment used for allocation
///
/// Defaults to max(8, pointer_size)
pub const MAXIMUM_ALIGNMENT: usize = if mem::size_of::<*const ()>() > 8 {
    mem::size_of::<*const ()>()
} else {
    8
};

/// Size of uintptr_t in bytes
pub const UINTPTR_SIZE: usize = mem::size_of::<usize>();

// -- Version (lines 875-881)

/// Pack a version number into a u32
///
/// Format: major * 1000000 + minor * 1000 + patch
#[inline]
pub const fn pack_version(major: u32, minor: u32, patch: u32) -> u32 {
    major * 1000000 + minor * 1000 + patch
}

/// Unpack a version number from a u32
#[inline]
pub const fn unpack_version(version: u32) -> (u32, u32, u32) {
    let major = version / 1000000;
    let minor = (version / 1000) % 1000;
    let patch = version % 1000;
    (major, minor, patch)
}

/// ufbx source version
pub const SOURCE_VERSION: u32 = pack_version(0, 21, 2);

/// Get version as string
pub fn version_string() -> String {
    let (major, minor, patch) = unpack_version(SOURCE_VERSION);
    format!("{}.{}.{}", major, minor, patch)
}

// -- Inline Attributes (Rust equivalents of C macros)

/// Force a function to be inlined
///
/// Rust equivalent of `ufbxi_forceinline`
#[macro_export]
macro_rules! force_inline {
    () => {
        #[inline(always)]
    };
}

/// Prevent a function from being inlined
///
/// Rust equivalent of `ufbxi_noinline`
#[macro_export]
macro_rules! no_inline {
    () => {
        #[inline(never)]
    };
}

// -- Compiler Hints

/// Mark a condition as unlikely to be true
///
/// Rust equivalent of `ufbxi_unlikely`
#[inline(always)]
pub fn unlikely(b: bool) -> bool {
    #[cold]
    fn cold() {}

    if b {
        cold();
    }
    b
}

/// Mark a condition as likely to be true
#[inline(always)]
pub fn likely(b: bool) -> bool {
    if !b {
        #[cold]
        fn cold() {}
        cold();
    }
    b
}

// -- Static Assertions

/// Static assertion macro
///
/// Rust equivalent of `ufbx_static_assert`
#[macro_export]
macro_rules! static_assert {
    ($name:ident, $cond:expr) => {
        const _: () = {
            const fn assert() {
                if !$cond {
                    panic!(concat!("Static assertion failed: ", stringify!($name)));
                }
            }
            assert();
        };
    };
}

// Compile-time size checks (lines 843-854)
static_assert!(sizeof_bool, mem::size_of::<bool>() == 1);
static_assert!(sizeof_char, mem::size_of::<i8>() == 1);
static_assert!(sizeof_i8, mem::size_of::<i8>() == 1);
static_assert!(sizeof_i16, mem::size_of::<i16>() == 2);
static_assert!(sizeof_i32, mem::size_of::<i32>() == 4);
static_assert!(sizeof_i64, mem::size_of::<i64>() == 8);
static_assert!(sizeof_u8, mem::size_of::<u8>() == 1);
static_assert!(sizeof_u16, mem::size_of::<u16>() == 2);
static_assert!(sizeof_u32, mem::size_of::<u32>() == 4);
static_assert!(sizeof_u64, mem::size_of::<u64>() == 8);
static_assert!(sizeof_f32, mem::size_of::<f32>() == 4);
static_assert!(sizeof_f64, mem::size_of::<f64>() == 8);

// -- Floating Point Constants

/// Infinity constant
pub const INFINITY: f64 = f64::INFINITY;

/// NaN constant
pub const NAN: f64 = f64::NAN;

/// Float epsilon
pub const FLT_EPSILON: f32 = f32::EPSILON;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() {
        assert_eq!(SOURCE_VERSION, pack_version(0, 21, 2));
        assert_eq!(unpack_version(SOURCE_VERSION), (0, 21, 2));
        assert_eq!(version_string(), "0.21.2");
    }

    #[test]
    fn test_pack_unpack_version() {
        let v = pack_version(1, 2, 3);
        assert_eq!(v, 1002003);
        assert_eq!(unpack_version(v), (1, 2, 3));

        let v2 = pack_version(12, 345, 678);
        assert_eq!(unpack_version(v2), (12, 345, 678));
    }

    #[test]
    fn test_platform_constants() {
        // Just ensure they compile and have sensible values
        assert!(MAXIMUM_ALIGNMENT >= 8);
        assert!(UINTPTR_SIZE > 0);

        #[cfg(target_os = "windows")]
        assert_eq!(PATH_SEPARATOR, '\\');

        #[cfg(target_os = "linux")]
        assert_eq!(PATH_SEPARATOR, '/');
    }

    #[test]
    fn test_endianness() {
        // Most platforms are little-endian
        let bytes = 0x12345678u32.to_ne_bytes();
        if IS_LITTLE_ENDIAN {
            assert_eq!(bytes[0], 0x78);
        } else {
            assert_eq!(bytes[0], 0x12);
        }
    }

    #[test]
    fn test_unlikely() {
        assert!(!unlikely(false));
        assert!(unlikely(true));
    }

    #[test]
    fn test_likely() {
        assert!(!likely(false));
        assert!(likely(true));
    }

    #[test]
    fn test_constants() {
        assert!(INFINITY.is_infinite());
        assert!(NAN.is_nan());
        assert!(FLT_EPSILON > 0.0);
    }
}
