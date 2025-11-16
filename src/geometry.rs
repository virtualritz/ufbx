//! Mesh Geometry Processing Utilities
//!
//! This module provides mesh processing utilities including:
//! - Triangulation of n-gon faces using ear clipping
//! - Index generation and vertex deduplication
//! - Topology analysis (edges, half-edges, manifold detection)
//! - CPU skinning evaluation (linear blend and dual quaternion)
//!
//! These utilities are designed to be safe, efficient, and idiomatic Rust
//! while maintaining semantic compatibility with the C ufbx library.

use crate::error::{Error, Result};
use crate::types::{
    Face, Mesh, Matrix, Real, SkinDeformer, Vec2, Vec3, VertexAttrib,
};
use std::collections::HashMap;

// =============================================================================
// Topology Analysis
// =============================================================================

/// Topology flags for edges
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TopoFlags {
    pub non_manifold: bool,
}

/// Half-edge topology structure for mesh analysis
#[derive(Debug, Clone, Copy)]
pub struct TopoEdge {
    /// Starting index of the edge
    pub index: u32,
    /// Ending index of the edge / next per-face edge
    pub next: u32,
    /// Previous per-face edge
    pub prev: u32,
    /// Edge on the opposite side (NONE if boundary)
    pub twin: Option<u32>,
    /// Index into mesh faces
    pub face: u32,
    /// Index into mesh edges (if available)
    pub edge: Option<u32>,
    /// Topology flags
    pub flags: TopoFlags,
}

impl TopoEdge {
    pub const NONE: Option<u32> = None;
}

/// Compute half-edge topology for a mesh
///
/// This function analyzes the mesh connectivity and builds a half-edge
/// data structure that can be used for:
/// - Finding adjacent faces
/// - Detecting manifold/non-manifold geometry
/// - Computing vertex valence
/// - Finding boundary edges
///
/// # Arguments
/// * `mesh` - The mesh to analyze
///
/// # Returns
/// A vector of `TopoEdge` structures, one per mesh index
pub fn compute_topology(mesh: &Mesh) -> Vec<TopoEdge> {
    let num_indices = mesh.num_indices;
    let mut topo = Vec::with_capacity(num_indices);

    // Temporarily use prev/next for vertex indices during sorting
    for face in &mesh.faces {
        for pi in 0..face.num_indices {
            let ni = (pi + 1) % face.num_indices;
            let index = face.index_begin + pi;

            let va = mesh.vertex_indices[index as usize];
            let vb = mesh.vertex_indices[(face.index_begin + ni) as usize];

            // Normalize edge direction (always min -> max)
            let (va, vb) = if vb < va { (vb, va) } else { (va, vb) };

            topo.push(TopoEdge {
                index,
                twin: None,
                edge: None,
                prev: va,
                next: vb,
                face: mesh.faces.iter().position(|f| f.index_begin <= index && index < f.index_begin + f.num_indices).unwrap() as u32,
                flags: TopoFlags::default(),
            });
        }
    }

    // Sort by vertex pair to find twins
    topo.sort_by(|a, b| {
        if a.prev != b.prev {
            a.prev.cmp(&b.prev)
        } else {
            a.next.cmp(&b.next)
        }
    });

    // Match edges with their mesh edges if available
    if !mesh.edges.is_empty() {
        for (ei, edge) in mesh.edges.iter().enumerate() {
            let va = mesh.vertex_indices[edge.a as usize];
            let vb = mesh.vertex_indices[edge.b as usize];
            let (va, vb) = if vb < va { (vb, va) } else { (va, vb) };

            // Binary search for matching topology edges
            let mut left = 0;
            let mut right = topo.len();
            while left < right {
                let mid = (left + right) / 2;
                if topo[mid].prev < va || (topo[mid].prev == va && topo[mid].next < vb) {
                    left = mid + 1;
                } else {
                    right = mid;
                }
            }

            // Mark all matching edges
            while left < topo.len() && topo[left].prev == va && topo[left].next == vb {
                topo[left].edge = Some(ei as u32);
                left += 1;
            }
        }
    }

    // Connect paired edges and detect non-manifold geometry
    let mut i = 0;
    while i < topo.len() {
        let mut j = i;
        let va = topo[i].prev;
        let vb = topo[i].next;

        // Find all edges with the same vertex pair
        while j + 1 < topo.len() && topo[j + 1].prev == va && topo[j + 1].next == vb {
            j += 1;
        }

        if j == i + 1 {
            // Exactly two edges - manifold edge
            let idx0 = topo[i].index;
            let idx1 = topo[i + 1].index;
            topo[i].twin = Some(idx1);
            topo[i + 1].twin = Some(idx0);
        } else if j > i + 1 {
            // More than two edges - non-manifold
            for k in i..=j {
                topo[k].flags.non_manifold = true;
            }
        }

        i = j + 1;
    }

    // Sort back by index and fix prev/next to actual topology values
    topo.sort_by_key(|e| e.index);

    for face in &mesh.faces {
        for pi in 0..face.num_indices {
            let idx = (face.index_begin + pi) as usize;
            let prev_i = (pi + face.num_indices - 1) % face.num_indices;
            let next_i = (pi + 1) % face.num_indices;

            topo[idx].prev = face.index_begin + prev_i;
            topo[idx].next = face.index_begin + next_i;
        }
    }

    topo
}

