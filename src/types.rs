//! FBX Scene Data Structures
//!
//! This module contains idiomatic Rust representations of the FBX file format's
//! data structures, including scene graphs, meshes, materials, animations, and more.
//!
//! These types are designed to be safe, ergonomic, and follow Rust best practices
//! while maintaining semantic compatibility with the C ufbx library.

// =============================================================================
// Basic Types
// =============================================================================

/// Floating-point type used throughout ufbx (typically f64)
pub type Real = f64;

/// Null-terminated UTF-8 encoded string from an FBX file
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FbxString {
    pub data: String,
}

impl FbxString {
    pub fn new(s: impl Into<String>) -> Self {
        Self { data: s.into() }
    }

    pub fn as_str(&self) -> &str {
        &self.data
    }
}

impl From<String> for FbxString {
    fn from(s: String) -> Self {
        Self::new(s)
    }
}

impl From<&str> for FbxString {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

/// Opaque byte buffer blob
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Blob {
    pub data: Vec<u8>,
}

impl Blob {
    pub fn new(data: Vec<u8>) -> Self {
        Self { data }
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.data
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

// =============================================================================
// Math Types
// =============================================================================

/// 2D vector
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vec2 {
    pub x: Real,
    pub y: Real,
}

impl Vec2 {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };

    pub fn new(x: Real, y: Real) -> Self {
        Self { x, y }
    }
}

/// 3D vector
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vec3 {
    pub x: Real,
    pub y: Real,
    pub z: Real,
}

impl Vec3 {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0, z: 0.0 };
    pub const ONE: Self = Self { x: 1.0, y: 1.0, z: 1.0 };

    pub fn new(x: Real, y: Real, z: Real) -> Self {
        Self { x, y, z }
    }
}

/// 4D vector
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vec4 {
    pub x: Real,
    pub y: Real,
    pub z: Real,
    pub w: Real,
}

impl Vec4 {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0, z: 0.0, w: 0.0 };

    pub fn new(x: Real, y: Real, z: Real, w: Real) -> Self {
        Self { x, y, z, w }
    }
}

/// Quaternion rotation
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quat {
    pub x: Real,
    pub y: Real,
    pub z: Real,
    pub w: Real,
}

impl Quat {
    pub const IDENTITY: Self = Self { x: 0.0, y: 0.0, z: 0.0, w: 1.0 };

    pub fn new(x: Real, y: Real, z: Real, w: Real) -> Self {
        Self { x, y, z, w }
    }
}

/// Order in which Euler-angle rotation axes are applied for a transform.
///
/// The order in the name refers to the order of axes *applied*,
/// not the multiplication order: e.g., `XYZ` is `Z*Y*X`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RotationOrder {
    XYZ,
    XZY,
    YZX,
    YXZ,
    ZXY,
    ZYX,
    Spheric,
}

/// Explicit translation+rotation+scale transformation.
///
/// Note: Rotation is a quaternion, not Euler angles!
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform {
    pub translation: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
}

impl Transform {
    pub const IDENTITY: Self = Self {
        translation: Vec3::ZERO,
        rotation: Quat::IDENTITY,
        scale: Vec3::ONE,
    };

    pub fn new(translation: Vec3, rotation: Quat, scale: Vec3) -> Self {
        Self {
            translation,
            rotation,
            scale,
        }
    }
}

/// 4x3 matrix encoding an affine transformation.
///
/// `cols[0..2]` are the X/Y/Z basis vectors, `cols[3]` is the translation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Matrix {
    pub cols: [Vec3; 4],
}

impl Matrix {
    pub const IDENTITY: Self = Self {
        cols: [
            Vec3 { x: 1.0, y: 0.0, z: 0.0 },
            Vec3 { x: 0.0, y: 1.0, z: 0.0 },
            Vec3 { x: 0.0, y: 0.0, z: 1.0 },
            Vec3::ZERO,
        ],
    };

    pub fn new(cols: [Vec3; 4]) -> Self {
        Self { cols }
    }

    /// Get matrix element at row i, column j
    pub fn at(&self, row: usize, col: usize) -> Real {
        match row {
            0 => self.cols[col].x,
            1 => self.cols[col].y,
            2 => self.cols[col].z,
            _ => panic!("Matrix row index out of bounds"),
        }
    }
}

// =============================================================================
// Property System
// =============================================================================

/// Property value type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropType {
    Number,
    Vec2,
    Vec3,
    Vec4,
    String,
    Bool,
    Integer,
    Unknown,
}

/// Property flags
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PropFlags {
    pub animated: bool,
    pub user_defined: bool,
    pub hidden: bool,
    pub lock: bool,
    pub mute: bool,
    pub synthetic: bool,
    pub no_value: bool,
    pub not_found: bool,
    pub connected: bool,
    pub overridden: bool,
}

