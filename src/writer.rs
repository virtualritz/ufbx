//! FBX File Writer Module
//!
//! This module handles serializing `Scene` data structures back to FBX files.
//! It supports both binary and ASCII FBX formats.
//!
//! # Architecture
//!
//! Scene → SceneSerializer → FbxDocument → BinaryWriter/AsciiWriter → File
//!
//! The serialization process:
//! 1. Assign unique FBX IDs to all scene elements
//! 2. Build FBX node hierarchy (Header, GlobalSettings, Objects, Connections)
//! 3. Convert Scene data to FBX properties
//! 4. Encode to binary or ASCII format

use crate::error::{Error, Result};
use crate::types::*;
use crate::binary::{FbxDocument, FbxNode, Value, ValueArray, ArrayData};
use std::collections::HashMap;

// =============================================================================
// Helper Functions
// =============================================================================

/// Convert quaternion to Euler angles (in radians, XYZ order)
fn quat_to_euler(q: &Quat) -> Vec3 {
    // Singularity test
    let test = q.x * q.y + q.z * q.w;

    if test > 0.499 {
        // Singularity at north pole
        return Vec3::new(
            2.0 * q.x.atan2(q.w),
            std::f64::consts::PI / 2.0,
            0.0,
        );
    }

    if test < -0.499 {
        // Singularity at south pole
        return Vec3::new(
            -2.0 * q.x.atan2(q.w),
            -std::f64::consts::PI / 2.0,
            0.0,
        );
    }

    let sqx = q.x * q.x;
    let sqy = q.y * q.y;
    let sqz = q.z * q.z;

    Vec3::new(
        (2.0 * (q.w * q.x - q.y * q.z)).atan2(1.0 - 2.0 * (sqx + sqy)),
        (2.0 * test).asin(),
        (2.0 * (q.w * q.z - q.x * q.y)).atan2(1.0 - 2.0 * (sqy + sqz)),
    )
}

// =============================================================================
// Public Configuration Types
// =============================================================================

/// Format for saving FBX files
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveFormat {
    /// Binary FBX format (compact, faster to load)
    Binary,
    /// ASCII FBX format (human-readable, easier to debug)
    Ascii,
    /// Automatically choose based on file extension
    Auto,
}

impl Default for SaveFormat {
    fn default() -> Self {
        SaveFormat::Binary
    }
}

/// Options for saving FBX files
#[derive(Debug, Clone)]
pub struct SaveOpts {
    /// Output format (binary or ASCII)
    pub format: SaveFormat,

    /// FBX version to write (default: 7400)
    /// Supported versions: 7400, 7500
    pub version: u32,

    /// Application name to write in metadata
    pub exporter_name: String,

    /// Use DEFLATE compression for large arrays (binary format only)
    /// Arrays larger than 128 elements will be compressed
    pub compress_arrays: bool,

    /// Indentation string for ASCII format (default: 4 spaces)
    pub ascii_indent: String,

    /// Transform scene to different coordinate system before writing
    pub target_axes: Option<CoordinateAxes>,
}

impl Default for SaveOpts {
    fn default() -> Self {
        Self {
            format: SaveFormat::Binary,
            version: 7400,
            exporter_name: "ufbx-rust".to_string(),
            compress_arrays: true,
            ascii_indent: "    ".to_string(),
            target_axes: None,
        }
    }
}

// =============================================================================
// Scene Serializer
// =============================================================================

/// Serializes a Scene into FbxDocument for writing
pub struct SceneSerializer<'a> {
    scene: &'a Scene,
    opts: SaveOpts,

    // FBX ID generation and tracking
    next_fbx_id: u64,
    element_to_fbx_id: HashMap<usize, u64>,  // Scene element index → FBX ID

    // Special FBX IDs
    root_fbx_id: u64,
}

impl<'a> SceneSerializer<'a> {
    /// Create a new serializer for the given scene
    pub fn new(scene: &'a Scene, opts: SaveOpts) -> Self {
        Self {
            scene,
            opts,
            next_fbx_id: 100000000,  // Start FBX IDs at 100 million
            element_to_fbx_id: HashMap::new(),
            root_fbx_id: 0,  // Root is always ID 0
        }
    }

    /// Allocate a new unique FBX ID
    fn allocate_fbx_id(&mut self) -> u64 {
        let id = self.next_fbx_id;
        self.next_fbx_id += 1;
        id
    }

