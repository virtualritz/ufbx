//! Custom memory allocation
//!
//! This module provides a flexible memory allocation system that supports:
//! - Custom allocators for embedded systems
//! - Memory and allocation limits
//! - Allocation statistics tracking
//! - Chunk-based allocation strategies
//! - Alignment guarantees
//!
//! # Allocator Trait
//!
//! The `Allocator` trait defines the interface for custom allocators.
//! Implementors can provide their own allocation strategies.
//!
//! # Example
//!
//! ```rust
//! use ufbx::allocator::{Allocator, GlobalAllocator, AllocatorOpts};
//!
//! let opts = AllocatorOpts {
//!     memory_limit: 1024 * 1024, // 1MB limit
//!     allocation_limit: 1000,     // Max 1000 allocations
//!     ..Default::default()
//! };
//!
//! let allocator = GlobalAllocator::new(opts);
//! ```

use crate::error::{Error, ErrorType, Result};
use std::alloc::{self, Layout};
use std::ptr::NonNull;

/// Maximum alignment for allocations
pub const MAXIMUM_ALIGNMENT: usize = 16;

/// Default huge threshold (1MB) - threshold to swap from batched to individual allocations
pub const DEFAULT_HUGE_THRESHOLD: usize = 1024 * 1024;

/// Default maximum chunk size (16MB)
pub const DEFAULT_MAX_CHUNK_SIZE: usize = 16 * 1024 * 1024;

/// Allocator trait for custom memory allocation strategies
pub trait Allocator {
    /// Allocate memory of the given size
    ///
    /// Returns `None` if allocation fails.
    /// The returned pointer must be aligned to at least `size_align_mask(size)`.
    fn alloc(&mut self, size: usize) -> Option<NonNull<u8>>;

    /// Reallocate memory from `old_size` to `new_size`
    ///
    /// Returns `None` if reallocation fails.
    /// If `old_ptr` is `None`, this is equivalent to `alloc(new_size)`.
    /// If `new_size` is 0, this is equivalent to `free(old_ptr, old_size)`.
    fn realloc(
        &mut self,
        old_ptr: Option<NonNull<u8>>,
        old_size: usize,
        new_size: usize,
    ) -> Option<NonNull<u8>>;

    /// Free allocated memory
    ///
    /// # Safety
    ///
    /// `ptr` must have been allocated by this allocator with the given `size`.
    fn free(&mut self, ptr: NonNull<u8>, size: usize);

    /// Clean up the allocator (called on drop)
    fn cleanup(&mut self) {}
}

/// Options for configuring allocators
#[derive(Debug, Clone)]
pub struct AllocatorOpts {
    /// Maximum number of bytes to allocate before failing
    pub memory_limit: usize,

    /// Maximum number of allocations to attempt before failing
    pub allocation_limit: usize,

    /// Threshold to swap from batched allocations to individual ones
    /// Defaults to 1MB if set to zero
    pub huge_threshold: usize,

    /// Maximum size of a single allocation containing sub-allocations
    /// Defaults to 16MB if set to zero
    pub max_chunk_size: usize,
}

impl Default for AllocatorOpts {
    fn default() -> Self {
        Self {
            memory_limit: usize::MAX,
            allocation_limit: usize::MAX,
            huge_threshold: DEFAULT_HUGE_THRESHOLD,
            max_chunk_size: DEFAULT_MAX_CHUNK_SIZE,
        }
    }
}

/// Statistics tracked by the allocator
#[derive(Debug, Clone, Default)]
pub struct AllocatorStats {
    /// Current total allocated size
    pub current_size: usize,

    /// Peak allocated size
    pub peak_size: usize,

    /// Number of allocations made
    pub num_allocs: usize,

    /// Number of deallocations made
    pub num_frees: usize,
}

/// Internal allocator state managing limits and statistics
pub struct AllocatorState {
    /// Current allocated size
    pub current_size: usize,

    /// Maximum allowed allocated size
    pub max_size: usize,

    /// Number of allocations performed
    pub num_allocs: usize,

    /// Maximum number of allocations allowed
    pub max_allocs: usize,

    /// Size threshold for huge allocations
    pub huge_size: usize,

    /// Maximum chunk size
    pub chunk_max: usize,

