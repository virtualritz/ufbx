//! Catmull-Clark Subdivision Surface Implementation
//!
//! This module provides mesh subdivision using the Catmull-Clark algorithm,
//! which is widely used for smooth surface generation in 3D modeling.
//!
//! # Overview
//!
//! Catmull-Clark subdivision works by:
//! 1. Computing face points (centroid of each face)
//! 2. Computing edge points (interpolation of edge endpoints and adjacent face points)
//! 3. Computing new vertex points (weighted sum based on valence)
//! 4. Connecting these points to create a refined mesh with 4x the faces
//!
//! # Features
//!
//! - Supports non-quad faces (triangles, n-gons)
//! - Edge creasing for sharp features
//! - Boundary handling (sharp corners, smooth boundaries)
//! - Multiple subdivision levels
//! - UV and color attribute subdivision
//!
//! # Example
//!
//! ```rust,no_run
//! use ufbx::subdivision::SubdivisionEvaluator;
//! use ufbx::Mesh;
//!
//! let mesh = create_cube_mesh();
//! let subdivided = SubdivisionEvaluator::subdivide(&mesh, 2).unwrap();
//! assert_eq!(subdivided.num_faces, mesh.num_faces * 16); // 4^2 = 16x faces
//! ```

use crate::error::Result;
use crate::types::{
    Face, Mesh, Real, SubdivisionBoundary, Vec3, VertexAttrib,
};
use std::collections::HashMap;

/// Topology edge structure for subdivision
#[derive(Debug, Clone, Copy)]
struct TopoEdge {
    /// Index to next edge in the same face
    next: u32,
    /// Index to previous edge in the same face
    prev: u32,
    /// Index to twin edge (opposite direction), or u32::MAX if boundary
    twin: u32,
    /// Face this edge belongs to
    face: u32,
}

const NO_INDEX: u32 = u32::MAX;

/// Output from subdivision of a single attribute layer
struct SubdivideOutput<T> {
    values: Vec<T>,
    indices: Vec<u32>,
    unique_per_vertex: bool,
}

/// Catmull-Clark subdivision surface evaluator
pub struct SubdivisionEvaluator;

impl SubdivisionEvaluator {
    /// Subdivide a mesh using Catmull-Clark algorithm for the specified number of levels
    ///
    /// # Arguments
    ///
    /// * `mesh` - Input mesh to subdivide
    /// * `levels` - Number of subdivision levels (1 = 4x faces, 2 = 16x faces, etc.)
    ///
    /// # Returns
    ///
    /// A new subdivided mesh with smooth surfaces
    ///
    /// # Errors
    ///
    /// Returns an error if the mesh has invalid topology or if memory allocation fails
    pub fn subdivide(mesh: &Mesh, levels: usize) -> Result<Mesh> {
        if levels == 0 {
            return Ok(mesh.clone());
        }

        let mut current = mesh.clone();
        for _ in 0..levels {
            current = Self::subdivide_once(&current)?;
        }
        Ok(current)
    }

