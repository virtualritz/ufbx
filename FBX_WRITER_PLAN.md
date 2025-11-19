# FBX Writer Implementation Plan

## Overview

Implement a full FBX writer that can serialize `Scene` data structures back to FBX files (binary and ASCII formats). This will enable round-trip workflows: load → modify → save.

## Architecture

### Data Flow
```
Scene (types.rs)
    ↓
SceneSerializer (new: writer.rs)
    ↓
FbxDocument (binary.rs / ascii.rs types)
    ↓
BinaryWriter / AsciiWriter (new modules)
    ↓
File / Memory Buffer
```

### Module Structure

```
src/
├── writer.rs          # NEW: Scene → FbxDocument serialization
├── binary_writer.rs   # NEW: FbxDocument → Binary FBX
├── ascii_writer.rs    # NEW: FbxDocument → ASCII FBX
├── binary.rs          # EXTEND: Add Write types alongside Read types
├── ascii.rs           # EXTEND: Add Write types alongside Read types
├── scene.rs           # UNCHANGED: Reading logic
└── types.rs           # UNCHANGED: Core data structures
```

## Implementation Phases

### Phase 1: Core Writer Infrastructure (writer.rs)

**Purpose:** Convert Scene back to intermediate FBX representation

**Key Components:**

1. **SceneSerializer struct**
   ```rust
   pub struct SceneSerializer {
       scene: &Scene,
       opts: SaveOpts,
       fbx_id_counter: u64,
       element_to_fbx_id: HashMap<usize, u64>,  // Scene element index → FBX ID
   }
   ```

2. **SaveOpts configuration**
   ```rust
   pub struct SaveOpts {
       pub format: SaveFormat,           // Binary or ASCII
       pub version: u32,                 // FBX version (default: 7400)
       pub exporter_name: String,        // "ufbx-rust"
       pub compress_arrays: bool,        // Use DEFLATE for large arrays
       pub ascii_formatting: AsciiFormat, // Indentation, line breaks
       pub coordinate_system: CoordinateAxes, // Optional transform
   }

   pub enum SaveFormat {
       Binary,
       Ascii,
       Auto,  // Choose based on file extension
   }
   ```

3. **Core serialization logic**
   - Assign FBX IDs to all elements (nodes, meshes, materials, etc.)
   - Build FBX node hierarchy:
     - Header nodes (FBXHeaderExtension, FileId, CreationTime, etc.)
     - GlobalSettings
     - Objects (Geometry, Model, Material, etc.)
     - Connections (OO, OP types)
     - Takes (animation stacks)
   - Convert Scene data to FBX properties

4. **Node building functions**
   ```rust
   fn build_header_nodes(&mut self) -> Vec<FbxNode>
   fn build_global_settings(&mut self) -> FbxNode
   fn build_objects(&mut self) -> FbxNode
   fn build_geometry_node(&mut self, mesh: &Mesh) -> FbxNode
   fn build_model_node(&mut self, node: &Node) -> FbxNode
   fn build_material_node(&mut self, material: &Material) -> FbxNode
   fn build_connections(&mut self) -> FbxNode
   fn build_takes(&mut self) -> FbxNode
   ```

5. **Property conversion**
   ```rust
   fn vec3_to_properties(&self, v: Vec3) -> Vec<Value>
   fn color_to_properties(&self, c: Vec3) -> Vec<Value>
   fn material_map_to_node(&self, name: &str, map: &MaterialMap) -> FbxNode
   ```

**Outputs:** `FbxDocument` ready for binary/ASCII encoding

---

### Phase 2: Binary Writer (binary_writer.rs)

**Purpose:** Encode FbxDocument to binary FBX format

**Binary Format Structure:**
```
[Header: 27 bytes]
  - Magic: "Kaydara FBX Binary  \x00\x1a" (22 bytes)
  - Endian: 0x00 (1 byte, always little-endian)
  - Version: u32 LE (4 bytes)

[Node Tree]
  - Recursive nodes until NULL terminator

[Footer]
  - Unknown data (16 bytes padding)
  - Footer marker bytes
  - Version u32 (repeated)
  - Padding to 16-byte alignment
```

**Key Components:**

1. **BinaryWriter struct**
   ```rust
   pub struct BinaryWriter<W: Write> {
       writer: W,
       version: u32,
       bytes_written: u64,
   }
   ```

