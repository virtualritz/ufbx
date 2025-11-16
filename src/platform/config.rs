//! Platform configuration and feature flags
//!
//! This module provides compile-time configuration constants and feature flags
//! corresponding to the C configuration macros in ufbx.c lines 49-191.

// -- Configuration Constants (lines 51-73)

/// Maximum number of non-array values
pub const MAX_NON_ARRAY_VALUES: usize = 8;

/// Maximum node depth
pub const MAX_NODE_DEPTH: usize = 32;

/// Maximum XML depth
pub const MAX_XML_DEPTH: usize = 32;

/// Maximum skip size
pub const MAX_SKIP_SIZE: usize = 0x40000000;

/// Maximum map scan iterations
pub const MAP_MAX_SCAN: usize = 32;

/// KD-tree fast depth
pub const KD_FAST_DEPTH: usize = 6;

/// Huge max scan iterations
pub const HUGE_MAX_SCAN: usize = 16;

/// Minimum file format lookahead
pub const MIN_FILE_FORMAT_LOOKAHEAD: usize = 32;

/// Face group hash bits
pub const FACE_GROUP_HASH_BITS: usize = 8;

/// Minimum bytes for threaded deflate
pub const MIN_THREADED_DEFLATE_BYTES: usize = 256;

/// Minimum values for threaded ASCII parsing
pub const MIN_THREADED_ASCII_VALUES: usize = 64;

/// Geometry cache buffer size
pub const GEOMETRY_CACHE_BUFFER_SIZE: usize = 512;

/// Maximum NURBS order
pub const MAX_NURBS_ORDER: usize = 128;

/// Epsilon value for floating point comparisons
///
/// By default enough to have squares be non-denormal
pub const EPSILON_F32: f32 = 1.0842021795674597e-19f32;
pub const EPSILON_F64: f64 = 1.4916681462400413e-154f64;

/// Get epsilon for the real type
#[inline]
pub const fn epsilon<T>() -> T
where
    T: Copy,
{
    // This will be specialized for f32 and f64
    panic!("epsilon() only works for f32 and f64")
}

/// Specialized epsilon for f32
#[inline]
pub const fn epsilon_f32() -> f32 {
    EPSILON_F32
}

/// Specialized epsilon for f64
#[inline]
pub const fn epsilon_f64() -> f64 {
    EPSILON_F64
}

// -- Feature Flags (lines 74-191)

/// Feature flag type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Features {
    pub subdivision: bool,
    pub tessellation: bool,
    pub geometry_cache: bool,
    pub scene_evaluation: bool,
    pub skinning_evaluation: bool,
    pub animation_baking: bool,
    pub triangulation: bool,
    pub index_generation: bool,
    pub format_obj: bool,
    pub error_stack: bool,
}

impl Features {
    /// Default feature set (all features enabled unless minimal mode)
    pub const fn full() -> Self {
        Self {
            subdivision: true,
            tessellation: true,
            geometry_cache: true,
            scene_evaluation: true,
            skinning_evaluation: true,
            animation_baking: true,
            triangulation: true,
            index_generation: true,
            format_obj: true,
            error_stack: false,
        }
    }

    /// Minimal feature set
    pub const fn minimal() -> Self {
        Self {
            subdivision: false,
            tessellation: false,
            geometry_cache: false,
            scene_evaluation: false,
            skinning_evaluation: false,
            animation_baking: false,
            triangulation: false,
            index_generation: false,
            format_obj: false,
            error_stack: false,
        }
    }

    /// Development feature set (includes error stack)
    pub const fn dev() -> Self {
        Self {
            subdivision: true,
            tessellation: true,
            geometry_cache: true,
            scene_evaluation: true,
            skinning_evaluation: true,
            animation_baking: true,
            triangulation: true,
            index_generation: true,
            format_obj: true,
            error_stack: true,
        }
    }

    /// Check if XML parsing is needed (derived from geometry_cache)
    #[inline]
    pub const fn xml(&self) -> bool {
        self.geometry_cache
    }

    /// Check if KD-tree is needed (derived from triangulation)
    #[inline]
    pub const fn kd(&self) -> bool {
        self.triangulation
    }

    /// Check if partial features are enabled
    #[inline]
    pub const fn partial(&self) -> bool {
        !self.subdivision
            || !self.tessellation
            || !self.geometry_cache
            || !self.scene_evaluation
            || !self.skinning_evaluation
            || !self.animation_baking
            || !self.triangulation
            || !self.index_generation
            || !self.xml()
            || !self.kd()
            || !self.format_obj
    }
}

/// Default features
pub const DEFAULT_FEATURES: Features = Features::full();

// -- Regression and Debug Configuration

/// Clamp linear threshold for debugging
#[cfg(feature = "regression")]
#[inline]
pub const fn clamp_linear_threshold(_v: usize) -> usize {
    2
}

/// Normal linear threshold
#[cfg(not(feature = "regression"))]
#[inline]
pub const fn clamp_linear_threshold(v: usize) -> usize {
    v
}

/// Check if regression mode is enabled
#[inline]
pub const fn is_regression() -> bool {
    cfg!(feature = "regression")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_features() {
        let full = Features::full();
        assert!(full.subdivision);
        assert!(full.xml());
        assert!(full.kd());

        let minimal = Features::minimal();
        assert!(!minimal.subdivision);
        assert!(!minimal.xml());
        assert!(minimal.partial());
    }

    #[test]
    fn test_epsilon() {
        assert!(EPSILON_F32 > 0.0);
        assert!(EPSILON_F64 > 0.0);
    }

    #[test]
    fn test_constants() {
        assert_eq!(MAX_NON_ARRAY_VALUES, 8);
        assert_eq!(MAX_NODE_DEPTH, 32);
        assert_eq!(MAX_XML_DEPTH, 32);
    }
}