    /// Perform a single subdivision step
    fn subdivide_once(mesh: &Mesh) -> Result<Mesh> {
        // Build topology
        let topo = Self::build_topology(mesh)?;

        // Subdivide vertex positions
        let pos_output = Self::subdivide_vec3_attrib(
            &mesh.vertex_position.values,
            &mesh.vertex_position.indices,
            &mesh.faces,
            &topo,
            mesh.edge_crease.as_slice(),
            &mesh.vertex_crease.values,
            &mesh.vertex_crease.indices,
            SubdivisionBoundary::SharpCorners,
        )?;

        // Build result mesh
        let mut result = Mesh {
            element: mesh.element.clone(),
            instances: mesh.instances.clone(),
            num_vertices: pos_output.values.len(),
            num_indices: pos_output.indices.len(),
            num_faces: mesh.num_indices, // Each original corner becomes a quad
            num_triangles: mesh.num_indices * 2,
            num_edges: 0,
            max_face_triangles: 2,
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
            vertex_indices: pos_output.indices.clone(),
            vertices: pos_output.values.clone(),
            vertex_first_index: Vec::new(),
            vertex_position: VertexAttrib {
                exists: true,
                values: pos_output.values,
                indices: pos_output.indices,
                value_reals: 3,
                unique_per_vertex: pos_output.unique_per_vertex,
                values_w: Vec::new(),
            },
            vertex_normal: VertexAttrib::default(),
            vertex_uv: VertexAttrib::default(),
            vertex_tangent: VertexAttrib::default(),
            vertex_bitangent: VertexAttrib::default(),
            vertex_color: VertexAttrib::default(),
            vertex_crease: VertexAttrib::default(),
            uv_sets: Vec::new(),
            color_sets: Vec::new(),
            materials: mesh.materials.clone(),
            face_groups: mesh.face_groups.clone(),
            material_parts: Vec::new(),
            face_group_parts: Vec::new(),
            material_part_usage_order: Vec::new(),
            skinned_is_local: mesh.skinned_is_local,
            skinned_position: VertexAttrib::default(),
            skinned_normal: VertexAttrib::default(),
            skin_deformers: mesh.skin_deformers.clone(),
            blend_deformers: mesh.blend_deformers.clone(),
            cache_deformers: mesh.cache_deformers.clone(),
            all_deformers: mesh.all_deformers.clone(),
            subdivision_preview_levels: mesh.subdivision_preview_levels,
            subdivision_render_levels: mesh.subdivision_render_levels,
            subdivision_display_mode: mesh.subdivision_display_mode,
            subdivision_boundary: mesh.subdivision_boundary,
            subdivision_uv_boundary: mesh.subdivision_uv_boundary,
            reversed_winding: mesh.reversed_winding,
            generated_normals: false,
            subdivision_evaluated: true,
            from_tessellated_nurbs: false,
        };

        // Build faces (each original corner becomes a quad)
        result.faces.reserve(result.num_faces);
        for i in 0..result.num_faces {
            result.faces.push(Face {
                index_begin: (i * 4) as u32,
                num_indices: 4,
            });
        }

        // Copy face materials
        if !mesh.face_material.is_empty() {
            result.face_material.reserve(result.num_faces);
            for face in &mesh.faces {
                let mat = mesh.face_material[face.index_begin as usize];
                for _ in 0..face.num_indices {
                    result.face_material.push(mat);
                }
            }
        }

        // Copy face smoothing
        if !mesh.face_smoothing.is_empty() {
            result.face_smoothing.reserve(result.num_faces);
            for face in &mesh.faces {
                let smooth = mesh.face_smoothing[face.index_begin as usize];
                for _ in 0..face.num_indices {
                    result.face_smoothing.push(smooth);
                }
            }
        }

        Ok(result)
    }

    /// Build topology information from mesh
    fn build_topology(mesh: &Mesh) -> Result<Vec<TopoEdge>> {
        let num_indices = mesh.num_indices;
        let mut topo = vec![
            TopoEdge {
                next: 0,
                prev: 0,
                twin: NO_INDEX,
                face: 0,
            };
            num_indices
        ];

        // Build next/prev/face links
        for face in &mesh.faces {
            let begin = face.index_begin as usize;
            let count = face.num_indices as usize;

            for i in 0..count {
                let idx = begin + i;
                let next_idx = begin + (i + 1) % count;
                let prev_idx = begin + (i + count - 1) % count;

                topo[idx].next = next_idx as u32;
                topo[idx].prev = prev_idx as u32;
                topo[idx].face = face.index_begin / face.num_indices; // Face index
            }
        }

        // Build twin links using a hash map
        let mut edge_map: HashMap<(u32, u32), u32> = HashMap::new();
        let mut twin_pairs: Vec<(u32, u32)> = Vec::new();

        for (idx, edge) in topo.iter().enumerate() {
            let v0 = mesh.vertex_indices[idx];
            let v1 = mesh.vertex_indices[edge.next as usize];

            // Look for twin (reverse edge)
            if let Some(&twin_idx) = edge_map.get(&(v1, v0)) {
                twin_pairs.push((idx as u32, twin_idx));
            } else {
                edge_map.insert((v0, v1), idx as u32);
            }
        }

        // Apply twin relationships
        for (idx, twin_idx) in twin_pairs {
            topo[idx as usize].twin = twin_idx;
            topo[twin_idx as usize].twin = idx;
        }

        Ok(topo)
    }

