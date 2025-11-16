//! Bit manipulation utilities
//!
//! This module provides low-level bit operations including:
//! - Unaligned memory reads
//! - Bit manipulation (leading zero count)
//! - Bit conversion (type punning)
//! - Pointer alignment checks
//! - Wrapping shifts
//!
//! Corresponds to ufbx.c lines 745-991.

use std::mem;

// -- Unaligned Reads (lines 745-836)

/// Read an unsigned 8-bit value
#[inline(always)]
pub fn read_u8(ptr: &[u8]) -> u8 {
    ptr[0]
}

/// Read an unsigned 16-bit value (little-endian)
#[inline(always)]
pub fn read_u16(ptr: &[u8]) -> u16 {
    u16::from_le_bytes([ptr[0], ptr[1]])
}

/// Read an unsigned 32-bit value (little-endian)
#[inline(always)]
pub fn read_u32(ptr: &[u8]) -> u32 {
    u32::from_le_bytes([ptr[0], ptr[1], ptr[2], ptr[3]])
}

/// Read an unsigned 64-bit value (little-endian)
#[inline(always)]
pub fn read_u64(ptr: &[u8]) -> u64 {
    u64::from_le_bytes([
        ptr[0], ptr[1], ptr[2], ptr[3], ptr[4], ptr[5], ptr[6], ptr[7],
    ])
}

/// Read a 32-bit float (little-endian)
#[inline(always)]
pub fn read_f32(ptr: &[u8]) -> f32 {
    f32::from_le_bytes([ptr[0], ptr[1], ptr[2], ptr[3]])
}

/// Read a 64-bit float (little-endian)
#[inline(always)]
pub fn read_f64(ptr: &[u8]) -> f64 {
    f64::from_le_bytes([
        ptr[0], ptr[1], ptr[2], ptr[3], ptr[4], ptr[5], ptr[6], ptr[7],
    ])
}

/// Read a signed 8-bit value
#[inline(always)]
pub fn read_i8(ptr: &[u8]) -> i8 {
    read_u8(ptr) as i8
}

/// Read a signed 16-bit value (little-endian)
#[inline(always)]
pub fn read_i16(ptr: &[u8]) -> i16 {
    read_u16(ptr) as i16
}

/// Read a signed 32-bit value (little-endian)
#[inline(always)]
pub fn read_i32(ptr: &[u8]) -> i32 {
    read_u32(ptr) as i32
}

/// Read a signed 64-bit value (little-endian)
#[inline(always)]
pub fn read_i64(ptr: &[u8]) -> i64 {
    read_u64(ptr) as i64
}

// -- Bit Manipulation (lines 914-967)

/// Count leading zeros in a 32-bit value
///
/// Returns 32 if the value is 0.
#[inline(always)]
pub fn lzcnt32(v: u32) -> u32 {
    if v == 0 {
        32
    } else {
        v.leading_zeros()
    }
}

/// Count leading zeros in a 64-bit value
///
/// Returns 64 if the value is 0.
#[inline(always)]
pub fn lzcnt64(v: u64) -> u32 {
    if v == 0 {
        64
    } else {
        v.leading_zeros()
    }
}

/// Count trailing zeros in a 32-bit value
#[inline(always)]
pub fn tzcnt32(v: u32) -> u32 {
    if v == 0 {
        32
    } else {
        v.trailing_zeros()
    }
}

/// Count trailing zeros in a 64-bit value
#[inline(always)]
pub fn tzcnt64(v: u64) -> u32 {
    if v == 0 {
        64
    } else {
        v.trailing_zeros()
    }
}

// -- Bit Conversion (lines 969-977)

/// Convert between types via bit pattern (type punning)
///
/// This is safe in Rust using transmute with matching sizes.
#[inline(always)]
pub fn bit_cast<Src, Dst>(src: Src) -> Dst
where
    Src: Copy,
    Dst: Copy,
{
    assert_eq!(mem::size_of::<Src>(), mem::size_of::<Dst>());
    // SAFETY: Size is checked at runtime, and both types are Copy
    unsafe { mem::transmute_copy(&src) }
}

/// Convert f32 to u32 bit pattern
#[inline(always)]
pub fn f32_to_bits(f: f32) -> u32 {
    f.to_bits()
}

/// Convert u32 bit pattern to f32
#[inline(always)]
pub fn f32_from_bits(bits: u32) -> f32 {
    f32::from_bits(bits)
}

/// Convert f64 to u64 bit pattern
#[inline(always)]
pub fn f64_to_bits(f: f64) -> u64 {
    f.to_bits()
}

/// Convert u64 bit pattern to f64
#[inline(always)]
pub fn f64_from_bits(bits: u64) -> f64 {
    f64::from_bits(bits)
}

// -- Pointer Alignment (lines 979-990)

/// Check if a pointer is aligned to the given alignment
///
/// The alignment must be a power of two.
#[inline(always)]
pub fn is_aligned<T>(ptr: *const T, align: usize) -> bool {
    debug_assert!(align.is_power_of_two());
    (ptr as usize) & (align - 1) == 0
}

/// Check if a pointer is aligned to the given alignment mask
///
/// This version takes `align - 1` as the mask.
#[inline(always)]
pub fn is_aligned_mask<T>(ptr: *const T, mask: usize) -> bool {
    (ptr as usize) & mask == 0
}

