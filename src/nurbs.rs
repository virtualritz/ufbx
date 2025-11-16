//! NURBS Curve and Surface Evaluation
//!
//! This module provides evaluation and tessellation algorithms for NURBS
//! (Non-Uniform Rational B-Splines) curves and surfaces.
//!
//! ## Key Algorithms
//!
//! - **Cox-de Boor recursion**: Basis function evaluation for B-splines
//! - **De Boor's algorithm**: Efficient curve point evaluation
//! - **Tensor product surfaces**: Bivariate surface evaluation
//! - **Adaptive tessellation**: Convert NURBS to polygon mesh
//!
//! ## Features
//!
//! This module is only available with the `nurbs` feature flag enabled.

use crate::error::{Error, Result};
use crate::types::{Real, Vec2, Vec3, Vec4, Mesh, Face, VertexAttrib};

// =============================================================================
// NURBS Types
// =============================================================================

/// NURBS topology (periodicity) mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NurbsTopology {
    /// Open curve/surface with no wrapping
    Open,
    /// Periodic curve/surface
    Periodic,
    /// Closed curve/surface (repeats first control points after the end)
    Closed,
}

/// NURBS basis function definition
///
/// Defines the parametrization for one dimension (U or V) of a NURBS curve or surface.
/// Uses B-spline basis functions with knot vectors.
#[derive(Debug, Clone)]
pub struct NurbsBasis {
    /// Number of control points influencing a point (degree + 1)
    pub order: usize,

    /// Topology (periodicity) of the dimension
    pub topology: NurbsTopology,

    /// Knot vector subdividing the parameter range
    pub knot_vector: Vec<Real>,

    /// Parameter range [t_min, t_max]
    pub t_min: Real,
    pub t_max: Real,

    /// Parameter values of control points (span boundaries)
    pub spans: Vec<Real>,

    /// True if this is a 2D curve
    pub is_2d: bool,

    /// Number of control points to wrap at the end
    pub num_wrap_control_points: usize,

    /// True if the parametrization is well-defined
    pub valid: bool,
}

impl NurbsBasis {
    /// Get the degree of the basis (order - 1)
    pub fn degree(&self) -> usize {
        self.order.saturating_sub(1)
    }
}

/// NURBS curve definition
#[derive(Debug, Clone)]
pub struct NurbsCurve {
    /// Name of the curve
    pub name: String,

    /// Basis function for the curve
    pub basis: NurbsBasis,

    /// Control points (homogeneous coordinates: x, y, z, w)
    /// Note: These are NOT pre-multiplied by w
    pub control_points: Vec<Vec4>,
}

/// NURBS surface definition
#[derive(Debug, Clone)]
pub struct NurbsSurface {
    /// Name of the surface
    pub name: String,

    /// Basis function for U direction
    pub basis_u: NurbsBasis,

    /// Basis function for V direction
    pub basis_v: NurbsBasis,

    /// Number of control points in U direction
    pub num_control_points_u: usize,

    /// Number of control points in V direction
    pub num_control_points_v: usize,

    /// Control points in 2D grid (V * num_u + U)
    /// Note: These are NOT pre-multiplied by w
    pub control_points: Vec<Vec4>,

    /// Subdivision resolution for U direction
    pub span_subdivision_u: usize,

    /// Subdivision resolution for V direction
    pub span_subdivision_v: usize,

    /// Whether to flip normals when evaluated
    pub flip_normals: bool,
}

/// Result of evaluating a NURBS curve at a parameter value
#[derive(Debug, Clone, Copy)]
pub struct CurvePoint {
    /// Whether the evaluation was successful
    pub valid: bool,

    /// Position on the curve
    pub position: Vec3,

    /// Tangent vector (derivative with respect to parameter)
    pub derivative: Vec3,
}

/// Result of evaluating a NURBS surface at UV coordinates
#[derive(Debug, Clone, Copy)]
pub struct SurfacePoint {
    /// Whether the evaluation was successful
    pub valid: bool,

    /// Position on the surface
    pub position: Vec3,

    /// Partial derivative in U direction (tangent)
    pub derivative_u: Vec3,

