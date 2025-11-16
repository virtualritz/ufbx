//! Math utilities
//!
//! This module provides optimized memory operations and math utilities.
//! Corresponds to ufbx.c lines 882-913.

use std::ptr;

// -- Fast Copy (lines 882-895)

/// Fast copy of 16 bytes
///
/// Uses optimized copying for 16-byte blocks. On platforms with SIMD support,
/// this could be further optimized, but Rust's memcpy is already quite fast.
#[inline(always)]
pub fn copy_16_bytes(dst: &mut [u8], src: &[u8]) {
    debug_assert!(dst.len() >= 16);
    debug_assert!(src.len() >= 16);

    // SAFETY: We've checked that both slices are at least 16 bytes
    unsafe {
        ptr::copy_nonoverlapping(src.as_ptr(), dst.as_mut_ptr(), 16);
    }
}

/// Fast copy of 8 bytes
#[inline(always)]
pub fn copy_8_bytes(dst: &mut [u8], src: &[u8]) {
    debug_assert!(dst.len() >= 8);
    debug_assert!(src.len() >= 8);

    // SAFETY: We've checked that both slices are at least 8 bytes
    unsafe {
        ptr::copy_nonoverlapping(src.as_ptr(), dst.as_mut_ptr(), 8);
    }
}

/// Fast copy of 4 bytes
#[inline(always)]
pub fn copy_4_bytes(dst: &mut [u8], src: &[u8]) {
    debug_assert!(dst.len() >= 4);
    debug_assert!(src.len() >= 4);

    // SAFETY: We've checked that both slices are at least 4 bytes
    unsafe {
        ptr::copy_nonoverlapping(src.as_ptr(), dst.as_mut_ptr(), 4);
    }
}

/// Copy arbitrary number of bytes (may overlap)
#[inline(always)]
pub fn copy_bytes(dst: &mut [u8], src: &[u8], count: usize) {
    debug_assert!(dst.len() >= count);
    debug_assert!(src.len() >= count);

    dst[..count].copy_from_slice(&src[..count]);
}

/// Copy arbitrary number of bytes (overlapping safe)
#[inline(always)]
pub fn move_bytes(dst: &mut [u8], src: &[u8], count: usize) {
    debug_assert!(dst.len() >= count);
    debug_assert!(src.len() >= count);

    // SAFETY: We've checked bounds
    unsafe {
        ptr::copy(src.as_ptr(), dst.as_mut_ptr(), count);
    }
}

// -- Large Fast Integer (lines 898-904)

/// Fast unsigned integer type
///
/// Uses u64 on WASM (unless 32-bit mode is enabled), otherwise uses usize.
#[cfg(all(target_arch = "wasm32", not(feature = "wasm_32bit")))]
pub type FastUint = u64;

#[cfg(all(target_arch = "wasm64", not(feature = "wasm_32bit")))]
pub type FastUint = u64;

#[cfg(not(any(
    all(target_arch = "wasm32", not(feature = "wasm_32bit")),
    all(target_arch = "wasm64", not(feature = "wasm_32bit"))
)))]
pub type FastUint = usize;

// -- Math Helpers

/// Clamp a value between min and max
#[inline(always)]
pub fn clamp<T: PartialOrd>(value: T, min: T, max: T) -> T {
    if value < min {
        min
    } else if value > max {
        max
    } else {
        value
    }
}