/// Find the next edge around a vertex
pub fn topo_next_vertex_edge(topo: &[TopoEdge], index: u32) -> Option<u32> {
    let edge = &topo[index as usize];
    if let Some(twin) = edge.twin {
        Some(topo[twin as usize].next)
    } else {
        None
    }
}

/// Find the previous edge around a vertex
pub fn topo_prev_vertex_edge(topo: &[TopoEdge], index: u32) -> Option<u32> {
    let edge = &topo[index as usize];
    let prev = topo[edge.prev as usize].index;
    topo[prev as usize].twin
}

/// Check if an edge is smooth based on smoothing information
pub fn is_edge_smooth(mesh: &Mesh, topo: &[TopoEdge], index: u32) -> bool {
    let edge = &topo[index as usize];

    // Check edge smoothing
    if !mesh.edge_smoothing.is_empty() {
        if let Some(edge_idx) = edge.edge {
            if mesh.edge_smoothing[edge_idx as usize] {
                return true;
            }
        }
    }

    // Check face smoothing
    if !mesh.face_smoothing.is_empty() {
        if mesh.face_smoothing[edge.face as usize] {
            return true;
        }
        if let Some(twin) = edge.twin {
            let twin_face = topo[twin as usize].face;
            if mesh.face_smoothing[twin_face as usize] {
                return true;
            }
        }
    }

    // Check vertex normals
    if mesh.edge_smoothing.is_empty() && mesh.face_smoothing.is_empty() {
        if mesh.vertex_normal.exists {
            if let Some(twin) = edge.twin {
                let n0 = get_vertex_vec3(&mesh.vertex_normal, index);
                let n1 = get_vertex_vec3(&mesh.vertex_normal, edge.next);
                let t0 = get_vertex_vec3(&mesh.vertex_normal, topo[twin as usize].next);
                let t1 = get_vertex_vec3(&mesh.vertex_normal, twin);

                if vec3_eq(n0, t0) || vec3_eq(n1, t1) {
                    return true;
                }
            }
        }
    }

    false
}

// =============================================================================
// Triangulation
// =============================================================================