    /// Name for error reporting
    pub name: String,

    /// Statistics
    pub stats: AllocatorStats,
}

impl AllocatorState {
    /// Create a new allocator state from options
    pub fn new(opts: &AllocatorOpts, name: impl Into<String>) -> Self {
        Self {
            current_size: 0,
            max_size: opts.memory_limit,
            num_allocs: 0,
            max_allocs: opts.allocation_limit,
            huge_size: if opts.huge_threshold == 0 {
                DEFAULT_HUGE_THRESHOLD
            } else {
                opts.huge_threshold
            },
            chunk_max: if opts.max_chunk_size == 0 {
                DEFAULT_MAX_CHUNK_SIZE
            } else {
                opts.max_chunk_size
            },
            name: name.into(),
            stats: AllocatorStats::default(),
        }
    }

    /// Check if an allocation would overflow
    fn does_overflow(total: usize, a: usize, b: usize) -> bool {
        // If `a` and `b` have at most 4 bits per usize byte, the product can't overflow
        if ((a | b) >> (std::mem::size_of::<usize>() * 4)) != 0 {
            if a != 0 && total / a != b {
                return true;
            }
        }
        false
    }

    /// Check if we can allocate the requested size
    pub fn check_allocation(&mut self, total: usize) -> Result<()> {
        // Check for overflow
        if total > usize::MAX / 2 {
            return Err(Error::new(ErrorType::OutOfMemory)
                .with_info(format!("Allocation too large: {}", total)));
        }

        // Check memory limit
        if total > self.max_size.saturating_sub(self.current_size) {
            return Err(Error::new(ErrorType::MemoryLimit)
                .with_info(format!("Memory limit exceeded in {}", self.name)));
        }

        // Check allocation limit
        if self.num_allocs >= self.max_allocs {
            return Err(Error::new(ErrorType::AllocationLimit)
                .with_info(format!("Allocation limit exceeded in {}", self.name)));
        }

        Ok(())
    }

    /// Record an allocation
    pub fn record_alloc(&mut self, size: usize) {
        self.current_size += size;
        self.num_allocs += 1;
        self.stats.num_allocs += 1;

        if self.current_size > self.stats.peak_size {
            self.stats.peak_size = self.current_size;
        }
        self.stats.current_size = self.current_size;
    }

    /// Record a deallocation
    pub fn record_free(&mut self, size: usize) {
        self.current_size = self.current_size.saturating_sub(size);
        self.stats.num_frees += 1;
        self.stats.current_size = self.current_size;
    }
}

/// Get the alignment mask for a given size
#[inline]
pub fn size_align_mask(size: usize) -> usize {
    // Align to all bits below the lowest set one in `size` up to maximum alignment
    ((size ^ (size.wrapping_sub(1))) >> 1) & (MAXIMUM_ALIGNMENT - 1)
}

/// Align a value to the given alignment mask
#[inline]
pub fn align_to_mask(value: usize, align_mask: usize) -> usize {
    value + ((0usize.wrapping_sub(value)) & align_mask)
}

/// Check if a pointer is aligned to the given mask
#[inline]
pub fn is_aligned_mask(ptr: *const u8, align_mask: usize) -> bool {
    (ptr as usize & align_mask) == 0
}

/// Global allocator using Rust's standard allocator
pub struct GlobalAllocator {
    state: AllocatorState,
}

impl GlobalAllocator {
    /// Create a new global allocator with the given options
    pub fn new(opts: AllocatorOpts) -> Self {
        Self {
            state: AllocatorState::new(&opts, "GlobalAllocator"),
        }
    }

    /// Create a new global allocator with default options
    pub fn new_default() -> Self {
        Self::new(AllocatorOpts::default())
    }

    /// Get the current allocator statistics
    pub fn stats(&self) -> &AllocatorStats {
        &self.state.stats
    }