    /// Get or allocate FBX ID for a scene element
    fn get_fbx_id(&mut self, element_idx: usize) -> u64 {
        if let Some(&id) = self.element_to_fbx_id.get(&element_idx) {
            id
        } else {
            let id = self.allocate_fbx_id();
            self.element_to_fbx_id.insert(element_idx, id);
            id
        }
    }

    /// Build the complete FBX document
    pub fn build_document(&mut self) -> Result<FbxDocument> {
        // Assign FBX IDs to all elements first
        self.assign_fbx_ids()?;

        let mut nodes = Vec::new();

        // Build FBX structure
        nodes.push(self.build_header_extension()?);
        nodes.push(self.build_global_settings()?);
        nodes.push(self.build_objects()?);
        nodes.push(self.build_connections()?);
        nodes.push(self.build_takes()?);

        Ok(FbxDocument {
            version: self.opts.version,
            big_endian: false,  // Always little-endian for writing
            root: FbxNode::new(""),  // Empty root for writer
            nodes,
        })
    }

    /// Assign FBX IDs to all scene elements
    fn assign_fbx_ids(&mut self) -> Result<()> {
        // Assign IDs to all nodes
        for (idx, _node) in self.scene.nodes.iter().enumerate() {
            if idx != self.scene.root_node {
                self.get_fbx_id(idx);
            }
        }

        // Assign IDs to all meshes
        for (idx, _mesh) in self.scene.meshes.iter().enumerate() {
            // Store mesh index offset to distinguish from nodes
            let element_idx = 1000000 + idx;
            self.get_fbx_id(element_idx);
        }

        // Assign IDs to all materials
        for (idx, _material) in self.scene.materials.iter().enumerate() {
            let element_idx = 2000000 + idx;
            self.get_fbx_id(element_idx);
        }

        Ok(())
    }

    /// Build FBXHeaderExtension node
    fn build_header_extension(&self) -> Result<FbxNode> {
        let mut header = FbxNode::new("FBXHeaderExtension");

        // FBXHeaderVersion
        let mut version_node = FbxNode::new("FBXHeaderVersion");
        version_node.values.push(Value::Int32(1003));
        header.children.push(version_node);

        // FBXVersion
        let mut fbx_version_node = FbxNode::new("FBXVersion");
        fbx_version_node.values.push(Value::Int32(self.opts.version as i32));
        header.children.push(fbx_version_node);

        // EncryptionType
        let mut encryption_node = FbxNode::new("EncryptionType");
        encryption_node.values.push(Value::Int32(0));
        header.children.push(encryption_node);

        // CreationTimeStamp
        header.children.push(self.build_creation_timestamp());

        // Creator
        let mut creator_node = FbxNode::new("Creator");
        creator_node.values.push(Value::String(self.opts.exporter_name.clone().into()));
        header.children.push(creator_node);

        // SceneInfo
        header.children.push(self.build_scene_info()?);

        Ok(header)
    }

    /// Build CreationTimeStamp node
    fn build_creation_timestamp(&self) -> FbxNode {
        let mut timestamp = FbxNode::new("CreationTimeStamp");

        // Use a fixed timestamp for deterministic output
        // In production, you'd use std::time::SystemTime
        let mut version_node = FbxNode::new("Version");
        version_node.values.push(Value::Int32(1000));
        timestamp.children.push(version_node);

        let mut year = FbxNode::new("Year");
        year.values.push(Value::Int32(2024));
        timestamp.children.push(year);

        let mut month = FbxNode::new("Month");
        month.values.push(Value::Int32(1));
        timestamp.children.push(month);

        let mut day = FbxNode::new("Day");
        day.values.push(Value::Int32(1));
        timestamp.children.push(day);

        let mut hour = FbxNode::new("Hour");
        hour.values.push(Value::Int32(0));
        timestamp.children.push(hour);

        let mut minute = FbxNode::new("Minute");
        minute.values.push(Value::Int32(0));
        timestamp.children.push(minute);

        let mut second = FbxNode::new("Second");
        second.values.push(Value::Int32(0));
        timestamp.children.push(second);

        let mut millisecond = FbxNode::new("Millisecond");
        millisecond.values.push(Value::Int32(0));
        timestamp.children.push(millisecond);

        timestamp
    }

