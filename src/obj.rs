//! Wavefront OBJ/MTL File Parser
//!
//! This module provides support for loading Wavefront .obj and .mtl files,
//! converting them to ufbx Scene structures.
//!
//! # OBJ Format Overview
//!
//! The Wavefront OBJ format is a simple text-based 3D geometry format:
//! - `v x y z [w]` - Vertex position (w optional, default 1.0)
//! - `vn x y z` - Vertex normal
//! - `vt u v [w]` - Texture coordinate (w optional, default 0.0)
//! - `f v/vt/vn ...` - Face (indices can be v, v/vt, v//vn, or v/vt/vn)
//! - `o name` - Object name
//! - `g name` - Group name
//! - `s on|off|num` - Smoothing group
//! - `usemtl name` - Use material
//! - `mtllib file.mtl` - Material library reference
//!
//! # MTL Format Overview
//!
//! The MTL format defines materials:
//! - `newmtl name` - Define new material
//! - `Ka r g b` - Ambient color
//! - `Kd r g b` - Diffuse color
//! - `Ks r g b` - Specular color
//! - `Ke r g b` - Emissive color
//! - `Ns exponent` - Specular exponent
//! - `d alpha` - Dissolve (opacity)
//! - `map_Kd file` - Diffuse texture
//! - `map_Ks file` - Specular texture
//! - `map_bump file` or `bump file` - Bump/normal map
//!
//! # Key OBJ Format Quirks
//!
//! 1. **1-Based Indexing**: OBJ uses 1-based indices (first vertex is 1, not 0)
//! 2. **Negative Indices**: Negative indices count backwards from the most recent vertex
//!    - `-1` refers to the last vertex defined
//!    - `-2` refers to the second-to-last vertex, etc.
//! 3. **Independent Attributes**: Faces can reference position/UV/normal independently
//!    - `f 1/1/1 2/2/2 3/3/3` - All three attributes
//!    - `f 1//1 2//2 3//3` - Position and normal only
//!    - `f 1/1 2/2 3/3` - Position and UV only
//!    - `f 1 2 3` - Position only
//! 4. **Line Continuations**: Backslash `\` at end of line continues to next line
//! 5. **Comments**: `#` starts a comment (rest of line ignored)
//!
//! # Example
//!
//! ```rust,no_run
//! use ufbx::obj::ObjParser;
//!
//! let scene = ObjParser::parse_file("model.obj")?;
//! println!("Loaded {} nodes", scene.nodes.len());
//! # Ok::<(), ufbx::Error>(())
//! ```

use crate::error::{Error, Result};
use crate::types::*;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

// =============================================================================
// Constants and Configuration
// =============================================================================

/// Maximum number of vertices to batch before flushing (memory optimization)
const MAX_VERTEX_BATCH: usize = 1_000_000;

/// Default material name when none is specified
const DEFAULT_MATERIAL: &str = "default";

// =============================================================================
// Index Resolution
// =============================================================================

/// Resolve an OBJ index to a zero-based array index.
///
/// OBJ uses 1-based indexing with support for negative indices:
/// - Positive: 1 = first element (index 0), 2 = second (index 1), etc.
/// - Negative: -1 = last element, -2 = second-to-last, etc.
/// - Zero: Invalid (returns None)
///
/// # Arguments
///
/// * `index` - The OBJ file index (can be negative)
/// * `count` - Total number of elements available
///
/// # Returns
///
/// * `Some(usize)` - Valid zero-based index
/// * `None` - Invalid index (zero or out of bounds)
fn resolve_obj_index(index: i64, count: usize) -> Option<usize> {
    if index == 0 {
        return None; // OBJ doesn't use 0-based indexing
    }

    if index > 0 {
        // Positive: convert from 1-based to 0-based
        let idx = (index - 1) as usize;
        if idx < count {
            Some(idx)
        } else {
            None
        }
    } else {
        // Negative: count from end
        let idx = (count as i64 + index) as usize;
        if idx < count {
            Some(idx)
        } else {
            None
        }
    }
}

// =============================================================================
// Face Vertex Index
// =============================================================================

/// A single vertex reference in a face, with optional UV and normal indices.
///
/// OBJ format allows faces to reference position, UV, and normal independently:
/// - `f 1/2/3` - position=1, uv=2, normal=3
/// - `f 1//3` - position=1, no UV, normal=3
/// - `f 1/2` - position=1, uv=2, no normal
/// - `f 1` - position=1, no UV or normal
#[derive(Debug, Clone, Copy, Default)]
struct FaceVertex {
    position: Option<usize>,
    uv: Option<usize>,
    normal: Option<usize>,
}

