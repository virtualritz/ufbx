//! Geometry Processing Demo
//!
//! This example demonstrates the mesh processing utilities in the geometry module.

use ufbx::geometry::*;
use ufbx::types::*;

fn main() {
    println!("=== ufbx Geometry Processing Demo ===\n");

    // Example 1: Topology Analysis
    demo_topology();

    // Example 2: Triangulation
    demo_triangulation();

    // Example 3: Index Generation
    demo_index_generation();

    // Example 4: Skinning
    demo_skinning();
}

fn demo_topology() {
    println!("1. TOPOLOGY ANALYSIS");
    println!("   Computing half-edge topology for a simple triangle mesh...");

    let mesh = create_triangle_mesh();
    let topo = compute_topology(&mesh);

    println!("   Found {} topology edges", topo.len());
    for (i, edge) in topo.iter().enumerate() {
        println!(
            "   Edge {}: index={}, next={}, prev={}, twin={:?}, face={}",
            i, edge.index, edge.next, edge.prev, edge.twin, edge.face
        );
    }

    // Check if edges are smooth
    for i in 0..topo.len() as u32 {
        let smooth = is_edge_smooth(&mesh, &topo, i);
        println!("   Edge {} is {}", i, if smooth { "smooth" } else { "hard" });
    }

    println!();
}

fn demo_triangulation() {
    println!("2. TRIANGULATION");
    println!("   Triangulating a quad mesh...");

    let quad_mesh = create_quad_mesh();
    let mut output = vec![0u32; 6]; // Max 2 triangles * 3 indices

    let num_tris = triangulate_face(&quad_mesh, quad_mesh.faces[0], &mut output);
    println!("   Generated {} triangles from quad", num_tris);
    println!("   Triangle 1: [{}, {}, {}]", output[0], output[1], output[2]);
    println!("   Triangle 2: [{}, {}, {}]", output[3], output[4], output[5]);

    // Triangulate entire mesh
    match triangulate_mesh(&quad_mesh) {
        Ok(tri_mesh) => {
            println!("   Triangulated mesh has {} faces", tri_mesh.num_faces);
            println!("   All faces are now triangles: {}", tri_mesh.num_triangles);
        }
        Err(e) => println!("   Error: {}", e),
    }

    println!();
}

fn demo_index_generation() {
    println!("3. INDEX GENERATION & DEDUPLICATION");
    println!("   Generating indices for vertex data with duplicates...");

    // Create vertex data with duplicates
    let vertices = vec![
        // Triangle 1
        0.0f32, 0.0, 0.0, // Vertex 0
        1.0, 0.0, 0.0,    // Vertex 1
        0.0, 1.0, 0.0,    // Vertex 2
        // Triangle 2 (shares edge with triangle 1)
        1.0f32, 0.0, 0.0, // Vertex 3 (duplicate of 1)
        1.0, 1.0, 0.0,    // Vertex 4
        0.0, 1.0, 0.0,    // Vertex 5 (duplicate of 2)
    ];

    let vertex_bytes: Vec<u8> = vertices
        .iter()
        .flat_map(|&f| f.to_le_bytes())
        .collect();

    let stream = VertexStream {
        data: &vertex_bytes,
        vertex_size: 12, // 3 floats * 4 bytes
    };

    match generate_indices(&[stream], 6) {
        Ok((unique_verts, indices)) => {
            println!("   Original vertices: 6");
            println!("   Unique vertices: {}", unique_verts[0].len());
            println!("   Indices: {:?}", indices);
            println!("   Deduplication reduced {} vertices to {} unique",
                     6, unique_verts[0].len());
        }
        Err(e) => println!("   Error: {}", e),
    }

    println!();
}

fn demo_skinning() {
    println!("4. CPU SKINNING EVALUATION");
    println!("   Applying bone transforms to mesh vertices...");

    // Create a simple mesh
    let mut mesh = Mesh {
        num_vertices: 3,
        vertices: vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.5, 1.0, 0.0),
        ],
        ..Default::default()
    };

    // Create skin deformer with simple weights
    let skin = SkinDeformer {
        vertices: vec![
            SkinVertex {
                weight_begin: 0,
                num_weights: 1,
                dq_weight: 0.0,
            },
            SkinVertex {
                weight_begin: 1,
                num_weights: 1,
                dq_weight: 0.0,
            },
            SkinVertex {
                weight_begin: 2,
                num_weights: 2,
                dq_weight: 0.0,
            },
        ],
        weights: vec![
            SkinWeight {
                cluster_index: 0,
                weight: 1.0,
            },
            SkinWeight {
                cluster_index: 0,
                weight: 1.0,
            },
            SkinWeight {
                cluster_index: 0,
                weight: 0.5,
            },
            SkinWeight {
                cluster_index: 1,
                weight: 0.5,
            },
        ],
        ..Default::default()
    };

    // Create bone matrices (translation only for simplicity)
    let bone_matrices = vec![
        Matrix {
            cols: [
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(0.0, 1.0, 0.0),
                Vec3::new(0.0, 0.0, 1.0),
                Vec3::new(1.0, 0.0, 0.0), // Translate X by 1
            ],
        },
        Matrix {
            cols: [
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(0.0, 1.0, 0.0),
                Vec3::new(0.0, 0.0, 1.0),
                Vec3::new(0.0, 1.0, 0.0), // Translate Y by 1
            ],
        },
    ];

    match apply_skinning(&mesh, &skin, &bone_matrices) {
        Ok(skinned_mesh) => {
            println!("   Original positions:");
            for (i, v) in mesh.vertices.iter().enumerate() {
                println!("     Vertex {}: ({:.2}, {:.2}, {:.2})", i, v.x, v.y, v.z);
            }
            println!("   Skinned positions:");
            for (i, v) in skinned_mesh.vertices.iter().enumerate() {
                println!("     Vertex {}: ({:.2}, {:.2}, {:.2})", i, v.x, v.y, v.z);
            }
        }
        Err(e) => println!("   Error: {}", e),
    }

    println!();
}

// Helper functions to create test meshes

fn create_triangle_mesh() -> Mesh {
    Mesh {
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
    }
}

fn create_quad_mesh() -> Mesh {
    Mesh {
        num_vertices: 4,
        num_indices: 4,
        num_faces: 1,
        faces: vec![Face {
            index_begin: 0,
            num_indices: 4,
        }],
        vertex_indices: vec![0, 1, 2, 3],
        vertices: vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(1.0, 1.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        ],
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
    }
}