/// Generic property value
#[derive(Debug, Clone, PartialEq)]
pub enum PropValue {
    Number(Real),
    Vec2(Vec2),
    Vec3(Vec3),
    Vec4(Vec4),
    String(FbxString),
    Bool(bool),
    Integer(i64),
}

/// A single property with name and value
#[derive(Debug, Clone, PartialEq)]
pub struct Prop {
    pub name: FbxString,
    pub value: PropValue,
    pub flags: PropFlags,
}

/// Collection of properties
#[derive(Debug, Clone, Default)]
pub struct Props {
    pub props: Vec<Prop>,
}

impl Props {
    pub fn new() -> Self {
        Self { props: Vec::new() }
    }

    pub fn find(&self, name: &str) -> Option<&Prop> {
        self.props.iter().find(|p| p.name.as_str() == name)
    }
}

// =============================================================================
// Element System
// =============================================================================

/// Element type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElementType {
    Unknown,
    Node,
    Mesh,
    Light,
    Camera,
    Bone,
    Empty,
    LineCurve,
    NurbsCurve,
    NurbsSurface,
    NurbsTrimSurface,
    NurbsTrimBoundary,
    ProceduralGeometry,
    StereoCamera,
    CameraSwitcher,
    Marker,
    LodGroup,
    SkinDeformer,
    SkinCluster,
    BlendDeformer,
    BlendChannel,
    BlendShape,
    CacheDeformer,
    CacheFile,
    Material,
    Texture,
    Video,
    Shader,
    ShaderBinding,
    AnimStack,
    AnimLayer,
    AnimValue,
    AnimCurve,
    DisplayLayer,
    SelectionSet,
    SelectionNode,
    Character,
    Constraint,
    AudioLayer,
    AudioClip,
    Pose,
    MetadataObject,
}

/// Connection between two elements
#[derive(Debug, Clone)]
pub struct Connection {
    pub src: usize,  // Element ID
    pub dst: usize,  // Element ID
    pub src_prop: Option<FbxString>,
    pub dst_prop: Option<FbxString>,
}

/// Base element data common to all element types
#[derive(Debug, Clone)]
pub struct Element {
    pub name: FbxString,
    pub props: Props,
    pub element_id: u32,
    pub typed_id: u32,
    pub element_type: ElementType,
    pub connections_src: Vec<Connection>,
    pub connections_dst: Vec<Connection>,
}

impl Element {
    pub fn new(name: impl Into<FbxString>, element_type: ElementType) -> Self {
        Self {
            name: name.into(),
            props: Props::new(),
            element_id: 0,
            typed_id: 0,
            element_type,
            connections_src: Vec::new(),
            connections_dst: Vec::new(),
        }
    }
}

// =============================================================================
// Node Hierarchy
// =============================================================================

/// Inherit type specifies how hierarchical node transforms are combined.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InheritMode {
    /// Normal matrix composition: `R*S*r*s`
    Normal,
    /// Ignore parent scale: `R*r*s` (segment scale compensate)
    IgnoreParentScale,
    /// Apply parent scale component-wise: `R*r*S*s`
    ComponentwiseScale,
}

/// Axis used to mirror transformations for handedness conversion
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MirrorAxis {
    None,
    X,
    Y,
    Z,
}

/// Scene graph node containing transformation and attached elements.
///
/// Nodes form the scene transformation hierarchy and can contain attached
/// elements such as meshes or lights.
#[derive(Debug, Clone)]
pub struct Node {
    pub element: Element,

    // Hierarchy
    pub parent: Option<usize>,  // Parent node ID
    pub children: Vec<usize>,   // Child node IDs

    // Attached elements
    pub mesh: Option<usize>,
    pub light: Option<usize>,
    pub camera: Option<usize>,
    pub bone: Option<usize>,
    pub attrib: Option<usize>,
    pub attrib_type: ElementType,
    pub all_attribs: Vec<usize>,

    // Helpers
    pub geometry_transform_helper: Option<usize>,
    pub scale_helper: Option<usize>,

    // Transforms
    pub inherit_mode: InheritMode,
    pub original_inherit_mode: InheritMode,
    pub local_transform: Transform,
    pub geometry_transform: Transform,
    pub inherit_scale: Vec3,
    pub inherit_scale_node: Option<usize>,

    // Rotation
    pub rotation_order: RotationOrder,
    pub euler_rotation: Vec3,

    // Matrices
    pub node_to_parent: Matrix,
    pub node_to_world: Matrix,
    pub geometry_to_node: Matrix,
    pub geometry_to_world: Matrix,
    pub unscaled_node_to_world: Matrix,