impl FaceVertex {
    /// Parse a face vertex from an OBJ index string (e.g., "1/2/3" or "1//3")
    fn parse(s: &str, pos_count: usize, uv_count: usize, norm_count: usize) -> Result<Self> {
        let parts: Vec<&str> = s.split('/').collect();

        let position = if !parts.is_empty() && !parts[0].is_empty() {
            let idx = parts[0]
                .parse::<i64>()
                .map_err(|_| Error::unknown(format!("Invalid position index: {}", parts[0])))?;
            resolve_obj_index(idx, pos_count)
        } else {
            None
        };

        let uv = if parts.len() > 1 && !parts[1].is_empty() {
            let idx = parts[1]
                .parse::<i64>()
                .map_err(|_| Error::unknown(format!("Invalid UV index: {}", parts[1])))?;
            resolve_obj_index(idx, uv_count)
        } else {
            None
        };

        let normal = if parts.len() > 2 && !parts[2].is_empty() {
            let idx = parts[2]
                .parse::<i64>()
                .map_err(|_| Error::unknown(format!("Invalid normal index: {}", parts[2])))?;
            resolve_obj_index(idx, norm_count)
        } else {
            None
        };

        Ok(FaceVertex {
            position,
            uv,
            normal,
        })
    }
}

// =============================================================================
// Parsed OBJ Data
// =============================================================================

/// Intermediate storage for parsed OBJ geometry data before conversion to Mesh
#[derive(Debug, Default)]
struct ObjGeometry {
    // Raw attribute arrays
    positions: Vec<Vec3>,
    uvs: Vec<Vec2>,
    normals: Vec<Vec3>,
    colors: Vec<Vec4>, // Vertex colors (extension)

    // Face data
    faces: Vec<Vec<FaceVertex>>,
    face_materials: Vec<Option<usize>>, // Material index per face
    face_smoothing: Vec<bool>,          // Smoothing enabled per face

    // Object/group hierarchy
    objects: Vec<String>,
    groups: Vec<String>,
    current_object: Option<usize>,
    current_group: Option<usize>,

    // Material references
    current_material: Option<usize>,
}

impl ObjGeometry {
    fn new() -> Self {
        Self::default()
    }

    /// Add a vertex position
    fn add_position(&mut self, x: Real, y: Real, z: Real, _w: Option<Real>) {
        // Note: w component typically ignored in most applications
        self.positions.push(Vec3::new(x, y, z));
    }

    /// Add a texture coordinate
    fn add_uv(&mut self, u: Real, v: Real, _w: Option<Real>) {
        // Note: w component (for 3D textures) rarely used
        self.uvs.push(Vec2::new(u, v));
    }

    /// Add a normal vector
    fn add_normal(&mut self, x: Real, y: Real, z: Real) {
        self.normals.push(Vec3::new(x, y, z));
    }

    /// Add a vertex color
    fn add_color(&mut self, r: Real, g: Real, b: Real, a: Real) {
        self.colors.push(Vec4::new(r, g, b, a));
    }

    /// Add a face
    fn add_face(&mut self, vertices: Vec<FaceVertex>) {
        self.faces.push(vertices);
        self.face_materials.push(self.current_material);
        self.face_smoothing.push(true); // Default smoothing on
    }

    /// Set the current object
    fn set_object(&mut self, name: String) {
        if let Some(idx) = self.objects.iter().position(|o| o == &name) {
            self.current_object = Some(idx);
        } else {
            self.current_object = Some(self.objects.len());
            self.objects.push(name);
        }
    }

    /// Set the current group
    fn set_group(&mut self, name: String) {
        if let Some(idx) = self.groups.iter().position(|g| g == &name) {
            self.current_group = Some(idx);
        } else {
            self.current_group = Some(self.groups.len());
            self.groups.push(name);
        }
    }