    /// Allocate memory with error handling
    pub fn alloc_checked(&mut self, size: usize, count: usize) -> Result<NonNull<u8>> {
        // Always succeed with a dummy buffer for zero-size allocations
        if count == 0 {
            // Return a well-aligned non-null pointer for zero-size allocations
            return Ok(NonNull::new(MAXIMUM_ALIGNMENT as *mut u8).unwrap());
        }

        let total = size.checked_mul(count).ok_or_else(|| {
            Error::new(ErrorType::OutOfMemory)
                .with_info("Allocation size overflow")
        })?;

        self.state.check_allocation(total)?;

        let ptr = self.alloc(total).ok_or_else(|| {
            Error::new(ErrorType::OutOfMemory)
                .with_info(format!("Failed to allocate {} bytes in {}", total, self.state.name))
        })?;

        self.state.record_alloc(total);

        Ok(ptr)
    }

    /// Reallocate memory with error handling
    pub fn realloc_checked(
        &mut self,
        old_ptr: Option<NonNull<u8>>,
        old_size: usize,
        old_count: usize,
        new_count: usize,
        size: usize,
    ) -> Result<Option<NonNull<u8>>> {
        // Handle zero-size cases
        if old_count == 0 {
            return Ok(Some(self.alloc_checked(size, new_count)?));
        }

        if new_count == 0 {
            if let Some(ptr) = old_ptr {
                let old_total = old_size * old_count;
                self.free(ptr, old_total);
                self.state.record_free(old_total);
            }
            return Ok(None);
        }

        let old_total = old_size * old_count;
        let new_total = size.checked_mul(new_count).ok_or_else(|| {
            Error::new(ErrorType::OutOfMemory)
                .with_info("Reallocation size overflow")
        })?;

        self.state.check_allocation(new_total.saturating_sub(old_total))?;

        let new_ptr = self.realloc(old_ptr, old_total, new_total).ok_or_else(|| {
            Error::new(ErrorType::OutOfMemory)
                .with_info(format!("Failed to reallocate {} bytes in {}", new_total, self.state.name))
        })?;

        self.state.record_free(old_total);
        self.state.record_alloc(new_total);

        Ok(Some(new_ptr))
    }
}

impl Allocator for GlobalAllocator {
    fn alloc(&mut self, size: usize) -> Option<NonNull<u8>> {
        if size == 0 {
            return NonNull::new(MAXIMUM_ALIGNMENT as *mut u8);
        }

        let align = size_align_mask(size).max(1).next_power_of_two();
        let layout = Layout::from_size_align(size, align).ok()?;

        let ptr = unsafe { alloc::alloc(layout) };
        NonNull::new(ptr)
    }

    fn realloc(
        &mut self,
        old_ptr: Option<NonNull<u8>>,
        old_size: usize,
        new_size: usize,
    ) -> Option<NonNull<u8>> {
        if old_size == 0 {
            return self.alloc(new_size);
        }

        if new_size == 0 {
            if let Some(ptr) = old_ptr {
                self.free(ptr, old_size);
            }
            return None;
        }

        let old_align = size_align_mask(old_size).max(1).next_power_of_two();
        let new_align = size_align_mask(new_size).max(1).next_power_of_two();

        let old_layout = Layout::from_size_align(old_size, old_align).ok()?;

        let ptr = old_ptr?;

        let new_ptr = if old_align == new_align {
            unsafe { alloc::realloc(ptr.as_ptr(), old_layout, new_size) }
        } else {
            // If alignment changes, allocate new + copy + free old
            let new_layout = Layout::from_size_align(new_size, new_align).ok()?;
            let new_ptr = unsafe { alloc::alloc(new_layout) };
            if !new_ptr.is_null() {
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        ptr.as_ptr(),
                        new_ptr,
                        old_size.min(new_size),
                    );
                    alloc::dealloc(ptr.as_ptr(), old_layout);
                }
            }
            new_ptr
        };

        NonNull::new(new_ptr)
    }

    fn free(&mut self, ptr: NonNull<u8>, size: usize) {
        if size == 0 {
            return;
        }

        let align = size_align_mask(size).max(1).next_power_of_two();
        let layout = Layout::from_size_align(size, align).unwrap();

        unsafe {
            alloc::dealloc(ptr.as_ptr(), layout);
        }
    }
}

impl Drop for GlobalAllocator {
    fn drop(&mut self) {
        self.cleanup();
    }
}

/// Helper struct for managing allocated arrays with automatic growth
pub struct GrowableArray<T> {
    ptr: Option<NonNull<T>>,
    capacity: usize,
    len: usize,
}