    // Adjustments
    pub adjust_pre_translation: Vec3,
    pub adjust_pre_rotation: Quat,
    pub adjust_pre_scale: Real,
    pub adjust_post_rotation: Quat,
    pub adjust_post_scale: Real,
    pub adjust_translation_scale: Real,
    pub adjust_mirror_axis: MirrorAxis,

    // Materials
    pub materials: Vec<usize>,

    // Bind pose
    pub bind_pose: Option<usize>,

    // Flags
    pub visible: bool,
    pub is_root: bool,
    pub has_geometry_transform: bool,
    pub has_adjust_transform: bool,
    pub has_root_adjust_transform: bool,
    pub is_geometry_transform_helper: bool,
    pub is_scale_helper: bool,
    pub is_scale_compensate_parent: bool,

    pub node_depth: u32,
}

// =============================================================================
// Mesh Geometry
// =============================================================================

/// Vertex attribute descriptor
#[derive(Debug, Clone)]
pub struct VertexAttrib<T> {
    pub exists: bool,
    pub values: Vec<T>,
    pub indices: Vec<u32>,
    pub value_reals: usize,
    pub unique_per_vertex: bool,
    pub values_w: Vec<Real>,
}

impl<T> Default for VertexAttrib<T> {
    fn default() -> Self {
        Self {
            exists: false,
            values: Vec::new(),
            indices: Vec::new(),
            value_reals: 0,
            unique_per_vertex: false,
            values_w: Vec::new(),
        }
    }
}

/// UV texture coordinate set
#[derive(Debug, Clone)]
pub struct UvSet {
    pub name: FbxString,
    pub index: u32,
    pub vertex_uv: VertexAttrib<Vec2>,
    pub vertex_tangent: VertexAttrib<Vec3>,
    pub vertex_bitangent: VertexAttrib<Vec3>,
}

/// Color attribute set
#[derive(Debug, Clone)]
pub struct ColorSet {
    pub name: FbxString,
    pub index: u32,
    pub vertex_color: VertexAttrib<Vec4>,
}

/// Mesh edge
#[derive(Debug, Clone, Copy)]
pub struct Edge {
    pub a: u32,
    pub b: u32,
}

/// Polygonal face
#[derive(Debug, Clone, Copy)]
pub struct Face {
    pub index_begin: u32,
    pub num_indices: u32,
}

/// Mesh part (subset for material or group)
#[derive(Debug, Clone)]
pub struct MeshPart {
    pub index: u32,
    pub num_faces: usize,
    pub num_triangles: usize,
    pub num_empty_faces: usize,
    pub num_point_faces: usize,
    pub num_line_faces: usize,
    pub face_indices: Vec<u32>,
}

/// Face group
#[derive(Debug, Clone)]
pub struct FaceGroup {
    pub id: i32,
    pub name: FbxString,
}

/// Subdivision display mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubdivisionDisplayMode {
    Disabled,
    Hull,
    HullAndSmooth,
    Smooth,
}

/// Subdivision boundary mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubdivisionBoundary {
    Default,
    Legacy,
    SharpCorners,
    SharpNone,
    SharpBoundary,
    SharpInterior,
}

/// Polygonal mesh geometry
#[derive(Debug, Clone)]
pub struct Mesh {
    pub element: Element,
    pub instances: Vec<usize>,  // Node IDs

    // Counts
    pub num_vertices: usize,
    pub num_indices: usize,
    pub num_faces: usize,
    pub num_triangles: usize,
    pub num_edges: usize,
    pub max_face_triangles: usize,
    pub num_empty_faces: usize,
    pub num_point_faces: usize,
    pub num_line_faces: usize,

    // Topology
    pub faces: Vec<Face>,
    pub face_smoothing: Vec<bool>,
    pub face_material: Vec<u32>,
    pub face_group: Vec<u32>,
    pub face_hole: Vec<bool>,

    pub edges: Vec<Edge>,
    pub edge_smoothing: Vec<bool>,
    pub edge_crease: Vec<Real>,
    pub edge_visibility: Vec<bool>,

    // Vertices
    pub vertex_indices: Vec<u32>,
    pub vertices: Vec<Vec3>,
    pub vertex_first_index: Vec<u32>,

    // Vertex attributes
    pub vertex_position: VertexAttrib<Vec3>,
    pub vertex_normal: VertexAttrib<Vec3>,
    pub vertex_uv: VertexAttrib<Vec2>,
    pub vertex_tangent: VertexAttrib<Vec3>,
    pub vertex_bitangent: VertexAttrib<Vec3>,
    pub vertex_color: VertexAttrib<Vec4>,
    pub vertex_crease: VertexAttrib<Real>,

