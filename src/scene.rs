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
            0  // TODO: Extract from node.values[0]
        } else {
            return Err(Error::unknown("Object node missing FBX ID"));
        };

        // Second property is name (may be "Name::Type" in ASCII, "Name\x00\x01Type" in binary)
        let (name, sub_type) = if node.values.len() > 1 {
            // TODO: Split name/type from node.values[1]
            ("".to_string(), "".to_string())
        } else {
            ("".to_string(), "".to_string())
        };

        // Third property (if present) is sub-type
        let sub_type = if node.values.len() > 2 {
            // TODO: Extract from node.values[2]
            sub_type
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

    fn parse_property_binary(&mut self, _node: &BinaryNode) -> Result<Prop> {
        // Property format: P: "Name", "Type", "SubType", "Flags", Value1, Value2, ...
        // TODO: Parse property values
        Ok(Prop {
            name: FbxString::new(""),
            value: PropValue::Integer(0),
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

        // TODO: Parse connection type, IDs, and properties
        let conn = TempConnection {
            src_fbx_id: 0,  // TODO
            dst_fbx_id: 0,  // TODO
            src_prop: None,
            dst_prop: None,
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
        // TODO: Create mesh from data
        // - Extract vertices, indices, normals, UVs
        // - Build face list
        // - Handle vertex attributes
        Ok(())
    }

    fn create_light(&mut self, _data: &ElementData) -> Result<()> {
        // TODO: Create light
        Ok(())
    }

    fn create_camera(&mut self, _data: &ElementData) -> Result<()> {
        // TODO: Create camera
        Ok(())
    }

    fn create_material(&mut self, _data: &ElementData) -> Result<()> {
        // TODO: Create material
        Ok(())
    }

    fn extract_local_transform(&self, _props: &Props) -> Transform {
        // TODO: Extract Lcl Translation, Lcl Rotation, Lcl Scaling from properties
        Transform::IDENTITY
    }

    fn build_node_hierarchy(&mut self) -> Result<()> {
        // Use connections to build parent-child relationships
        // For now, just a stub - TODO: properly build from connections
        Ok(())
    }

    fn attach_attributes(&mut self) -> Result<()> {
        // Use connections to attach meshes, lights, cameras to nodes
        // TODO: properly attach from connections
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
        if let Some(root_idx) = Some(self.scene.root_node) {
            self.compute_node_transform(root_idx, Matrix::IDENTITY)?;
        }
        Ok(())
    }

    fn compute_node_transform(&mut self, node_idx: usize, parent_world: Matrix) -> Result<()> {
        if node_idx >= self.scene.nodes.len() {
            return Ok(());
        }

        // Compute this node's world transform
        let local_matrix = self.transform_to_matrix(self.scene.nodes[node_idx].local_transform);
        let world_matrix = matrix_mul(&parent_world, &local_matrix);

        self.scene.nodes[node_idx].node_to_world = world_matrix;

        // Recurse to children
        let children: Vec<usize> = self.scene.nodes[node_idx].children.clone();
        for child_idx in children {
            self.compute_node_transform(child_idx, world_matrix)?;
        }

        Ok(())
    }

    fn transform_to_matrix(&self, _transform: Transform) -> Matrix {
        // TODO: Convert Transform to Matrix
        Matrix::IDENTITY
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
                // Sub-type distinguishes different model types
                match sub_type {
                    "Mesh" => ElementType::Mesh,
                    "Light" => ElementType::Light,
                    "Camera" => ElementType::Camera,
                    "Null" => ElementType::Node,
                    "LimbNode" => ElementType::Bone,
                    _ => ElementType::Node,
                }
            }
            "Geometry" => ElementType::Mesh,
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
    // TODO: Implement matrix multiplication
    Matrix::IDENTITY
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

    #[test]
    fn test_scene_builder_new() {
        let builder = SceneBuilder::new();
        assert_eq!(builder.elements.len(), 0);
        assert_eq!(builder.connections.len(), 0);
    }

    #[test]
    fn test_element_type_mapping() {
        assert_eq!(
            SceneBuilder::name_to_element_type("Model", "Mesh"),
            ElementType::Mesh
        );
        assert_eq!(
            SceneBuilder::name_to_element_type("Model", "Light"),
            ElementType::Light
        );
        assert_eq!(
            SceneBuilder::name_to_element_type("Geometry", ""),
            ElementType::Mesh
        );
    }
}
