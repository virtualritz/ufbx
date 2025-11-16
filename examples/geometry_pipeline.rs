//! Complete Mesh Processing Pipeline
//!
//! This example demonstrates a complete mesh processing pipeline using
//! the geometry module, showing how different utilities work together.

use ufbx::geometry::*;
use ufbx::types::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Complete Mesh Processing Pipeline ===\n");

    // Step 1: Load/Create mesh
    let mesh = create_complex_mesh();
    print_mesh_stats("Original Mesh", &mesh);

    // Step 2: Triangulate mesh (if needed)
    let tri_mesh = if needs_triangulation(&mesh) {
        println!("\n📐 Triangulating mesh...");
        let result = triangulate_mesh(&mesh)?;
        print_mesh_stats("Triangulated Mesh", &result);
        result
    } else {
        mesh
    };

    // Step 3: Analyze topology
    println!("\n🔍 Analyzing topology...");
    let topo = compute_topology(&tri_mesh);
    analyze_topology(&tri_mesh, &topo);

    // Step 4: Generate optimized indices
    println!("\n⚡ Generating optimized indices...");
    let (unique_verts, indices) = optimize_for_gpu(&tri_mesh)?;
    println!("   Optimized: {} vertices → {} unique vertices",
             tri_mesh.num_vertices, unique_verts[0].len());
    println!("   Index buffer: {} indices", indices.len());
    println!("   Memory saved: {:.1}%",
             100.0 * (1.0 - unique_verts[0].len() as f64 / tri_mesh.num_vertices as f64));

    // Step 5: Apply skinning (if applicable)
    if has_skin_deformer(&tri_mesh) {
        println!("\n🦴 Applying skeletal animation...");
        let skinned = apply_character_skinning(&tri_mesh)?;
        print_mesh_stats("Skinned Mesh", &skinned);
    }

    println!("\n✅ Pipeline complete!\n");
    Ok(())
}