    // Multi-layer attributes
    pub uv_sets: Vec<UvSet>,
    pub color_sets: Vec<ColorSet>,

    // Materials
    pub materials: Vec<usize>,
    pub face_groups: Vec<FaceGroup>,
    pub material_parts: Vec<MeshPart>,
    pub face_group_parts: Vec<MeshPart>,
    pub material_part_usage_order: Vec<u32>,

    // Skinning
    pub skinned_is_local: bool,
    pub skinned_position: VertexAttrib<Vec3>,
    pub skinned_normal: VertexAttrib<Vec3>,

    // Deformers
    pub skin_deformers: Vec<usize>,
    pub blend_deformers: Vec<usize>,
    pub cache_deformers: Vec<usize>,
    pub all_deformers: Vec<usize>,

    // Subdivision
    pub subdivision_preview_levels: u32,
    pub subdivision_render_levels: u32,
    pub subdivision_display_mode: SubdivisionDisplayMode,
    pub subdivision_boundary: SubdivisionBoundary,
    pub subdivision_uv_boundary: SubdivisionBoundary,

    // Flags
    pub reversed_winding: bool,
    pub generated_normals: bool,
    pub subdivision_evaluated: bool,
    pub from_tessellated_nurbs: bool,
}

// =============================================================================
// Lights
// =============================================================================

/// Light source type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LightType {
    Point,
    Directional,
    Spot,
    Area,
    Volume,
}

/// Light intensity decay mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LightDecay {
    None,
    Linear,
    Quadratic,
    Cubic,
}

/// Area light shape
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LightAreaShape {
    Rectangle,
    Sphere,
}

/// Light source attached to a node
#[derive(Debug, Clone)]
pub struct Light {
    pub element: Element,
    pub instances: Vec<usize>,

    pub color: Vec3,
    pub intensity: Real,
    pub local_direction: Vec3,

    pub light_type: LightType,
    pub decay: LightDecay,
    pub area_shape: LightAreaShape,
    pub inner_angle: Real,
    pub outer_angle: Real,

    pub cast_light: bool,
    pub cast_shadows: bool,
}

// =============================================================================
// Cameras
// =============================================================================

/// Camera projection mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectionMode {
    Perspective,
    Orthographic,
}

/// Aspect ratio mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AspectMode {
    WindowSize,
    FixedRatio,
    FixedResolution,
    FixedWidth,
    FixedHeight,
}

/// Aperture mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApertureMode {
    HorizontalAndVertical,
    Horizontal,
    Vertical,
    FocalLength,
}

/// Gate fit mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateFit {
    None,
    Vertical,
    Horizontal,
    Fill,
    Overscan,
    Stretch,
}

/// Camera attached to a node
#[derive(Debug, Clone)]
pub struct Camera {
    pub element: Element,
    pub instances: Vec<usize>,

    pub projection_mode: ProjectionMode,
    pub resolution_is_pixels: bool,
    pub resolution: Vec2,
    pub field_of_view_deg: Vec2,
    pub orthographic_extent: Real,
    pub orthographic_size: Vec2,
    pub projection_plane: Vec2,
    pub aspect_ratio: Real,

    pub near_plane: Real,
    pub far_plane: Real,

    pub aspect_mode: AspectMode,
    pub aperture_mode: ApertureMode,
    pub gate_fit: GateFit,

    pub aperture_size_inch: Vec2,
    pub film_size_inch: Vec2,
    pub squeeze_ratio: Real,
}

// =============================================================================
// Materials & Textures
// =============================================================================

/// Shader type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShaderType {
    Unknown,
    FbxLambert,
    FbxPhong,
    OslStandardSurface,
    ArnoldStandardSurface,
    ThreeDsMaxPhysicalMaterial,
    ThreeDsMaxPbrMetalRough,
    ThreeDsMaxPbrSpecGloss,
    GltfMaterial,
    OpenPbrMaterial,
    ShaderfxGraph,
    BlenderPhong,
    WavefrontMtl,
}

/// Material property map
#[derive(Debug, Clone)]
pub struct MaterialMap {
    pub value_real: Real,
    pub value_vec2: Vec2,
    pub value_vec3: Vec3,
    pub value_vec4: Vec4,
    pub value_int: i64,
    pub texture: Option<usize>,
    pub has_value: bool,
    pub texture_enabled: bool,
    pub feature_disabled: bool,
    pub value_components: u8,
}

impl Default for MaterialMap {
    fn default() -> Self {
        Self {
            value_real: 0.0,
            value_vec2: Vec2::ZERO,
            value_vec3: Vec3::ZERO,
            value_vec4: Vec4::ZERO,
            value_int: 0,
            texture: None,
            has_value: false,
            texture_enabled: false,
            feature_disabled: false,
            value_components: 0,
        }
    }
}

