//! Memory buffers and dynamic arrays
//!
//! This module provides a chunked memory buffer system similar to the C implementation.
//! Buffers can be used as either chunked linear memory allocators or non-contiguous stacks.
//!
//! # Features
//!
//! - Chunked allocation with geometric growth
//! - Support for both ordered (stack-like) and unordered (arena) modes
//! - Efficient handling of huge allocations
//! - Custom allocator support
//! - Memory reuse and pooling

pub mod buffer;

pub use buffer::{Buffer, BufferChunk, BufferState, AlignedAlloc};

use std::alloc::{GlobalAlloc, Layout};
use std::ptr::NonNull;

/// Zero-size buffer for empty allocations
/// This allows us to return a valid non-null pointer for zero-size allocations
#[cfg(not(test))]
static ZERO_SIZE_BUFFER: [u8; 64] = [0; 64];

#[cfg(test)]
static ZERO_SIZE_BUFFER: [u8; 4096] = [0; 4096];

/// Get a pointer to the zero-size buffer
#[inline]
pub fn zero_size_buffer() -> *const u8 {
    ZERO_SIZE_BUFFER.as_ptr()
}

/// Alignment mask calculation based on size
/// Aligns to all bits below the lowest set bit in `size` up to maximum alignment.
#[inline]
pub fn size_align_mask(size: usize) -> usize {
    const MAX_ALIGN: usize = 16; // UFBX_MAXIMUM_ALIGNMENT
    ((size ^ (size.wrapping_sub(1))) >> 1) & (MAX_ALIGN - 1)
}

/// Align a value to a mask
#[inline]
pub fn align_to_mask(value: usize, align_mask: usize) -> usize {
    value.wrapping_add((0usize.wrapping_sub(value)) & align_mask)
}

/// Check if a value is aligned to a mask
#[inline]
pub fn is_aligned_mask(value: usize, align_mask: usize) -> bool {
    (value & align_mask) == 0
}

/// Check if multiplication would overflow
#[inline]
pub fn does_overflow(total: usize, a: usize, b: usize) -> bool {
    // If `a` and `b` have at most 4 bits per size_t byte, the product can't overflow.
    if ((a | b) >> (std::mem::size_of::<usize>() * 4)) != 0 {
        if a != 0 && total / a != b {
            return true;
        }
    }
    false
}

/// Allocator interface for memory buffers
pub trait Allocator {
    /// Allocate memory
    fn alloc(&mut self, size: usize) -> Option<NonNull<u8>>;

    /// Free previously allocated memory
    fn free(&mut self, ptr: NonNull<u8>, size: usize);

    /// Reallocate memory (optional, defaults to alloc+copy+free)
    fn realloc(&mut self, ptr: NonNull<u8>, old_size: usize, new_size: usize) -> Option<NonNull<u8>> {
        if old_size == new_size {
            return Some(ptr);
        }

        let new_ptr = self.alloc(new_size)?;

        // Copy old data
        unsafe {
            std::ptr::copy_nonoverlapping(
                ptr.as_ptr(),
                new_ptr.as_ptr(),
                old_size.min(new_size)
            );
        }

        self.free(ptr, old_size);
        Some(new_ptr)
    }

    /// Get huge allocation threshold
    fn huge_size(&self) -> usize;

    /// Get maximum chunk size
    fn chunk_max(&self) -> usize;
}

/// Standard global allocator wrapper
pub struct GlobalAllocator {
    huge_size: usize,
    chunk_max: usize,
}

impl GlobalAllocator {
    /// Create a new global allocator with default settings
    pub fn new() -> Self {
        Self {
            huge_size: 1024 * 1024, // 1 MB
            chunk_max: 16 * 1024 * 1024, // 16 MB
        }
    }

    /// Create with custom settings
    pub fn with_settings(huge_size: usize, chunk_max: usize) -> Self {
        Self { huge_size, chunk_max }
    }
}

impl Default for GlobalAllocator {
    fn default() -> Self {
        Self::new()
    }
}

impl Allocator for GlobalAllocator {
    fn alloc(&mut self, size: usize) -> Option<NonNull<u8>> {
        if size == 0 {
            return NonNull::new(zero_size_buffer() as *mut u8);
        }

        let layout = Layout::from_size_align(size, 8).ok()?;
        let ptr = unsafe { std::alloc::System.alloc(layout) };
        NonNull::new(ptr)
    }

    fn free(&mut self, ptr: NonNull<u8>, size: usize) {
        if size == 0 || ptr.as_ptr() == zero_size_buffer() as *mut u8 {
            return;
        }

        let layout = Layout::from_size_align(size, 8).unwrap();
        unsafe {
            std::alloc::System.dealloc(ptr.as_ptr(), layout);
        }
    }

    fn huge_size(&self) -> usize {
        self.huge_size
    }

    fn chunk_max(&self) -> usize {
        self.chunk_max
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_size_align_mask() {
        assert_eq!(size_align_mask(1), 0);
        assert_eq!(size_align_mask(2), 1);
        assert_eq!(size_align_mask(4), 3);
        assert_eq!(size_align_mask(8), 7);
        assert_eq!(size_align_mask(16), 15);
        assert_eq!(size_align_mask(32), 15); // Capped at MAX_ALIGN
    }

    #[test]
    fn test_align_to_mask() {
        assert_eq!(align_to_mask(0, 7), 0);
        assert_eq!(align_to_mask(1, 7), 8);
        assert_eq!(align_to_mask(8, 7), 8);
        assert_eq!(align_to_mask(9, 7), 16);
    }

    #[test]
    fn test_does_overflow() {
        assert!(!does_overflow(10, 2, 5));
        assert!(does_overflow(10, 100, 100)); // 100 * 100 = 10000, not 10
        assert!(!does_overflow(0, 0, 5));
    }

    #[test]
    fn test_zero_size_buffer() {
        let ptr = zero_size_buffer();
        assert!(!ptr.is_null());
    }
}
