//! Simple example demonstrating OBJ file loading

use ufbx::obj::ObjParser;
use std::env;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    let path = if args.len() > 1 {
        &args[1]
    } else {
        "data/blender_279_default.obj"
    };

    println!("Loading OBJ file: {}", path);
    let scene = ObjParser::parse_file(path)?;

    println!("\n=== Scene Information ===");
    println!("Format: {:?}", scene.metadata.file_format);
    println!("ASCII: {}", scene.metadata.ascii);
    println!("Nodes: {}", scene.nodes.len());
    println!("Meshes: {}", scene.meshes.len());

    for (i, node) in scene.nodes.iter().enumerate() {
        println!("\nNode {}: {}", i, node.element.name.as_str());
        println!("  Root: {}", node.is_root);
        if let Some(mesh_idx) = node.mesh {
            println!("  Has mesh: index {}", mesh_idx);
        }
    }

    for (i, mesh) in scene.meshes.iter().enumerate() {
        println!("\n=== Mesh {} ===", i);
        println!("Name: {}", mesh.element.name.as_str());
        println!("Vertices: {}", mesh.num_vertices);
        println!("Faces: {}", mesh.num_faces);
        println!("Indices: {}", mesh.num_indices);
        println!("Triangles: {}", mesh.num_triangles);

        if mesh.vertex_position.exists {
            println!("Position values: {}", mesh.vertex_position.values.len());
            if !mesh.vertex_position.values.is_empty() {
                let first = &mesh.vertex_position.values[0];
                println!("  First vertex: ({}, {}, {})", first.x, first.y, first.z);
            }
        }

        if mesh.vertex_normal.exists {
            println!("Normal values: {}", mesh.vertex_normal.values.len());
        }

        if mesh.vertex_uv.exists {
            println!("UV values: {}", mesh.vertex_uv.values.len());
        }

        if mesh.vertex_color.exists {
            println!("Color values: {}", mesh.vertex_color.values.len());
        }
    }

    Ok(())
}