/// Material feature info
#[derive(Debug, Clone, Copy)]
pub struct MaterialFeatureInfo {
    pub enabled: bool,
    pub is_explicit: bool,
}

/// Material PBR maps
#[derive(Debug, Clone, Default)]
pub struct MaterialPbrMaps {
    pub base_factor: MaterialMap,
    pub base_color: MaterialMap,
    pub roughness: MaterialMap,
    pub metalness: MaterialMap,
    pub diffuse_roughness: MaterialMap,
    pub specular_factor: MaterialMap,
    pub specular_color: MaterialMap,
    pub specular_ior: MaterialMap,
    pub emission_factor: MaterialMap,
    pub emission_color: MaterialMap,
    pub opacity: MaterialMap,
    pub normal_map: MaterialMap,
    pub displacement_map: MaterialMap,
    pub ambient_occlusion: MaterialMap,
}

/// Material FBX maps
#[derive(Debug, Clone, Default)]
pub struct MaterialFbxMaps {
    pub diffuse_factor: MaterialMap,
    pub diffuse_color: MaterialMap,
    pub specular_factor: MaterialMap,
    pub specular_color: MaterialMap,
    pub specular_exponent: MaterialMap,
    pub reflection_factor: MaterialMap,
    pub reflection_color: MaterialMap,
    pub transparency_factor: MaterialMap,
    pub transparency_color: MaterialMap,
    pub emission_factor: MaterialMap,
    pub emission_color: MaterialMap,
    pub ambient_factor: MaterialMap,
    pub ambient_color: MaterialMap,
    pub normal_map: MaterialMap,
    pub bump: MaterialMap,
    pub bump_factor: MaterialMap,
    pub displacement_factor: MaterialMap,
    pub displacement: MaterialMap,
}

/// Material definition
#[derive(Debug, Clone)]
pub struct Material {
    pub element: Element,

    pub shader_type: ShaderType,
    pub shader: Option<usize>,

    pub fbx: MaterialFbxMaps,
    pub pbr: MaterialPbrMaps,

    pub textures: Vec<usize>,
}

/// Texture type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextureType {
    File,
    Layered,
    Procedural,
    ShaderTexture,
}

/// Blend mode for layered textures
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlendMode {
    Translucent,
    Additive,
    Modulate,
    Modulate2,
    Over,
    Normal,
    Dissolve,
    Darken,
    ColorBurn,
    LinearBurn,
    DarkerColor,
    Lighten,
    Screen,
    ColorDodge,
    LinearDodge,
    LighterColor,
    SoftLight,
    HardLight,
    VividLight,
    LinearLight,
    PinLight,
    HardMix,
    Difference,
    Exclusion,
    Subtract,
    Divide,
    Hue,
    Saturation,
    Color,
    Luminosity,
    Overlay,
    MaxBlendModes,
}

/// Texture wrapping mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WrapMode {
    Repeat,
    Clamp,
}

/// Texture layer
#[derive(Debug, Clone)]
pub struct TextureLayer {
    pub texture: usize,
    pub blend_mode: BlendMode,
    pub alpha: Real,
}

/// Texture file reference
#[derive(Debug, Clone)]
pub struct TextureFile {
    pub index: u32,
    pub filename: FbxString,
    pub absolute_filename: FbxString,
    pub relative_filename: FbxString,
    pub raw_filename: Blob,
    pub raw_absolute_filename: Blob,
    pub raw_relative_filename: Blob,
    pub content: Blob,
}

/// Texture definition
#[derive(Debug, Clone)]
pub struct Texture {
    pub element: Element,
    pub instances: Vec<usize>,

    pub texture_type: TextureType,
    pub filename: FbxString,
    pub absolute_filename: FbxString,
    pub relative_filename: FbxString,
    pub raw_filename: Blob,
    pub content: Blob,

    pub video: Option<usize>,
    pub file_index: u32,
    pub has_file: bool,

    pub layers: Vec<TextureLayer>,
    pub file_textures: Vec<usize>,

    pub uv_set: FbxString,
    pub wrap_u: WrapMode,
    pub wrap_v: WrapMode,

    pub has_uv_transform: bool,
    pub uv_transform: Transform,
    pub texture_to_uv: Matrix,
    pub uv_to_texture: Matrix,
}

// =============================================================================
// Deformers
// =============================================================================

/// Skinning method
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkinningMethod {
    Linear,
    Rigid,
    DualQuaternion,
    BlendedDqLinear,
}