2. **Node encoding (version-dependent)**
   ```rust
   fn write_node(&mut self, node: &FbxNode, depth: u32) -> Result<()>
   fn write_node_32bit(&mut self, node: &FbxNode) -> Result<()>  // FBX < 7500
   fn write_node_64bit(&mut self, node: &FbxNode) -> Result<()>  // FBX >= 7500
   ```

3. **Property encoding**
   ```rust
   fn write_property(&mut self, value: &Value) -> Result<()>
   fn write_bool(&mut self, v: bool) -> Result<()>
   fn write_i16(&mut self, v: i16) -> Result<()>
   fn write_i32(&mut self, v: i32) -> Result<()>
   fn write_i64(&mut self, v: i64) -> Result<()>
   fn write_f32(&mut self, v: f32) -> Result<()>
   fn write_f64(&mut self, v: f64) -> Result<()>
   fn write_string(&mut self, s: &str) -> Result<()>
   ```

4. **Array encoding with compression**
   ```rust
   fn write_array_i32(&mut self, arr: &[i32], compress: bool) -> Result<()>
   fn write_array_f32(&mut self, arr: &[f32], compress: bool) -> Result<()>
   fn write_array_f64(&mut self, arr: &[f64], compress: bool) -> Result<()>
   fn compress_array(&self, data: &[u8]) -> Result<Vec<u8>>  // DEFLATE
   ```

5. **Helper functions**
   ```rust
   fn write_header(&mut self, version: u32) -> Result<()>
   fn write_footer(&mut self) -> Result<()>
   fn write_null_node(&mut self) -> Result<()>  // 13 or 25 zero bytes
   ```

**Dependencies:**
- `byteorder` for endian-safe writes
- `flate2` for DEFLATE compression

---

### Phase 3: ASCII Writer (ascii_writer.rs)

**Purpose:** Encode FbxDocument to ASCII FBX format

**ASCII Format Structure:**
```
; FBX 7.4.0 project file
; Created by ufbx-rust

FBXHeaderExtension:  {
    FBXHeaderVersion: 1003
    FBXVersion: 7400
    ...
}

GlobalSettings:  {
    Version: 1000
    Properties70:  {
        P: "UpAxis", "int", "Integer", "",1
        ...
    }
}

Objects:  {
    Geometry: 123456789, "Geometry::Cube", "Mesh" {
        Vertices: *24 {
            a: -1.0,1.0,-1.0,1.0,1.0,-1.0,...
        }
        ...
    }
}

Connections:  {
    C: "OO",123456789,0
    ...
}
```

**Key Components:**

1. **AsciiWriter struct**
   ```rust
   pub struct AsciiWriter<W: Write> {
       writer: W,
       indent_level: usize,
       indent_string: String,  // Default: "    " (4 spaces)
       version: u32,
   }
   ```

2. **Node encoding**
   ```rust
   fn write_node(&mut self, node: &FbxNode) -> Result<()>
   fn write_indent(&mut self) -> Result<()>
   fn write_node_header(&mut self, name: &str) -> Result<()>
   ```

3. **Property encoding**
   ```rust
   fn write_property(&mut self, value: &Value) -> Result<()>
   fn write_property_list(&mut self, values: &[Value]) -> Result<()>
   fn format_bool(&self, v: bool) -> String
   fn format_number(&self, v: f64) -> String
   fn format_string(&self, s: &str) -> String  // Escape quotes
   ```

4. **Array encoding**
   ```rust
   fn write_array(&mut self, name: &str, values: &[Value]) -> Result<()>
   fn write_array_compact(&mut self, arr: &[f64]) -> Result<()>
   ```

5. **Formatting helpers**
   ```rust
   fn push_indent(&mut self)
   fn pop_indent(&mut self)
   fn write_comment(&mut self, comment: &str) -> Result<()>
   ```

**Special handling:**
- Property70 format: `P: "Name", "Type", "Type2", "Flags", Value...`
- Array compact format: `*count { a: val1,val2,val3,... }`
- String escaping: quotes become `\"`

---

### Phase 4: Public API (lib.rs)

**New public functions:**