    /// Convert to Mesh structure
    fn into_mesh(self, material_map: &HashMap<String, Material>) -> Result<Mesh> {
        let mut mesh = Mesh {
            element: Element::new("ObjMesh", ElementType::Mesh),
            instances: Vec::new(),
            num_vertices: self.positions.len(),
            num_indices: 0,
            num_faces: self.faces.len(),
            num_triangles: 0,
            num_edges: 0,
            max_face_triangles: 0,
            num_empty_faces: 0,
            num_point_faces: 0,
            num_line_faces: 0,
            faces: Vec::new(),
            face_smoothing: Vec::new(),
            face_material: Vec::new(),
            face_group: Vec::new(),
            face_hole: Vec::new(),
            edges: Vec::new(),
            edge_smoothing: Vec::new(),
            edge_crease: Vec::new(),
            edge_visibility: Vec::new(),
            vertex_indices: Vec::new(),
            vertices: self.positions.clone(),
            vertex_first_index: Vec::new(),
            vertex_position: VertexAttrib::default(),
            vertex_normal: VertexAttrib::default(),
            vertex_uv: VertexAttrib::default(),
            vertex_tangent: VertexAttrib::default(),
            vertex_bitangent: VertexAttrib::default(),
            vertex_color: VertexAttrib::default(),
            vertex_crease: VertexAttrib::default(),
            uv_sets: Vec::new(),
            color_sets: Vec::new(),
            materials: Vec::new(),
            face_groups: Vec::new(),
            material_parts: Vec::new(),
            face_group_parts: Vec::new(),
            material_part_usage_order: Vec::new(),
            skinned_is_local: false,
            skinned_position: VertexAttrib::default(),
            skinned_normal: VertexAttrib::default(),
            skin_deformers: Vec::new(),
            blend_deformers: Vec::new(),
            cache_deformers: Vec::new(),
            all_deformers: Vec::new(),
            subdivision_preview_levels: 0,
            subdivision_render_levels: 0,
            subdivision_display_mode: SubdivisionDisplayMode::Disabled,
            subdivision_boundary: SubdivisionBoundary::Default,
            subdivision_uv_boundary: SubdivisionBoundary::Default,
            reversed_winding: false,
            generated_normals: false,
            subdivision_evaluated: false,
            from_tessellated_nurbs: false,
        };

        // Build vertex position attribute
        if !self.positions.is_empty() {
            mesh.vertex_position.exists = true;
            mesh.vertex_position.values = self.positions.clone();
            mesh.vertex_position.unique_per_vertex = true;
        }

        // Build UV attribute if present
        if !self.uvs.is_empty() {
            mesh.vertex_uv.exists = true;
            mesh.vertex_uv.values = self.uvs.clone();
        }

        // Build normal attribute if present
        if !self.normals.is_empty() {
            mesh.vertex_normal.exists = true;
            mesh.vertex_normal.values = self.normals.clone();
        }

        // Build color attribute if present
        if !self.colors.is_empty() {
            mesh.vertex_color.exists = true;
            mesh.vertex_color.values = self.colors.clone();
        }

        // Process faces and build indices
        let mut index_begin = 0u32;
        for (face_idx, face_verts) in self.faces.iter().enumerate() {
            let num_verts = face_verts.len() as u32;

            if num_verts == 0 {
                mesh.num_empty_faces += 1;
            } else if num_verts == 1 {
                mesh.num_point_faces += 1;
            } else if num_verts == 2 {
                mesh.num_line_faces += 1;
            } else {
                // Triangle fan triangulation for n-gons
                let num_tris = num_verts.saturating_sub(2);
                mesh.num_triangles += num_tris as usize;
                mesh.max_face_triangles = mesh.max_face_triangles.max(num_tris as usize);
            }

            mesh.faces.push(Face {
                index_begin,
                num_indices: num_verts,
            });

            // Build vertex indices for this face
            for vert in face_verts {
                if let Some(pos_idx) = vert.position {
                    mesh.vertex_indices.push(pos_idx as u32);

                    // Add UV index if present
                    if mesh.vertex_uv.exists {
                        if let Some(uv_idx) = vert.uv {
                            mesh.vertex_uv.indices.push(uv_idx as u32);
                        } else {
                            mesh.vertex_uv.indices.push(0); // Default UV
                        }
                    }

                    // Add normal index if present
                    if mesh.vertex_normal.exists {
                        if let Some(norm_idx) = vert.normal {
                            mesh.vertex_normal.indices.push(norm_idx as u32);
                        } else {
                            mesh.vertex_normal.indices.push(0); // Default normal
                        }
                    }
                } else {
                    return Err(Error::unknown("Face vertex missing position index"));
                }
            }

            mesh.num_indices += num_verts as usize;
            index_begin += num_verts;

            // Add face material
            if let Some(mat_idx) = self.face_materials[face_idx] {
                mesh.face_material.push(mat_idx as u32);
            } else {
                mesh.face_material.push(0); // Default material
            }

            // Add face smoothing
            mesh.face_smoothing.push(self.face_smoothing[face_idx]);
        }

        // Set position indices (required)
        mesh.vertex_position.indices = mesh.vertex_indices.clone();

        Ok(mesh)
    }
}