    /// Partial derivative in V direction (binormal)
    pub derivative_v: Vec3,
}

// =============================================================================
// NURBS Evaluation
// =============================================================================

/// NURBS evaluator using De Boor's algorithm
pub struct NurbsEvaluator;

impl NurbsEvaluator {
    /// Maximum NURBS order supported (matches ufbx C implementation)
    const MAX_ORDER: usize = 4;

    /// Evaluate NURBS basis functions at parameter value u
    ///
    /// Implements the Cox-de Boor recursion formula for B-spline basis functions.
    ///
    /// # Arguments
    ///
    /// * `basis` - NURBS basis definition
    /// * `u` - Parameter value to evaluate at
    /// * `weights` - Output buffer for basis function weights (length >= order)
    /// * `derivatives` - Optional output buffer for derivatives (length >= order)
    ///
    /// # Returns
    ///
    /// Base index of the first control point, or None if evaluation failed
    pub fn evaluate_basis(
        basis: &NurbsBasis,
        u: Real,
        weights: &mut [Real],
        derivatives: Option<&mut [Real]>,
    ) -> Option<usize> {
        if !basis.valid || basis.order == 0 {
            return None;
        }

        let degree = basis.degree();
        if degree < 1 {
            return None;
        }

        // Clamp parameter to valid range and find knot span
        let (knot, u) = Self::find_knot_span(basis, u)?;

        if knot < degree {
            return None;
        }

        // Check output buffer sizes
        if weights.len() < basis.order {
            return Some(knot - degree);
        }

        // Initialize first basis function
        weights[0] = 1.0;

        // Build up basis functions using Cox-de Boor recursion
        for p in 1..=degree {
            let mut prev = 0.0;
            let mut g = 1.0 - Self::nurbs_weight(&basis.knot_vector, knot - p + 1, p, u);
            let mut dg = if derivatives.is_some() && p == degree {
                Self::nurbs_deriv(&basis.knot_vector, knot - p + 1, p)
            } else {
                0.0
            };

            for i in (1..=p).rev() {
                let f = Self::nurbs_weight(&basis.knot_vector, knot - p + i, p, u);
                let weight = weights[i - 1];
                weights[i] = f * weight + g * prev;

                if let Some(ref mut derivs) = derivatives {
                    if p == degree {
                        let df = Self::nurbs_deriv(&basis.knot_vector, knot - p + i, p);
                        if i < derivs.len() {
                            derivs[i] = df * weight - dg * prev;
                        }
                        dg = df;
                    }
                }

                prev = weight;
                g = 1.0 - f;
            }

            weights[0] = g * prev;
            if let Some(ref mut derivs) = derivatives {
                if p == degree && derivs.len() > 0 {
                    derivs[0] = -dg * prev;
                }
            }
        }

        Some(knot - degree)
    }

    /// Find the knot span containing parameter u
    ///
    /// Uses binary search to find the interval [knot[i], knot[i+1]) containing u.
    fn find_knot_span(basis: &NurbsBasis, u: Real) -> Option<(usize, Real)> {
        let degree = basis.degree();
        let knots = &basis.knot_vector;

        // Clamp to valid range
        let (knot, u) = if u <= basis.t_min {
            (degree, basis.t_min)
        } else if u >= basis.t_max {
            (knots.len().checked_sub(degree + 2)?, basis.t_max)
        } else {
            // Binary search for the knot span
            let mut low = 0;
            let mut high = knots.len() - 1;
            let mut knot = degree;

            while low <= high {
                let mid = (low + high) / 2;
                if mid + 1 >= knots.len() {
                    break;
                }

                if knots[mid + 1] <= u {
                    low = mid + 1;
                    knot = mid + 1;
                } else if knots[mid] <= u && u < knots[mid + 1] {
                    knot = mid;
                    break;
                } else {
                    if mid == 0 {
                        break;
                    }
                    high = mid - 1;
                }
            }

            (knot, u)
        };

        Some((knot, u))
    }