/// Skin weight per vertex
#[derive(Debug, Clone, Copy)]
pub struct SkinVertex {
    pub weight_begin: u32,
    pub num_weights: u32,
    pub dq_weight: Real,
}

/// Individual bone weight
#[derive(Debug, Clone, Copy)]
pub struct SkinWeight {
    pub cluster_index: u32,
    pub weight: Real,
}

/// Skin deformer binding skeleton to mesh
#[derive(Debug, Clone)]
pub struct SkinDeformer {
    pub element: Element,

    pub skinning_method: SkinningMethod,
    pub clusters: Vec<usize>,
    pub vertices: Vec<SkinVertex>,
    pub weights: Vec<SkinWeight>,
    pub max_weights_per_vertex: usize,

    pub num_dq_weights: usize,
    pub dq_vertices: Vec<u32>,
    pub dq_weights: Vec<Real>,
}

/// Single bone cluster
#[derive(Debug, Clone)]
pub struct SkinCluster {
    pub element: Element,

    pub bone_node: Option<usize>,
    pub geometry_to_bone: Matrix,
    pub mesh_node_to_bone: Matrix,
    pub bind_to_world: Matrix,
    pub geometry_to_world: Matrix,
    pub geometry_to_world_transform: Transform,

    pub num_weights: usize,
    pub vertices: Vec<u32>,
    pub weights: Vec<Real>,
}

/// Blend shape keyframe
#[derive(Debug, Clone)]
pub struct BlendKeyframe {
    pub shape: usize,
    pub target_weight: Real,
    pub effective_weight: Real,
}

/// Blend shape channel
#[derive(Debug, Clone)]
pub struct BlendChannel {
    pub element: Element,

    pub weight: Real,
    pub keyframes: Vec<BlendKeyframe>,
    pub target_shape: Option<usize>,
}

/// Blend shape with vertex offsets
#[derive(Debug, Clone)]
pub struct BlendShape {
    pub element: Element,

    pub num_offsets: usize,
    pub offset_vertices: Vec<u32>,
    pub position_offsets: Vec<Vec3>,
    pub normal_offsets: Vec<Vec3>,
    pub offset_weights: Vec<Real>,
}

/// Blend deformer
#[derive(Debug, Clone)]
pub struct BlendDeformer {
    pub element: Element,
    pub channels: Vec<usize>,
}

// =============================================================================
// Animation
// =============================================================================

/// Animation interpolation mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interpolation {
    ConstantPrev,
    ConstantNext,
    Linear,
    Cubic,
}

/// Extrapolation mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtrapolationMode {
    Constant,
    Repeat,
    Mirror,
    Slope,
    RepeatRelative,
}

/// Extrapolation settings
#[derive(Debug, Clone, Copy)]
pub struct Extrapolation {
    pub mode: ExtrapolationMode,
    pub repeat_count: i32,
}

/// Cubic tangent
#[derive(Debug, Clone, Copy)]
pub struct Tangent {
    pub dx: f32,
    pub dy: f32,
}

/// Animation keyframe
#[derive(Debug, Clone, Copy)]
pub struct Keyframe {
    pub time: f64,
    pub value: Real,
    pub interpolation: Interpolation,
    pub left: Tangent,
    pub right: Tangent,
}

/// Animation curve
#[derive(Debug, Clone)]
pub struct AnimCurve {
    pub element: Element,

    pub keyframes: Vec<Keyframe>,
    pub pre_extrapolation: Extrapolation,
    pub post_extrapolation: Extrapolation,

    pub min_value: Real,
    pub max_value: Real,
    pub min_time: f64,
    pub max_time: f64,
}

/// Animation value (3 curves for X/Y/Z or R/G/B)
#[derive(Debug, Clone)]
pub struct AnimValue {
    pub element: Element,

    pub default_value: Vec3,
    pub curves: [Option<usize>; 3],
}

/// Animated property
#[derive(Debug, Clone)]
pub struct AnimProp {
    pub element: usize,
    pub prop_name: FbxString,
    pub anim_value: usize,
}

/// Animation layer
#[derive(Debug, Clone)]
pub struct AnimLayer {
    pub element: Element,

    pub weight: Real,
    pub weight_is_animated: bool,
    pub blended: bool,
    pub additive: bool,
    pub compose_rotation: bool,
    pub compose_scale: bool,

    pub anim_values: Vec<usize>,
    pub anim_props: Vec<AnimProp>,
}

/// Animation stack (take)
#[derive(Debug, Clone)]
pub struct AnimStack {
    pub element: Element,

    pub time_begin: f64,
    pub time_end: f64,
    pub layers: Vec<usize>,
}

/// Animation descriptor
#[derive(Debug, Clone)]
pub struct Anim {
    pub time_begin: f64,
    pub time_end: f64,
    pub layers: Vec<usize>,
}