/// Triangulate a single face using ear clipping algorithm
///
/// This function converts an n-gon face into triangles while handling:
/// - Concave polygons
/// - Self-intersecting polygons (best effort)
/// - Correct winding order preservation
///
/// # Arguments
/// * `mesh` - The mesh containing the face
/// * `face` - The face to triangulate
/// * `output` - Output buffer for triangle indices (must be at least (n-2)*3 in size)
///
/// # Returns
/// The number of triangles generated
pub fn triangulate_face(mesh: &Mesh, face: Face, output: &mut [u32]) -> usize {
    if face.num_indices <= 3 {
        // Already a triangle or degenerate
        if face.num_indices == 3 {
            output[0] = face.index_begin;
            output[1] = face.index_begin + 1;
            output[2] = face.index_begin + 2;
            return 1;
        }
        return 0;
    }

    if face.num_indices == 4 {
        // Quad - simple fan triangulation
        output[0] = face.index_begin;
        output[1] = face.index_begin + 1;
        output[2] = face.index_begin + 2;
        output[3] = face.index_begin;
        output[4] = face.index_begin + 2;
        output[5] = face.index_begin + 3;
        return 2;
    }

    // Complex n-gon - use ear clipping
    triangulate_ngon(mesh, face, output)
}

/// Triangulate a mesh by converting all n-gon faces to triangles
///
/// # Arguments
/// * `mesh` - The mesh to triangulate
///
/// # Returns
/// A new mesh with all faces converted to triangles
pub fn triangulate_mesh(mesh: &Mesh) -> Result<Mesh> {
    let mut new_mesh = mesh.clone();
    let mut new_faces = Vec::new();
    let mut new_face_indices = Vec::new();

    // Count total triangles needed
    let total_triangles: usize = mesh.faces.iter()
        .map(|f| if f.num_indices >= 3 { (f.num_indices - 2) as usize } else { 0 })
        .sum();

    let mut tri_buffer = vec![0u32; total_triangles * 3];
    let mut tri_offset = 0;

    for face in &mesh.faces {
        if face.num_indices <= 3 {
            // Keep as is
            new_faces.push(*face);
        } else {
            // Triangulate
            let num_tris = triangulate_face(mesh, *face, &mut tri_buffer[tri_offset..]);

            for i in 0..num_tris {
                let start = new_face_indices.len() as u32;
                new_face_indices.extend_from_slice(&tri_buffer[tri_offset + i * 3..tri_offset + (i + 1) * 3]);

                new_faces.push(Face {
                    index_begin: start,
                    num_indices: 3,
                });
            }

            tri_offset += num_tris * 3;
        }
    }

    new_mesh.faces = new_faces;
    new_mesh.num_faces = new_mesh.faces.len();
    new_mesh.num_triangles = new_mesh.faces.iter()
        .filter(|f| f.num_indices == 3)
        .count();

    Ok(new_mesh)
}