    /// Calculate NURBS basis weight (blend factor)
    ///
    /// Returns the weight for blending between adjacent basis functions.
    fn nurbs_weight(knots: &[Real], knot: usize, degree: usize, u: Real) -> Real {
        if knot >= knots.len() || knots.len() - knot < degree {
            return 0.0;
        }

        let prev_u = knots[knot];
        let next_u = knots[knot + degree];

        if prev_u >= next_u {
            return 0.0;
        }
        if u <= prev_u {
            return 0.0;
        }
        if u >= next_u {
            return 1.0;
        }

        (u - prev_u) / (next_u - prev_u)
    }

    /// Calculate NURBS basis derivative weight
    fn nurbs_deriv(knots: &[Real], knot: usize, degree: usize) -> Real {
        if knot >= knots.len() || knots.len() - knot < degree {
            return 0.0;
        }

        let prev_u = knots[knot];
        let next_u = knots[knot + degree];

        if prev_u >= next_u {
            return 0.0;
        }

        degree as Real / (next_u - prev_u)
    }

    /// Evaluate a NURBS curve at parameter value u
    ///
    /// Uses De Boor's algorithm to compute the position and derivative.
    ///
    /// # Arguments
    ///
    /// * `curve` - NURBS curve to evaluate
    /// * `u` - Parameter value (typically in range [t_min, t_max])
    ///
    /// # Returns
    ///
    /// Curve point with position and derivative, or invalid point if evaluation failed
    pub fn evaluate_curve(curve: &NurbsCurve, u: Real) -> CurvePoint {
        let mut result = CurvePoint {
            valid: false,
            position: Vec3::ZERO,
            derivative: Vec3::ZERO,
        };

        if curve.control_points.is_empty() {
            return result;
        }

        let order = curve.basis.order.min(Self::MAX_ORDER);
        let mut weights = [0.0; Self::MAX_ORDER];
        let mut derivs = [0.0; Self::MAX_ORDER];

        let base = match Self::evaluate_basis(
            &curve.basis,
            u,
            &mut weights[..order],
            Some(&mut derivs[..order]),
        ) {
            Some(b) => b,
            None => return result,
        };

        // Accumulate weighted control points (rational curve evaluation)
        let mut p = Vec4::ZERO;
        let mut d = Vec4::ZERO;

        for i in 0..order {
            let ix = (base + i) % curve.control_points.len();
            let cp = curve.control_points[ix];

            let weight = weights[i] * cp.w;
            let deriv = derivs[i] * cp.w;

            p.x += cp.x * weight;
            p.y += cp.y * weight;
            p.z += cp.z * weight;
            p.w += weight;

            d.x += cp.x * deriv;
            d.y += cp.y * deriv;
            d.z += cp.z * deriv;
            d.w += deriv;
        }

        // Perspective divide for rational curves
        if p.w.abs() < 1e-10 {
            return result;
        }

        let rcp_w = 1.0 / p.w;
        result.valid = true;
        result.position.x = p.x * rcp_w;
        result.position.y = p.y * rcp_w;
        result.position.z = p.z * rcp_w;

        // Derivative with quotient rule
        result.derivative.x = (d.x - d.w * result.position.x) * rcp_w;
        result.derivative.y = (d.y - d.w * result.position.y) * rcp_w;
        result.derivative.z = (d.z - d.w * result.position.z) * rcp_w;

        result
    }