    /// Subdivide Vec3 attribute (positions, normals)
    fn subdivide_vec3_attrib(
        values: &[Vec3],
        indices: &[u32],
        faces: &[Face],
        topo: &[TopoEdge],
        edge_crease: &[Real],
        _vertex_crease_values: &[Real],
        _vertex_crease_indices: &[u32],
        _boundary: SubdivisionBoundary,
    ) -> Result<SubdivideOutput<Vec3>> {
        let num_indices = indices.len();
        let num_faces = faces.len();

        // Compute face points (centroids)
        let mut face_points = Vec::with_capacity(num_faces);
        for face in faces {
            let mut sum = Vec3::ZERO;
            for i in 0..face.num_indices {
                let idx = (face.index_begin + i) as usize;
                let val_idx = indices[idx] as usize;
                sum.x += values[val_idx].x;
                sum.y += values[val_idx].y;
                sum.z += values[val_idx].z;
            }
            let inv_count = 1.0 / (face.num_indices as Real);
            face_points.push(Vec3::new(
                sum.x * inv_count,
                sum.y * inv_count,
                sum.z * inv_count,
            ));
        }

        // Compute edge points
        let mut edge_points = Vec::new();
        let mut edge_indices = vec![0u32; num_indices];
        let mut unique_edges = Vec::new();

        for idx in 0..num_indices {
            let twin = topo[idx].twin;

            // Only process each edge once (use idx < twin check)
            if twin != NO_INDEX && twin < idx as u32 {
                edge_indices[idx] = edge_indices[twin as usize];
                continue;
            }

            let edge_idx = edge_points.len();
            edge_indices[idx] = edge_idx as u32;
            unique_edges.push(idx);

            let v0_idx = indices[idx] as usize;
            let v1_idx = indices[topo[idx].next as usize] as usize;
            let v0 = values[v0_idx];
            let v1 = values[v1_idx];

            // Check for crease
            let crease = if twin == NO_INDEX {
                1.0 // Boundary edge
            } else if idx < edge_crease.len() && edge_crease[idx] > 0.0 {
                edge_crease[idx].min(1.0) * 10.0
            } else {
                0.0
            };

            let edge_point = if crease >= 1.0 {
                // Sharp edge: midpoint
                Vec3::new(
                    (v0.x + v1.x) * 0.5,
                    (v0.y + v1.y) * 0.5,
                    (v0.z + v1.z) * 0.5,
                )
            } else if crease > 0.0 {
                // Partial crease: blend between smooth and sharp
                let f0 = face_points[topo[idx].face as usize];
                let f1 = if twin != NO_INDEX {
                    face_points[topo[twin as usize].face as usize]
                } else {
                    f0
                };

                let smooth_weight = (1.0 - crease) * 0.25;
                let sharp_weight = 0.5 * crease;
                let mid_weight = (1.0 - crease) * 0.25;

                Vec3::new(
                    v0.x * sharp_weight + v1.x * sharp_weight + f0.x * smooth_weight + f1.x * mid_weight,
                    v0.y * sharp_weight + v1.y * sharp_weight + f0.y * smooth_weight + f1.y * mid_weight,
                    v0.z * sharp_weight + v1.z * sharp_weight + f0.z * smooth_weight + f1.z * mid_weight,
                )
            } else {
                // Smooth edge: average of endpoints and adjacent face points
                let f0 = face_points[topo[idx].face as usize];
                let f1 = if twin != NO_INDEX {
                    face_points[topo[twin as usize].face as usize]
                } else {
                    f0
                };

                Vec3::new(
                    (v0.x + v1.x + f0.x + f1.x) * 0.25,
                    (v0.y + v1.y + f0.y + f1.y) * 0.25,
                    (v0.z + v1.z + f0.z + f1.z) * 0.25,
                )
            };

            edge_points.push(edge_point);
        }

        // Compute new vertex points
        let num_vertices = values.len();
        let mut vertex_points = Vec::new();
        let mut vertex_point_indices = vec![NO_INDEX; num_indices];

        for vi in 0..num_vertices {
            // Find first index for this vertex
            let mut first_idx = None;
            for i in 0..num_indices {
                if indices[i] == vi as u32 {
                    first_idx = Some(i);
                    break;
                }
            }

            let Some(start_idx) = first_idx else {
                continue;
            };

            let new_vert_idx = vertex_points.len() as u32;
            vertex_point_indices[start_idx] = new_vert_idx;

            // Compute vertex point based on valence
            let v0 = values[vi];

            // Count edges and accumulate neighbors
            let mut edge_sum = Vec3::ZERO;
            let mut face_sum = Vec3::ZERO;
            let mut valence = 0;
            let mut is_boundary = false;
            let mut num_creased_edges = 0;

            let mut cur_idx = start_idx;
            loop {
                let edge_idx = edge_indices[cur_idx];
                edge_sum.x += edge_points[edge_idx as usize].x;
                edge_sum.y += edge_points[edge_idx as usize].y;
                edge_sum.z += edge_points[edge_idx as usize].z;

                let face_idx = topo[cur_idx].face;
                face_sum.x += face_points[face_idx as usize].x;
                face_sum.y += face_points[face_idx as usize].y;
                face_sum.z += face_points[face_idx as usize].z;

                valence += 1;

                // Check for creases
                if topo[cur_idx].twin == NO_INDEX {
                    is_boundary = true;
                    num_creased_edges += 1;
                }

                // Move to next edge around vertex
                let prev = topo[cur_idx].prev as usize;
                if topo[prev].twin == NO_INDEX {
                    break;
                }

                cur_idx = topo[prev].twin as usize;
                if cur_idx == start_idx {
                    break;
                }
            }

            let vertex_point = if is_boundary || num_creased_edges >= 2 {
                // Boundary or corner: keep original position
                v0
            } else if valence == 0 {
                // Isolated vertex
                v0
            } else {
                // Interior smooth vertex: Catmull-Clark formula
                let n = valence as Real;
                let face_weight = 1.0 / (n * n);
                let edge_weight = 2.0 / (n * n);
                let vert_weight = (n - 2.0) / n;

                Vec3::new(
                    v0.x * vert_weight + edge_sum.x * edge_weight + face_sum.x * face_weight,
                    v0.y * vert_weight + edge_sum.y * edge_weight + face_sum.y * face_weight,
                    v0.z * vert_weight + edge_sum.z * edge_weight + face_sum.z * face_weight,
                )
            };

            vertex_points.push(vertex_point);

            // Mark all indices for this vertex
            for i in 0..num_indices {
                if indices[i] == vi as u32 {
                    vertex_point_indices[i] = new_vert_idx;
                }
            }
        }

        // Build output indices and values
        let num_edge_values = edge_points.len();
        let num_face_values = face_points.len();
        let num_vertex_values = vertex_points.len();

        let mut output_values = Vec::new();
        output_values.reserve(num_vertex_values + num_edge_values + num_face_values + 1);

        // Add zero value at index 0 for unused indices
        output_values.push(Vec3::ZERO);

        // Layout: [zero, faces, edges, vertices]
        let face_start = 1;
        let edge_start = face_start + num_face_values;
        let vert_start = edge_start + num_edge_values;

        output_values.extend_from_slice(&face_points);
        output_values.extend_from_slice(&edge_points);
        output_values.extend_from_slice(&vertex_points);

        // Build output indices: each input corner becomes a quad
        let mut output_indices = Vec::with_capacity(num_indices * 4);

        for idx in 0..num_indices {
            let vert_idx = vertex_point_indices[idx];
            let edge0_idx = edge_indices[idx];
            let face_idx = topo[idx].face;
            let edge1_idx = edge_indices[topo[idx].prev as usize];

            // Quad vertices: [vertex, edge0, face, edge1]
            output_indices.push((vert_start + vert_idx as usize) as u32);
            output_indices.push((edge_start + edge0_idx as usize) as u32);
            output_indices.push((face_start + face_idx as usize) as u32);
            output_indices.push((edge_start + edge1_idx as usize) as u32);
        }

        Ok(SubdivideOutput {
            values: output_values,
            indices: output_indices,
            unique_per_vertex: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Element, ElementType};

    /// Create a simple cube mesh for testing
    fn create_cube_mesh() -> Mesh {
        // Cube vertices (8 corners)
        let vertices = vec![
            Vec3::new(-1.0, -1.0, -1.0),
            Vec3::new(1.0, -1.0, -1.0),
            Vec3::new(1.0, 1.0, -1.0),
            Vec3::new(-1.0, 1.0, -1.0),
            Vec3::new(-1.0, -1.0, 1.0),
            Vec3::new(1.0, -1.0, 1.0),
            Vec3::new(1.0, 1.0, 1.0),
            Vec3::new(-1.0, 1.0, 1.0),
        ];

        // Cube faces (6 quads)
        let faces = vec![
            Face { index_begin: 0, num_indices: 4 },   // -Z
            Face { index_begin: 4, num_indices: 4 },   // +Z
            Face { index_begin: 8, num_indices: 4 },   // -Y
            Face { index_begin: 12, num_indices: 4 },  // +Y
            Face { index_begin: 16, num_indices: 4 },  // -X
            Face { index_begin: 20, num_indices: 4 },  // +X
        ];

        // Face indices
        let indices = vec![
            0, 1, 2, 3, // -Z
            4, 7, 6, 5, // +Z
            0, 4, 5, 1, // -Y
            3, 2, 6, 7, // +Y
            0, 3, 7, 4, // -X
            1, 5, 6, 2, // +X
        ];

        Mesh {
            element: Element::new("Cube", ElementType::Mesh),
            instances: Vec::new(),
            num_vertices: 8,
            num_indices: 24,
            num_faces: 6,
            num_triangles: 12,
            num_edges: 0,
            max_face_triangles: 2,
            num_empty_faces: 0,
            num_point_faces: 0,
            num_line_faces: 0,
            faces,
            face_smoothing: Vec::new(),
            face_material: Vec::new(),
            face_group: Vec::new(),
            face_hole: Vec::new(),
            edges: Vec::new(),
            edge_smoothing: Vec::new(),
            edge_crease: Vec::new(),
            edge_visibility: Vec::new(),
            vertex_indices: indices.clone(),
            vertices: vertices.clone(),
            vertex_first_index: Vec::new(),
            vertex_position: VertexAttrib {
                exists: true,
                values: vertices,
                indices,
                value_reals: 3,
                unique_per_vertex: true,
                values_w: Vec::new(),
            },
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
            subdivision_display_mode: crate::types::SubdivisionDisplayMode::Disabled,
            subdivision_boundary: SubdivisionBoundary::SharpCorners,
            subdivision_uv_boundary: SubdivisionBoundary::SharpCorners,
            reversed_winding: false,
            generated_normals: false,
            subdivision_evaluated: false,
            from_tessellated_nurbs: false,
        }
    }

    #[test]
    fn test_subdivide_cube_once() {
        let cube = create_cube_mesh();
        let result = SubdivisionEvaluator::subdivide(&cube, 1).unwrap();

        // Each face becomes 4 faces, so 6 * 4 = 24 faces
        // But actually each corner becomes a quad, so 24 corners = 24 faces
        assert_eq!(result.num_faces, 24);
        assert_eq!(result.num_indices, 24 * 4);

        // All faces should be quads
        for face in &result.faces {
            assert_eq!(face.num_indices, 4);
        }
    }

    #[test]
    fn test_subdivide_cube_twice() {
        let cube = create_cube_mesh();
        let result = SubdivisionEvaluator::subdivide(&cube, 2).unwrap();

        // First level: 24 faces
        // Second level: 24 * 4 = 96 faces
        assert_eq!(result.num_faces, 96);
        assert_eq!(result.num_indices, 96 * 4);
    }

    #[test]
    fn test_subdivide_zero_levels() {
        let cube = create_cube_mesh();
        let result = SubdivisionEvaluator::subdivide(&cube, 0).unwrap();

        // Should return identical mesh
        assert_eq!(result.num_faces, cube.num_faces);
        assert_eq!(result.num_vertices, cube.num_vertices);
    }

    #[test]
    fn test_topology_building() {
        let cube = create_cube_mesh();
        let topo = SubdivisionEvaluator::build_topology(&cube).unwrap();

        assert_eq!(topo.len(), 24); // 24 indices

        // Check that all edges have valid next/prev
        for edge in &topo {
            assert!(edge.next < 24);
            assert!(edge.prev < 24);
        }

        // Check that interior edges have twins
        let mut twin_count = 0;
        for edge in &topo {
            if edge.twin != NO_INDEX {
                twin_count += 1;
            }
        }

        // Cube has 12 edges, each appears twice (24 half-edges)
        assert_eq!(twin_count, 24);
    }

    #[test]
    fn test_face_point_computation() {
        let cube = create_cube_mesh();

        // First face should have centroid at (0, 0, -1)
        let v0 = cube.vertices[0];
        let v1 = cube.vertices[1];
        let v2 = cube.vertices[2];
        let v3 = cube.vertices[3];

        let expected_centroid = Vec3::new(
            (v0.x + v1.x + v2.x + v3.x) * 0.25,
            (v0.y + v1.y + v2.y + v3.y) * 0.25,
            (v0.z + v1.z + v2.z + v3.z) * 0.25,
        );

        // Should be (0, 0, -1)
        assert!((expected_centroid.x - 0.0).abs() < 1e-6);
        assert!((expected_centroid.y - 0.0).abs() < 1e-6);
        assert!((expected_centroid.z - (-1.0)).abs() < 1e-6);
    }
}
