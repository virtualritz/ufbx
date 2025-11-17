//! Scene Construction Module
//!
//! This is the **CRITICAL PATH** - converting parsed FBX DOM nodes into the usable Scene structure.
//!
//! # Responsibilities
//!
//! 1. **DOM → Scene Conversion** - Take parsed nodes from binary/ASCII parsers and build Scene, Node, Mesh, Material, etc.
//! 2. **Object Resolution** - Resolve element IDs to references, build parent/child relationships
//! 3. **Property Interpretation** - Extract transform data, material properties, mesh vertices
//! 4. **Hierarchy Building** - Build scene graph from node relationships, apply transforms
//! 5. **Data Sanitization** - Validate indices, ensure UTF-8 strings, handle malformed data
//!
//! # Design
//!
//! This module uses a builder pattern to construct the scene in multiple passes:
//! - Pass 1: Parse all DOM nodes into raw elements with FBX IDs
//! - Pass 2: Resolve connections between elements
//! - Pass 3: Build hierarchies and attach attributes to nodes
//! - Pass 4: Finalize data (compute transforms, validate indices, etc.)

use crate::error::{Error, Result};
use crate::types::*;
use crate::binary::{FbxDocument, FbxNode as BinaryNode};
use crate::ascii::AsciiNode;
use std::collections::HashMap;

// =============================================================================
// Scene Builder - Main Entry Point
// =============================================================================

/// Builder for constructing a Scene from parsed FBX data.
///
/// This handles the complex multi-pass process of:
/// 1. Creating elements from DOM nodes
/// 2. Resolving ID references to element indices
/// 3. Building hierarchies
/// 4. Finalizing data structures
pub struct SceneBuilder {
    // Scene being built
    scene: Scene,

    // Element tracking
    elements: Vec<ElementData>,
    element_map: HashMap<u64, usize>,  // FBX ID → element index

    // Connection tracking
    connections: Vec<TempConnection>,

    // Version and metadata
    version: u32,
    is_ascii: bool,
    exporter: Exporter,

    // Processing options
    opts: SceneOpts,

    // Temporary data for building
    node_indices: Vec<usize>,  // Indices into scene.nodes
    template_props: HashMap<String, Props>,  // Property templates by type
}

/// Temporary connection data before resolution
#[derive(Debug, Clone)]
struct TempConnection {
    src_fbx_id: u64,
    dst_fbx_id: u64,
    src_prop: Option<String>,
    dst_prop: Option<String>,
}

/// Temporary element data during construction
#[derive(Debug, Clone)]
struct ElementData {
    fbx_id: u64,
    element_type: ElementType,
    type_name: String,
    sub_type: String,
    name: String,
    props: Props,
    fbx_node: Option<BinaryNode>,  // Store original FBX node for geometry extraction
}

/// Options for scene construction
#[derive(Debug, Clone)]
pub struct SceneOpts {
    /// How to handle geometry transforms (offset from origin)
    pub geometry_transform_handling: GeometryTransformHandling,

    /// How to handle non-standard inheritance modes
    pub inherit_mode_handling: InheritModeHandling,

    /// How to handle pivot points
    pub pivot_handling: PivotHandling,

    /// Generate normals if missing
    pub generate_missing_normals: bool,

    /// Ignore animation data to save memory
    pub ignore_animation: bool,

    /// Ignore embedded data (textures, etc.) to save memory
    pub ignore_embedded: bool,

    /// Strict mode - fail on any inconsistency
    pub strict: bool,
}