    /// Evaluate a NURBS surface at parameter values (u, v)
    ///
    /// Uses tensor product evaluation: evaluate in U direction, then V direction.
    ///
    /// # Arguments
    ///
    /// * `surface` - NURBS surface to evaluate
    /// * `u` - U parameter value
    /// * `v` - V parameter value
    ///
    /// # Returns
    ///
    /// Surface point with position and partial derivatives
    pub fn evaluate_surface(surface: &NurbsSurface, u: Real, v: Real) -> SurfacePoint {
        let mut result = SurfacePoint {
            valid: false,
            position: Vec3::ZERO,
            derivative_u: Vec3::ZERO,
            derivative_v: Vec3::ZERO,
        };

        if surface.num_control_points_u == 0 || surface.num_control_points_v == 0 {
            return result;
        }

        let order_u = surface.basis_u.order.min(Self::MAX_ORDER);
        let order_v = surface.basis_v.order.min(Self::MAX_ORDER);

        let mut weights_u = [0.0; Self::MAX_ORDER];
        let mut weights_v = [0.0; Self::MAX_ORDER];
        let mut derivs_u = [0.0; Self::MAX_ORDER];
        let mut derivs_v = [0.0; Self::MAX_ORDER];

        let base_u = match Self::evaluate_basis(
            &surface.basis_u,
            u,
            &mut weights_u[..order_u],
            Some(&mut derivs_u[..order_u]),
        ) {
            Some(b) => b,
            None => return result,
        };

        let base_v = match Self::evaluate_basis(
            &surface.basis_v,
            v,
            &mut weights_v[..order_v],
            Some(&mut derivs_v[..order_v]),
        ) {
            Some(b) => b,
            None => return result,
        };

        // Tensor product evaluation
        let mut p = Vec4::ZERO;
        let mut du = Vec4::ZERO;
        let mut dv = Vec4::ZERO;

        let num_u = surface.num_control_points_u;
        let num_v = surface.num_control_points_v;

        for vi in 0..order_v {
            let vix = (base_v + vi) % num_v;
            let weight_v = weights_v[vi];
            let deriv_v = derivs_v[vi];

            for ui in 0..order_u {
                let uix = (base_u + ui) % num_u;
                let cp_idx = vix * num_u + uix;

                if cp_idx >= surface.control_points.len() {
                    return result;
                }

                let cp = surface.control_points[cp_idx];
                let weight_u = weights_u[ui];
                let deriv_u = derivs_u[ui];

                let weight = weight_u * weight_v * cp.w;
                let wderiv_u = deriv_u * weight_v * cp.w;
                let wderiv_v = deriv_v * weight_u * cp.w;

                p.x += cp.x * weight;
                p.y += cp.y * weight;
                p.z += cp.z * weight;
                p.w += weight;

                du.x += cp.x * wderiv_u;
                du.y += cp.y * wderiv_u;
                du.z += cp.z * wderiv_u;
                du.w += wderiv_u;

                dv.x += cp.x * wderiv_v;
                dv.y += cp.y * wderiv_v;
                dv.z += cp.z * wderiv_v;
                dv.w += wderiv_v;
            }
        }

        // Perspective divide
        if p.w.abs() < 1e-10 {
            return result;
        }

        let rcp_w = 1.0 / p.w;
        result.valid = true;
        result.position.x = p.x * rcp_w;
        result.position.y = p.y * rcp_w;
        result.position.z = p.z * rcp_w;

        // Partial derivatives with quotient rule
        result.derivative_u.x = (du.x - du.w * result.position.x) * rcp_w;
        result.derivative_u.y = (du.y - du.w * result.position.y) * rcp_w;
        result.derivative_u.z = (du.z - du.w * result.position.z) * rcp_w;

        result.derivative_v.x = (dv.x - dv.w * result.position.x) * rcp_w;
        result.derivative_v.y = (dv.y - dv.w * result.position.y) * rcp_w;
        result.derivative_v.z = (dv.z - dv.w * result.position.z) * rcp_w;

        result
    }