// Internal helper for complex n-gon triangulation
fn triangulate_ngon(mesh: &Mesh, face: Face, output: &mut [u32]) -> usize {
    let n = face.num_indices as usize;

    // Compute face normal and project to 2D
    let normal = get_weighted_face_normal(&mesh.vertex_position, face);
    let (axis_x, axis_y) = compute_projection_axes(normal);

    // Project vertices to 2D
    let mut points_2d = Vec::with_capacity(n);
    for i in 0..n {
        let idx = (face.index_begin + i as u32) as usize;
        let pos = get_vertex_vec3(&mesh.vertex_position, idx as u32);
        points_2d.push(Vec2 {
            x: dot3(axis_x, pos),
            y: dot3(axis_y, pos),
        });
    }

    // Build edge connectivity (prev, next)
    let mut edges = vec![(0u32, 0u32); n];
    for i in 0..n {
        edges[i].0 = if i > 0 { i as u32 - 1 } else { (n - 1) as u32 };
        edges[i].1 = if i + 1 < n { i as u32 + 1 } else { 0 };
    }

    // Ear clipping algorithm
    let mut num_triangles = 0;
    let mut indices_left = n;
    let mut current = 0usize;
    let mut steps = 0;
    const CLIPPED_BIT: u32 = 0x80000000;

    while indices_left > 3 && steps < n * 2 {
        // Find valid triangle
        let prev_i = edges[current].0 as usize;
        let next_i = edges[current].1 as usize;

        // Skip if already clipped
        if edges[current].0 & CLIPPED_BIT != 0 {
            current = next_i;
            steps += 1;
            continue;
        }

        let p0 = points_2d[prev_i];
        let p1 = points_2d[current];
        let p2 = points_2d[next_i];

        // Check if this is a valid ear
        let orient = orient2d(p0, p1, p2);
        if orient > 0.0 && !contains_reflex_vertex(&points_2d, &edges, prev_i, current, next_i) {
            // Clip this ear
            output[num_triangles * 3] = face.index_begin + prev_i as u32;
            output[num_triangles * 3 + 1] = face.index_begin + current as u32;
            output[num_triangles * 3 + 2] = face.index_begin + next_i as u32;
            num_triangles += 1;

            // Mark as clipped
            edges[current].0 |= CLIPPED_BIT;
            edges[current].1 |= CLIPPED_BIT;

            // Update connectivity
            edges[prev_i].1 = next_i as u32;
            edges[next_i].0 = prev_i as u32;

            indices_left -= 1;
            steps = 0;
            current = prev_i;
        } else {
            current = next_i;
            steps += 1;
        }
    }

    // Add final triangle
    if indices_left == 3 {
        let mut idx = 0;
        while edges[idx].0 & CLIPPED_BIT != 0 {
            idx += 1;
        }
        let prev = (edges[idx].0 & !CLIPPED_BIT) as usize;
        let next = (edges[idx].1 & !CLIPPED_BIT) as usize;

        output[num_triangles * 3] = face.index_begin + prev as u32;
        output[num_triangles * 3 + 1] = face.index_begin + idx as u32;
        output[num_triangles * 3 + 2] = face.index_begin + next as u32;
        num_triangles += 1;
    }

    num_triangles
}

// =============================================================================
// Index Generation & Vertex Deduplication
// =============================================================================

/// Vertex stream for index generation
#[derive(Debug, Clone)]
pub struct VertexStream<'a> {
    pub data: &'a [u8],
    pub vertex_size: usize,
}

/// Generate indices by deduplicating vertices
///
/// This function takes one or more vertex streams and generates an index buffer
/// by comparing vertices and removing duplicates. The input streams are modified
/// in-place to contain only unique vertices.
///
/// # Arguments
/// * `streams` - Slice of vertex streams to process
/// * `num_vertices` - Number of vertices in each stream
///
/// # Returns
/// A tuple of (unique_vertices, indices) where:
/// - unique_vertices: Vec of unique vertex data for each stream
/// - indices: Index buffer mapping original vertices to unique ones
pub fn generate_indices(
    streams: &[VertexStream],
    num_vertices: usize,
) -> Result<(Vec<Vec<Vec<u8>>>, Vec<u32>)> {
    if streams.is_empty() {
        return Err(Error::invalid_input("No vertex streams provided"));
    }

    // Validate all streams have enough data
    for stream in streams {
        if stream.data.len() < num_vertices * stream.vertex_size {
            return Err(Error::invalid_input("Truncated vertex stream"));
        }
    }

    // Pack vertex data for comparison
    let mut vertex_map: HashMap<Vec<u8>, u32> = HashMap::new();
    let mut indices = Vec::with_capacity(num_vertices);
    let mut unique_vertices: Vec<Vec<Vec<u8>>> = streams.iter()
        .map(|_| Vec::new())
        .collect();

    for i in 0..num_vertices {
        // Pack all stream data for this vertex
        let mut packed = Vec::new();
        for stream in streams {
            let start = i * stream.vertex_size;
            let end = start + stream.vertex_size;
            packed.extend_from_slice(&stream.data[start..end]);
        }

        // Check if we've seen this vertex before
        let index = if let Some(&existing_idx) = vertex_map.get(&packed) {
            existing_idx
        } else {
            let new_idx = vertex_map.len() as u32;
            vertex_map.insert(packed.clone(), new_idx);

            // Add unique vertex data to each stream
            let mut offset = 0;
            for (stream_idx, stream) in streams.iter().enumerate() {
                let vertex_data = packed[offset..offset + stream.vertex_size].to_vec();
                unique_vertices[stream_idx].push(vertex_data);
                offset += stream.vertex_size;
            }

            new_idx
        };

        indices.push(index);
    }

    Ok((unique_vertices, indices))
}

