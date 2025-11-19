//! # ufbx - Pure Rust FBX File Loader and Writer
//!
//! A safe, pure Rust implementation of FBX file loading and writing.
//! This is a complete port of the C implementation to Rust, requiring no C/C++ toolchain.
//!
//! ## Features
//!
//! - Load binary and ASCII FBX files (version 3000+)
//! - Write binary and ASCII FBX files (with `writer` feature)
//! - Load Wavefront OBJ files
//! - Safe: no unsafe code where possible
//! - Memory efficient with custom allocators
//! - Supports meshes, skinning, blend shapes, NURBS, animations, and more
//!
//! ## Example - Loading
//!
//! ```rust,no_run
//! use ufbx::{load_file, Scene};
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let scene = load_file("model.fbx", &Default::default())?;
//!
//!     for node in &scene.nodes {
//!         if !node.is_root {
//!             println!("Object: {}", node.name);
//!         }
//!     }
//!
//!     Ok(())
//! }
//! ```
//!
//! ## Example - Writing
//!
//! ```rust,no_run
//! # #[cfg(feature = "writer")]
//! # {
//! use ufbx::{load_file, save_file, SaveOpts, SaveFormat};
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let scene = load_file("input.fbx", &Default::default())?;
//!
//!     save_file(&scene, "output.fbx", &SaveOpts {
//!         format: SaveFormat::Binary,
//!         version: 7400,
//!         compress_arrays: true,
//!         ..Default::default()
//!     })?;
//!
//!     Ok(())
//! }
//! # }
//! ```

#![warn(missing_docs)]
#![cfg_attr(not(feature = "std"), no_std)]

// Core modules - FBX-specific domain logic only

pub mod error;         // Error types and Result
pub mod types;         // FBX scene data structures (Scene, Node, Mesh, etc.)
pub mod binary;        // FBX binary format parser
pub mod ascii;         // FBX ASCII format parser
pub mod scene;         // Scene graph construction from parsed data

// Writer modules (behind feature flag)
#[cfg(feature = "writer")]
pub mod writer;        // Scene serialization to FBX

#[cfg(feature = "writer")]
pub mod binary_writer; // Binary FBX encoder

#[cfg(feature = "writer")]
pub mod ascii_writer;  // ASCII FBX encoder

// Optional feature modules
#[cfg(feature = "obj-support")]
pub mod obj;           // Wavefront OBJ/MTL parser

#[cfg(feature = "nurbs")]
pub mod nurbs;         // NURBS curve/surface evaluation

#[cfg(feature = "subdivision")]
pub mod subdivision;   // Catmull-Clark subdivision surfaces

pub mod geometry;      // Mesh processing (triangulation, skinning, topology)
pub mod animation;     // Animation curve evaluation and blending

// Re-exports for convenience
pub use error::{Error, Result};
pub use types::*;

// Public API functions for loading
pub use scene::{load_file, load_memory, SceneOpts, SceneBuilder};

// Public API functions for writing (behind feature flag)
#[cfg(feature = "writer")]
pub use writer::{save_file, save_memory, SaveOpts, SaveFormat};

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