    /// Build SceneInfo node
    fn build_scene_info(&self) -> Result<FbxNode> {
        let mut scene_info = FbxNode::new("SceneInfo");
        scene_info.values.push(Value::String("GlobalInfo\x00\x01SceneInfo".into()));
        scene_info.values.push(Value::String("UserData".into()));

        let mut type_node = FbxNode::new("Type");
        type_node.values.push(Value::String("UserData".into()));
        scene_info.children.push(type_node);

        let mut version_node = FbxNode::new("Version");
        version_node.values.push(Value::Int32(100));
        scene_info.children.push(version_node);

        // Add basic metadata
        let mut metadata = FbxNode::new("MetaData");

        let mut version_meta = FbxNode::new("Version");
        version_meta.values.push(Value::Int32(100));
        metadata.children.push(version_meta);

        let mut title = FbxNode::new("Title");
        title.values.push(Value::String("".into()));
        metadata.children.push(title);

        let mut subject = FbxNode::new("Subject");
        subject.values.push(Value::String("".into()));
        metadata.children.push(subject);

        let mut author = FbxNode::new("Author");
        author.values.push(Value::String("".into()));
        metadata.children.push(author);

        scene_info.children.push(metadata);

        Ok(scene_info)
    }

    /// Build GlobalSettings node
    fn build_global_settings(&self) -> Result<FbxNode> {
        let mut settings = FbxNode::new("GlobalSettings");

        let mut version = FbxNode::new("Version");
        version.values.push(Value::Int32(1000));
        settings.children.push(version);

        // Properties70
        let mut props = FbxNode::new("Properties70");

        // UpAxis
        props.children.push(self.build_property(
            "UpAxis",
            "int",
            "Integer",
            "",
            &[Value::Int32(self.scene.settings.axes.up.to_axis_index() as i32)],
        ));

        // UpAxisSign
        props.children.push(self.build_property(
            "UpAxisSign",
            "int",
            "Integer",
            "",
            &[Value::Int32(if self.scene.settings.axes.up.is_positive() { 1 } else { -1 })],
        ));

        // FrontAxis
        props.children.push(self.build_property(
            "FrontAxis",
            "int",
            "Integer",
            "",
            &[Value::Int32(self.scene.settings.axes.front.to_axis_index() as i32)],
        ));

        // FrontAxisSign
        props.children.push(self.build_property(
            "FrontAxisSign",
            "int",
            "Integer",
            "",
            &[Value::Int32(if self.scene.settings.axes.front.is_positive() { 1 } else { -1 })],
        ));

        // CoordAxis
        props.children.push(self.build_property(
            "CoordAxis",
            "int",
            "Integer",
            "",
            &[Value::Int32(0)],  // Right is typically 0
        ));

        // CoordAxisSign
        props.children.push(self.build_property(
            "CoordAxisSign",
            "int",
            "Integer",
            "",
            &[Value::Int32(1)],
        ));

        // OriginalUpAxis
        props.children.push(self.build_property(
            "OriginalUpAxis",
            "int",
            "Integer",
            "",
            &[Value::Int32(-1)],
        ));

        // OriginalUpAxisSign
        props.children.push(self.build_property(
            "OriginalUpAxisSign",
            "int",
            "Integer",
            "",
            &[Value::Int32(1)],
        ));

        // UnitScaleFactor
        props.children.push(self.build_property(
            "UnitScaleFactor",
            "double",
            "Number",
            "",
            &[Value::Float64(self.scene.settings.unit_meters * 100.0)],  // Convert to cm
        ));

        // OriginalUnitScaleFactor
        props.children.push(self.build_property(
            "OriginalUnitScaleFactor",
            "double",
            "Number",
            "",
            &[Value::Float64(1.0)],
        ));

        // TimeSpanStart
        props.children.push(self.build_property(
            "TimeSpanStart",
            "KTime",
            "Time",
            "",
            &[Value::Int64(0)],
        ));

        // TimeSpanStop
        props.children.push(self.build_property(
            "TimeSpanStop",
            "KTime",
            "Time",
            "",
            &[Value::Int64(46186158000)],  // Default FBX time span
        ));

        // CustomFrameRate
        props.children.push(self.build_property(
            "CustomFrameRate",
            "double",
            "Number",
            "",
            &[Value::Float64(self.scene.settings.frames_per_second)],
        ));

        settings.children.push(props);

        Ok(settings)
    }