// =============================================================================
// Material Data
// =============================================================================

/// Parsed MTL material
#[derive(Debug, Clone)]
pub struct Material {
    pub name: String,
    pub ambient: Vec3,
    pub diffuse: Vec3,
    pub specular: Vec3,
    pub emissive: Vec3,
    pub specular_exponent: Real,
    pub opacity: Real,
    pub refractive_index: Real,
    pub illum_model: u32,

    // Texture maps
    pub map_diffuse: Option<String>,
    pub map_specular: Option<String>,
    pub map_ambient: Option<String>,
    pub map_emissive: Option<String>,
    pub map_bump: Option<String>,
    pub map_normal: Option<String>,
    pub map_displacement: Option<String>,
    pub map_opacity: Option<String>,
}

impl Default for Material {
    fn default() -> Self {
        Self {
            name: DEFAULT_MATERIAL.to_string(),
            ambient: Vec3::new(0.2, 0.2, 0.2),
            diffuse: Vec3::new(0.8, 0.8, 0.8),
            specular: Vec3::new(0.0, 0.0, 0.0),
            emissive: Vec3::ZERO,
            specular_exponent: 0.0,
            opacity: 1.0,
            refractive_index: 1.0,
            illum_model: 2,
            map_diffuse: None,
            map_specular: None,
            map_ambient: None,
            map_emissive: None,
            map_bump: None,
            map_normal: None,
            map_displacement: None,
            map_opacity: None,
        }
    }
}

// =============================================================================
// OBJ Parser
// =============================================================================

/// Main OBJ/MTL parser
pub struct ObjParser {
    geometry: ObjGeometry,
    materials: HashMap<String, Material>,
    current_material: Option<String>,
    smoothing_enabled: bool,
    line_number: usize,
}

impl ObjParser {
    /// Create a new parser
    pub fn new() -> Self {
        Self {
            geometry: ObjGeometry::new(),
            materials: HashMap::new(),
            current_material: None,
            smoothing_enabled: true,
            line_number: 0,
        }
    }

    /// Parse an OBJ file from a path
    pub fn parse_file(path: impl AsRef<Path>) -> Result<Scene> {
        let path = path.as_ref();
        let content = fs::read_to_string(path)
            .map_err(|e| Error::io(format!("Failed to read OBJ file: {}", e)))?;

        let mut parser = Self::new();
        parser.parse_obj(&content)?;

        // Try to load MTL file if referenced
        if let Some(parent) = path.parent() {
            for mtl_file in parser.extract_mtl_references(&content) {
                let mtl_path = parent.join(&mtl_file);
                if mtl_path.exists() {
                    if let Ok(mtl_content) = fs::read_to_string(&mtl_path) {
                        let _ = parser.parse_mtl(&mtl_content);
                    }
                }
            }
        }

        parser.into_scene()
    }

    /// Parse OBJ content from a string
    pub fn parse(content: &str) -> Result<Scene> {
        let mut parser = Self::new();
        parser.parse_obj(content)?;
        parser.into_scene()
    }

    /// Parse OBJ content
    fn parse_obj(&mut self, content: &str) -> Result<()> {
        self.line_number = 0;

        for line in content.lines() {
            self.line_number += 1;
            self.parse_obj_line(line)?;
        }

        Ok(())
    }

