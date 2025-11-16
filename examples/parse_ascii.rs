use std::fs;
use ufbx::ascii;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = args.get(1).unwrap_or(&"data/blender_292_circle_7300_ascii.fbx".to_string()).clone();
    
    let content = fs::read_to_string(&path).unwrap();
    
    match ascii::parse_ascii(&content) {
        Ok(nodes) => {
            println!("Successfully parsed {} root nodes from {}", nodes.len(), path);
            for (i, node) in nodes.iter().take(5).enumerate() {
                println!("Node {}: {} ({} values, {} children)", 
                    i, node.name, node.values.len(), node.children.len());
            }
        }
        Err(e) => {
            eprintln!("Parse error: {}", e);
            std::process::exit(1);
        }
    }
}