    /// Tessellate a NURBS curve to line segments
    ///
    /// # Arguments
    ///
    /// * `curve` - NURBS curve to tessellate
    /// * `segments_per_span` - Number of line segments per knot span
    ///
    /// # Returns
    ///
    /// Vector of points along the curve, or error if tessellation failed
    pub fn tessellate_curve(
        curve: &NurbsCurve,
        segments_per_span: usize,
    ) -> Result<Vec<Vec3>> {
        if !curve.basis.valid || curve.control_points.is_empty() {
            return Err(Error::BadNurbs { description: "Invalid NURBS curve basis or empty control points".to_string() });
        }

        let num_sub = if segments_per_span > 0 {
            segments_per_span
        } else {
            4 // Default subdivision
        };

        let num_spans = curve.basis.spans.len().saturating_sub(1);
        if num_spans == 0 {
            return Err(Error::BadNurbs { description: "NURBS curve has no spans".to_string() });
        }

        let is_open = curve.basis.topology == NurbsTopology::Open;

        // Calculate number of output vertices
        let num_indices = num_spans + (num_spans - 1) * (num_sub - 1);
        let num_vertices = if is_open {
            num_indices
        } else {
            num_indices - 1
        };

        let mut vertices = Vec::with_capacity(num_vertices);

        for span_ix in 0..num_spans {
            let num_splits = if span_ix + 1 == num_spans { 1 } else { num_sub };

            for sub_ix in 0..num_splits {
                let ix = span_ix * num_sub + sub_ix;

                if ix < num_vertices {
                    // Interpolate parameter value within span
                    let u = if sub_ix == 0 {
                        curve.basis.spans[span_ix]
                    } else {
                        let t = sub_ix as Real / num_sub as Real;
                        let u0 = curve.basis.spans[span_ix];
                        let u1 = curve.basis.spans[span_ix + 1];
                        u0 * (1.0 - t) + u1 * t
                    };

                    let point = Self::evaluate_curve(curve, u);
                    if !point.valid {
                        return Err(Error::BadNurbs { description: "Failed to evaluate NURBS curve".to_string() });
                    }

                    vertices.push(point.position);
                }
            }
        }

        Ok(vertices)
    }

