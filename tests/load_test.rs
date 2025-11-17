//! Integration tests for FBX file loading

use ufbx::{load_file, load_memory, SceneOpts};

#[test]
fn test_mesh_data_extraction() {
    // Test that mesh vertex data is being extracted correctly
    let result = load_file(
        "data/blender_272_cube_7400_binary.fbx",
        &SceneOpts::default()
    );

    match result {
        Ok(scene) => {
            println!("Loaded scene for mesh data extraction test");
            println!("  Nodes: {}", scene.nodes.len());
            println!("  Meshes: {}", scene.meshes.len());

            // Check that we have meshes
            assert!(scene.meshes.len() > 0, "Scene should have at least one mesh");

            // Check each mesh
            for (i, mesh) in scene.meshes.iter().enumerate() {
                println!("\nMesh #{}: {}", i, mesh.element.name.as_str());
                println!("  Vertices: {}", mesh.num_vertices);
                println!("  Indices: {}", mesh.num_indices);
                println!("  Faces: {}", mesh.num_faces);
                println!("  Position exists: {}", mesh.vertex_position.exists);

                // A cube should have 8 vertices (or more if subdivided)
                if mesh.element.name.as_str().contains("Cube") {
                    assert!(mesh.num_vertices >= 8, "Cube should have at least 8 vertices");
                    assert!(mesh.num_faces >= 6, "Cube should have at least 6 faces");
                    assert!(mesh.vertex_position.exists, "Cube should have position data");
                }

                // Verify vertex data consistency
                if mesh.vertex_position.exists {
                    assert_eq!(mesh.vertices.len(), mesh.num_vertices,
                        "vertices array length should match num_vertices");
                    assert_eq!(mesh.vertex_position.values.len(), mesh.num_vertices,
                        "vertex_position.values length should match num_vertices");
                }

                // Verify face data consistency
                if !mesh.faces.is_empty() {
                    for face in &mesh.faces {
                        assert!(face.num_indices >= 3, "Face should have at least 3 indices");
                        let end_idx = face.index_begin + face.num_indices;
                        assert!(end_idx <= mesh.vertex_indices.len() as u32,
                            "Face indices should be within vertex_indices bounds");
                    }
                }
            }

            // Check node hierarchy
            println!("\nNode hierarchy:");
            for (i, node) in scene.nodes.iter().enumerate() {
                let parent_str = if let Some(parent) = node.parent {
                    format!("parent={}", parent)
                } else {
                    "root".to_string()
                };

                let mesh_str = if let Some(mesh_idx) = node.mesh {
                    format!("mesh={}", mesh_idx)
                } else {
                    "none".to_string()
                };

                println!("  Node #{}: {} ({}, {})",
                    i, node.element.name.as_str(), parent_str, mesh_str);

                // Verify parent relationships
                if let Some(parent_idx) = node.parent {
                    assert!(parent_idx < scene.nodes.len(), "Parent index should be valid");
                    assert!(scene.nodes[parent_idx].children.contains(&i),
                        "Parent should have this node in its children list");
                }
            }

            println!("\n✓ Mesh data extraction test passed!");
        }
        Err(e) => {
            panic!("Failed to load scene for mesh data extraction: {:?}", e);
        }
    }
}

#[test]
fn test_load_simple_cube() {
    // Test loading a simple cube FBX file
    let result = load_file(
        "data/blender_272_cube_7400_binary.fbx",
        &SceneOpts::default()
    );

    match result {
        Ok(scene) => {
            println!("Successfully loaded scene!");
            println!("  Version: {}", scene.metadata.version);
            println!("  Nodes: {}", scene.nodes.len());
            println!("  Meshes: {}", scene.meshes.len());

            // Basic validation
            assert!(scene.nodes.len() > 0, "Scene should have at least one node");
        }
        Err(e) => {
            println!("Failed to load scene: {:?}", e);
            panic!("Failed to load test file: {:?}", e);
        }
    }
}

#[test]
fn test_load_memory() {
    // Test loading from memory
    let data = std::fs::read("data/blender_272_cube_7400_binary.fbx").unwrap();
    let result = load_memory(&data, &SceneOpts::default());

    match result {
        Ok(scene) => {
            println!("Successfully loaded from memory!");
            assert!(scene.metadata.version > 0, "Version should be set");
        }
        Err(e) => {
            println!("Failed to load from memory: {:?}", e);
            panic!("Failed to load from memory: {:?}", e);
        }
    }
}

#[test]
fn test_detect_binary_format() {
    // Test binary format detection
    let data = std::fs::read("data/blender_272_cube_7400_binary.fbx").unwrap();

    assert!(data.len() > 27, "File should be large enough");
    assert_eq!(
        &data[0..22],
        b"Kaydara FBX Binary  \x00\x1a",
        "Should have FBX binary magic"
    );
}

#[test]
fn test_load_ascii_fbx() {
    // Test ASCII FBX loading
    let result = load_file(
        "data/blender_279_ball_6100_ascii.fbx",
        &SceneOpts::default()
    );

    match result {
        Ok(scene) => {
            println!("Successfully loaded ASCII FBX!");
            println!("  Version: {}", scene.metadata.version);
            assert!(scene.metadata.ascii, "Should be marked as ASCII");
        }
        Err(e) => {
            println!("Failed to load ASCII FBX: {:?}", e);
            panic!("Failed to load ASCII file: {:?}", e);
        }
    }
}

#[test]
fn test_multiple_files() {
    // Test loading multiple different files
    let test_files = vec![
        "data/blender_272_cube_7400_binary.fbx",
        "data/blender_279_ball_7400_binary.fbx",
        "data/blender_279_default_7400_binary.fbx",
    ];

    for file in test_files {
        println!("\nTesting file: {}", file);
        let result = load_file(file, &SceneOpts::default());

        match result {
            Ok(scene) => {
                println!("  ✓ Loaded successfully");
                println!("    Nodes: {}, Meshes: {}", scene.nodes.len(), scene.meshes.len());
            }
            Err(e) => {
                println!("  ✗ Failed: {:?}", e);
                panic!("Failed to load {}: {:?}", file, e);
            }
        }
    }
}