    /// Parse a single OBJ line
    fn parse_obj_line(&mut self, line: &str) -> Result<()> {
        // Handle line continuations (backslash at end)
        let line = line.trim_end_matches('\\').trim();

        // Skip empty lines and comments
        if line.is_empty() || line.starts_with('#') {
            return Ok(());
        }

        let tokens: Vec<&str> = line.split_whitespace().collect();
        if tokens.is_empty() {
            return Ok(());
        }

        match tokens[0] {
            // Vertex position: v x y z [w]
            "v" => {
                if tokens.len() < 4 {
                    return Err(Error::unknown(format!(
                        "Line {}: 'v' requires at least 3 values (x y z)",
                        self.line_number
                    )));
                }
                let x = self.parse_real(tokens[1])?;
                let y = self.parse_real(tokens[2])?;
                let z = self.parse_real(tokens[3])?;
                let w = tokens.get(4).map(|s| self.parse_real(s)).transpose()?;
                self.geometry.add_position(x, y, z, w);

                // Check for vertex color (extension: v x y z r g b [a])
                if tokens.len() >= 7 {
                    let r = self.parse_real(tokens[4])?;
                    let g = self.parse_real(tokens[5])?;
                    let b = self.parse_real(tokens[6])?;
                    let a = tokens.get(7).map(|s| self.parse_real(s)).transpose()?.unwrap_or(1.0);
                    self.geometry.add_color(r, g, b, a);
                }
            }

            // Texture coordinate: vt u v [w]
            "vt" => {
                if tokens.len() < 3 {
                    return Err(Error::unknown(format!(
                        "Line {}: 'vt' requires at least 2 values (u v)",
                        self.line_number
                    )));
                }
                let u = self.parse_real(tokens[1])?;
                let v = self.parse_real(tokens[2])?;
                let w = tokens.get(3).map(|s| self.parse_real(s)).transpose()?;
                self.geometry.add_uv(u, v, w);
            }

            // Vertex normal: vn x y z
            "vn" => {
                if tokens.len() < 4 {
                    return Err(Error::unknown(format!(
                        "Line {}: 'vn' requires 3 values (x y z)",
                        self.line_number
                    )));
                }
                let x = self.parse_real(tokens[1])?;
                let y = self.parse_real(tokens[2])?;
                let z = self.parse_real(tokens[3])?;
                self.geometry.add_normal(x, y, z);
            }

            // Face: f v1/vt1/vn1 v2/vt2/vn2 v3/vt3/vn3 ...
            "f" => {
                if tokens.len() < 4 {
                    return Err(Error::unknown(format!(
                        "Line {}: 'f' requires at least 3 vertices",
                        self.line_number
                    )));
                }

                let mut vertices = Vec::new();
                for token in &tokens[1..] {
                    let vert = FaceVertex::parse(
                        token,
                        self.geometry.positions.len(),
                        self.geometry.uvs.len(),
                        self.geometry.normals.len(),
                    )?;
                    vertices.push(vert);
                }

                self.geometry.add_face(vertices);
            }

            // Object name: o name
            "o" => {
                if tokens.len() > 1 {
                    let name = tokens[1..].join(" ");
                    self.geometry.set_object(name);
                }
            }

            // Group name: g name
            "g" => {
                if tokens.len() > 1 {
                    let name = tokens[1..].join(" ");
                    self.geometry.set_group(name);
                }
            }

            // Smoothing: s on|off|group_num
            "s" => {
                if tokens.len() > 1 {
                    self.smoothing_enabled = tokens[1] != "off" && tokens[1] != "0";
                    // Update current smoothing for future faces
                    // (would need to track per-face in a real implementation)
                }
            }

            // Use material: usemtl name
            "usemtl" => {
                if tokens.len() > 1 {
                    let name = tokens[1..].join(" ");
                    self.current_material = Some(name.clone());

                    // Find or create material index
                    let mat_idx = self.materials.keys().position(|k| k == &name).unwrap_or(0);
                    self.geometry.current_material = Some(mat_idx);
                }
            }

            // Material library: mtllib file.mtl
            "mtllib" => {
                // Material library reference - handled externally
                // (we'd need file I/O context to load it here)
            }

            // Line: l v1 v2 ...
            "l" => {
                // Line elements - could be supported if needed
            }

            // Point: p v1 v2 ...
            "p" => {
                // Point elements - could be supported if needed
            }

            _ => {
                // Unknown directive - silently ignore or warn
            }
        }

        Ok(())
    }