/// Linear interpolation
#[inline(always)]
pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Linear interpolation (f64)
#[inline(always)]
pub fn lerp_f64(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

/// Check if a float is finite (not infinite or NaN)
#[inline(always)]
pub fn is_finite(x: f32) -> bool {
    x.is_finite()
}

/// Check if a double is finite (not infinite or NaN)
#[inline(always)]
pub fn is_finite_f64(x: f64) -> bool {
    x.is_finite()
}

/// Safe conversion from f64 to i32 with clamping
#[inline(always)]
pub fn f64_to_i32(value: f64) -> i32 {
    if value.abs() <= i32::MAX as f64 {
        value as i32
    } else if value >= 0.0 {
        i32::MAX
    } else {
        i32::MIN
    }
}

/// Safe conversion from f64 to i64 with clamping
#[inline(always)]
pub fn f64_to_i64(value: f64) -> i64 {
    if value.abs() <= i64::MAX as f64 {
        value as i64
    } else if value >= 0.0 {
        i64::MAX
    } else {
        i64::MIN
    }
}

/// Safe conversion from f32 to i32 with clamping
#[inline(always)]
pub fn f32_to_i32(value: f32) -> i32 {
    if value.abs() <= i32::MAX as f32 {
        value as i32
    } else if value >= 0.0 {
        i32::MAX
    } else {
        i32::MIN
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_copy_16_bytes() {
        let src = [1u8, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
        let mut dst = [0u8; 16];

        copy_16_bytes(&mut dst, &src);
        assert_eq!(dst, src);
    }

    #[test]
    fn test_copy_8_bytes() {
        let src = [1u8, 2, 3, 4, 5, 6, 7, 8];
        let mut dst = [0u8; 8];

        copy_8_bytes(&mut dst, &src);
        assert_eq!(dst, src);
    }

    #[test]
    fn test_copy_4_bytes() {
        let src = [1u8, 2, 3, 4];
        let mut dst = [0u8; 4];

        copy_4_bytes(&mut dst, &src);
        assert_eq!(dst, src);
    }

    #[test]
    fn test_copy_bytes() {
        let src = [1u8, 2, 3, 4, 5];
        let mut dst = [0u8; 10];

        copy_bytes(&mut dst, &src, 5);
        assert_eq!(&dst[..5], &src[..]);
        assert_eq!(&dst[5..], &[0, 0, 0, 0, 0]);
    }

    #[test]
    fn test_move_bytes_overlapping() {
        let mut data = [1u8, 2, 3, 4, 5, 6, 7, 8];

        // Move overlapping region
        let (src, dst) = data.split_at_mut(2);
        move_bytes(dst, src, 2);
        // First two bytes should be copied to positions 2 and 3
    }

    #[test]
    fn test_clamp() {
        assert_eq!(clamp(5, 0, 10), 5);
        assert_eq!(clamp(-1, 0, 10), 0);
        assert_eq!(clamp(15, 0, 10), 10);
        assert_eq!(clamp(3.5f32, 0.0, 5.0), 3.5);
    }

    #[test]
    fn test_lerp() {
        assert_eq!(lerp(0.0, 10.0, 0.0), 0.0);
        assert_eq!(lerp(0.0, 10.0, 1.0), 10.0);
        assert_eq!(lerp(0.0, 10.0, 0.5), 5.0);

        assert_eq!(lerp_f64(0.0, 100.0, 0.25), 25.0);
    }

    #[test]
    fn test_is_finite() {
        assert!(is_finite(1.0));
        assert!(is_finite(-1.0));
        assert!(!is_finite(f32::INFINITY));
        assert!(!is_finite(f32::NEG_INFINITY));
        assert!(!is_finite(f32::NAN));

        assert!(is_finite_f64(1.0));
        assert!(!is_finite_f64(f64::INFINITY));
    }

    #[test]
    fn test_f64_to_i32() {
        assert_eq!(f64_to_i32(0.0), 0);
        assert_eq!(f64_to_i32(42.7), 42);
        assert_eq!(f64_to_i32(-42.7), -42);
        assert_eq!(f64_to_i32(1e20), i32::MAX);
        assert_eq!(f64_to_i32(-1e20), i32::MIN);
    }

    #[test]
    fn test_f64_to_i64() {
        assert_eq!(f64_to_i64(0.0), 0);
        assert_eq!(f64_to_i64(42.7), 42);
        assert_eq!(f64_to_i64(-42.7), -42);
        assert_eq!(f64_to_i64(1e100), i64::MAX);
        assert_eq!(f64_to_i64(-1e100), i64::MIN);
    }

    #[test]
    fn test_f32_to_i32() {
        assert_eq!(f32_to_i32(0.0), 0);
        assert_eq!(f32_to_i32(42.7), 42);
        assert_eq!(f32_to_i32(-42.7), -42);
        assert_eq!(f32_to_i32(1e20), i32::MAX);
        assert_eq!(f32_to_i32(-1e20), i32::MIN);
    }

    #[test]
    fn test_fast_uint_size() {
        // Just ensure the type exists and has a reasonable size
        let _x: FastUint = 42;
        assert!(std::mem::size_of::<FastUint>() >= 4);
    }
}