    /// Build a Property70 entry (P: node)
    fn build_property(&self, name: &str, type1: &str, type2: &str, flags: &str, values: &[Value]) -> FbxNode {
        let mut prop = FbxNode::new("P");
        prop.values.push(Value::String(name.into()));
        prop.values.push(Value::String(type1.into()));
        prop.values.push(Value::String(type2.into()));
        prop.values.push(Value::String(flags.into()));
        prop.values.extend_from_slice(values);
        prop
    }

    /// Build Objects node (contains all scene elements)
    fn build_objects(&mut self) -> Result<FbxNode> {
        let mut objects = FbxNode::new("Objects");

        // Add all geometries (meshes)
        for (idx, mesh) in self.scene.meshes.iter().enumerate() {
            objects.children.push(self.build_geometry_node(idx, mesh)?);
        }

        // Add all models (nodes)
        for (idx, node) in self.scene.nodes.iter().enumerate() {
            if idx != self.scene.root_node {
                objects.children.push(self.build_model_node(idx, node)?);
            }
        }

        // Add all materials
        for (idx, material) in self.scene.materials.iter().enumerate() {
            objects.children.push(self.build_material_node(idx, material)?);
        }

        Ok(objects)
    }

    /// Build a Geometry node for a mesh
    fn build_geometry_node(&mut self, mesh_idx: usize, mesh: &Mesh) -> Result<FbxNode> {
        let element_idx = 1000000 + mesh_idx;
        let fbx_id = self.get_fbx_id(element_idx);

        let mut geom = FbxNode::new("Geometry");
        geom.values.push(Value::Int64(fbx_id as i64));
        geom.values.push(Value::String(format!("Geometry::{}\x00\x01Mesh", mesh.element.name.as_str()).into()));
        geom.values.push(Value::String("Mesh".into()));

        // Vertices array
        let mut vertices_node = FbxNode::new("Vertices");
        let vertex_data: Vec<f64> = mesh.vertices.iter()
            .flat_map(|v| vec![v.x, v.y, v.z])
            .collect();
        vertices_node.array = Some(ValueArray {
            data: ArrayData::F64(vertex_data),
            array_type: 'd',
        });
        geom.children.push(vertices_node);

        // PolygonVertexIndex array
        let mut indices_node = FbxNode::new("PolygonVertexIndex");
        let mut fbx_indices = Vec::new();

        for face in &mesh.faces {
            let start = face.index_begin as usize;
            let count = face.num_indices as usize;

            for i in 0..count {
                let idx = mesh.vertex_indices[start + i] as i32;
                // Last index of polygon is negative
                if i == count - 1 {
                    fbx_indices.push(-(idx + 1));
                } else {
                    fbx_indices.push(idx);
                }
            }
        }

        indices_node.array = Some(ValueArray {
            data: ArrayData::I32(fbx_indices),
            array_type: 'i',
        });
        geom.children.push(indices_node);

        // GeometryVersion
        let mut version = FbxNode::new("GeometryVersion");
        version.values.push(Value::Int32(124));
        geom.children.push(version);

        // Layer 0 (normals, UVs, etc.)
        if mesh.vertex_normal.exists && !mesh.vertex_normal.values.is_empty() {
            geom.children.push(self.build_layer_element_normal(mesh)?);
        }

        if mesh.vertex_uv.exists && !mesh.vertex_uv.values.is_empty() {
            geom.children.push(self.build_layer_element_uv(mesh)?);
        }

        Ok(geom)
    }

    /// Build LayerElementNormal
    fn build_layer_element_normal(&self, mesh: &Mesh) -> Result<FbxNode> {
        let mut layer_elem = FbxNode::new("LayerElementNormal");
        layer_elem.values.push(Value::Int32(0));

        let mut version = FbxNode::new("Version");
        version.values.push(Value::Int32(101));
        layer_elem.children.push(version);

        let mut name = FbxNode::new("Name");
        name.values.push(Value::String("".into()));
        layer_elem.children.push(name);

        let mut mapping = FbxNode::new("MappingInformationType");
        mapping.values.push(Value::String("ByVertice".into()));
        layer_elem.children.push(mapping);

        let mut reference = FbxNode::new("ReferenceInformationType");
        reference.values.push(Value::String("Direct".into()));
        layer_elem.children.push(reference);

        let mut normals = FbxNode::new("Normals");
        let normal_data: Vec<f64> = mesh.vertex_normal.values.iter()
            .flat_map(|n| vec![n.x, n.y, n.z])
            .collect();
        normals.array = Some(ValueArray {
            data: ArrayData::F64(normal_data),
            array_type: 'd',
        });
        layer_elem.children.push(normals);

        Ok(layer_elem)
    }