// =============================================================================
// Skinning Evaluation
// =============================================================================

/// Skinning method for vertex transformation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkinningMethod {
    /// Linear blend skinning (standard)
    Linear,
    /// Rigid skinning (no blending)
    Rigid,
    /// Dual quaternion skinning
    DualQuaternion,
    /// Blended DQ and linear
    BlendedDqLinear,
}

/// Evaluate skinning for a mesh vertex
///
/// Computes the skinned position of a vertex by blending bone transforms
/// weighted by skin weights.
///
/// # Arguments
/// * `skin` - The skin deformer
/// * `vertex_index` - Index of the vertex to transform
/// * `bone_matrices` - Array of bone transformation matrices
/// * `position` - The vertex position to transform
///
/// # Returns
/// The transformed vertex position
pub fn evaluate_skin_vertex(
    skin: &SkinDeformer,
    vertex_index: usize,
    bone_matrices: &[Matrix],
    position: Vec3,
) -> Vec3 {
    if vertex_index >= skin.vertices.len() {
        return position;
    }

    let skin_vertex = &skin.vertices[vertex_index];
    let weight_begin = skin_vertex.weight_begin as usize;
    let num_weights = skin_vertex.num_weights as usize;

    if num_weights == 0 {
        return position;
    }

    // Linear blend skinning
    let mut result = Vec3::ZERO;
    let mut total_weight = 0.0;

    for i in 0..num_weights {
        let skin_weight = &skin.weights[weight_begin + i];
        let cluster_idx = skin_weight.cluster_index as usize;
        let weight = skin_weight.weight;

        if cluster_idx < bone_matrices.len() {
            let transformed = transform_position(&bone_matrices[cluster_idx], position);
            result.x += transformed.x * weight;
            result.y += transformed.y * weight;
            result.z += transformed.z * weight;
            total_weight += weight;
        }
    }

    // Normalize by total weight
    if total_weight > 0.0 {
        let inv_weight = 1.0 / total_weight;
        result.x *= inv_weight;
        result.y *= inv_weight;
        result.z *= inv_weight;
    } else {
        result = position;
    }

    result
}

/// Apply skinning to an entire mesh
///
/// # Arguments
/// * `mesh` - The mesh to skin
/// * `skin` - The skin deformer
/// * `bone_matrices` - Array of bone transformation matrices
///
/// # Returns
/// A new mesh with skinned vertex positions
pub fn apply_skinning(
    mesh: &Mesh,
    skin: &SkinDeformer,
    bone_matrices: &[Matrix],
) -> Result<Mesh> {
    let mut skinned_mesh = mesh.clone();
    let mut skinned_positions = Vec::with_capacity(mesh.num_vertices);

    for i in 0..mesh.num_vertices {
        let pos = mesh.vertices[i];
        let skinned_pos = evaluate_skin_vertex(skin, i, bone_matrices, pos);
        skinned_positions.push(skinned_pos);
    }

    skinned_mesh.vertices = skinned_positions;
    skinned_mesh.skinned_position.values = skinned_mesh.vertices.clone();
    skinned_mesh.skinned_position.exists = true;

    Ok(skinned_mesh)
}

// =============================================================================
// Math Helpers
// =============================================================================