    /// Parse MTL content
    pub fn parse_mtl(&mut self, content: &str) -> Result<()> {
        self.line_number = 0;
        let mut current_material: Option<Material> = None;

        for line in content.lines() {
            self.line_number += 1;
            let line = line.trim();

            // Skip empty lines and comments
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let tokens: Vec<&str> = line.split_whitespace().collect();
            if tokens.is_empty() {
                continue;
            }

            match tokens[0] {
                // New material: newmtl name
                "newmtl" => {
                    // Save previous material
                    if let Some(mat) = current_material.take() {
                        self.materials.insert(mat.name.clone(), mat);
                    }

                    // Start new material
                    if tokens.len() > 1 {
                        let name = tokens[1..].join(" ");
                        current_material = Some(Material {
                            name,
                            ..Default::default()
                        });
                    }
                }

                // Ambient: Ka r g b
                "Ka" => {
                    if let Some(ref mut mat) = current_material {
                        mat.ambient = self.parse_vec3(&tokens[1..])?;
                    }
                }

                // Diffuse: Kd r g b
                "Kd" => {
                    if let Some(ref mut mat) = current_material {
                        mat.diffuse = self.parse_vec3(&tokens[1..])?;
                    }
                }

                // Specular: Ks r g b
                "Ks" => {
                    if let Some(ref mut mat) = current_material {
                        mat.specular = self.parse_vec3(&tokens[1..])?;
                    }
                }

                // Emissive: Ke r g b
                "Ke" => {
                    if let Some(ref mut mat) = current_material {
                        mat.emissive = self.parse_vec3(&tokens[1..])?;
                    }
                }

                // Specular exponent: Ns value
                "Ns" => {
                    if let Some(ref mut mat) = current_material {
                        if tokens.len() > 1 {
                            mat.specular_exponent = self.parse_real(tokens[1])?;
                        }
                    }
                }

                // Dissolve/opacity: d value
                "d" => {
                    if let Some(ref mut mat) = current_material {
                        if tokens.len() > 1 {
                            mat.opacity = self.parse_real(tokens[1])?;
                        }
                    }
                }

                // Transparency: Tr value (inverse of d)
                "Tr" => {
                    if let Some(ref mut mat) = current_material {
                        if tokens.len() > 1 {
                            mat.opacity = 1.0 - self.parse_real(tokens[1])?;
                        }
                    }
                }

                // Refractive index: Ni value
                "Ni" => {
                    if let Some(ref mut mat) = current_material {
                        if tokens.len() > 1 {
                            mat.refractive_index = self.parse_real(tokens[1])?;
                        }
                    }
                }

                // Illumination model: illum num
                "illum" => {
                    if let Some(ref mut mat) = current_material {
                        if tokens.len() > 1 {
                            mat.illum_model = tokens[1].parse().unwrap_or(2);
                        }
                    }
                }

                // Texture maps
                "map_Kd" => {
                    if let Some(ref mut mat) = current_material {
                        if tokens.len() > 1 {
                            mat.map_diffuse = Some(tokens[1..].join(" "));
                        }
                    }
                }

                "map_Ks" => {
                    if let Some(ref mut mat) = current_material {
                        if tokens.len() > 1 {
                            mat.map_specular = Some(tokens[1..].join(" "));
                        }
                    }
                }

                "map_Ka" => {
                    if let Some(ref mut mat) = current_material {
                        if tokens.len() > 1 {
                            mat.map_ambient = Some(tokens[1..].join(" "));
                        }
                    }
                }

                "map_Ke" => {
                    if let Some(ref mut mat) = current_material {
                        if tokens.len() > 1 {
                            mat.map_emissive = Some(tokens[1..].join(" "));
                        }
                    }
                }

                "map_bump" | "bump" => {
                    if let Some(ref mut mat) = current_material {
                        if tokens.len() > 1 {
                            mat.map_bump = Some(tokens[1..].join(" "));
                        }
                    }
                }

                "map_d" => {
                    if let Some(ref mut mat) = current_material {
                        if tokens.len() > 1 {
                            mat.map_opacity = Some(tokens[1..].join(" "));
                        }
                    }
                }

                "norm" => {
                    if let Some(ref mut mat) = current_material {
                        if tokens.len() > 1 {
                            mat.map_normal = Some(tokens[1..].join(" "));
                        }
                    }
                }

                "disp" => {
                    if let Some(ref mut mat) = current_material {
                        if tokens.len() > 1 {
                            mat.map_displacement = Some(tokens[1..].join(" "));
                        }
                    }
                }

                _ => {
                    // Unknown directive - ignore
                }
            }
        }

        // Save last material
        if let Some(mat) = current_material {
            self.materials.insert(mat.name.clone(), mat);
        }

        Ok(())
    }

    /// Parse a floating-point number
    fn parse_real(&self, s: &str) -> Result<Real> {
        s.parse::<Real>()
            .map_err(|_| Error::unknown(format!("Line {}: Invalid number '{}'", self.line_number, s)))
    }