impl Default for SceneOpts {
    fn default() -> Self {
        Self {
            geometry_transform_handling: GeometryTransformHandling::PreserveNodes,
            inherit_mode_handling: InheritModeHandling::Helper,
            pivot_handling: PivotHandling::Retain,
            generate_missing_normals: true,
            ignore_animation: false,
            ignore_embedded: false,
            strict: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeometryTransformHandling {
    PreserveNodes,     // Keep geometry transform as separate helper nodes
    ModifyGeometry,    // Apply transform to geometry vertices directly
    Preserve,          // Keep transform in place (may break some tools)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InheritModeHandling {
    Helper,            // Use helper nodes for non-standard modes
    Preserve,          // Keep original (may not work in all engines)
    Compensate,        // Mathematically compensate
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PivotHandling {
    Retain,            // Keep pivot transforms as-is
    AdjustToPivot,     // Adjust transforms to match pivot points
}

impl SceneBuilder {
    /// Create a new scene builder with default options
    pub fn new() -> Self {
        Self::with_opts(SceneOpts::default())
    }

    /// Create a new scene builder with custom options
    pub fn with_opts(opts: SceneOpts) -> Self {
        Self {
            scene: Scene {
                metadata: Metadata {
                    version: 0,
                    file_format: FileFormat::Fbx,
                    exporter: Exporter::Unknown,
                    exporter_version: 0,
                    creator: FbxString::new(""),
                    is_big_endian: false,
                    filename: FbxString::new(""),
                    relative_root: FbxString::new(""),
                    raw_filename: Blob::new(vec![]),
                    raw_relative_root: Blob::new(vec![]),
                    ascii: false,
                    ktime: 46186158000, // Default FBX 7.x KTime
                    original_file_path: FbxString::new(""),
                },
                settings: SceneSettings {
                    axes: CoordinateAxes {
                        right: CoordinateAxis::PositiveX,
                        up: CoordinateAxis::PositiveY,
                        front: CoordinateAxis::PositiveZ,
                    },
                    unit_meters: 1.0,
                    frames_per_second: 30.0,
                    ambient_color: Vec3::ZERO,
                    default_camera: FbxString::new(""),
                    original_axis_up: CoordinateAxis::PositiveY,
                    original_unit_meters: 1.0,
                    space_scale: 1.0,
                    time_mode: 0,
                    time_protocol: 0,
                    snap_mode: 0,
                },
                root_node: 0,
                anim: None,
                unknowns: vec![],
                nodes: vec![],
                meshes: vec![],
                lights: vec![],
                cameras: vec![],
                materials: vec![],
                textures: vec![],
                texture_files: vec![],
                skin_deformers: vec![],
                skin_clusters: vec![],
                blend_deformers: vec![],
                blend_channels: vec![],
                blend_shapes: vec![],
                anim_stacks: vec![],
                anim_layers: vec![],
                anim_values: vec![],
                anim_curves: vec![],
                connections: vec![],
            },
            elements: vec![],
            element_map: HashMap::new(),
            connections: vec![],
            version: 0,
            is_ascii: false,
            exporter: Exporter::Unknown,
            opts,
            node_indices: vec![],
            template_props: HashMap::new(),
        }
    }

    /// Build a scene from a binary FBX document
    pub fn from_binary(doc: FbxDocument, opts: SceneOpts) -> Result<Scene> {
        let mut builder = Self::with_opts(opts);
        builder.version = doc.version;
        builder.is_ascii = false;
        builder.scene.metadata.version = doc.version;
        builder.scene.metadata.ascii = false;

        // Read the document structure
        builder.read_binary_header(&doc)?;
        builder.process_binary_nodes(&doc.root.children)?;
        builder.resolve_connections()?;
        builder.build_hierarchy()?;
        builder.finalize()?;

        Ok(builder.scene)
    }

    /// Build a scene from ASCII FBX nodes
    pub fn from_ascii(nodes: Vec<AsciiNode>, version: u32, opts: SceneOpts) -> Result<Scene> {
        let mut builder = Self::with_opts(opts);
        builder.version = version;
        builder.is_ascii = true;
        builder.scene.metadata.version = version;
        builder.scene.metadata.ascii = true;

        // Process nodes
        builder.process_ascii_nodes(&nodes)?;
        builder.resolve_connections()?;
        builder.build_hierarchy()?;
        builder.finalize()?;

        Ok(builder.scene)
    }

    // =========================================================================
    // Binary Processing
    // =========================================================================

    fn read_binary_header(&mut self, _doc: &FbxDocument) -> Result<()> {
        // TODO: Extract metadata from header extension nodes
        // - FBXHeaderExtension/FBXVersion
        // - FBXHeaderExtension/Creator
        // - FBXHeaderExtension/SceneInfo
        Ok(())
    }

    fn process_binary_nodes(&mut self, nodes: &[BinaryNode]) -> Result<()> {
        for node in nodes {
            self.process_binary_node(node)?;
        }
        Ok(())
    }

    fn process_binary_node(&mut self, node: &BinaryNode) -> Result<()> {
        match node.name.as_str() {
            "Objects" => self.process_objects_binary(&node.children)?,
            "Connections" => self.process_connections_binary(&node.children)?,
            "Definitions" => self.process_definitions_binary(&node.children)?,
            "GlobalSettings" => self.process_global_settings_binary(node)?,
            _ => {
                // Unknown top-level node, skip
            }
        }
        Ok(())
    }

    fn process_objects_binary(&mut self, nodes: &[BinaryNode]) -> Result<()> {
        for node in nodes {
            self.create_element_from_binary(node)?;
        }
        Ok(())
    }

    fn create_element_from_binary(&mut self, node: &BinaryNode) -> Result<()> {
        // FBX objects are of form: TypeName: FbxId, "Name::Type", SubType
        let type_name = node.name.clone();

        // First property is always the FBX ID
        let fbx_id = if !node.values.is_empty() {
            // Parse FBX ID from first property
            node.values[0].as_i64()
                .ok_or_else(|| Error::unknown("FBX ID is not a number"))? as u64
        } else {
            return Err(Error::unknown("Object node missing FBX ID"));
        };

        // Second property is name (may be "Name::Type" in ASCII, "Name\x00\x01Type" in binary)
        let (name, sub_type) = if node.values.len() > 1 {
            // Extract name string and split on \x00\x01 separator for binary files
            if let Some(name_str) = node.values[1].as_string() {
                // Check for binary separator \x00\x01
                if let Some(sep_idx) = name_str.find("\x00\x01") {
                    let name = name_str[..sep_idx].to_string();
                    let type_part = name_str[sep_idx + 2..].to_string();
                    (name, type_part)
                } else if let Some(sep_idx) = name_str.find("::") {
                    // ASCII separator ::
                    let name = name_str[..sep_idx].to_string();
                    let type_part = name_str[sep_idx + 2..].to_string();
                    (name, type_part)
                } else {
                    (name_str.to_string(), "".to_string())
                }
            } else {
                ("".to_string(), "".to_string())
            }
        } else {
            ("".to_string(), "".to_string())
        };

        // Third property (if present) is sub-type
        let sub_type = if node.values.len() > 2 {
            // Extract sub-type from third property
            if let Some(type_str) = node.values[2].as_string() {
                type_str.to_string()
            } else {
                sub_type
            }
        } else {
            sub_type
        };

        // Read properties from "Properties70" child node
        let props = self.read_properties_binary(node)?;

        // Create element data
        let element_type = Self::name_to_element_type(&type_name, &sub_type);
        let element_idx = self.elements.len();

        self.elements.push(ElementData {
            fbx_id,
            element_type,
            type_name,
            sub_type,
            name,
            props,
            fbx_node: Some(node.clone()),  // Store for geometry extraction
        });

        self.element_map.insert(fbx_id, element_idx);

        Ok(())
    }

    fn read_properties_binary(&mut self, node: &BinaryNode) -> Result<Props> {
        // Look for Properties70 child node
        for child in &node.children {
            if child.name == "Properties70" {
                return self.parse_properties70_binary(child);
            }
        }
        Ok(Props::new())
    }

    fn parse_properties70_binary(&mut self, node: &BinaryNode) -> Result<Props> {
        let mut props = Props::new();

        for child in &node.children {
            if child.name == "P" {
                let prop = self.parse_property_binary(child)?;
                props.props.push(prop);
            }
        }

        // Sort properties by name for binary search
        props.props.sort_by(|a, b| a.name.as_str().cmp(b.name.as_str()));

        Ok(props)
    }

    fn parse_property_binary(&mut self, node: &BinaryNode) -> Result<Prop> {
        // Property format: P: "Name", "Type", "SubType", "Flags", Value1, Value2, ...
        if node.values.len() < 4 {
            return Err(Error::unknown("Property node has too few values"));
        }

        let name = node.values[0].as_string()
            .ok_or_else(|| Error::unknown("Property name is not a string"))?
            .to_string();

        let type_str = node.values[1].as_string()
            .unwrap_or("");

        let _flags_str = node.values[3].as_string().unwrap_or("");

        // Parse property value based on type
        let value = if node.values.len() >= 5 {
            match type_str {
                "Vector3D" | "Vector" | "Lcl Translation" | "Lcl Rotation" | "Lcl Scaling"
                | "Color" | "ColorRGB" => {
                    // Vec3 - read 3 doubles/floats
                    let x = node.values.get(4).and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let y = node.values.get(5).and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let z = node.values.get(6).and_then(|v| v.as_f64()).unwrap_or(0.0);
                    PropValue::Vec3(Vec3::new(x, y, z))
                }
                "Vector2D" => {
                    let x = node.values.get(4).and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let y = node.values.get(5).and_then(|v| v.as_f64()).unwrap_or(0.0);
                    PropValue::Vec2(Vec2::new(x, y))
                }
                "Vector4D" | "ColorRGBA" => {
                    let x = node.values.get(4).and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let y = node.values.get(5).and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let z = node.values.get(6).and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let w = node.values.get(7).and_then(|v| v.as_f64()).unwrap_or(0.0);
                    PropValue::Vec4(Vec4::new(x, y, z, w))
                }
                "bool" | "Bool" => {
                    let val = node.values[4].as_i64().unwrap_or(0);
                    PropValue::Bool(val != 0)
                }
                "int" | "Integer" | "enum" | "Enum" => {
                    let val = node.values[4].as_i64().unwrap_or(0);
                    PropValue::Integer(val)
                }
                "double" | "Double" | "Number" | "Float" | "double2" | "double3" | "double4" => {
                    let val = node.values[4].as_f64().unwrap_or(0.0);
                    PropValue::Number(val)
                }
                "KString" | "String" | "object" | "KTime" | "DateTime" | "Compound" => {
                    if let Some(s) = node.values[4].as_string() {
                        PropValue::String(FbxString::new(s))
                    } else {
                        PropValue::String(FbxString::new(""))
                    }
                }
                _ => {
                    // Default: try number
                    if let Some(val) = node.values[4].as_f64() {
                        PropValue::Number(val)
                    } else if let Some(s) = node.values[4].as_string() {
                        PropValue::String(FbxString::new(s))
                    } else {
                        PropValue::Integer(node.values[4].as_i64().unwrap_or(0))
                    }
                }
            }
        } else {
            PropValue::Integer(0)
        };

        Ok(Prop {
            name: FbxString::new(name),
            value,
            flags: PropFlags {
                animated: false,
                user_defined: false,
                hidden: false,
                lock: false,
                mute: false,
                synthetic: false,
                no_value: false,
                not_found: false,
                connected: false,
                overridden: false,
            },
        })
    }

    fn process_connections_binary(&mut self, nodes: &[BinaryNode]) -> Result<()> {
        for node in nodes {
            if node.name == "C" {
                self.parse_connection_binary(node)?;
            }
        }
        Ok(())
    }

    fn parse_connection_binary(&mut self, node: &BinaryNode) -> Result<()> {
        // Connection format: C: "OO"|"OP"|"PP", SrcId, DstId [, SrcProp, DstProp]
        if node.values.len() < 3 {
            return Err(Error::unknown("Connection node has too few properties"));
        }

        // Parse connection type (OO = Object-Object, OP = Object-Property, PP = Property-Property)
        let _conn_type = node.values[0].as_string().unwrap_or("OO");

        // Parse source and destination IDs
        let src_fbx_id = node.values[1].as_i64()
            .ok_or_else(|| Error::unknown("Connection source ID is not a number"))? as u64;
        let dst_fbx_id = node.values[2].as_i64()
            .ok_or_else(|| Error::unknown("Connection destination ID is not a number"))? as u64;

        // Parse optional property names
        let src_prop = if node.values.len() > 3 {
            node.values[3].as_string().map(|s| s.to_string())
        } else {
            None
        };

        let dst_prop = if node.values.len() > 4 {
            node.values[4].as_string().map(|s| s.to_string())
        } else {
            None
        };

        let conn = TempConnection {
            src_fbx_id,
            dst_fbx_id,
            src_prop,
            dst_prop,
        };

        self.connections.push(conn);
        Ok(())
    }

    fn process_definitions_binary(&mut self, nodes: &[BinaryNode]) -> Result<()> {
        // Definitions contain property templates for different element types
        for node in nodes {
            if node.name == "ObjectType" {
                self.parse_object_type_binary(node)?;
            }
        }
        Ok(())
    }

    fn parse_object_type_binary(&mut self, _node: &BinaryNode) -> Result<()> {
        // TODO: Parse property templates
        Ok(())
    }

    fn process_global_settings_binary(&mut self, _node: &BinaryNode) -> Result<()> {
        // TODO: Parse global settings
        Ok(())
    }

    // =========================================================================
    // ASCII Processing (similar structure to binary)
    // =========================================================================


    fn process_ascii_nodes(&mut self, nodes: &[AsciiNode]) -> Result<()> {
        for node in nodes {
            self.process_ascii_node(node)?;
        }
        Ok(())
    }

    fn process_ascii_node(&mut self, node: &AsciiNode) -> Result<()> {
        match node.name.as_str() {
            "Objects" => self.process_objects_ascii(&node.children)?,
            "Connections" => self.process_connections_ascii(&node.children)?,
            "Definitions" => self.process_definitions_ascii(&node.children)?,
            "GlobalSettings" => self.process_global_settings_ascii(node)?,
            _ => {}
        }
        Ok(())
    }

    fn process_objects_ascii(&mut self, _nodes: &[AsciiNode]) -> Result<()> {
        // TODO: Similar to binary
        Ok(())
    }

    fn process_connections_ascii(&mut self, _nodes: &[AsciiNode]) -> Result<()> {
        // TODO: Similar to binary
        Ok(())
    }

    fn process_definitions_ascii(&mut self, _nodes: &[AsciiNode]) -> Result<()> {
        // TODO: Similar to binary
        Ok(())
    }

    fn process_global_settings_ascii(&mut self, _node: &AsciiNode) -> Result<()> {
        // TODO: Similar to binary
        Ok(())
    }

    // =========================================================================
    // Connection Resolution
    // =========================================================================

    fn resolve_connections(&mut self) -> Result<()> {
        // Convert temporary connections (with FBX IDs) to final connections (with element indices)
        let mut resolved_connections = Vec::new();

        for conn in &self.connections {
            // Look up source and destination elements
            let src_idx = self.element_map.get(&conn.src_fbx_id);
            let dst_idx = self.element_map.get(&conn.dst_fbx_id);

            if let (Some(&src), Some(&dst)) = (src_idx, dst_idx) {
                resolved_connections.push(Connection {
                    src,
                    dst,
                    src_prop: conn.src_prop.as_ref().map(|s| FbxString::new(s.clone())),
                    dst_prop: conn.dst_prop.as_ref().map(|s| FbxString::new(s.clone())),
                });
            } else if self.opts.strict {
                return Err(Error::unknown("Connection references non-existent element"));
            }
            // In non-strict mode, skip invalid connections
        }

        self.scene.connections = resolved_connections;

        // Sort connections for efficient lookup by source
        self.scene.connections.sort_by_key(|c| (c.src, c.dst));

        Ok(())
    }

    // =========================================================================
    // Hierarchy Building
    // =========================================================================

    fn build_hierarchy(&mut self) -> Result<()> {
        // Create root node if it doesn't exist
        self.ensure_root_node()?;

        // Process all elements and create typed structures
        self.create_typed_elements()?;

        // Build parent-child relationships for nodes
        self.build_node_hierarchy()?;

        // Attach attributes (meshes, lights, etc.) to nodes
        self.attach_attributes()?;

        Ok(())
    }

    fn ensure_root_node(&mut self) -> Result<()> {
        // Find or create the root node (FBX ID 0)
        if !self.element_map.contains_key(&0) {
            // Create synthetic root node
            let root_idx = self.scene.nodes.len();
            self.scene.nodes.push(Node {
                element: Element::new("RootNode", ElementType::Node),
                parent: None,
                children: vec![],
                mesh: None,
                light: None,
                camera: None,
                bone: None,
                attrib: None,
                attrib_type: ElementType::Unknown,
                all_attribs: vec![],
                geometry_transform_helper: None,
                scale_helper: None,
                local_transform: Transform::IDENTITY,
                geometry_transform: Transform::IDENTITY,
                node_to_parent: Matrix::IDENTITY,
                node_to_world: Matrix::IDENTITY,
                geometry_to_node: Matrix::IDENTITY,
                geometry_to_world: Matrix::IDENTITY,
                inherit_mode: InheritMode::Normal,
                is_root: true,
                is_geometry_transform_helper: false,
                is_scale_helper: false,
                visible: true,
                rotation_order: RotationOrder::XYZ,
                euler_rotation: Vec3::ZERO,
                original_inherit_mode: InheritMode::Normal,
                inherit_scale: Vec3::ONE,
                inherit_scale_node: None,
                unscaled_node_to_world: Matrix::IDENTITY,
                adjust_pre_translation: Vec3::ZERO,
                adjust_pre_rotation: Quat::IDENTITY,
                adjust_pre_scale: 1.0,
                adjust_post_rotation: Quat::IDENTITY,
                adjust_post_scale: 1.0,
                adjust_translation_scale: 1.0,
                adjust_mirror_axis: MirrorAxis::None,
                materials: vec![],
                bind_pose: None,
                has_geometry_transform: false,
                has_adjust_transform: false,
                has_root_adjust_transform: false,
            is_scale_compensate_parent: false,
            node_depth: 0,
            });
            self.scene.root_node = root_idx;
            self.element_map.insert(0, root_idx);
        }
        Ok(())
    }

    fn create_typed_elements(&mut self) -> Result<()> {
        // Convert raw ElementData into typed structures (Node, Mesh, Material, etc.)
        // Clone the elements to avoid borrow checker issues
        let elements_copy = self.elements.clone();
        for data in &elements_copy {
            match data.element_type {
                ElementType::Node => self.create_node(data)?,
                ElementType::Mesh => self.create_mesh(data)?,
                ElementType::Light => self.create_light(data)?,
                ElementType::Camera => self.create_camera(data)?,
                ElementType::Material => self.create_material(data)?,
                _ => {
                    // For now, skip unknown types or add to unknowns list
                    if self.opts.strict {
                        return Err(Error::unknown(format!("Unknown element type: {:?}", data.element_type)));
                    }
                }
            }
        }
        Ok(())
    }

    fn create_node(&mut self, data: &ElementData) -> Result<()> {
        let node = Node {
            element: Element {
                name: FbxString::new(data.name.clone()),
                props: data.props.clone(),
                element_id: self.scene.nodes.len() as u32,
                typed_id: self.scene.nodes.len() as u32,
                element_type: ElementType::Node,
                connections_src: vec![],
                connections_dst: vec![],
            },
            parent: None,
            children: vec![],
            mesh: None,
            light: None,
            camera: None,
            bone: None,
            attrib: None,
            attrib_type: ElementType::Unknown,
            all_attribs: vec![],
            geometry_transform_helper: None,
            scale_helper: None,
            local_transform: self.extract_local_transform(&data.props),
            geometry_transform: Transform::IDENTITY,
            node_to_parent: Matrix::IDENTITY,
            node_to_world: Matrix::IDENTITY,
            geometry_to_node: Matrix::IDENTITY,
            geometry_to_world: Matrix::IDENTITY,
            inherit_mode: InheritMode::Normal,
            original_inherit_mode: InheritMode::Normal,
            inherit_scale: Vec3::ONE,
            inherit_scale_node: None,
            is_root: false,
            is_geometry_transform_helper: false,
            is_scale_helper: false,
            visible: true,
            rotation_order: RotationOrder::XYZ,
            euler_rotation: Vec3::ZERO,
            unscaled_node_to_world: Matrix::IDENTITY,
            adjust_pre_translation: Vec3::ZERO,
            adjust_pre_rotation: Quat::IDENTITY,
            adjust_pre_scale: 1.0,
            adjust_post_rotation: Quat::IDENTITY,
            adjust_post_scale: 1.0,
            adjust_translation_scale: 1.0,
            adjust_mirror_axis: MirrorAxis::None,
            materials: vec![],
            bind_pose: None,
            has_geometry_transform: false,
            has_adjust_transform: false,
            has_root_adjust_transform: false,
            is_scale_compensate_parent: false,
            node_depth: 0,
        };

        let idx = self.scene.nodes.len();
        self.scene.nodes.push(node);
        self.node_indices.push(idx);

        Ok(())
    }

    fn create_mesh(&mut self, data: &ElementData) -> Result<()> {
        // Extract mesh geometry data from the FBX node
        let (vertices, polygon_vertex_indices, normals, uvs, colors, faces, vertex_indices) =
            if let Some(ref fbx_node) = data.fbx_node {
                self.extract_mesh_geometry(fbx_node)?
            } else {
                // No FBX node (might be from ASCII), create empty mesh
                (vec![], vec![], vec![], vec![], vec![], vec![], vec![])
            };

        let num_vertices = vertices.len();
        let num_indices = vertex_indices.len();
        let num_faces = faces.len();

        // Build vertex position attribute
        let vertex_position = if !vertices.is_empty() {
            VertexAttrib {
                exists: true,
                values: vertices.clone(),
                indices: (0..num_vertices as u32).collect(),
                value_reals: 3,
                unique_per_vertex: true,
                values_w: vec![],
            }
        } else {
            VertexAttrib::default()
        };

        // Build vertex normal attribute
        let vertex_normal = if !normals.is_empty() {
            VertexAttrib {
                exists: true,
                values: normals,
                indices: (0..num_indices as u32).collect(),
                value_reals: 3,
                unique_per_vertex: false,
                values_w: vec![],
            }
        } else {
            VertexAttrib::default()
        };

        // Build vertex UV attribute
        let vertex_uv = if !uvs.is_empty() {
            VertexAttrib {
                exists: true,
                values: uvs,
                indices: (0..num_indices as u32).collect(),
                value_reals: 2,
                unique_per_vertex: false,
                values_w: vec![],
            }
        } else {
            VertexAttrib::default()
        };

        // Build vertex color attribute
        let vertex_color = if !colors.is_empty() {
            VertexAttrib {
                exists: true,
                values: colors,
                indices: (0..num_indices as u32).collect(),
                value_reals: 4,
                unique_per_vertex: false,
                values_w: vec![],
            }
        } else {
            VertexAttrib::default()
        };

        let mesh = Mesh {
            element: Element {
                name: FbxString::new(data.name.clone()),
                props: data.props.clone(),
                element_id: self.scene.meshes.len() as u32,
                typed_id: self.scene.meshes.len() as u32,
                element_type: ElementType::Mesh,
                connections_src: vec![],
                connections_dst: vec![],
            },
            instances: vec![],
            num_vertices,
            num_indices,
            num_faces,
            num_triangles: 0,  // TODO: Calculate from faces
            num_edges: 0,
            max_face_triangles: 0,
            num_empty_faces: 0,
            num_point_faces: 0,
            num_line_faces: 0,
            faces,
            face_smoothing: vec![],
            face_material: vec![],
            face_group: vec![],
            face_hole: vec![],
            edges: vec![],
            edge_smoothing: vec![],
            edge_crease: vec![],
            edge_visibility: vec![],
            vertex_indices,
            vertices,
            vertex_first_index: vec![],
            vertex_position,
            vertex_normal,
            vertex_uv,
            vertex_tangent: VertexAttrib::default(),
            vertex_bitangent: VertexAttrib::default(),
            vertex_color,
            vertex_crease: VertexAttrib::default(),
            uv_sets: vec![],
            color_sets: vec![],
            materials: vec![],
            face_groups: vec![],
            material_parts: vec![],
            face_group_parts: vec![],
            material_part_usage_order: vec![],
            skinned_is_local: false,
            skinned_position: VertexAttrib::default(),
            skinned_normal: VertexAttrib::default(),
            skin_deformers: vec![],
            blend_deformers: vec![],
            cache_deformers: vec![],
            all_deformers: vec![],
            subdivision_preview_levels: 0,
            subdivision_render_levels: 0,
            subdivision_display_mode: SubdivisionDisplayMode::Disabled,
            subdivision_boundary: SubdivisionBoundary::Default,
            subdivision_uv_boundary: SubdivisionBoundary::Default,
            reversed_winding: false,
            generated_normals: false,
            subdivision_evaluated: false,
            from_tessellated_nurbs: false,
        };

        self.scene.meshes.push(mesh);
        Ok(())
    }

    fn extract_mesh_geometry(&self, fbx_node: &BinaryNode) -> Result<(Vec<Vec3>, Vec<i32>, Vec<Vec3>, Vec<Vec2>, Vec<Vec4>, Vec<Face>, Vec<u32>)> {
        // Extract vertices (positions)
        let vertices = self.read_vertex_positions(fbx_node)?;

        // Extract polygon vertex indices
        let polygon_vertex_indices = self.read_polygon_indices(fbx_node)?;

        // Extract normals (optional)
        let normals = self.read_vertex_normals(fbx_node)?;

        // Extract UVs (optional)
        let uvs = self.read_vertex_uvs(fbx_node)?;

        // Extract vertex colors (optional)
        let colors = self.read_vertex_colors(fbx_node)?;

        // Build faces and vertex_indices from polygon_vertex_indices
        let (faces, vertex_indices) = self.build_faces(&polygon_vertex_indices);

        Ok((vertices, polygon_vertex_indices, normals, uvs, colors, faces, vertex_indices))
    }

    fn read_vertex_positions(&self, fbx_node: &BinaryNode) -> Result<Vec<Vec3>> {
        // Find "Vertices" child node
        for child in &fbx_node.children {
            if child.name == "Vertices" {
                if let Some(ref array) = child.array {
                    return match &array.data {
                        crate::binary::ArrayData::F64(data) => {
                            // Convert flat array to Vec3s
                            let mut positions = Vec::new();
                            for chunk in data.chunks(3) {
                                if chunk.len() == 3 {
                                    positions.push(Vec3::new(chunk[0], chunk[1], chunk[2]));
                                }
                            }
                            Ok(positions)
                        }
                        crate::binary::ArrayData::F32(data) => {
                            let mut positions = Vec::new();
                            for chunk in data.chunks(3) {
                                if chunk.len() == 3 {
                                    positions.push(Vec3::new(chunk[0] as f64, chunk[1] as f64, chunk[2] as f64));
                                }
                            }
                            Ok(positions)
                        }
                        _ => Err(Error::unknown("Vertices array has wrong type")),
                    };
                }
            }
        }
        Ok(vec![])
    }

    fn read_polygon_indices(&self, fbx_node: &BinaryNode) -> Result<Vec<i32>> {
        // Find "PolygonVertexIndex" child node
        for child in &fbx_node.children {
            if child.name == "PolygonVertexIndex" {
                if let Some(ref array) = child.array {
                    return match &array.data {
                        crate::binary::ArrayData::I32(data) => Ok(data.clone()),
                        crate::binary::ArrayData::I64(data) => {
                            Ok(data.iter().map(|&v| v as i32).collect())
                        }
                        _ => Err(Error::unknown("PolygonVertexIndex array has wrong type")),
                    };
                }
            }
        }
        Ok(vec![])
    }

    fn read_vertex_normals(&self, fbx_node: &BinaryNode) -> Result<Vec<Vec3>> {
        // Find "LayerElementNormal" child node
        for child in &fbx_node.children {
            if child.name == "LayerElementNormal" {
                // Find "Normals" child within LayerElementNormal
                for subchild in &child.children {
                    if subchild.name == "Normals" {
                        if let Some(ref array) = subchild.array {
                            return match &array.data {
                                crate::binary::ArrayData::F64(data) => {
                                    let mut normals = Vec::new();
                                    for chunk in data.chunks(3) {
                                        if chunk.len() == 3 {
                                            normals.push(Vec3::new(chunk[0], chunk[1], chunk[2]));
                                        }
                                    }
                                    Ok(normals)
                                }
                                crate::binary::ArrayData::F32(data) => {
                                    let mut normals = Vec::new();
                                    for chunk in data.chunks(3) {
                                        if chunk.len() == 3 {
                                            normals.push(Vec3::new(chunk[0] as f64, chunk[1] as f64, chunk[2] as f64));
                                        }
                                    }
                                    Ok(normals)
                                }
                                _ => Err(Error::unknown("Normals array has wrong type")),
                            };
                        }
                    }
                }
            }
        }
        Ok(vec![])
    }

    fn read_vertex_uvs(&self, fbx_node: &BinaryNode) -> Result<Vec<Vec2>> {
        // Find "LayerElementUV" child node
        for child in &fbx_node.children {
            if child.name == "LayerElementUV" {
                // Find "UV" child within LayerElementUV
                for subchild in &child.children {
                    if subchild.name == "UV" {
                        if let Some(ref array) = subchild.array {
                            return match &array.data {
                                crate::binary::ArrayData::F64(data) => {
                                    let mut uvs = Vec::new();
                                    for chunk in data.chunks(2) {
                                        if chunk.len() == 2 {
                                            uvs.push(Vec2::new(chunk[0], chunk[1]));
                                        }
                                    }
                                    Ok(uvs)
                                }
                                crate::binary::ArrayData::F32(data) => {
                                    let mut uvs = Vec::new();
                                    for chunk in data.chunks(2) {
                                        if chunk.len() == 2 {
                                            uvs.push(Vec2::new(chunk[0] as f64, chunk[1] as f64));
                                        }
                                    }
                                    Ok(uvs)
                                }
                                _ => Err(Error::unknown("UV array has wrong type")),
                            };
                        }
                    }
                }
            }
        }
        Ok(vec![])
    }

    fn read_vertex_colors(&self, fbx_node: &BinaryNode) -> Result<Vec<Vec4>> {
        // Find "LayerElementColor" child node
        for child in &fbx_node.children {
            if child.name == "LayerElementColor" {
                // Find "Colors" child within LayerElementColor
                for subchild in &child.children {
                    if subchild.name == "Colors" {
                        if let Some(ref array) = subchild.array {
                            return match &array.data {
                                crate::binary::ArrayData::F64(data) => {
                                    let mut colors = Vec::new();
                                    for chunk in data.chunks(4) {
                                        if chunk.len() == 4 {
                                            colors.push(Vec4::new(chunk[0], chunk[1], chunk[2], chunk[3]));
                                        } else if chunk.len() == 3 {
                                            // Some FBX files store RGB without alpha
                                            colors.push(Vec4::new(chunk[0], chunk[1], chunk[2], 1.0));
                                        }
                                    }
                                    Ok(colors)
                                }
                                crate::binary::ArrayData::F32(data) => {
                                    let mut colors = Vec::new();
                                    for chunk in data.chunks(4) {
                                        if chunk.len() == 4 {
                                            colors.push(Vec4::new(chunk[0] as f64, chunk[1] as f64, chunk[2] as f64, chunk[3] as f64));
                                        } else if chunk.len() == 3 {
                                            colors.push(Vec4::new(chunk[0] as f64, chunk[1] as f64, chunk[2] as f64, 1.0));
                                        }
                                    }
                                    Ok(colors)
                                }
                                _ => Err(Error::unknown("Colors array has wrong type")),
                            };
                        }
                    }
                }
            }
        }
        Ok(vec![])
    }

    fn build_faces(&self, polygon_vertex_indices: &[i32]) -> (Vec<Face>, Vec<u32>) {
        // FBX polygon encoding: last index of each polygon is bitwise-negated
        // Example: [0, 1, ~2, 3, 4, ~5] = triangle(0,1,2) + triangle(3,4,5)
        // Negative encoding: ~n = -(n+1), so to get actual index: if idx < 0 { -(idx+1) } else { idx }

        let mut faces = Vec::new();
        let mut vertex_indices = Vec::new();
        let mut current_face_start = 0u32;

        for &poly_idx in polygon_vertex_indices {
            // Decode the index
            let actual_idx = if poly_idx < 0 {
                // Last index of polygon (negative)
                (-(poly_idx + 1)) as u32
            } else {
                poly_idx as u32
            };

            vertex_indices.push(actual_idx);

            // If this was a negative index, end of polygon
            if poly_idx < 0 {
                let num_indices = vertex_indices.len() as u32 - current_face_start;
                faces.push(Face {
                    index_begin: current_face_start,
                    num_indices,
                });
                current_face_start = vertex_indices.len() as u32;
            }
        }

        (faces, vertex_indices)
    }

    fn create_light(&mut self, data: &ElementData) -> Result<()> {
        // Create basic light structure
        let light = Light {
            element: Element {
                name: FbxString::new(data.name.clone()),
                props: data.props.clone(),
                element_id: self.scene.lights.len() as u32,
                typed_id: self.scene.lights.len() as u32,
                element_type: ElementType::Light,
                connections_src: vec![],
                connections_dst: vec![],
            },
            instances: vec![],
            color: Vec3::new(1.0, 1.0, 1.0),
            intensity: 100.0,
            local_direction: Vec3::new(0.0, -1.0, 0.0),
            light_type: LightType::Point,
            decay: LightDecay::None,
            area_shape: LightAreaShape::Rectangle,
            inner_angle: 0.0,
            outer_angle: 45.0,
            cast_light: true,
            cast_shadows: true,
        };

        self.scene.lights.push(light);
        Ok(())
    }

    fn create_camera(&mut self, data: &ElementData) -> Result<()> {
        // Create basic camera structure
        let camera = Camera {
            element: Element {
                name: FbxString::new(data.name.clone()),
                props: data.props.clone(),
                element_id: self.scene.cameras.len() as u32,
                typed_id: self.scene.cameras.len() as u32,
                element_type: ElementType::Camera,
                connections_src: vec![],
                connections_dst: vec![],
            },
            instances: vec![],
            projection_mode: ProjectionMode::Perspective,
            resolution_is_pixels: false,
            resolution: Vec2::new(1920.0, 1080.0),
            field_of_view_deg: Vec2::new(40.0, 40.0),
            orthographic_extent: 1.0,
            orthographic_size: Vec2::new(1.0, 1.0),
            projection_plane: Vec2::new(1.0, 1.0),
            aspect_ratio: 16.0 / 9.0,
            near_plane: 0.1,
            far_plane: 1000.0,
            aspect_mode: AspectMode::WindowSize,
            aperture_mode: ApertureMode::HorizontalAndVertical,
            gate_fit: GateFit::None,
            aperture_size_inch: Vec2::new(1.0, 1.0),
            film_size_inch: Vec2::new(1.0, 1.0),
            squeeze_ratio: 1.0,
        };

        self.scene.cameras.push(camera);
        Ok(())
    }

    fn create_material(&mut self, data: &ElementData) -> Result<()> {
        // Create basic material structure
        let material = Material {
            element: Element {
                name: FbxString::new(data.name.clone()),
                props: data.props.clone(),
                element_id: self.scene.materials.len() as u32,
                typed_id: self.scene.materials.len() as u32,
                element_type: ElementType::Material,
                connections_src: vec![],
                connections_dst: vec![],
            },
            shader_type: ShaderType::FbxPhong,
            shader: None,
            fbx: MaterialFbxMaps::default(),
            pbr: MaterialPbrMaps::default(),
            textures: vec![],
        };

        self.scene.materials.push(material);
        Ok(())
    }

    fn extract_local_transform(&self, props: &Props) -> Transform {
        // Extract Lcl Translation, Lcl Rotation, Lcl Scaling from properties
        let translation = if let Some(prop) = props.find("Lcl Translation") {
            match &prop.value {
                PropValue::Vec3(v) => *v,
                _ => Vec3::ZERO,
            }
        } else {
            Vec3::ZERO
        };

        let rotation_euler = if let Some(prop) = props.find("Lcl Rotation") {
            match &prop.value {
                PropValue::Vec3(v) => *v,
                _ => Vec3::ZERO,
            }
        } else {
            Vec3::ZERO
        };

        let scale = if let Some(prop) = props.find("Lcl Scaling") {
            match &prop.value {
                PropValue::Vec3(v) => *v,
                _ => Vec3::ONE,
            }
        } else {
            Vec3::ONE
        };

        // Convert Euler angles (in degrees) to quaternion
        // For now, use a simple conversion - in a real implementation this would
        // respect the rotation order
        let rotation = euler_to_quat(rotation_euler);

        Transform {
            translation,
            rotation,
            scale,
        }
    }

    fn build_node_hierarchy(&mut self) -> Result<()> {
        // Build parent-child relationships from connections
        // We need to iterate over connections and find Object-Object connections
        // where both src and dst are nodes

        // First, build a mapping from element indices to node indices
        let mut element_to_node: HashMap<usize, usize> = HashMap::new();
        for (node_idx, node) in self.scene.nodes.iter().enumerate() {
            element_to_node.insert(node.element.element_id as usize, node_idx);
        }

        // Clone connections to avoid borrow checker issues
        let connections = self.scene.connections.clone();

        for conn in &connections {
            // Skip property connections (we only want Object-Object connections)
            if conn.src_prop.is_some() || conn.dst_prop.is_some() {
                continue;
            }

            // Check if both src and dst are nodes
            let src_node_idx = element_to_node.get(&conn.src);
            let dst_node_idx = element_to_node.get(&conn.dst);

            if let (Some(&src_idx), Some(&dst_idx)) = (src_node_idx, dst_node_idx) {
                // Skip self-connections (would create cycle)
                if src_idx == dst_idx {
                    continue;
                }

                // Connection from src to dst means dst is parent of src
                // In FBX, connections are: C: "OO", child_id, parent_id

                // Set parent relationship (only if not already set to avoid cycles)
                if self.scene.nodes[src_idx].parent.is_none() {
                    self.scene.nodes[src_idx].parent = Some(dst_idx);

                    // Add to children list
                    if !self.scene.nodes[dst_idx].children.contains(&src_idx) {
                        self.scene.nodes[dst_idx].children.push(src_idx);
                    }
                }
            }
        }

        // Attach any orphaned nodes to the root
        let root_idx = self.scene.root_node;
        for node_idx in 0..self.scene.nodes.len() {
            if node_idx != root_idx && self.scene.nodes[node_idx].parent.is_none() {
                self.scene.nodes[node_idx].parent = Some(root_idx);
                if !self.scene.nodes[root_idx].children.contains(&node_idx) {
                    self.scene.nodes[root_idx].children.push(node_idx);
                }
            }
        }

        Ok(())
    }

    fn attach_attributes(&mut self) -> Result<()> {
        // Attach meshes, lights, cameras to nodes using connections

        // Build mappings from element indices to typed indices
        let mut element_to_mesh: HashMap<usize, usize> = HashMap::new();
        for (mesh_idx, mesh) in self.scene.meshes.iter().enumerate() {
            element_to_mesh.insert(mesh.element.element_id as usize, mesh_idx);
        }

        let mut element_to_light: HashMap<usize, usize> = HashMap::new();
        for (light_idx, light) in self.scene.lights.iter().enumerate() {
            element_to_light.insert(light.element.element_id as usize, light_idx);
        }

        let mut element_to_camera: HashMap<usize, usize> = HashMap::new();
        for (camera_idx, camera) in self.scene.cameras.iter().enumerate() {
            element_to_camera.insert(camera.element.element_id as usize, camera_idx);
        }

        let mut element_to_node: HashMap<usize, usize> = HashMap::new();
        for (node_idx, node) in self.scene.nodes.iter().enumerate() {
            element_to_node.insert(node.element.element_id as usize, node_idx);
        }

        // Clone connections to avoid borrow checker issues
        let connections = self.scene.connections.clone();

        for conn in &connections {
            // Skip property connections
            if conn.src_prop.is_some() || conn.dst_prop.is_some() {
                continue;
            }

            // Check if dst is a node
            if let Some(&node_idx) = element_to_node.get(&conn.dst) {
                // Check if src is a mesh, light, or camera
                if let Some(&mesh_idx) = element_to_mesh.get(&conn.src) {
                    // Attach mesh to node
                    self.scene.nodes[node_idx].mesh = Some(mesh_idx);
                    self.scene.nodes[node_idx].attrib = Some(conn.src);
                    self.scene.nodes[node_idx].attrib_type = ElementType::Mesh;

                    // Add node to mesh instances
                    if !self.scene.meshes[mesh_idx].instances.contains(&node_idx) {
                        self.scene.meshes[mesh_idx].instances.push(node_idx);
                    }
                } else if let Some(&light_idx) = element_to_light.get(&conn.src) {
                    // Attach light to node
                    self.scene.nodes[node_idx].light = Some(light_idx);
                    self.scene.nodes[node_idx].attrib = Some(conn.src);
                    self.scene.nodes[node_idx].attrib_type = ElementType::Light;

                    // Add node to light instances
                    if !self.scene.lights[light_idx].instances.contains(&node_idx) {
                        self.scene.lights[light_idx].instances.push(node_idx);
                    }
                } else if let Some(&camera_idx) = element_to_camera.get(&conn.src) {
                    // Attach camera to node
                    self.scene.nodes[node_idx].camera = Some(camera_idx);
                    self.scene.nodes[node_idx].attrib = Some(conn.src);
                    self.scene.nodes[node_idx].attrib_type = ElementType::Camera;

                    // Add node to camera instances
                    if !self.scene.cameras[camera_idx].instances.contains(&node_idx) {
                        self.scene.cameras[camera_idx].instances.push(node_idx);
                    }
                }
            }
        }

        Ok(())
    }

    // =========================================================================
    // Finalization
    // =========================================================================

    fn finalize(&mut self) -> Result<()> {
        // Compute world transforms for all nodes
        self.compute_transforms()?;

        // Generate missing normals if requested
        if self.opts.generate_missing_normals {
            self.generate_normals()?;
        }

        // Validate all indices
        self.validate_indices()?;

        Ok(())
    }

    fn compute_transforms(&mut self) -> Result<()> {
        // Depth-first traversal to compute world transforms
        let root_idx = self.scene.root_node;
        let mut visited = vec![false; self.scene.nodes.len()];
        self.compute_node_transform_safe(root_idx, Matrix::IDENTITY, &mut visited)?;
        Ok(())
    }

    fn compute_node_transform_safe(&mut self, node_idx: usize, parent_world: Matrix, visited: &mut [bool]) -> Result<()> {
        if node_idx >= self.scene.nodes.len() {
            return Ok(());
        }

        // Detect cycles - if we've already visited this node, skip it
        if visited[node_idx] {
            return Ok(());
        }
        visited[node_idx] = true;

        // Compute this node's world transform
        let local_matrix = self.transform_to_matrix(self.scene.nodes[node_idx].local_transform);
        let world_matrix = matrix_mul(&parent_world, &local_matrix);

        self.scene.nodes[node_idx].node_to_world = world_matrix;

        // Recurse to children
        let children: Vec<usize> = self.scene.nodes[node_idx].children.clone();
        for child_idx in children {
            self.compute_node_transform_safe(child_idx, world_matrix, visited)?;
        }

        Ok(())
    }

    fn transform_to_matrix(&self, transform: Transform) -> Matrix {
        // Convert Transform to Matrix: M = T * R * S
        // Build rotation matrix from quaternion
        let q = transform.rotation;
        let s = transform.scale;
        let t = transform.translation;

        // Quaternion to rotation matrix
        let xx = q.x * q.x;
        let yy = q.y * q.y;
        let zz = q.z * q.z;
        let xy = q.x * q.y;
        let xz = q.x * q.z;
        let yz = q.y * q.z;
        let wx = q.w * q.x;
        let wy = q.w * q.y;
        let wz = q.w * q.z;

        // Matrix columns (with scale applied)
        let col0 = Vec3::new(
            (1.0 - 2.0 * (yy + zz)) * s.x,
            (2.0 * (xy + wz)) * s.x,
            (2.0 * (xz - wy)) * s.x,
        );

        let col1 = Vec3::new(
            (2.0 * (xy - wz)) * s.y,
            (1.0 - 2.0 * (xx + zz)) * s.y,
            (2.0 * (yz + wx)) * s.y,
        );

        let col2 = Vec3::new(
            (2.0 * (xz + wy)) * s.z,
            (2.0 * (yz - wx)) * s.z,
            (1.0 - 2.0 * (xx + yy)) * s.z,
        );

        Matrix {
            cols: [col0, col1, col2, t],
        }
    }

    fn generate_normals(&mut self) -> Result<()> {
        // TODO: Generate normals for meshes that don't have them
        Ok(())
    }

    fn validate_indices(&mut self) -> Result<()> {
        // TODO: Validate all mesh indices are in bounds
        Ok(())
    }

    // =========================================================================
    // Utilities
    // =========================================================================

    fn name_to_element_type(type_name: &str, sub_type: &str) -> ElementType {
        match type_name {
            "Model" => {
                // All Model nodes are scene nodes, not geometry
                // The sub_type tells us what kind of attribute they might have
                ElementType::Node
            }
            "Geometry" => ElementType::Mesh,
            "NodeAttribute" => {
                // NodeAttribute can be Light, Camera, etc.
                match sub_type {
                    "Light" => ElementType::Light,
                    "Camera" => ElementType::Camera,
                    _ => ElementType::Unknown,
                }
            }
            "Material" => ElementType::Material,
            "Texture" => ElementType::Texture,
            "Video" => ElementType::Video,
            "Deformer" => {
                match sub_type {
                    "Skin" => ElementType::SkinDeformer,
                    "Cluster" => ElementType::SkinCluster,
                    "BlendShape" => ElementType::BlendDeformer,
                    "BlendShapeChannel" => ElementType::BlendChannel,
                    _ => ElementType::Unknown,
                }
            }
            "AnimationStack" => ElementType::AnimStack,
            "AnimationLayer" => ElementType::AnimLayer,
            "AnimationCurveNode" => ElementType::AnimValue,
            "AnimationCurve" => ElementType::AnimCurve,
            _ => ElementType::Unknown,
        }
    }
}

impl Default for SceneBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Matrix Utilities
// =============================================================================

fn matrix_mul(a: &Matrix, b: &Matrix) -> Matrix {
    // Multiply two 4x3 affine transformation matrices: result = a * b
    // Each matrix has 3 basis vectors (cols[0..2]) and translation (cols[3])

    let mut result = Matrix::IDENTITY;

    // For each column of b (first 3 are basis vectors, 4th is translation)
    for i in 0..3 {
        // Transform basis vector by matrix a
        result.cols[i] = Vec3::new(
            a.cols[0].x * b.cols[i].x + a.cols[1].x * b.cols[i].y + a.cols[2].x * b.cols[i].z,
            a.cols[0].y * b.cols[i].x + a.cols[1].y * b.cols[i].y + a.cols[2].y * b.cols[i].z,
            a.cols[0].z * b.cols[i].x + a.cols[1].z * b.cols[i].y + a.cols[2].z * b.cols[i].z,
        );
    }

    // Transform translation: a * b.translation + a.translation
    result.cols[3] = Vec3::new(
        a.cols[0].x * b.cols[3].x + a.cols[1].x * b.cols[3].y + a.cols[2].x * b.cols[3].z + a.cols[3].x,
        a.cols[0].y * b.cols[3].x + a.cols[1].y * b.cols[3].y + a.cols[2].y * b.cols[3].z + a.cols[3].y,
        a.cols[0].z * b.cols[3].x + a.cols[1].z * b.cols[3].y + a.cols[2].z * b.cols[3].z + a.cols[3].z,
    );

    result
}

/// Convert Euler angles (in degrees) to quaternion
/// This is a simplified version that assumes XYZ rotation order
fn euler_to_quat(euler_deg: Vec3) -> Quat {
    // Convert degrees to radians
    let x = euler_deg.x.to_radians();
    let y = euler_deg.y.to_radians();
    let z = euler_deg.z.to_radians();

    // Compute half angles
    let cx = (x * 0.5).cos();
    let sx = (x * 0.5).sin();
    let cy = (y * 0.5).cos();
    let sy = (y * 0.5).sin();
    let cz = (z * 0.5).cos();
    let sz = (z * 0.5).sin();

    // Compute quaternion for XYZ order: qz * qy * qx
    Quat {
        w: cx * cy * cz + sx * sy * sz,
        x: sx * cy * cz - cx * sy * sz,
        y: cx * sy * cz + sx * cy * sz,
        z: cx * cy * sz - sx * sy * cz,
    }
}

// =============================================================================
// Public API
// =============================================================================

/// Load an FBX file from disk
pub fn load_file(path: &str, opts: &SceneOpts) -> Result<Scene> {
    use std::fs::File;
    use std::io::Read;

    let mut file = File::open(path)
        .map_err(|e| Error::io(e.to_string()))?;

    let mut data = Vec::new();
    file.read_to_end(&mut data)
        .map_err(|e| Error::io(e.to_string()))?;

    load_memory(&data, opts)
}

/// Load an FBX file from memory
pub fn load_memory(data: &[u8], opts: &SceneOpts) -> Result<Scene> {
    // Detect format
    if data.len() < 27 {
        return Err(Error::unknown("File too small"));
    }

    // Check for binary magic
    if &data[0..22] == b"Kaydara FBX Binary  \x00\x1a" {
        // Binary FBX
        let doc = crate::binary::parse_binary(data)?;
        SceneBuilder::from_binary(doc, opts.clone())
    } else {
        // Try ASCII
        let text = std::str::from_utf8(data)
            .map_err(|_| Error::unknown("Invalid UTF-8 in ASCII FBX"))?;
        let nodes = crate::ascii::parse_ascii(text)?;
        // TODO: Extract version from ASCII header
        let version = 7400; // Default version
        SceneBuilder::from_ascii(nodes, version, opts.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binary::Value;

    #[test]
    fn test_scene_builder_new() {
        let builder = SceneBuilder::new();
        assert_eq!(builder.elements.len(), 0);
        assert_eq!(builder.connections.len(), 0);
    }

    #[test]
    fn test_element_type_mapping() {
        // All Model nodes are scene nodes (regardless of sub-type)
        assert_eq!(
            SceneBuilder::name_to_element_type("Model", "Mesh"),
            ElementType::Node
        );
        assert_eq!(
            SceneBuilder::name_to_element_type("Model", "Null"),
            ElementType::Node
        );
        // Geometry nodes contain actual mesh data
        assert_eq!(
            SceneBuilder::name_to_element_type("Geometry", ""),
            ElementType::Mesh
        );
        // NodeAttribute contains lights, cameras, etc.
        assert_eq!(
            SceneBuilder::name_to_element_type("NodeAttribute", "Light"),
            ElementType::Light
        );
        assert_eq!(
            SceneBuilder::name_to_element_type("NodeAttribute", "Camera"),
            ElementType::Camera
        );
    }

    #[test]
    fn test_transform_to_matrix_identity() {
        let builder = SceneBuilder::new();
        let transform = Transform::IDENTITY;
        let matrix = builder.transform_to_matrix(transform);

        // Check diagonal elements are 1.0
        assert!((matrix.at(0, 0) - 1.0).abs() < 1e-10);
        assert!((matrix.at(1, 1) - 1.0).abs() < 1e-10);
        assert!((matrix.at(2, 2) - 1.0).abs() < 1e-10);

        // Check translation is zero
        assert_eq!(matrix.cols[3].x, 0.0);
        assert_eq!(matrix.cols[3].y, 0.0);
        assert_eq!(matrix.cols[3].z, 0.0);
    }

    #[test]
    fn test_euler_to_quat_identity() {
        let quat = euler_to_quat(Vec3::ZERO);
        assert!((quat.w - 1.0).abs() < 1e-10);
        assert!(quat.x.abs() < 1e-10);
        assert!(quat.y.abs() < 1e-10);
        assert!(quat.z.abs() < 1e-10);
    }

    #[test]
    fn test_matrix_multiplication_identity() {
        let m1 = Matrix::IDENTITY;
        let m2 = Matrix::IDENTITY;
        let result = matrix_mul(&m1, &m2);

        assert!((result.at(0, 0) - 1.0).abs() < 1e-10);
        assert!((result.at(1, 1) - 1.0).abs() < 1e-10);
        assert!((result.at(2, 2) - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_property_parsing() {
        let mut builder = SceneBuilder::new();

        // Create a simple property node
        let mut node = crate::binary::FbxNode::new("P".to_string());
        node.values.push(Value::String(FbxString::new("TestProp")));
        node.values.push(Value::String(FbxString::new("double")));
        node.values.push(Value::String(FbxString::new("Number")));
        node.values.push(Value::String(FbxString::new("")));
        node.values.push(Value::Number { i: 0, f: 42.5 });

        let prop = builder.parse_property_binary(&node).unwrap();
        assert_eq!(prop.name.as_str(), "TestProp");
        match prop.value {
            PropValue::Number(v) => assert!((v - 42.5).abs() < 1e-10),
            _ => panic!("Expected number property"),
        }
    }

    #[test]
    fn test_extract_transform_from_props() {
        let builder = SceneBuilder::new();

        let mut props = Props::new();
        props.props.push(Prop {
            name: FbxString::new("Lcl Translation"),
            value: PropValue::Vec3(Vec3::new(1.0, 2.0, 3.0)),
            flags: PropFlags {
                animated: false,
                user_defined: false,
                hidden: false,
                lock: false,
                mute: false,
                synthetic: false,
                no_value: false,
                not_found: false,
                connected: false,
                overridden: false,
            },
        });

        props.props.push(Prop {
            name: FbxString::new("Lcl Scaling"),
            value: PropValue::Vec3(Vec3::new(2.0, 2.0, 2.0)),
            flags: PropFlags {
                animated: false,
                user_defined: false,
                hidden: false,
                lock: false,
                mute: false,
                synthetic: false,
                no_value: false,
                not_found: false,
                connected: false,
                overridden: false,
            },
        });

        let transform = builder.extract_local_transform(&props);
        assert_eq!(transform.translation.x, 1.0);
        assert_eq!(transform.translation.y, 2.0);
        assert_eq!(transform.translation.z, 3.0);
        assert_eq!(transform.scale.x, 2.0);
        assert_eq!(transform.scale.y, 2.0);
        assert_eq!(transform.scale.z, 2.0);
    }
}