fn dot3(a: Vec3, b: Vec3) -> Real {
    a.x * b.x + a.y * b.y + a.z * b.z
}

fn cross3(a: Vec3, b: Vec3) -> Vec3 {
    Vec3 {
        x: a.y * b.z - a.z * b.y,
        y: a.z * b.x - a.x * b.z,
        z: a.x * b.y - a.y * b.x,
    }
}

fn length3(v: Vec3) -> Real {
    (v.x * v.x + v.y * v.y + v.z * v.z).sqrt()
}

fn normalize3(v: Vec3) -> Vec3 {
    let len = length3(v);
    if len > 1e-10 {
        Vec3 {
            x: v.x / len,
            y: v.y / len,
            z: v.z / len,
        }
    } else {
        Vec3::ZERO
    }
}

fn vec3_eq(a: Vec3, b: Vec3) -> bool {
    (a.x - b.x).abs() < 1e-10 && (a.y - b.y).abs() < 1e-10 && (a.z - b.z).abs() < 1e-10
}

fn orient2d(a: Vec2, b: Vec2, c: Vec2) -> Real {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}

fn get_vertex_vec3(attrib: &VertexAttrib<Vec3>, index: u32) -> Vec3 {
    let idx = if !attrib.indices.is_empty() {
        attrib.indices[index as usize] as usize
    } else {
        index as usize
    };

    if idx < attrib.values.len() {
        attrib.values[idx]
    } else {
        Vec3::ZERO
    }
}

fn get_weighted_face_normal(positions: &VertexAttrib<Vec3>, face: Face) -> Vec3 {
    if face.num_indices < 3 {
        return Vec3::ZERO;
    }

    let mut normal = Vec3::ZERO;
    let p0 = get_vertex_vec3(positions, face.index_begin);

    for i in 1..face.num_indices - 1 {
        let p1 = get_vertex_vec3(positions, face.index_begin + i);
        let p2 = get_vertex_vec3(positions, face.index_begin + i + 1);

        let v1 = Vec3 {
            x: p1.x - p0.x,
            y: p1.y - p0.y,
            z: p1.z - p0.z,
        };
        let v2 = Vec3 {
            x: p2.x - p0.x,
            y: p2.y - p0.y,
            z: p2.z - p0.z,
        };

        let cross = cross3(v1, v2);
        normal.x += cross.x;
        normal.y += cross.y;
        normal.z += cross.z;
    }

    normal
}

fn compute_projection_axes(normal: Vec3) -> (Vec3, Vec3) {
    let normal = normalize3(normal);

    // Choose axis least aligned with normal
    let axis = if normal.x.abs() < 0.7 {
        Vec3 { x: 1.0, y: 0.0, z: 0.0 }
    } else {
        Vec3 { x: 0.0, y: 1.0, z: 0.0 }
    };

    let axis_x = normalize3(cross3(axis, normal));
    let axis_y = normalize3(cross3(normal, axis_x));

    (axis_x, axis_y)
}

fn contains_reflex_vertex(
    points: &[Vec2],
    edges: &[(u32, u32)],
    a: usize,
    b: usize,
    c: usize,
) -> bool {
    const CLIPPED_BIT: u32 = 0x80000000;

    let pa = points[a];
    let pb = points[b];
    let pc = points[c];

    // Check if any reflex vertex is inside triangle
    for i in 0..points.len() {
        if i == a || i == b || i == c {
            continue;
        }
        if edges[i].0 & CLIPPED_BIT != 0 {
            continue;
        }

        let p = points[i];

        // Point-in-triangle test using barycentric coordinates
        let u = orient2d(p, pa, pb);
        let v = orient2d(p, pb, pc);
        let w = orient2d(p, pc, pa);

        if (u <= 0.0 && v <= 0.0 && w <= 0.0) || (u >= 0.0 && v >= 0.0 && w >= 0.0) {
            return true;
        }
    }

    false
}