// =============================================================================
// Scene
// =============================================================================

/// Coordinate axis
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordinateAxis {
    PositiveX,
    NegativeX,
    PositiveY,
    NegativeY,
    PositiveZ,
    NegativeZ,
}

impl CoordinateAxis {
    /// Convert axis to index (0=X, 1=Y, 2=Z)
    pub fn to_axis_index(&self) -> usize {
        match self {
            CoordinateAxis::PositiveX | CoordinateAxis::NegativeX => 0,
            CoordinateAxis::PositiveY | CoordinateAxis::NegativeY => 1,
            CoordinateAxis::PositiveZ | CoordinateAxis::NegativeZ => 2,
        }
    }

    /// Check if axis is positive
    pub fn is_positive(&self) -> bool {
        match self {
            CoordinateAxis::PositiveX | CoordinateAxis::PositiveY | CoordinateAxis::PositiveZ => true,
            CoordinateAxis::NegativeX | CoordinateAxis::NegativeY | CoordinateAxis::NegativeZ => false,
        }
    }
}

/// Coordinate axes configuration
#[derive(Debug, Clone, Copy)]
pub struct CoordinateAxes {
    pub right: CoordinateAxis,
    pub up: CoordinateAxis,
    pub front: CoordinateAxis,
}

/// FBX file format version
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileFormat {
    Unknown,
    Fbx,
    Obj,
    Mtl,
}

/// FBX exporter
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exporter {
    Unknown,
    FbxSdk,
    Blender,
    MotionBuilder,
    Maya,
    Max,
    Cinema4d,
}

/// Scene metadata
#[derive(Debug, Clone)]
pub struct Metadata {
    pub version: u32,
    pub file_format: FileFormat,
    pub exporter: Exporter,
    pub exporter_version: u32,

    pub creator: FbxString,
    pub is_big_endian: bool,
    pub filename: FbxString,
    pub relative_root: FbxString,
    pub raw_filename: Blob,
    pub raw_relative_root: Blob,

    pub ascii: bool,
    pub ktime: i64,
    pub original_file_path: FbxString,
}

/// Scene settings
#[derive(Debug, Clone)]
pub struct SceneSettings {
    pub axes: CoordinateAxes,
    pub unit_meters: Real,
    pub frames_per_second: f64,
    pub ambient_color: Vec3,
    pub default_camera: FbxString,

    pub original_axis_up: CoordinateAxis,
    pub original_unit_meters: Real,
    pub space_scale: Real,
    pub time_mode: i32,
    pub time_protocol: i32,
    pub snap_mode: i32,
}

/// Root scene structure
#[derive(Debug, Clone)]
pub struct Scene {
    pub metadata: Metadata,
    pub settings: SceneSettings,

    pub root_node: usize,
    pub anim: Option<Anim>,

    // All elements
    pub unknowns: Vec<Element>,
    pub nodes: Vec<Node>,
    pub meshes: Vec<Mesh>,
    pub lights: Vec<Light>,
    pub cameras: Vec<Camera>,
    pub materials: Vec<Material>,
    pub textures: Vec<Texture>,
    pub texture_files: Vec<TextureFile>,

    // Deformers
    pub skin_deformers: Vec<SkinDeformer>,
    pub skin_clusters: Vec<SkinCluster>,
    pub blend_deformers: Vec<BlendDeformer>,
    pub blend_channels: Vec<BlendChannel>,
    pub blend_shapes: Vec<BlendShape>,

    // Animation
    pub anim_stacks: Vec<AnimStack>,
    pub anim_layers: Vec<AnimLayer>,
    pub anim_values: Vec<AnimValue>,
    pub anim_curves: Vec<AnimCurve>,

    // Connections
    pub connections: Vec<Connection>,
}

