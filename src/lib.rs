//! # ufbx - Pure Rust FBX File Loader
//!
//! A safe, pure Rust implementation of the ufbx FBX file loader.
//! This is a complete port of the C implementation to Rust, requiring no C/C++ toolchain.
//!
//! ## Features
//!
//! - Load binary and ASCII FBX files (version 3000+)
//! - Load Wavefront OBJ files
//! - Safe: no unsafe code where possible
//! - Memory efficient with custom allocators
//! - Supports meshes, skinning, blend shapes, NURBS, animations, and more
//!
//! ## Example
//!
//! ```rust,no_run
//! use ufbx::{load_file, LoadOpts};
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let scene = load_file("model.fbx", &LoadOpts::default())?;
//!
//!     for node in &scene.nodes {
//!         if !node.is_root {
//!             println!("Object: {}", node.name);
//!             if let Some(mesh) = &node.mesh {
//!                 println!("-> mesh with {} faces", mesh.faces.len());
//!             }
//!         }
//!     }
//!
//!     Ok(())
//! }
//! ```

#![warn(missing_docs)]
#![cfg_attr(not(feature = "std"), no_std)]

// Module organization following the C implementation structure

// Phase 1: Foundation & Core Infrastructure
pub mod platform;      // Platform detection, alignment, version
pub mod utils;         // Utilities, math, bit manipulation
pub mod deflate;       // DEFLATE compression (optional)
pub mod error;         // Error handling and reporting
pub mod allocator;     // Custom memory allocation
pub mod memory;        // Memory buffers and management
pub mod hash;          // Hash maps and functions
pub mod strings;       // String pool and interning
pub mod threading;     // Thread pool and atomics

// Phase 2: Type Definitions & I/O
pub mod types;         // Core FBX data structures
pub mod progress;      // Progress reporting callbacks
pub mod io;            // I/O abstraction layer

// Phase 3: Parsing Infrastructure
pub mod xml;           // XML parser
pub mod fbx_types;     // FBX value type system
pub mod dom;           // DOM node operations
pub mod binary_parse;  // Binary FBX parser
pub mod ascii_parse;   // ASCII FBX parser

// Phase 4: File Format Support
pub mod parsing;       // General parsing logic
pub mod obj;           // Wavefront OBJ support

// Phase 5: Scene Processing
pub mod scene;         // Scene construction and processing

// Phase 6: Advanced Features
pub mod geometry;      // Geometry processing and caches
pub mod animation;     // Animation evaluation and baking
pub mod nurbs;         // NURBS curves and surfaces
pub mod topology;      // Mesh topology operations
pub mod subdivision;   // Subdivision surface evaluation

// Phase 7: Public API
pub mod api;           // Main public API

// Re-exports for convenience
pub use api::{load_file, load_memory, free_scene};
pub use error::{Error, Result};
pub use types::*;

/// Library version information
pub const VERSION_MAJOR: u32 = 0;
pub const VERSION_MINOR: u32 = 21;
pub const VERSION_PATCH: u32 = 2;

/// Get the library version as a string
pub fn version() -> &'static str {
    concat!(
        env!("CARGO_PKG_VERSION_MAJOR"), ".",
        env!("CARGO_PKG_VERSION_MINOR"), ".",
        env!("CARGO_PKG_VERSION_PATCH")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() {
        assert_eq!(version(), "0.21.2");
    }
}