    /// Tessellate a NURBS surface to a polygon mesh
    ///
    /// # Arguments
    ///
    /// * `surface` - NURBS surface to tessellate
    /// * `u_resolution` - Number of subdivisions per U span
    /// * `v_resolution` - Number of subdivisions per V span
    ///
    /// # Returns
    ///
    /// Mesh with positions, normals, UVs, tangents, and faces
    pub fn tessellate_surface(
        surface: &NurbsSurface,
        u_resolution: usize,
        v_resolution: usize,
    ) -> Result<Mesh> {
        if !surface.basis_u.valid || !surface.basis_v.valid {
            return Err(Error::BadNurbs { description: "Invalid NURBS surface basis".to_string() });
        }

        if surface.num_control_points_u == 0 || surface.num_control_points_v == 0 {
            return Err(Error::BadNurbs { description: "NURBS surface has no control points".to_string() });
        }

        let sub_u = if u_resolution > 0 { u_resolution } else { 4 };
        let sub_v = if v_resolution > 0 { v_resolution } else { 4 };

        let spans_u = surface.basis_u.spans.len().saturating_sub(1);
        let spans_v = surface.basis_v.spans.len().saturating_sub(1);

        if spans_u == 0 || spans_v == 0 {
            return Err(Error::BadNurbs { description: "NURBS surface has no spans".to_string() });
        }

        let open_u = surface.basis_u.topology == NurbsTopology::Open;
        let open_v = surface.basis_v.topology == NurbsTopology::Open;

        // Calculate grid dimensions
        let indices_u = spans_u + (spans_u - 1) * (sub_u - 1);
        let indices_v = spans_v + (spans_v - 1) * (sub_v - 1);
        let faces_u = (spans_u - 1) * sub_u;
        let faces_v = (spans_v - 1) * sub_v;

        let num_indices = indices_u * indices_v;
        let num_faces = faces_u * faces_v;

        // Pre-allocate buffers
        let mut positions = Vec::with_capacity(num_indices);
        let mut uvs = Vec::with_capacity(num_indices);
        let mut tangents = Vec::with_capacity(num_indices);
        let mut bitangents = Vec::with_capacity(num_indices);

        // Evaluate surface at grid points
        for span_v in 0..=spans_v {
            let splits_v = if span_v == spans_v { 1 } else { sub_v };

            for split_v in 0..splits_v {
                let v_param = Self::compute_param(&surface.basis_v.spans, span_v, split_v, splits_v);
                let original_v = v_param;

                for span_u in 0..=spans_u {
                    let splits_u = if span_u == spans_u { 1 } else { sub_u };

                    for split_u in 0..splits_u {
                        let u_param = Self::compute_param(&surface.basis_u.spans, span_u, split_u, splits_u);
                        let original_u = u_param;

                        let point = Self::evaluate_surface(surface, u_param, v_param);
                        if !point.valid {
                            return Err(Error::BadNurbs { description: "Failed to evaluate NURBS surface".to_string() });
                        }

                        positions.push(point.position);
                        uvs.push(Vec2::new(original_u, original_v));
                        tangents.push(Self::normalize_vec3(point.derivative_u));
                        bitangents.push(Self::normalize_vec3(point.derivative_v));
                    }
                }
            }
        }

        // Generate faces (quads)
        let mut faces = Vec::with_capacity(num_faces);
        let mut vertex_indices = Vec::with_capacity(num_faces * 4);

        for face_v in 0..faces_v {
            for face_u in 0..faces_u {
                let i0 = (face_v + 0) * indices_u + (face_u + 0);
                let i1 = (face_v + 0) * indices_u + (face_u + 1);
                let i2 = (face_v + 1) * indices_u + (face_u + 1);
                let i3 = (face_v + 1) * indices_u + (face_u + 0);

                let index_begin = vertex_indices.len() as u32;

                // Check for degenerate quads and convert to triangles
                let indices = [i0, i1, i2, i3];
                let unique_indices: Vec<_> = {
                    let mut unique = Vec::new();
                    for &idx in &indices {
                        if unique.is_empty() || unique.last() != Some(&idx) {
                            unique.push(idx);
                        }
                    }
                    unique
                };

                let num_indices = unique_indices.len().min(4) as u32;

                for &idx in &unique_indices[..num_indices as usize] {
                    vertex_indices.push(idx as u32);
                }

                faces.push(Face {
                    index_begin,
                    num_indices,
                });
            }
        }

        // Compute normals from cross product of tangent and bitangent
        let normals: Vec<Vec3> = tangents
            .iter()
            .zip(bitangents.iter())
            .map(|(&t, &b)| {
                let normal = Self::cross_product(t, b);
                if surface.flip_normals {
                    Vec3::new(-normal.x, -normal.y, -normal.z)
                } else {
                    Self::normalize_vec3(normal)
                }
            })
            .collect();

        // Build mesh
        let num_vertices = positions.len();

        Ok(Mesh {
            element: crate::types::Element::new("", crate::types::ElementType::Mesh),
            instances: Vec::new(),
            num_vertices,
            num_indices: vertex_indices.len(),
            num_faces,
            num_triangles: num_faces, // Conservative estimate
            num_edges: 0,
            max_face_triangles: 2,
            num_empty_faces: 0,
            num_point_faces: 0,
            num_line_faces: 0,
            faces,
            face_smoothing: vec![true; num_faces],
            face_material: Vec::new(),
            face_group: Vec::new(),
            face_hole: Vec::new(),
            edges: Vec::new(),
            edge_smoothing: Vec::new(),
            edge_crease: Vec::new(),
            edge_visibility: Vec::new(),
            vertex_indices,
            vertices: positions.clone(),
            vertex_first_index: Vec::new(),
            vertex_position: VertexAttrib {
                exists: true,
                values: positions,
                indices: (0..num_vertices as u32).collect(),
                value_reals: 3,
                unique_per_vertex: true,
                values_w: Vec::new(),
            },
            vertex_normal: VertexAttrib {
                exists: true,
                values: normals,
                indices: (0..num_vertices as u32).collect(),
                value_reals: 3,
                unique_per_vertex: true,
                values_w: Vec::new(),
            },
            vertex_uv: VertexAttrib {
                exists: true,
                values: uvs,
                indices: (0..num_indices as u32).collect(),
                value_reals: 2,
                unique_per_vertex: false,
                values_w: Vec::new(),
            },
            vertex_tangent: VertexAttrib {
                exists: true,
                values: tangents,
                indices: (0..num_indices as u32).collect(),
                value_reals: 3,
                unique_per_vertex: false,
                values_w: Vec::new(),
            },
            vertex_bitangent: VertexAttrib {
                exists: true,
                values: bitangents,
                indices: (0..num_indices as u32).collect(),
                value_reals: 3,
                unique_per_vertex: false,
                values_w: Vec::new(),
            },
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
            subdivision_display_mode: crate::types::SubdivisionDisplayMode::Disabled,
            subdivision_boundary: crate::types::SubdivisionBoundary::Default,
            subdivision_uv_boundary: crate::types::SubdivisionBoundary::Default,
            reversed_winding: false,
            generated_normals: true,
            subdivision_evaluated: false,
            from_tessellated_nurbs: true,
        })
    }