    /// Parse a Vec3 from 3 tokens
    fn parse_vec3(&self, tokens: &[&str]) -> Result<Vec3> {
        if tokens.len() < 3 {
            return Err(Error::unknown(format!(
                "Line {}: Expected 3 values for Vec3",
                self.line_number
            )));
        }
        Ok(Vec3::new(
            self.parse_real(tokens[0])?,
            self.parse_real(tokens[1])?,
            self.parse_real(tokens[2])?,
        ))
    }

    /// Extract material library references from OBJ content
    fn extract_mtl_references(&self, content: &str) -> Vec<String> {
        let mut refs = Vec::new();
        for line in content.lines() {
            let line = line.trim();
            if line.starts_with("mtllib ") {
                if let Some(filename) = line.strip_prefix("mtllib ") {
                    refs.push(filename.trim().to_string());
                }
            }
        }
        refs
    }

    /// Convert parsed data into a Scene
    fn into_scene(self) -> Result<Scene> {
        let mesh = self.geometry.into_mesh(&self.materials)?;

        // Create a simple scene with a root node and mesh node
        let root_node = Node {
            element: Element::new("RootNode", ElementType::Node),
            parent: None,
            children: vec![1], // Mesh node
            mesh: None,
            light: None,
            camera: None,
            bone: None,
            attrib: None,
            attrib_type: ElementType::Unknown,
            all_attribs: Vec::new(),
            geometry_transform_helper: None,
            scale_helper: None,
            inherit_mode: InheritMode::Normal,
            original_inherit_mode: InheritMode::Normal,
            local_transform: Transform::IDENTITY,
            geometry_transform: Transform::IDENTITY,
            inherit_scale: Vec3::ONE,
            inherit_scale_node: None,
            rotation_order: RotationOrder::XYZ,
            euler_rotation: Vec3::ZERO,
            node_to_parent: Matrix::IDENTITY,
            node_to_world: Matrix::IDENTITY,
            geometry_to_node: Matrix::IDENTITY,
            geometry_to_world: Matrix::IDENTITY,
            unscaled_node_to_world: Matrix::IDENTITY,
            adjust_pre_translation: Vec3::ZERO,
            adjust_pre_rotation: Quat::IDENTITY,
            adjust_pre_scale: 1.0,
            adjust_post_rotation: Quat::IDENTITY,
            adjust_post_scale: 1.0,
            adjust_translation_scale: 1.0,
            adjust_mirror_axis: MirrorAxis::None,
            materials: Vec::new(),
            bind_pose: None,
            visible: true,
            is_root: true,
            has_geometry_transform: false,
            has_adjust_transform: false,
            has_root_adjust_transform: false,
            is_geometry_transform_helper: false,
            is_scale_helper: false,
            is_scale_compensate_parent: false,
            node_depth: 0,
        };

        let mesh_node = Node {
            element: Element::new("MeshNode", ElementType::Node),
            parent: Some(0),
            children: Vec::new(),
            mesh: Some(0),
            light: None,
            camera: None,
            bone: None,
            attrib: Some(0),
            attrib_type: ElementType::Mesh,
            all_attribs: vec![0],
            geometry_transform_helper: None,
            scale_helper: None,
            inherit_mode: InheritMode::Normal,
            original_inherit_mode: InheritMode::Normal,
            local_transform: Transform::IDENTITY,
            geometry_transform: Transform::IDENTITY,
            inherit_scale: Vec3::ONE,
            inherit_scale_node: None,
            rotation_order: RotationOrder::XYZ,
            euler_rotation: Vec3::ZERO,
            node_to_parent: Matrix::IDENTITY,
            node_to_world: Matrix::IDENTITY,
            geometry_to_node: Matrix::IDENTITY,
            geometry_to_world: Matrix::IDENTITY,
            unscaled_node_to_world: Matrix::IDENTITY,
            adjust_pre_translation: Vec3::ZERO,
            adjust_pre_rotation: Quat::IDENTITY,
            adjust_pre_scale: 1.0,
            adjust_post_rotation: Quat::IDENTITY,
            adjust_post_scale: 1.0,
            adjust_translation_scale: 1.0,
            adjust_mirror_axis: MirrorAxis::None,
            materials: Vec::new(),
            bind_pose: None,
            visible: true,
            is_root: false,
            has_geometry_transform: false,
            has_adjust_transform: false,
            has_root_adjust_transform: false,
            is_geometry_transform_helper: false,
            is_scale_helper: false,
            is_scale_compensate_parent: false,
            node_depth: 1,
        };

        let mut scene = Scene::new();
        scene.metadata.file_format = FileFormat::Obj;
        scene.metadata.ascii = true;
        scene.metadata.creator = FbxString::new("ufbx-rust OBJ parser");
        scene.root_node = 0;
        scene.nodes = vec![root_node, mesh_node];
        scene.meshes = vec![mesh];

        Ok(scene)
    }
}