impl Scene {
    pub fn new() -> Self {
        Self {
            metadata: Metadata {
                version: 0,
                file_format: FileFormat::Unknown,
                exporter: Exporter::Unknown,
                exporter_version: 0,
                creator: FbxString::new(""),
                is_big_endian: false,
                filename: FbxString::new(""),
                relative_root: FbxString::new(""),
                raw_filename: Blob::new(Vec::new()),
                raw_relative_root: Blob::new(Vec::new()),
                ascii: false,
                ktime: 0,
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
            unknowns: Vec::new(),
            nodes: Vec::new(),
            meshes: Vec::new(),
            lights: Vec::new(),
            cameras: Vec::new(),
            materials: Vec::new(),
            textures: Vec::new(),
            texture_files: Vec::new(),
            skin_deformers: Vec::new(),
            skin_clusters: Vec::new(),
            blend_deformers: Vec::new(),
            blend_channels: Vec::new(),
            blend_shapes: Vec::new(),
            anim_stacks: Vec::new(),
            anim_layers: Vec::new(),
            anim_values: Vec::new(),
            anim_curves: Vec::new(),
            connections: Vec::new(),
        }
    }
}

impl Default for Scene {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Default Implementations
// =============================================================================

impl Default for Mesh {
    fn default() -> Self {
        Self {
            element: Element::new("", ElementType::Mesh),
            instances: Vec::new(),
            num_vertices: 0,
            num_indices: 0,
            num_faces: 0,
            num_triangles: 0,
            num_edges: 0,
            max_face_triangles: 0,
            num_empty_faces: 0,
            num_point_faces: 0,
            num_line_faces: 0,
            faces: Vec::new(),
            face_smoothing: Vec::new(),
            face_material: Vec::new(),
            face_group: Vec::new(),
            face_hole: Vec::new(),
            edges: Vec::new(),
            edge_smoothing: Vec::new(),
            edge_crease: Vec::new(),
            edge_visibility: Vec::new(),
            vertex_indices: Vec::new(),
            vertices: Vec::new(),
            vertex_first_index: Vec::new(),
            vertex_position: VertexAttrib::default(),
            vertex_normal: VertexAttrib::default(),
            vertex_uv: VertexAttrib::default(),
            vertex_tangent: VertexAttrib::default(),
            vertex_bitangent: VertexAttrib::default(),
            vertex_color: VertexAttrib::default(),
            vertex_crease: VertexAttrib::default(),
            uv_sets: Vec::new(),
            color_sets: Vec::new(),
            materials: Vec::new(),
            face_groups: Vec::new(),
            material_parts: Vec::new(),
            face_group_parts: Vec::new(),
            material_part_usage_order: Vec::new(),
            skinned_is_local: false,
            skinned_position: VertexAttrib::default(),
            skinned_normal: VertexAttrib::default(),
            skin_deformers: Vec::new(),
            blend_deformers: Vec::new(),
            cache_deformers: Vec::new(),
            all_deformers: Vec::new(),
            subdivision_preview_levels: 0,
            subdivision_render_levels: 0,
            subdivision_display_mode: SubdivisionDisplayMode::Disabled,
            subdivision_boundary: SubdivisionBoundary::Default,
            subdivision_uv_boundary: SubdivisionBoundary::Default,
            reversed_winding: false,
            generated_normals: false,
            subdivision_evaluated: false,
            from_tessellated_nurbs: false,
        }
    }
}

impl Default for SkinDeformer {
    fn default() -> Self {
        Self {
            element: Element::new("", ElementType::SkinDeformer),
            skinning_method: SkinningMethod::Linear,
            clusters: Vec::new(),
            vertices: Vec::new(),
            weights: Vec::new(),
            max_weights_per_vertex: 0,
            num_dq_weights: 0,
            dq_vertices: Vec::new(),
            dq_weights: Vec::new(),
        }
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vec3_constants() {
        assert_eq!(Vec3::ZERO.x, 0.0);
        assert_eq!(Vec3::ZERO.y, 0.0);
        assert_eq!(Vec3::ZERO.z, 0.0);

        assert_eq!(Vec3::ONE.x, 1.0);
        assert_eq!(Vec3::ONE.y, 1.0);
        assert_eq!(Vec3::ONE.z, 1.0);
    }

    #[test]
    fn test_transform_identity() {
        let t = Transform::IDENTITY;
        assert_eq!(t.translation, Vec3::ZERO);
        assert_eq!(t.rotation, Quat::IDENTITY);
        assert_eq!(t.scale, Vec3::ONE);
    }

    #[test]
    fn test_matrix_identity() {
        let m = Matrix::IDENTITY;
        assert_eq!(m.at(0, 0), 1.0);
        assert_eq!(m.at(1, 1), 1.0);
        assert_eq!(m.at(2, 2), 1.0);
        assert_eq!(m.at(0, 1), 0.0);
    }

    #[test]
    fn test_scene_creation() {
        let scene = Scene::new();
        assert_eq!(scene.nodes.len(), 0);
        assert_eq!(scene.meshes.len(), 0);
    }

    #[test]
    fn test_fbx_string() {
        let s = FbxString::new("test");
        assert_eq!(s.as_str(), "test");

        let s2: FbxString = "hello".into();
        assert_eq!(s2.as_str(), "hello");
    }

    #[test]
    fn test_blob() {
        let data = vec![1, 2, 3, 4];
        let blob = Blob::new(data.clone());
        assert_eq!(blob.len(), 4);
        assert!(!blob.is_empty());
        assert_eq!(blob.as_slice(), &data[..]);
    }
}