fn create_complex_mesh() -> Mesh {
    // Create a mesh with mixed face types (triangles, quads, n-gons)
    let mut mesh = Mesh::default();

    // Define vertices
    mesh.vertices = vec![
        // Quad 1
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(1.0, 1.0, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
        // Pentagon
        Vec3::new(2.0, 0.0, 0.0),
        Vec3::new(2.5, 0.5, 0.0),
        Vec3::new(2.5, 1.5, 0.0),
        Vec3::new(2.0, 2.0, 0.0),
        Vec3::new(1.5, 1.0, 0.0),
        // Triangle
        Vec3::new(3.0, 0.0, 0.0),
        Vec3::new(4.0, 0.0, 0.0),
        Vec3::new(3.5, 1.0, 0.0),
    ];

    mesh.num_vertices = mesh.vertices.len();

    // Define faces
    mesh.faces = vec![
        Face { index_begin: 0, num_indices: 4 },  // Quad
        Face { index_begin: 4, num_indices: 5 },  // Pentagon
        Face { index_begin: 9, num_indices: 3 },  // Triangle
    ];
    mesh.num_faces = mesh.faces.len();

    // Vertex indices
    mesh.vertex_indices = (0..12).collect();
    mesh.num_indices = 12;

    // Copy to vertex_position attribute
    mesh.vertex_position = VertexAttrib {
        exists: true,
        values: mesh.vertices.clone(),
        indices: vec![],
        value_reals: 3,
        unique_per_vertex: true,
        values_w: vec![],
    };

    // Add some smoothing
    mesh.face_smoothing = vec![true, true, false];
    mesh.edge_smoothing = vec![];

    mesh
}

fn print_mesh_stats(label: &str, mesh: &Mesh) {
    println!("\n{}", label);
    println!("   Vertices: {}", mesh.num_vertices);
    println!("   Indices: {}", mesh.num_indices);
    println!("   Faces: {}", mesh.num_faces);
    println!("   Triangles: {}", mesh.num_triangles);

    if !mesh.faces.is_empty() {
        let face_types = count_face_types(&mesh.faces);
        println!("   Face breakdown:");
        for (sides, count) in face_types {
            let name = match sides {
                3 => "triangles",
                4 => "quads",
                _ => "n-gons",
            };
            println!("     - {}: {}", name, count);
        }
    }
}

fn count_face_types(faces: &[Face]) -> Vec<(u32, usize)> {
    let mut counts = std::collections::HashMap::new();
    for face in faces {
        *counts.entry(face.num_indices).or_insert(0) += 1;
    }

    let mut result: Vec<_> = counts.into_iter().collect();
    result.sort_by_key(|(sides, _)| *sides);
    result
}

fn needs_triangulation(mesh: &Mesh) -> bool {
    mesh.faces.iter().any(|f| f.num_indices > 3)
}

fn analyze_topology(mesh: &Mesh, topo: &[TopoEdge]) {
    let mut boundary_edges = 0;
    let mut non_manifold_edges = 0;
    let mut smooth_edges = 0;
    let mut hard_edges = 0;

    for (i, edge) in topo.iter().enumerate() {
        if edge.twin.is_none() {
            boundary_edges += 1;
        }
        if edge.flags.non_manifold {
            non_manifold_edges += 1;
        }

        if is_edge_smooth(mesh, topo, i as u32) {
            smooth_edges += 1;
        } else {
            hard_edges += 1;
        }
    }

    println!("   Total edges: {}", topo.len());
    println!("   Boundary edges: {}", boundary_edges);
    println!("   Non-manifold edges: {}", non_manifold_edges);
    println!("   Smooth edges: {}", smooth_edges);
    println!("   Hard edges: {}", hard_edges);

    // Compute mesh manifoldness
    let is_manifold = non_manifold_edges == 0;
    let is_closed = boundary_edges == 0;

    println!("   Mesh type: {}", match (is_manifold, is_closed) {
        (true, true) => "Closed manifold (watertight)",
        (true, false) => "Open manifold (has boundaries)",
        (false, true) => "Non-manifold closed",
        (false, false) => "Non-manifold open",
    });
}

fn optimize_for_gpu(mesh: &Mesh) -> ufbx::error::Result<(Vec<Vec<Vec<u8>>>, Vec<u32>)> {
    // Convert Vec3 to bytes for index generation
    let vertex_count = mesh.num_vertices;
    let vertex_size = std::mem::size_of::<Vec3>();

    // Convert positions to bytes
    let mut position_bytes = Vec::with_capacity(vertex_count * vertex_size);
    for vertex in &mesh.vertices {
        // Pack as f64 (Real)
        position_bytes.extend_from_slice(&vertex.x.to_le_bytes());
        position_bytes.extend_from_slice(&vertex.y.to_le_bytes());
        position_bytes.extend_from_slice(&vertex.z.to_le_bytes());
    }

    let position_stream = VertexStream {
        data: &position_bytes,
        vertex_size,
    };

    generate_indices(&[position_stream], vertex_count)
}

fn has_skin_deformer(_mesh: &Mesh) -> bool {
    // In a real application, check if mesh has skin deformers
    false
}

fn apply_character_skinning(mesh: &Mesh) -> ufbx::error::Result<Mesh> {
    // Create example skin deformer
    let skin = SkinDeformer {
        vertices: vec![
            SkinVertex {
                weight_begin: 0,
                num_weights: 2,
                dq_weight: 0.0,
            },
            SkinVertex {
                weight_begin: 2,
                num_weights: 2,
                dq_weight: 0.0,
            },
        ],
        weights: vec![
            SkinWeight { cluster_index: 0, weight: 0.7 },
            SkinWeight { cluster_index: 1, weight: 0.3 },
            SkinWeight { cluster_index: 0, weight: 0.5 },
            SkinWeight { cluster_index: 1, weight: 0.5 },
        ],
        ..Default::default()
    };

    // Create example bone matrices
    let bone_matrices = vec![
        Matrix::IDENTITY,
        Matrix {
            cols: [
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(0.0, 1.0, 0.0),
                Vec3::new(0.0, 0.0, 1.0),
                Vec3::new(0.5, 0.5, 0.0), // Translate
            ],
        },
    ];

    apply_skinning(mesh, &skin, &bone_matrices)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_complex_mesh_creation() {
        let mesh = create_complex_mesh();
        assert_eq!(mesh.num_vertices, 12);
        assert_eq!(mesh.num_faces, 3);
        assert!(needs_triangulation(&mesh));
    }

    #[test]
    fn test_pipeline() {
        let result = main();
        assert!(result.is_ok());
    }
}