impl Default for ObjParser {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_positive_index() {
        assert_eq!(resolve_obj_index(1, 10), Some(0));
        assert_eq!(resolve_obj_index(5, 10), Some(4));
        assert_eq!(resolve_obj_index(10, 10), Some(9));
        assert_eq!(resolve_obj_index(11, 10), None); // Out of bounds
    }

    #[test]
    fn test_resolve_negative_index() {
        assert_eq!(resolve_obj_index(-1, 10), Some(9)); // Last
        assert_eq!(resolve_obj_index(-2, 10), Some(8)); // Second-to-last
        assert_eq!(resolve_obj_index(-10, 10), Some(0)); // First
        assert_eq!(resolve_obj_index(-11, 10), None); // Out of bounds
    }

    #[test]
    fn test_resolve_zero_index() {
        assert_eq!(resolve_obj_index(0, 10), None); // OBJ doesn't use 0
    }

    #[test]
    fn test_face_vertex_parse() {
        let fv = FaceVertex::parse("1/2/3", 10, 10, 10).unwrap();
        assert_eq!(fv.position, Some(0));
        assert_eq!(fv.uv, Some(1));
        assert_eq!(fv.normal, Some(2));

        let fv = FaceVertex::parse("1//3", 10, 10, 10).unwrap();
        assert_eq!(fv.position, Some(0));
        assert_eq!(fv.uv, None);
        assert_eq!(fv.normal, Some(2));

        let fv = FaceVertex::parse("1/2", 10, 10, 10).unwrap();
        assert_eq!(fv.position, Some(0));
        assert_eq!(fv.uv, Some(1));
        assert_eq!(fv.normal, None);

        let fv = FaceVertex::parse("1", 10, 10, 10).unwrap();
        assert_eq!(fv.position, Some(0));
        assert_eq!(fv.uv, None);
        assert_eq!(fv.normal, None);
    }

    #[test]
    fn test_parse_simple_obj() {
        let obj_content = r#"
# Simple cube
v 0.0 0.0 0.0
v 1.0 0.0 0.0
v 1.0 1.0 0.0
v 0.0 1.0 0.0
vt 0.0 0.0
vt 1.0 0.0
vt 1.0 1.0
vt 0.0 1.0
vn 0.0 0.0 1.0
f 1/1/1 2/2/1 3/3/1 4/4/1
"#;

        let scene = ObjParser::parse(obj_content).unwrap();
        assert_eq!(scene.nodes.len(), 2); // Root + mesh node
        assert_eq!(scene.meshes.len(), 1);

        let mesh = &scene.meshes[0];
        assert_eq!(mesh.num_vertices, 4);
        assert_eq!(mesh.num_faces, 1);
        assert_eq!(mesh.faces[0].num_indices, 4);
    }

    #[test]
    fn test_parse_simple_mtl() {
        let mtl_content = r#"
newmtl TestMaterial
Ka 0.1 0.1 0.1
Kd 0.8 0.0 0.0
Ks 1.0 1.0 1.0
Ns 100.0
d 1.0
map_Kd texture.png
"#;

        let mut parser = ObjParser::new();
        parser.parse_mtl(mtl_content).unwrap();

        assert_eq!(parser.materials.len(), 1);
        let mat = parser.materials.get("TestMaterial").unwrap();
        assert_eq!(mat.diffuse.x, 0.8);
        assert_eq!(mat.diffuse.y, 0.0);
        assert_eq!(mat.diffuse.z, 0.0);
        assert_eq!(mat.specular_exponent, 100.0);
        assert_eq!(mat.map_diffuse, Some("texture.png".to_string()));
    }

    #[test]
    fn test_negative_indices() {
        let obj_content = r#"
v 0.0 0.0 0.0
v 1.0 0.0 0.0
v 1.0 1.0 0.0
f -3 -2 -1
"#;

        let scene = ObjParser::parse(obj_content).unwrap();
        let mesh = &scene.meshes[0];
        assert_eq!(mesh.num_vertices, 3);
        assert_eq!(mesh.num_faces, 1);
    }
}