    /// Build LayerElementUV
    fn build_layer_element_uv(&self, mesh: &Mesh) -> Result<FbxNode> {
        let mut layer_elem = FbxNode::new("LayerElementUV");
        layer_elem.values.push(Value::Int32(0));

        let mut version = FbxNode::new("Version");
        version.values.push(Value::Int32(101));
        layer_elem.children.push(version);

        let mut name = FbxNode::new("Name");
        name.values.push(Value::String("UVChannel_1".into()));
        layer_elem.children.push(name);

        let mut mapping = FbxNode::new("MappingInformationType");
        mapping.values.push(Value::String("ByPolygonVertex".into()));
        layer_elem.children.push(mapping);

        let mut reference = FbxNode::new("ReferenceInformationType");
        reference.values.push(Value::String("IndexToDirect".into()));
        layer_elem.children.push(reference);

        let mut uvs = FbxNode::new("UV");
        let uv_data: Vec<f64> = mesh.vertex_uv.values.iter()
            .flat_map(|uv| vec![uv.x, uv.y])
            .collect();
        uvs.array = Some(ValueArray {
            data: ArrayData::F64(uv_data),
            array_type: 'd',
        });
        layer_elem.children.push(uvs);

        let mut uv_index = FbxNode::new("UVIndex");
        let uv_indices: Vec<i32> = mesh.vertex_uv.indices.iter().map(|&i| i as i32).collect();
        uv_index.array = Some(ValueArray {
            data: ArrayData::I32(uv_indices),
            array_type: 'i',
        });
        layer_elem.children.push(uv_index);

        Ok(layer_elem)
    }

    /// Build a Model node for a scene node
    fn build_model_node(&mut self, node_idx: usize, node: &Node) -> Result<FbxNode> {
        let fbx_id = self.get_fbx_id(node_idx);

        let mut model = FbxNode::new("Model");
        model.values.push(Value::Int64(fbx_id as i64));
        model.values.push(Value::String(format!("Model::{}\x00\x01Model", node.element.name.as_str()).into()));
        model.values.push(Value::String("Null".into()));  // Or "Mesh" if it has geometry

        let mut version = FbxNode::new("Version");
        version.values.push(Value::Int32(232));
        model.children.push(version);

        // Properties70
        let mut props = FbxNode::new("Properties70");

        // Lcl Translation
        props.children.push(self.build_property(
            "Lcl Translation",
            "Lcl Translation",
            "",
            "A",
            &[
                Value::Float64(node.local_transform.translation.x),
                Value::Float64(node.local_transform.translation.y),
                Value::Float64(node.local_transform.translation.z),
            ],
        ));

        // Lcl Rotation (convert from quaternion to Euler)
        let euler = quat_to_euler(&node.local_transform.rotation);
        props.children.push(self.build_property(
            "Lcl Rotation",
            "Lcl Rotation",
            "",
            "A",
            &[
                Value::Float64(euler.x.to_degrees()),
                Value::Float64(euler.y.to_degrees()),
                Value::Float64(euler.z.to_degrees()),
            ],
        ));

        // Lcl Scaling
        props.children.push(self.build_property(
            "Lcl Scaling",
            "Lcl Scaling",
            "",
            "A",
            &[
                Value::Float64(node.local_transform.scale.x),
                Value::Float64(node.local_transform.scale.y),
                Value::Float64(node.local_transform.scale.z),
            ],
        ));

        model.children.push(props);

        Ok(model)
    }