```rust
#[cfg(feature = "writer")]
pub use writer::{save_file, save_memory, SaveOpts, SaveFormat};

#[cfg(feature = "writer")]
pub fn save_file(scene: &Scene, path: &str, opts: &SaveOpts) -> Result<()> {
    let data = save_memory(scene, opts)?;
    std::fs::write(path, data)?;
    Ok(())
}

#[cfg(feature = "writer")]
pub fn save_memory(scene: &Scene, opts: &SaveOpts) -> Result<Vec<u8>> {
    let mut serializer = SceneSerializer::new(scene, opts);
    let doc = serializer.build_document()?;

    match opts.format {
        SaveFormat::Binary => {
            let mut buf = Vec::new();
            let mut writer = BinaryWriter::new(&mut buf, opts.version);
            writer.write_document(&doc)?;
            Ok(buf)
        }
        SaveFormat::Ascii => {
            let mut buf = Vec::new();
            let mut writer = AsciiWriter::new(&mut buf, opts.version);
            writer.write_document(&doc)?;
            Ok(buf)
        }
        SaveFormat::Auto => unreachable!("Auto should be resolved earlier"),
    }
}
```

---

### Phase 5: Testing & Validation

**Test Strategy:**

1. **Unit Tests** (per module)
   ```rust
   #[cfg(test)]
   mod tests {
       #[test]
       fn test_write_binary_header() { ... }

       #[test]
       fn test_write_property_i32() { ... }

       #[test]
       fn test_compress_array() { ... }

       #[test]
       fn test_ascii_string_escaping() { ... }
   }
   ```

2. **Integration Tests** (tests/writer_test.rs)
   ```rust
   #[test]
   fn test_round_trip_binary() {
       // Load existing FBX
       let scene = load_file("data/blender_272_cube_7400_binary.fbx", &Default::default()).unwrap();

       // Save to memory
       let data = save_memory(&scene, &SaveOpts {
           format: SaveFormat::Binary,
           version: 7400,
           ..Default::default()
       }).unwrap();

       // Load saved data
       let scene2 = load_memory(&data, &Default::default()).unwrap();

       // Verify equivalence
       assert_eq!(scene.nodes.len(), scene2.nodes.len());
       assert_eq!(scene.meshes.len(), scene2.meshes.len());
       // ... more assertions
   }

   #[test]
   fn test_round_trip_ascii() { ... }

   #[test]
   fn test_external_validation() {
       // Save file and attempt to load in Blender/Maya (manual test)
       // Check for warnings/errors
   }
   ```

3. **Validation Tests**
   ```rust
   #[test]
   fn test_write_large_mesh() {
       // 1M vertices, test compression
   }

   #[test]
   fn test_write_complex_scene() {
       // Deep hierarchy, many materials
   }

   #[test]
   fn test_coordinate_transform() {
       // Verify axis conversion
   }
   ```

4. **Compatibility Tests**
   - Load files from Blender, Maya, 3ds Max
   - Save and reload in each tool
   - Verify no data loss

---

## Cargo.toml Changes

```toml
[features]
default = ["std", "obj-support", "parallel"]
std = []
obj-support = []
nurbs = []
subdivision = []
geometry-cache = []
parallel = ["rayon"]
writer = []  # NEW: Enable FBX writing functionality

[dependencies]
byteorder = "1.5"        # Already present
flate2 = "1.0"           # Already present
thiserror = "1.0"        # Already present
rayon = { version = "1.8", optional = true }
smallvec = "1.11"        # Already present
ahash = "0.8"            # Already present

# Writer-specific dependencies (none needed - reuse existing)
```

---

## Critical Implementation Details

### 1. FBX ID Assignment
- Must be unique 64-bit integers
- Cannot be 0 (reserved)
- Common approach: sequential starting from 100000000
- Store bidirectional mapping: Scene index ↔ FBX ID

### 2. Connections
FBX uses explicit connections between objects:
- `OO` (Object-Object): mesh → node, material → mesh
- `OP` (Object-Property): texture → material.DiffuseColor

Example:
```
C: "OO", 234567890, 123456789  // mesh 234567890 → node 123456789
C: "OP", 345678901, 234567890, "DiffuseColor"  // texture → material property
```

### 3. Properties70 Format
Standard way to encode properties:
```
P: "PropertyName", "Type", "SubType", "Flags", Value1, Value2, ...

Examples:
P: "UpAxis", "int", "Integer", "", 1
P: "DiffuseColor", "Color", "", "A", 0.8, 0.8, 0.8
P: "Lcl Translation", "Lcl Translation", "", "A", 0.0, 0.0, 0.0
```

