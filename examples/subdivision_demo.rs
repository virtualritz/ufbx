//! Subdivision Surface Demo
//!
//! This example demonstrates the Catmull-Clark subdivision implementation.
//! It creates a simple cube mesh and subdivides it multiple times to show
//! how the surface becomes progressively smoother.

use ufbx::subdivision::SubdivisionEvaluator;
use ufbx::types::{Element, ElementType, Face, Mesh, SubdivisionBoundary, Vec3, VertexAttrib};

/// Create a simple cube mesh (8 vertices, 6 quad faces)
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
        Face { index_begin: 0, num_indices: 4 },   // -Z face
        Face { index_begin: 4, num_indices: 4 },   // +Z face
        Face { index_begin: 8, num_indices: 4 },   // -Y face
        Face { index_begin: 12, num_indices: 4 },  // +Y face
        Face { index_begin: 16, num_indices: 4 },  // -X face
        Face { index_begin: 20, num_indices: 4 },  // +X face
    ];

    // Face vertex indices
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
        subdivision_display_mode: ufbx::types::SubdivisionDisplayMode::Disabled,
        subdivision_boundary: SubdivisionBoundary::SharpCorners,
        subdivision_uv_boundary: SubdivisionBoundary::SharpCorners,
        reversed_winding: false,
        generated_normals: false,
        subdivision_evaluated: false,
        from_tessellated_nurbs: false,
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Catmull-Clark Subdivision Demo");
    println!("================================\n");

    // Create a cube mesh
    let cube = create_cube_mesh();
    println!("Original cube mesh:");
    println!("  Vertices: {}", cube.num_vertices);
    println!("  Faces: {}", cube.num_faces);
    println!("  Indices: {}", cube.num_indices);
    println!();

    // Subdivide 1 level
    let subdivided_1 = SubdivisionEvaluator::subdivide(&cube, 1)?;
    println!("After 1 subdivision level:");
    println!("  Vertices: {}", subdivided_1.num_vertices);
    println!("  Faces: {}", subdivided_1.num_faces);
    println!("  Indices: {}", subdivided_1.num_indices);
    println!("  Growth: {}x vertices, {}x faces",
        subdivided_1.num_vertices as f64 / cube.num_vertices as f64,
        subdivided_1.num_faces as f64 / cube.num_faces as f64);
    println!();

    // Subdivide 2 levels
    let subdivided_2 = SubdivisionEvaluator::subdivide(&cube, 2)?;
    println!("After 2 subdivision levels:");
    println!("  Vertices: {}", subdivided_2.num_vertices);
    println!("  Faces: {}", subdivided_2.num_faces);
    println!("  Indices: {}", subdivided_2.num_indices);
    println!("  Growth: {}x vertices, {}x faces",
        subdivided_2.num_vertices as f64 / cube.num_vertices as f64,
        subdivided_2.num_faces as f64 / cube.num_faces as f64);
    println!();

    // Subdivide 3 levels
    let subdivided_3 = SubdivisionEvaluator::subdivide(&cube, 3)?;
    println!("After 3 subdivision levels:");
    println!("  Vertices: {}", subdivided_3.num_vertices);
    println!("  Faces: {}", subdivided_3.num_faces);
    println!("  Indices: {}", subdivided_3.num_indices);
    println!("  Growth: {}x vertices, {}x faces",
        subdivided_3.num_vertices as f64 / cube.num_vertices as f64,
        subdivided_3.num_faces as f64 / cube.num_faces as f64);
    println!();

    println!("Subdivision complete!");
    println!("\nNote: Each level increases face count by ~4x");
    println!("The cube becomes progressively more spherical with subdivision.");

    Ok(())
}