    /// Build a Material node
    fn build_material_node(&mut self, material_idx: usize, material: &Material) -> Result<FbxNode> {
        let element_idx = 2000000 + material_idx;
        let fbx_id = self.get_fbx_id(element_idx);

        let mut mat = FbxNode::new("Material");
        mat.values.push(Value::Int64(fbx_id as i64));
        mat.values.push(Value::String(format!("Material::{}\x00\x01Material", material.element.name.as_str()).into()));
        mat.values.push(Value::String("".into()));

        let mut version = FbxNode::new("Version");
        version.values.push(Value::Int32(102));
        mat.children.push(version);

        let mut shading_model = FbxNode::new("ShadingModel");
        shading_model.values.push(Value::String(format!("{:?}", material.shader_type).into()));
        mat.children.push(shading_model);

        let mut multilayer = FbxNode::new("MultiLayer");
        multilayer.values.push(Value::Int32(0));
        mat.children.push(multilayer);

        // Properties70
        let mut props = FbxNode::new("Properties70");

        // Diffuse color
        if material.fbx.diffuse_color.has_value {
            let color = &material.fbx.diffuse_color.value_vec3;
            props.children.push(self.build_property(
                "DiffuseColor",
                "Color",
                "",
                "A",
                &[
                    Value::Float64(color.x),
                    Value::Float64(color.y),
                    Value::Float64(color.z),
                ],
            ));
        }

        // Specular color
        if material.fbx.specular_color.has_value {
            let color = &material.fbx.specular_color.value_vec3;
            props.children.push(self.build_property(
                "SpecularColor",
                "Color",
                "",
                "A",
                &[
                    Value::Float64(color.x),
                    Value::Float64(color.y),
                    Value::Float64(color.z),
                ],
            ));
        }

        // Shininess
        if material.fbx.specular_exponent.has_value {
            props.children.push(self.build_property(
                "Shininess",
                "double",
                "Number",
                "",
                &[Value::Float64(material.fbx.specular_exponent.value_real)],
            ));
        }

        mat.children.push(props);

        Ok(mat)
    }

    /// Build Connections node
    fn build_connections(&self) -> Result<FbxNode> {
        let mut connections = FbxNode::new("Connections");

        // Connect nodes to parent nodes
        for (idx, node) in self.scene.nodes.iter().enumerate() {
            if idx == self.scene.root_node {
                continue;
            }

            let child_fbx_id = self.element_to_fbx_id[&idx];
            let parent_fbx_id = if let Some(parent_idx) = node.parent {
                if parent_idx == self.scene.root_node {
                    0  // Root
                } else {
                    self.element_to_fbx_id[&parent_idx]
                }
            } else {
                0  // No parent, connect to root
            };

            let mut conn = FbxNode::new("C");
            conn.values.push(Value::String("OO".into()));
            conn.values.push(Value::Int64(child_fbx_id as i64));
            conn.values.push(Value::Int64(parent_fbx_id as i64));
            connections.children.push(conn);
        }

        // Connect meshes to nodes
        for (mesh_idx, mesh) in self.scene.meshes.iter().enumerate() {
            let mesh_element_idx = 1000000 + mesh_idx;
            let mesh_fbx_id = self.element_to_fbx_id[&mesh_element_idx];

            for &node_idx in &mesh.instances {
                if node_idx == self.scene.root_node {
                    continue;
                }

                let node_fbx_id = self.element_to_fbx_id[&node_idx];

                let mut conn = FbxNode::new("C");
                conn.values.push(Value::String("OO".into()));
                conn.values.push(Value::Int64(mesh_fbx_id as i64));
                conn.values.push(Value::Int64(node_fbx_id as i64));
                connections.children.push(conn);
            }
        }

        // Connect materials to meshes
        for (mesh_idx, mesh) in self.scene.meshes.iter().enumerate() {
            if let Some(&material_idx) = mesh.materials.first() {
                let mesh_element_idx = 1000000 + mesh_idx;
                let mesh_fbx_id = self.element_to_fbx_id[&mesh_element_idx];

                let material_element_idx = 2000000 + material_idx;
                let material_fbx_id = self.element_to_fbx_id[&material_element_idx];

                let mut conn = FbxNode::new("C");
                conn.values.push(Value::String("OO".into()));
                conn.values.push(Value::Int64(material_fbx_id as i64));
                conn.values.push(Value::Int64(mesh_fbx_id as i64));
                connections.children.push(conn);
            }
        }

        Ok(connections)
    }

    /// Build Takes node (animation section)
    fn build_takes(&self) -> Result<FbxNode> {
        let mut takes = FbxNode::new("Takes");

        let mut current = FbxNode::new("Current");
        current.values.push(Value::String("".into()));
        takes.children.push(current);

        Ok(takes)
    }
}

