//! Integration tests for FBX file loading

use ufbx::{load_file, load_memory, SceneOpts};

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