/// Get the alignment of a pointer
#[inline(always)]
pub fn alignment_of<T>(ptr: *const T) -> usize {
    let addr = ptr as usize;
    if addr == 0 {
        return usize::MAX;
    }
    // Find the lowest set bit (which is the alignment)
    addr & addr.wrapping_neg()
}

// -- Wrapping Right Shift (lines 906-912)

/// Wrapping right shift for 64-bit values
///
/// Wraps the shift amount to prevent undefined behavior.
#[inline(always)]
pub fn wrap_shr64(a: u64, b: u32) -> u64 {
    a >> (b & 63)
}

/// Wrapping right shift for 32-bit values
#[inline(always)]
pub fn wrap_shr32(a: u32, b: u32) -> u32 {
    a >> (b & 31)
}

/// Wrapping left shift for 64-bit values
#[inline(always)]
pub fn wrap_shl64(a: u64, b: u32) -> u64 {
    a << (b & 63)
}

/// Wrapping left shift for 32-bit values
#[inline(always)]
pub fn wrap_shl32(a: u32, b: u32) -> u32 {
    a << (b & 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_integers() {
        let data = [0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0];

        assert_eq!(read_u8(&data), 0x12);
        assert_eq!(read_u16(&data), 0x3412); // Little-endian
        assert_eq!(read_u32(&data), 0x78563412);
        assert_eq!(read_u64(&data), 0xF0DEBC9A78563412);

        assert_eq!(read_i8(&data), 0x12);
        assert_eq!(read_i16(&data), 0x3412u16 as i16);
    }

    #[test]
    fn test_read_floats() {
        // Test with known bit patterns
        let f32_bits = 3.14f32.to_le_bytes();
        assert_eq!(read_f32(&f32_bits), 3.14f32);

        let f64_bits = 3.14159265359f64.to_le_bytes();
        assert_eq!(read_f64(&f64_bits), 3.14159265359f64);
    }

    #[test]
    fn test_lzcnt() {
        assert_eq!(lzcnt32(0), 32);
        assert_eq!(lzcnt32(1), 31);
        assert_eq!(lzcnt32(0x80000000), 0);
        assert_eq!(lzcnt32(0x00000001), 31);

        assert_eq!(lzcnt64(0), 64);
        assert_eq!(lzcnt64(1), 63);
        assert_eq!(lzcnt64(0x8000000000000000), 0);
    }

    #[test]
    fn test_tzcnt() {
        assert_eq!(tzcnt32(0), 32);
        assert_eq!(tzcnt32(1), 0);
        assert_eq!(tzcnt32(2), 1);
        assert_eq!(tzcnt32(4), 2);
        assert_eq!(tzcnt32(0x80000000), 31);

        assert_eq!(tzcnt64(0), 64);
        assert_eq!(tzcnt64(1), 0);
        assert_eq!(tzcnt64(0x8000000000000000), 63);
    }

    #[test]
    fn test_bit_cast() {
        let f = 3.14f32;
        let bits: u32 = bit_cast(f);
        let f2: f32 = bit_cast(bits);
        assert_eq!(f, f2);
    }

    #[test]
    fn test_float_bits() {
        let f = 3.14f32;
        let bits = f32_to_bits(f);
        assert_eq!(f32_from_bits(bits), f);

        let d = 3.14159265359f64;
        let bits = f64_to_bits(d);
        assert_eq!(f64_from_bits(bits), d);
    }

    #[test]
    fn test_alignment() {
        let x = 0x1000usize as *const u8;
        assert!(is_aligned(x, 16));
        assert!(is_aligned(x, 8));
        assert!(is_aligned(x, 4));
        assert!(!is_aligned((x as usize + 1) as *const u8, 2));

        assert!(is_aligned_mask(x, 15)); // 16-byte aligned (mask = 15)
        assert!(!is_aligned_mask((x as usize + 8) as *const u8, 15));
    }

    #[test]
    fn test_alignment_of() {
        let x = 0x1000usize as *const u8;
        assert!(alignment_of(x) >= 16);

        let y = 0x1008usize as *const u8;
        assert_eq!(alignment_of(y), 8);

        let z = 0x1001usize as *const u8;
        assert_eq!(alignment_of(z), 1);
    }

    #[test]
    fn test_wrapping_shift() {
        assert_eq!(wrap_shr64(0xFF, 4), 0x0F);
        assert_eq!(wrap_shr64(0xFF, 64), 0xFF); // Wraps to 0
        assert_eq!(wrap_shr64(0xFF, 65), 0x7F); // Wraps to 1

        assert_eq!(wrap_shr32(0xFF, 4), 0x0F);
        assert_eq!(wrap_shr32(0xFF, 32), 0xFF); // Wraps to 0

        assert_eq!(wrap_shl64(0xFF, 4), 0xFF0);
        assert_eq!(wrap_shl32(0xFF, 4), 0xFF0);
    }

    #[test]
    fn test_read_boundary() {
        // Test reading at different offsets
        let data = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];

        assert_eq!(read_u16(&data[0..]), 0x0201);
        assert_eq!(read_u16(&data[1..]), 0x0302);
        assert_eq!(read_u32(&data[2..]), 0x06050403);
    }
}