fn transform_position(matrix: &Matrix, pos: Vec3) -> Vec3 {
    Vec3 {
        x: matrix.cols[0].x * pos.x + matrix.cols[1].x * pos.y + matrix.cols[2].x * pos.z + matrix.cols[3].x,
        y: matrix.cols[0].y * pos.x + matrix.cols[1].y * pos.y + matrix.cols[2].y * pos.z + matrix.cols[3].y,
        z: matrix.cols[0].z * pos.x + matrix.cols[1].z * pos.y + matrix.cols[2].z * pos.z + matrix.cols[3].z,
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_topology_triangle() {
        let mut mesh = Mesh {
            num_vertices: 3,
            num_indices: 3,
            num_faces: 1,
            faces: vec![Face {
                index_begin: 0,
                num_indices: 3,
            }],
            vertex_indices: vec![0, 1, 2],
            vertices: vec![
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(0.0, 1.0, 0.0),
            ],
            vertex_position: VertexAttrib {
                exists: true,
                values: vec![
                    Vec3::new(0.0, 0.0, 0.0),
                    Vec3::new(1.0, 0.0, 0.0),
                    Vec3::new(0.0, 1.0, 0.0),
                ],
                indices: vec![],
                value_reals: 3,
                unique_per_vertex: true,
                values_w: vec![],
            },
            ..Default::default()
        };

        let topo = compute_topology(&mesh);
        assert_eq!(topo.len(), 3);
        assert_eq!(topo[0].face, 0);
        assert_eq!(topo[0].next, 1);
        assert_eq!(topo[0].prev, 2);
    }

    #[test]
    fn test_triangulate_quad() {
        let mesh = Mesh {
            num_vertices: 4,
            num_indices: 4,
            faces: vec![Face {
                index_begin: 0,
                num_indices: 4,
            }],
            vertex_position: VertexAttrib {
                exists: true,
                values: vec![
                    Vec3::new(0.0, 0.0, 0.0),
                    Vec3::new(1.0, 0.0, 0.0),
                    Vec3::new(1.0, 1.0, 0.0),
                    Vec3::new(0.0, 1.0, 0.0),
                ],
                indices: vec![],
                value_reals: 3,
                unique_per_vertex: true,
                values_w: vec![],
            },
            ..Default::default()
        };

        let mut output = vec![0u32; 6];
        let num_tris = triangulate_face(&mesh, mesh.faces[0], &mut output);
        assert_eq!(num_tris, 2);
        assert_eq!(output[0], 0);
        assert_eq!(output[1], 1);
        assert_eq!(output[2], 2);
    }

    #[test]
    fn test_orient2d() {
        let a = Vec2::new(0.0, 0.0);
        let b = Vec2::new(1.0, 0.0);
        let c = Vec2::new(0.0, 1.0);

        let orient = orient2d(a, b, c);
        assert!(orient > 0.0); // Counter-clockwise
    }

    #[test]
    fn test_skinning_single_bone() {
        let skin = SkinDeformer {
            vertices: vec![SkinVertex {
                weight_begin: 0,
                num_weights: 1,
                dq_weight: 0.0,
            }],
            weights: vec![SkinWeight {
                cluster_index: 0,
                weight: 1.0,
            }],
            ..Default::default()
        };

        let matrices = vec![Matrix {
            cols: [
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(0.0, 1.0, 0.0),
                Vec3::new(0.0, 0.0, 1.0),
                Vec3::new(1.0, 0.0, 0.0), // Translation
            ],
        }];

        let pos = Vec3::new(0.0, 0.0, 0.0);
        let skinned = evaluate_skin_vertex(&skin, 0, &matrices, pos);

        // Should translate by (1, 0, 0)
        assert!((skinned.x - 1.0).abs() < 1e-6);
        assert!(skinned.y.abs() < 1e-6);
        assert!(skinned.z.abs() < 1e-6);
    }
}