impl<T> GrowableArray<T> {
    /// Create a new empty growable array
    pub fn new() -> Self {
        Self {
            ptr: None,
            capacity: 0,
            len: 0,
        }
    }

    /// Ensure the array can hold at least `required` elements
    pub fn grow<A: Allocator>(
        &mut self,
        allocator: &mut A,
        required: usize,
    ) -> Result<()> {
        if required <= self.capacity {
            return Ok(());
        }

        let new_capacity = self.capacity.max(required).max(4) * 2;
        let old_size = std::mem::size_of::<T>() * self.capacity;
        let new_size = std::mem::size_of::<T>() * new_capacity;

        let old_ptr = self.ptr.map(|p| p.cast::<u8>());
        let new_ptr = allocator
            .realloc(old_ptr, old_size, new_size)
            .ok_or_else(|| Error::new(ErrorType::OutOfMemory))?;

        self.ptr = Some(new_ptr.cast::<T>());
        self.capacity = new_capacity;

        Ok(())
    }

    /// Get the current capacity
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Get the current length
    pub fn len(&self) -> usize {
        self.len
    }

    /// Check if the array is empty
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

impl<T> Default for GrowableArray<T> {
    fn default() -> Self {
        Self::new()
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
    }

    #[test]
    fn test_align_to_mask() {
        assert_eq!(align_to_mask(0, 7), 0);
        assert_eq!(align_to_mask(1, 7), 8);
        assert_eq!(align_to_mask(8, 7), 8);
        assert_eq!(align_to_mask(9, 7), 16);
    }

    #[test]
    fn test_global_allocator_basic() {
        let mut allocator = GlobalAllocator::new_default();

        let ptr = allocator.alloc(1024).unwrap();
        assert!(!ptr.as_ptr().is_null());

        allocator.free(ptr, 1024);
    }

    #[test]
    fn test_global_allocator_checked() {
        let mut allocator = GlobalAllocator::new_default();

        let ptr = allocator.alloc_checked(256, 4).unwrap();
        assert!(!ptr.as_ptr().is_null());

        allocator.free(ptr, 1024);
    }

    #[test]
    fn test_global_allocator_realloc() {
        let mut allocator = GlobalAllocator::new_default();

        let ptr1 = allocator.alloc(512).unwrap();
        let ptr2 = allocator.realloc(Some(ptr1), 512, 1024).unwrap();

        assert!(!ptr2.as_ptr().is_null());

        allocator.free(ptr2, 1024);
    }

    #[test]
    fn test_memory_limit() {
        let opts = AllocatorOpts {
            memory_limit: 1024,
            ..Default::default()
        };

        let mut allocator = GlobalAllocator::new(opts);

        // Should succeed
        let ptr1 = allocator.alloc_checked(512, 1).unwrap();

        // Should fail due to limit
        let result = allocator.alloc_checked(1024, 1);
        assert!(result.is_err());

        allocator.free(ptr1, 512);
    }

    #[test]
    fn test_allocation_limit() {
        let opts = AllocatorOpts {
            allocation_limit: 2,
            ..Default::default()
        };

        let mut allocator = GlobalAllocator::new(opts);

        // First two should succeed
        let _ptr1 = allocator.alloc_checked(64, 1).unwrap();
        let _ptr2 = allocator.alloc_checked(64, 1).unwrap();

        // Third should fail
        let result = allocator.alloc_checked(64, 1);
        assert!(result.is_err());
    }

    #[test]
    fn test_zero_size_allocation() {
        let mut allocator = GlobalAllocator::new_default();

        let ptr = allocator.alloc_checked(128, 0).unwrap();
        assert!(!ptr.as_ptr().is_null());

        // Zero-size allocations don't need to be freed
    }

    #[test]
    fn test_allocator_stats() {
        let mut allocator = GlobalAllocator::new_default();

        let ptr = allocator.alloc_checked(1024, 1).unwrap();
        let stats = allocator.stats();

        assert_eq!(stats.num_allocs, 1);
        assert_eq!(stats.current_size, 1024);

        allocator.free(ptr, 1024);
        allocator.state.record_free(1024);

        let stats = allocator.stats();
        assert_eq!(stats.num_frees, 1);
        assert_eq!(stats.current_size, 0);
        assert_eq!(stats.peak_size, 1024);
    }
}