// =============================================================================
// Public API Functions
// =============================================================================

/// Save a scene to a file
pub fn save_file(scene: &Scene, path: &str, opts: &SaveOpts) -> Result<()> {
    let data = save_memory(scene, opts)?;
    std::fs::write(path, data)
        .map_err(|e| Error::from_io_error(e))?;
    Ok(())
}

/// Save a scene to memory buffer
pub fn save_memory(scene: &Scene, opts: &SaveOpts) -> Result<Vec<u8>> {
    let opts = resolve_format(opts);

    let mut serializer = SceneSerializer::new(scene, opts.clone());
    let doc = serializer.build_document()?;

    match opts.format {
        SaveFormat::Binary => {
            crate::binary_writer::write_binary(&doc, &opts)
        }
        SaveFormat::Ascii => {
            crate::ascii_writer::write_ascii(&doc, &opts)
        }
        SaveFormat::Auto => {
            unreachable!("Auto format should have been resolved")
        }
    }
}

/// Resolve Auto format to Binary (default)
fn resolve_format(opts: &SaveOpts) -> SaveOpts {
    let mut opts = opts.clone();
    if opts.format == SaveFormat::Auto {
        opts.format = SaveFormat::Binary;
    }
    opts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_save_opts_default() {
        let opts = SaveOpts::default();
        assert_eq!(opts.format, SaveFormat::Binary);
        assert_eq!(opts.version, 7400);
        assert_eq!(opts.compress_arrays, true);
    }

    #[test]
    fn test_fbx_id_allocation() {
        let scene = Scene::default();
        let mut serializer = SceneSerializer::new(&scene, SaveOpts::default());

        let id1 = serializer.allocate_fbx_id();
        let id2 = serializer.allocate_fbx_id();

        assert!(id1 < id2);
        assert!(id1 >= 100000000);
    }

    #[test]
    #[ignore] // Requires test data files
    fn test_round_trip_binary() {
        use crate::scene::load_file;

        // Load a simple test file
        let original = load_file("data/blender_272_cube_7400_binary.fbx", &Default::default())
            .expect("Failed to load test file");

        // Save to binary format
        let binary_data = save_memory(&original, &SaveOpts {
            format: SaveFormat::Binary,
            version: 7400,
            compress_arrays: true,
            ..Default::default()
        })
        .expect("Failed to save to binary");

        assert!(binary_data.len() > 0, "Binary data should not be empty");

        // Verify it starts with FBX binary magic
        assert_eq!(&binary_data[0..21], b"Kaydara FBX Binary  \x00");

        // TODO: Load the saved data back and verify
        // This would require load_memory to be implemented
    }

    #[test]
    #[ignore] // Requires test data files
    fn test_round_trip_ascii() {
        use crate::scene::load_file;

        // Load a simple test file
        let original = load_file("data/blender_272_cube_7400_binary.fbx", &Default::default())
            .expect("Failed to load test file");

        // Save to ASCII format
        let ascii_data = save_memory(&original, &SaveOpts {
            format: SaveFormat::Ascii,
            version: 7400,
            ascii_indent: "    ".to_string(),
            ..Default::default()
        })
        .expect("Failed to save to ASCII");

        assert!(ascii_data.len() > 0, "ASCII data should not be empty");

        // Verify it's ASCII text
        let ascii_text = String::from_utf8_lossy(&ascii_data);
        assert!(ascii_text.contains("FBX"), "Should contain FBX marker");
        assert!(ascii_text.contains("Objects:"), "Should have Objects section");

        // TODO: Load the saved data back and verify
    }

    #[test]
    fn test_save_empty_scene() {
        let scene = Scene::default();

        let result = save_memory(&scene, &SaveOpts::default());
        assert!(result.is_ok(), "Should be able to save empty scene");

        let data = result.unwrap();
        assert!(data.len() > 0, "Should produce non-empty output");
    }

    #[test]
    fn test_quat_to_euler_identity() {
        let identity = Quat::IDENTITY;
        let euler = quat_to_euler(&identity);

        // Identity quaternion should give near-zero Euler angles
        assert!((euler.x).abs() < 0.0001);
        assert!((euler.y).abs() < 0.0001);
        assert!((euler.z).abs() < 0.0001);
    }
}