### 4. Array Compression
For arrays > 128 elements, use DEFLATE:
```rust
let raw_bytes = bytemuck::cast_slice::<f32, u8>(vertices);
let compressed = compress(raw_bytes)?;

write_u32(vertices.len() as u32);       // element count
write_u32(1);                           // encoding: 1 = DEFLATE
write_u32(compressed.len() as u32);     // compressed size
write_all(&compressed);                 // compressed data
```

### 5. Footer Format (Binary)
```
[16 bytes unknown/padding]
[16 bytes footer marker: varies by version]
[4 bytes: version as u32 LE]
[padding to 16-byte alignment]
```

### 6. Mesh Encoding
Vertices array:
```
Vertices: *24 {
    a: -1.0,1.0,-1.0,1.0,1.0,-1.0,...
}
```

Polygon indices (negative marks polygon end):
```
PolygonVertexIndex: *24 {
    a: 0,1,2,-4,4,5,6,-8,...
}
// Triangle: [0,1,2,3] → encoded as [0,1,2,-4]
// Quad:     [4,5,6,7] → encoded as [4,5,6,-8]
```

---

## Implementation Order

### Week 1: Foundation
- [x] ~~Plan complete~~
- [ ] Create writer.rs skeleton
- [ ] Implement SaveOpts and public API
- [ ] Implement FBX ID assignment logic
- [ ] Write basic Scene → FbxDocument structure

### Week 2: Binary Writer
- [ ] Create binary_writer.rs
- [ ] Implement header/footer writing
- [ ] Implement property encoding (scalars)
- [ ] Implement array encoding without compression
- [ ] Add DEFLATE compression

### Week 3: Scene Serialization
- [ ] Implement header nodes generation
- [ ] Implement GlobalSettings
- [ ] Implement Geometry nodes (meshes)
- [ ] Implement Model nodes (scene graph)
- [ ] Implement Material nodes
- [ ] Implement Connections

### Week 4: ASCII Writer & Testing
- [ ] Create ascii_writer.rs
- [ ] Implement ASCII formatting
- [ ] Write unit tests for all modules
- [ ] Write integration round-trip tests
- [ ] Test with external tools (Blender)
- [ ] Documentation and examples

---

## Success Criteria

1. ✅ **Compilation**: All code compiles with `--features writer`
2. ✅ **Round-trip test**: Load → Save → Load produces identical Scene
3. ✅ **External validation**: Blender can load saved files without errors
4. ✅ **Format support**: Both binary and ASCII formats work
5. ✅ **Compression**: Large meshes use DEFLATE compression
6. ✅ **Documentation**: All public APIs documented
7. ✅ **Tests**: 90%+ code coverage in writer modules

---

## Potential Challenges

1. **FBX Spec Ambiguity**: Not fully documented, must reverse-engineer from files
2. **Version Differences**: FBX 6100, 7100, 7400, 7500 have subtle differences
3. **Tool Compatibility**: Each tool (Blender, Maya, etc.) has quirks
4. **Compression Edge Cases**: When to compress, threshold tuning
5. **Coordinate Systems**: Handling different axis conventions

---

## Future Enhancements (Out of Scope)

- Animation writing (keyframes, curves)
- Texture embedding (Video nodes)
- Advanced materials (PBR shaders)
- Geometry cache
- Constraints
- Cameras and lights (basic support initially)

---

## Example Usage

```rust
use ufbx::{load_file, save_file, SaveOpts, SaveFormat};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load FBX file
    let mut scene = load_file("input.fbx", &Default::default())?;

    // Modify scene
    scene.nodes[1].name = "Modified_Node".into();

    // Save as binary FBX
    save_file(&scene, "output.fbx", &SaveOpts {
        format: SaveFormat::Binary,
        version: 7400,
        compress_arrays: true,
        ..Default::default()
    })?;

    // Save as ASCII FBX for debugging
    save_file(&scene, "output_ascii.fbx", &SaveOpts {
        format: SaveFormat::Ascii,
        version: 7400,
        ..Default::default()
    })?;

    Ok(())
}
```

---

## Resources

- **Existing FBX files**: Use as reference (data/*.fbx)
- **C ufbx source**: ufbx.c contains encoding knowledge (if they add writer)
- **FBX SDK docs**: Autodesk documentation (limited)
- **Reverse engineering**: Hex dump comparisons

---

**End of Plan**
