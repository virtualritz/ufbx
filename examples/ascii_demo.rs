//! Demo of FBX ASCII parser capabilities

use ufbx::ascii::{parse_ascii, AsciiValue};

fn main() {
    let fbx_content = r#"; FBX 7.4.0 project file
; Created by ufbx Rust parser demo

FBXHeaderExtension: {
    FBXHeaderVersion: 1003
    FBXVersion: 7400
    CreationTimeStamp: {
        Version: 1000
        Year: 2025
        Month: 11
        Day: 16
    }
}

Objects: {
    Geometry: 12345, "Geometry::Cube", "Mesh" {
        Vertices: *24 {
            a: 0.0,0.0,0.0, 1.0,0.0,0.0, 1.0,1.0,0.0, 0.0,1.0,0.0,
               0.0,0.0,1.0, 1.0,0.0,1.0, 1.0,1.0,1.0, 0.0,1.0,1.0
        }
        
        Properties70: {
            P: "DefaultAttributeIndex", "int", "Integer", "",0
        }
    }
    
    Model: 67890, "Model::TestCube", "Mesh" {
        Version: 232
        Properties70: {
            P: "Lcl Translation", "Lcl Translation", "", "A",1.5,-2.3,0.0
            P: "Visible", "bool", "", "",1
        }
    }
}

Connections: {
    C: "OO", 67890, 12345
}
"#;

    match parse_ascii(fbx_content) {
        Ok(nodes) => {
            println!("=== FBX ASCII Parser Demo ===\n");
            
            // Show structure
            for node in &nodes {
                print_node(node, 0);
            }
            
            // Find specific data
            println!("\n=== Extracted Data ===");
            if let Some(objects) = nodes.iter().find(|n| n.name == "Objects") {
                for child in &objects.children {
                    if child.name == "Geometry" && child.values.len() >= 3 {
                        if let AsciiValue::Int(id) = child.values[0] {
                            println!("Geometry ID: {}", id);
                        }
                        if let AsciiValue::String(name) = &child.values[1] {
                            println!("Geometry Name: {}", name);
                        }
                        
                        // Find vertices
                        if let Some(verts) = child.children.iter().find(|n| n.name == "Vertices") {
                            if let Some(AsciiValue::Array(arr)) = verts.values.first() {
                                println!("Vertex count: {}", arr.len() / 3);
                                println!("First vertex: ({:?}, {:?}, {:?})", 
                                    arr.get(0), arr.get(1), arr.get(2));
                            }
                        }
                    }
                }
            }
            
            println!("\n✓ Successfully parsed and extracted FBX data!");
        }
        Err(e) => {
            eprintln!("Parse error: {}", e);
        }
    }
}

fn print_node(node: &ufbx::ascii::AsciiNode, indent: usize) {
    let prefix = "  ".repeat(indent);
    
    print!("{}{}", prefix, node.name);
    
    // Print values
    if !node.values.is_empty() {
        print!(": ");
        for (i, val) in node.values.iter().take(3).enumerate() {
            if i > 0 { print!(", "); }
            match val {
                AsciiValue::Int(n) => print!("{}", n),
                AsciiValue::Float(f) => print!("{:.2}", f),
                AsciiValue::String(s) => print!("\"{}\"", s),
                AsciiValue::Array(arr) => print!("[{}]", arr.len()),
            }
        }
        if node.values.len() > 3 {
            print!(", ...");
        }
    }
    
    println!(" {{{}}}", if node.children.is_empty() { "" } else { "..." });
    
    // Recursively print children
    for child in &node.children {
        print_node(child, indent + 1);
    }
}