    // Helper functions

    fn compute_param(spans: &[Real], span: usize, split: usize, num_splits: usize) -> Real {
        if split == 0 || span >= spans.len() {
            spans.get(span).copied().unwrap_or(0.0)
        } else {
            let t = split as Real / num_splits as Real;
            let u0 = spans[span];
            let u1 = spans.get(span + 1).copied().unwrap_or(u0);
            u0 * (1.0 - t) + u1 * t
        }
    }

    fn normalize_vec3(v: Vec3) -> Vec3 {
        let len_sq = v.x * v.x + v.y * v.y + v.z * v.z;
        if len_sq < 1e-10 {
            Vec3::ZERO
        } else {
            let len = len_sq.sqrt();
            Vec3::new(v.x / len, v.y / len, v.z / len)
        }
    }

    fn cross_product(a: Vec3, b: Vec3) -> Vec3 {
        Vec3::new(
            a.y * b.z - a.z * b.y,
            a.z * b.x - a.x * b.z,
            a.x * b.y - a.y * b.x,
        )
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nurbs_weight() {
        let knots = vec![0.0, 0.0, 0.0, 1.0, 2.0, 3.0, 3.0, 3.0];

        // Test at boundaries
        assert_eq!(NurbsEvaluator::nurbs_weight(&knots, 0, 3, 0.0), 0.0);
        assert_eq!(NurbsEvaluator::nurbs_weight(&knots, 0, 3, 3.0), 1.0);

        // Test in middle
        let w = NurbsEvaluator::nurbs_weight(&knots, 2, 2, 1.5);
        assert!((w - 0.5).abs() < 1e-6);
    }

    #[test]
    fn test_simple_curve() {
        // Linear curve from (0,0,0) to (1,0,0)
        let curve = NurbsCurve {
            name: "test".to_string(),
            basis: NurbsBasis {
                order: 2,
                topology: NurbsTopology::Open,
                knot_vector: vec![0.0, 0.0, 1.0, 1.0],
                t_min: 0.0,
                t_max: 1.0,
                spans: vec![0.0, 1.0],
                is_2d: false,
                num_wrap_control_points: 0,
                valid: true,
            },
            control_points: vec![
                Vec4::new(0.0, 0.0, 0.0, 1.0),
                Vec4::new(1.0, 0.0, 0.0, 1.0),
            ],
        };

        let p0 = NurbsEvaluator::evaluate_curve(&curve, 0.0);
        assert!(p0.valid);
        assert!((p0.position.x - 0.0).abs() < 1e-6);

        let p1 = NurbsEvaluator::evaluate_curve(&curve, 1.0);
        assert!(p1.valid);
        assert!((p1.position.x - 1.0).abs() < 1e-6);

        let pmid = NurbsEvaluator::evaluate_curve(&curve, 0.5);
        assert!(pmid.valid);
        assert!((pmid.position.x - 0.5).abs() < 1e-6);
    }

    #[test]
    fn test_tessellate_curve() {
        let curve = NurbsCurve {
            name: "test".to_string(),
            basis: NurbsBasis {
                order: 2,
                topology: NurbsTopology::Open,
                knot_vector: vec![0.0, 0.0, 1.0, 1.0],
                t_min: 0.0,
                t_max: 1.0,
                spans: vec![0.0, 1.0],
                is_2d: false,
                num_wrap_control_points: 0,
                valid: true,
            },
            control_points: vec![
                Vec4::new(0.0, 0.0, 0.0, 1.0),
                Vec4::new(1.0, 0.0, 0.0, 1.0),
            ],
        };

        let points = NurbsEvaluator::tessellate_curve(&curve, 4).unwrap();
        assert!(points.len() > 0);

        // First point should be at start
        assert!((points[0].x - 0.0).abs() < 1e-6);

        // Last point should be at end
        let last = points.last().unwrap();
        assert!((last.x - 1.0).abs() < 1e-3);
    }
}
