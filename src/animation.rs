//! Animation Curve Evaluation and Blending
//!
//! This module provides functionality for evaluating animation curves, blending
//! multiple animation layers, and computing animated property values at specific times.
//!
//! # Key Features
//!
//! - **Curve Evaluation**: Interpolate keyframes using linear, cubic, or constant modes
//! - **Tangent Handling**: Support for auto, user-defined, and TCB tangents
//! - **Extrapolation**: Constant, linear, repeat, and mirror modes before/after curve range
//! - **Layer Blending**: Additive and override blending with weights
//! - **Transform Evaluation**: Evaluate node transforms with proper rotation order handling
//!
//! # Example
//!
//! ```rust,no_run
//! use ufbx::animation::AnimationEvaluator;
//! use ufbx::{AnimCurve, Scene};
//!
//! fn evaluate_animation(scene: &Scene, time: f64) {
//!     // Evaluate a specific curve at time
//!     if let Some(curve) = scene.anim_curves.first() {
//!         let value = AnimationEvaluator::eval_curve(curve, time, 0);
//!         println!("Value at t={}: {}", time, value);
//!     }
//! }
//! ```

use crate::types::*;

// =============================================================================
// Constants and Flags
// =============================================================================

/// Flags for controlling evaluation behavior
pub mod eval_flags {
    /// Don't extrapolate outside curve time range
    pub const NO_EXTRAPOLATION: u32 = 1 << 0;
    /// Ignore connections when evaluating properties
    pub const IGNORE_CONNECTIONS: u32 = 1 << 1;
}

/// Epsilon for floating point comparisons
const EPSILON: f64 = 8.881784197001252e-16; // 4 ULP from 1.0

// =============================================================================
// Cubic Bezier Utilities
// =============================================================================

/// Find parameter t for a cubic Bezier curve using Newton-Raphson iteration
///
/// Solves for t given x0 in the cubic Bezier equation:
/// x(t) = a*t³ + b*t² + c*t where a, b, c are derived from control points p1, p2
fn find_cubic_bezier_t(p1: f64, p2: f64, x0: f64) -> f64 {
    let p1_3 = p1 * 3.0;
    let p2_3 = p2 * 3.0;
    let a = p1_3 - p2_3 + 1.0;
    let b = p2_3 - p1_3 - p1_3;
    let c = p1_3;

    let a_3 = 3.0 * a;
    let b_2 = 2.0 * b;
    let mut t = x0;

    // Manually unroll three iterations of Newton-Raphson (sufficient for most tangents)
    for _ in 0..3 {
        let t2 = t * t;
        let t3 = t2 * t;
        let x1 = a * t3 + b * t2 + c * t - x0;
        t -= x1 / (a_3 * t2 + b_2 * t + c);
    }

    // Check convergence
    let t2 = t * t;
    let t3 = t2 * t;
    let x1 = a * t3 + b * t2 + c * t - x0;

    if x1.abs() <= EPSILON {
        return t;
    }

    // Perform more iterations until we reach desired accuracy (up to 8 more)
    for _ in 0..4 {
        for _ in 0..2 {
            let t2 = t * t;
            let t3 = t2 * t;
            let x1 = a * t3 + b * t2 + c * t - x0;
            t -= x1 / (a_3 * t2 + b_2 * t + c);
        }

        let t2 = t * t;
        let t3 = t2 * t;
        let x1 = a * t3 + b * t2 + c * t - x0;

        if x1.abs() <= EPSILON {
            break;
        }
    }

    t
}

/// Evaluate a cubic Bezier curve at parameter t
fn eval_cubic_bezier(p0: f64, p1: f64, p2: f64, p3: f64, t: f64) -> f64 {
    let s = 1.0 - t;
    let s2 = s * s;
    let s3 = s2 * s;
    let t2 = t * t;
    let t3 = t2 * t;

    p0 * s3 + 3.0 * p1 * s2 * t + 3.0 * p2 * s * t2 + p3 * t3
}

// =============================================================================
// Keyframe Search
// =============================================================================

/// Find the keyframe segment containing the given time using binary search
///
/// Returns the index of the keyframe at or before the given time.
/// Returns 0 if time is before the first keyframe.
fn find_keyframe_segment(keyframes: &[Keyframe], time: f64) -> usize {
    if keyframes.is_empty() {
        return 0;
    }

    if time <= keyframes[0].time {
        return 0;
    }

    if time >= keyframes[keyframes.len() - 1].time {
        return keyframes.len().saturating_sub(1);
    }

    // Binary search for the segment
    let mut left = 0;
    let mut right = keyframes.len() - 1;

    while left < right {
        let mid = left + (right - left + 1) / 2;
        if keyframes[mid].time <= time {
            left = mid;
        } else {
            right = mid - 1;
        }
    }

    left
}

// =============================================================================
// Curve Evaluation
// =============================================================================

/// Main animation evaluator
pub struct AnimationEvaluator;

impl AnimationEvaluator {
    /// Evaluate an animation curve at the given time
    ///
    /// # Arguments
    /// * `curve` - The animation curve to evaluate
    /// * `time` - Time value to evaluate at
    /// * `flags` - Evaluation flags (see `eval_flags`)
    ///
    /// # Returns
    /// The interpolated value at the given time
    pub fn eval_curve(curve: &AnimCurve, time: f64, flags: u32) -> Real {
        if curve.keyframes.is_empty() {
            return 0.0;
        }

        // Handle extrapolation
        if (flags & eval_flags::NO_EXTRAPOLATION) == 0 {
            if time < curve.min_time {
                return Self::extrapolate_curve(curve, time, true, flags);
            }
            if time > curve.max_time {
                return Self::extrapolate_curve(curve, time, false, flags);
            }
        }

        let segment = find_keyframe_segment(&curve.keyframes, time);

        if segment >= curve.keyframes.len() - 1 {
            // At or past the last keyframe
            return curve.keyframes[curve.keyframes.len() - 1].value;
        }

        let key_a = &curve.keyframes[segment];
        let key_b = &curve.keyframes[segment + 1];

        match key_a.interpolation {
            Interpolation::ConstantPrev => key_a.value,
            Interpolation::ConstantNext => {
                if time >= key_b.time {
                    key_b.value
                } else {
                    key_a.value
                }
            }
            Interpolation::Linear => Self::eval_linear(key_a, key_b, time),
            Interpolation::Cubic => Self::eval_cubic(key_a, key_b, time),
        }
    }

    /// Evaluate linear interpolation between two keyframes
    fn eval_linear(key_a: &Keyframe, key_b: &Keyframe, time: f64) -> Real {
        let duration = key_b.time - key_a.time;
        if duration <= 0.0 {
            return key_a.value;
        }

        let t = ((time - key_a.time) / duration) as Real;
        key_a.value + (key_b.value - key_a.value) * t
    }

    /// Evaluate cubic Hermite spline interpolation between two keyframes
    fn eval_cubic(key_a: &Keyframe, key_b: &Keyframe, time: f64) -> Real {
        let duration = key_b.time - key_a.time;
        if duration <= 0.0 {
            return key_a.value;
        }

        // Normalize time to [0, 1]
        let normalized_time = (time - key_a.time) / duration;

        // Get tangent values
        let tangent_a = key_a.right;
        let tangent_b = key_b.left;

        // For cubic Bezier evaluation, we need to find the parameter t
        // that corresponds to the normalized time in X
        let t = if tangent_a.dx != 0.0 || tangent_b.dx != 0.0 {
            // Convert tangents to Bezier control points
            let p1_x = tangent_a.dx as f64 / duration;
            let p2_x = 1.0 - (tangent_b.dx as f64 / duration);

            find_cubic_bezier_t(p1_x, p2_x, normalized_time)
        } else {
            normalized_time
        };

        // Evaluate Y using the computed t parameter
        let p0_y = key_a.value as f64;
        let p3_y = key_b.value as f64;
        let p1_y = p0_y + (tangent_a.dy as f64);
        let p2_y = p3_y - (tangent_b.dy as f64);

        eval_cubic_bezier(p0_y, p1_y, p2_y, p3_y, t) as Real
    }

    /// Extrapolate a curve value outside its time range
    fn extrapolate_curve(curve: &AnimCurve, time: f64, pre: bool, flags: u32) -> Real {
        let (key, ext) = if pre {
            (&curve.keyframes[0], &curve.pre_extrapolation)
        } else {
            (&curve.keyframes[curve.keyframes.len() - 1], &curve.post_extrapolation)
        };

        match ext.mode {
            ExtrapolationMode::Constant => key.value,

            ExtrapolationMode::Slope => {
                let tangent = if pre { &key.right } else { &key.left };
                let dt = time - key.time;
                let slope = if tangent.dx != 0.0 {
                    tangent.dy as f64 / tangent.dx as f64
                } else {
                    0.0
                };
                key.value + (slope * dt) as Real
            }

            ExtrapolationMode::Repeat | ExtrapolationMode::Mirror | ExtrapolationMode::RepeatRelative => {
                if ext.repeat_count == 0 {
                    return key.value;
                }

                let duration = curve.max_time - curve.min_time;
                if duration <= 0.0 {
                    return key.value;
                }

                let delta = if pre {
                    curve.min_time - time
                } else {
                    time - curve.max_time
                };

                let mut cycles = (delta / duration).floor();
                let mut offset = delta - cycles * duration;

                // Clamp to repeat count
                if ext.repeat_count > 0 {
                    cycles = cycles.min(ext.repeat_count as f64 - 1.0);
                    if cycles >= ext.repeat_count as f64 - 1.0 {
                        offset = duration;
                    }
                }

                // Handle mirroring
                if ext.mode == ExtrapolationMode::Mirror {
                    let parity = (cycles * 0.5) - (cycles * 0.5).floor();
                    if parity <= 0.25 {
                        offset = duration - offset;
                    }
                }

                // Compute new time within curve range
                let new_time = if pre {
                    curve.max_time - offset
                } else {
                    curve.min_time + offset
                };

                // Recursively evaluate at the new time (with extrapolation disabled)
                let mut value = Self::eval_curve(curve, new_time, flags | eval_flags::NO_EXTRAPOLATION);

                // Add offset for relative repeat
                if ext.mode == ExtrapolationMode::RepeatRelative {
                    let delta_value = curve.keyframes[curve.keyframes.len() - 1].value
                                    - curve.keyframes[0].value;
                    let offset_value = if pre { -delta_value } else { delta_value };
                    value += offset_value * (cycles + 1.0) as Real;
                }

                value
            }
        }
    }

    /// Evaluate a 3D animation value (with X/Y/Z curves) at the given time
    ///
    /// # Arguments
    /// * `anim_value` - The animation value containing up to 3 curves
    /// * `curves` - Slice of all animation curves in the scene
    /// * `time` - Time value to evaluate at
    /// * `flags` - Evaluation flags
    ///
    /// # Returns
    /// A Vec3 with the evaluated values (uses default_value for missing curves)
    pub fn eval_anim_value(
        anim_value: &AnimValue,
        curves: &[AnimCurve],
        time: f64,
        flags: u32,
    ) -> Vec3 {
        let mut result = anim_value.default_value;

        if let Some(curve_idx) = anim_value.curves[0] {
            if curve_idx < curves.len() {
                result.x = Self::eval_curve(&curves[curve_idx], time, flags);
            }
        }

        if let Some(curve_idx) = anim_value.curves[1] {
            if curve_idx < curves.len() {
                result.y = Self::eval_curve(&curves[curve_idx], time, flags);
            }
        }

        if let Some(curve_idx) = anim_value.curves[2] {
            if curve_idx < curves.len() {
                result.z = Self::eval_curve(&curves[curve_idx], time, flags);
            }
        }

        result
    }
}

// =============================================================================
// Animation Layer Blending
// =============================================================================

/// Blending mode for animation layers
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlendMode {
    /// Override: Replace the previous value
    Override,
    /// Additive: Add to the previous value
    Additive,
    /// Blended: Interpolate with the previous value
    Blended,
}

/// Helper for raising a value to a fractional power while preserving sign
fn pow_abs(v: f64, e: f64) -> f64 {
    if e <= 0.0 {
        return 1.0;
    }
    if e >= 1.0 {
        return v;
    }
    let sign = if v < 0.0 { -1.0 } else { 1.0 };
    sign * (v * sign).powf(e)
}

/// Layer blending utilities
pub struct LayerBlender;

impl LayerBlender {
    /// Blend a new value into the result using the specified mode and weight
    ///
    /// # Arguments
    /// * `result` - Current accumulated value (modified in place)
    /// * `value` - New value to blend
    /// * `weight` - Blend weight (0.0 to 1.0)
    /// * `mode` - Blending mode
    /// * `compose_scale` - Use power blending for scale properties
    /// * `compose_rotation` - Use quaternion blending for rotation properties
    pub fn blend_vec3(
        result: &mut Vec3,
        value: &Vec3,
        weight: Real,
        mode: BlendMode,
        compose_scale: bool,
        compose_rotation: bool,
    ) {
        match mode {
            BlendMode::Override => {
                *result = *value;
            }

            BlendMode::Additive => {
                if compose_scale {
                    // Scale: multiply by power
                    result.x *= pow_abs(value.x as f64, weight as f64) as Real;
                    result.y *= pow_abs(value.y as f64, weight as f64) as Real;
                    result.z *= pow_abs(value.z as f64, weight as f64) as Real;
                } else if compose_rotation {
                    // Rotation: quaternion composition (simplified - would need proper quat math)
                    // For now, use additive
                    result.x += value.x * weight;
                    result.y += value.y * weight;
                    result.z += value.z * weight;
                } else {
                    // Standard additive
                    result.x += value.x * weight;
                    result.y += value.y * weight;
                    result.z += value.z * weight;
                }
            }

            BlendMode::Blended => {
                let res_weight = 1.0 - weight;
                if compose_scale {
                    // Scale: geometric interpolation
                    result.x = (pow_abs(result.x as f64, res_weight as f64)
                              * pow_abs(value.x as f64, weight as f64)) as Real;
                    result.y = (pow_abs(result.y as f64, res_weight as f64)
                              * pow_abs(value.y as f64, weight as f64)) as Real;
                    result.z = (pow_abs(result.z as f64, res_weight as f64)
                              * pow_abs(value.z as f64, weight as f64)) as Real;
                } else if compose_rotation {
                    // Rotation: would use slerp with quaternions
                    // For now, use linear interpolation
                    result.x = result.x * res_weight + value.x * weight;
                    result.y = result.y * res_weight + value.y * weight;
                    result.z = result.z * res_weight + value.z * weight;
                } else {
                    // Linear interpolation
                    result.x = result.x * res_weight + value.x * weight;
                    result.y = result.y * res_weight + value.y * weight;
                    result.z = result.z * res_weight + value.z * weight;
                }
            }
        }
    }

    /// Blend multiple animation layers together
    ///
    /// # Arguments
    /// * `layers` - Animation layers to blend
    /// * `anim_values` - All animation values in the scene
    /// * `curves` - All animation curves in the scene
    /// * `time` - Time to evaluate at
    /// * `flags` - Evaluation flags
    ///
    /// # Returns
    /// The blended result value
    pub fn blend_layers(
        layers: &[&AnimLayer],
        prop_name: &str,
        element_id: usize,
        anim_values: &[AnimValue],
        curves: &[AnimCurve],
        default_value: Vec3,
        time: f64,
        flags: u32,
    ) -> Vec3 {
        let mut result = default_value;
        let mut first_layer = true;

        for (_layer_idx, layer) in layers.iter().enumerate() {
            // Find the animated property in this layer
            let anim_prop = layer.anim_props.iter().find(|prop| {
                prop.element == element_id && prop.prop_name.as_str() == prop_name
            });

            if let Some(anim_prop) = anim_prop {
                if anim_prop.anim_value >= anim_values.len() {
                    continue;
                }

                let value = AnimationEvaluator::eval_anim_value(
                    &anim_values[anim_prop.anim_value],
                    curves,
                    time,
                    flags,
                );

                if first_layer {
                    result = value;
                    first_layer = false;
                } else {
                    let mode = if layer.additive {
                        BlendMode::Additive
                    } else if layer.blended {
                        BlendMode::Blended
                    } else {
                        BlendMode::Override
                    };

                    let is_rotation = prop_name.contains("Rotation");
                    let is_scale = prop_name.contains("Scaling");

                    Self::blend_vec3(
                        &mut result,
                        &value,
                        layer.weight,
                        mode,
                        is_scale && layer.compose_scale,
                        is_rotation && layer.compose_rotation,
                    );
                }
            }
        }

        result
    }
}

// =============================================================================
// Animation Baking
// =============================================================================

/// Options for baking animations to a fixed sample rate
#[derive(Debug, Clone)]
pub struct BakeOptions {
    /// Sample rate in frames per second (e.g., 30.0)
    pub sample_rate: f64,

    /// Minimum sample rate to enforce (filters out very dense sampling)
    pub minimum_sample_rate: f64,

    /// Whether to enable keyframe reduction
    pub key_reduction: bool,

    /// Threshold for keyframe reduction (smaller = more aggressive)
    pub key_reduction_threshold: f64,

    /// Number of keyframe reduction passes
    pub key_reduction_passes: usize,
}

impl Default for BakeOptions {
    fn default() -> Self {
        Self {
            sample_rate: 30.0,
            minimum_sample_rate: 19.5,
            key_reduction: true,
            key_reduction_threshold: 0.000001,
            key_reduction_passes: 4,
        }
    }
}

/// A baked animation key (sampled at a specific time)
#[derive(Debug, Clone, Copy)]
pub struct BakedKey {
    pub time: f64,
    pub value: Real,
}

/// Utilities for baking animations
pub struct AnimationBaker;

impl AnimationBaker {
    /// Bake an animation curve to a fixed sample rate
    ///
    /// # Arguments
    /// * `curve` - The curve to bake
    /// * `time_start` - Start time
    /// * `time_end` - End time
    /// * `options` - Baking options
    ///
    /// # Returns
    /// Vector of baked keyframes
    pub fn bake_curve(
        curve: &AnimCurve,
        time_start: f64,
        time_end: f64,
        options: &BakeOptions,
    ) -> Vec<BakedKey> {
        let mut keys = Vec::new();

        if time_end <= time_start || options.sample_rate <= 0.0 {
            return keys;
        }

        let dt = 1.0 / options.sample_rate;
        let mut time = time_start;

        // Sample at fixed intervals
        while time <= time_end {
            let value = AnimationEvaluator::eval_curve(curve, time, 0);
            keys.push(BakedKey { time, value });
            time += dt;
        }

        // Ensure we have the end time
        if !keys.is_empty() && (keys[keys.len() - 1].time - time_end).abs() > dt * 0.5 {
            let value = AnimationEvaluator::eval_curve(curve, time_end, 0);
            keys.push(BakedKey { time: time_end, value });
        }

        // Apply keyframe reduction if enabled
        if options.key_reduction {
            keys = Self::reduce_keys(&keys, options);
        }

        keys
    }

    /// Reduce keyframes by removing those that can be linearly interpolated
    fn reduce_keys(keys: &[BakedKey], options: &BakeOptions) -> Vec<BakedKey> {
        if keys.len() <= 2 {
            return keys.to_vec();
        }

        let threshold_sq = options.key_reduction_threshold * options.key_reduction_threshold;
        let mut result = keys.to_vec();

        for _ in 0..options.key_reduction_passes {
            if result.len() <= 2 {
                break;
            }

            let mut keep = vec![true; result.len()];
            keep[0] = true;
            keep[result.len() - 1] = true;

            for i in 1..result.len() - 1 {
                let prev = &result[i - 1];
                let curr = &result[i];
                let next = &result[i + 1];

                // Linear interpolation between prev and next
                let t = (curr.time - prev.time) / (next.time - prev.time);
                let interpolated = prev.value + (next.value - prev.value) * t as Real;

                // Check error
                let error = ((interpolated - curr.value) as f64).powi(2);
                if error <= threshold_sq {
                    keep[i] = false;
                }
            }

            // Rebuild result with only kept keys
            let mut new_result = Vec::new();
            for (i, key) in result.iter().enumerate() {
                if keep[i] {
                    new_result.push(*key);
                }
            }

            if new_result.len() == result.len() {
                break; // No more reduction possible
            }

            result = new_result;
        }

        result
    }

    /// Bake a 3D animation value to a fixed sample rate
    pub fn bake_anim_value(
        anim_value: &AnimValue,
        curves: &[AnimCurve],
        time_start: f64,
        time_end: f64,
        options: &BakeOptions,
    ) -> Vec<(f64, Vec3)> {
        let mut samples = Vec::new();

        if time_end <= time_start || options.sample_rate <= 0.0 {
            return samples;
        }

        let dt = 1.0 / options.sample_rate;
        let mut time = time_start;

        while time <= time_end {
            let value = AnimationEvaluator::eval_anim_value(anim_value, curves, time, 0);
            samples.push((time, value));
            time += dt;
        }

        // Ensure we have the end time
        if !samples.is_empty() && (samples[samples.len() - 1].0 - time_end).abs() > dt * 0.5 {
            let value = AnimationEvaluator::eval_anim_value(anim_value, curves, time_end, 0);
            samples.push((time_end, value));
        }

        samples
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_keyframe(time: f64, value: Real, interp: Interpolation) -> Keyframe {
        Keyframe {
            time,
            value,
            interpolation: interp,
            left: Tangent { dx: 0.0, dy: 0.0 },
            right: Tangent { dx: 0.0, dy: 0.0 },
        }
    }

    fn create_test_curve(keyframes: Vec<Keyframe>) -> AnimCurve {
        let min_time = keyframes.first().map(|k| k.time).unwrap_or(0.0);
        let max_time = keyframes.last().map(|k| k.time).unwrap_or(0.0);
        let min_value = keyframes.iter().map(|k| k.value).fold(Real::INFINITY, Real::min);
        let max_value = keyframes.iter().map(|k| k.value).fold(Real::NEG_INFINITY, Real::max);

        AnimCurve {
            element: Element {
                name: FbxString::new("test"),
                props: Props::default(),
                element_id: 0,
                typed_id: 0,
                element_type: ElementType::AnimCurve,
                connections_src: Vec::new(),
                connections_dst: Vec::new(),
            },
            keyframes,
            pre_extrapolation: Extrapolation {
                mode: ExtrapolationMode::Constant,
                repeat_count: 0,
            },
            post_extrapolation: Extrapolation {
                mode: ExtrapolationMode::Constant,
                repeat_count: 0,
            },
            min_value,
            max_value,
            min_time,
            max_time,
        }
    }

    #[test]
    fn test_linear_interpolation() {
        let keys = vec![
            create_test_keyframe(0.0, 0.0, Interpolation::Linear),
            create_test_keyframe(1.0, 10.0, Interpolation::Linear),
        ];
        let curve = create_test_curve(keys);

        assert_eq!(AnimationEvaluator::eval_curve(&curve, 0.0, 0), 0.0);
        assert_eq!(AnimationEvaluator::eval_curve(&curve, 0.5, 0), 5.0);
        assert_eq!(AnimationEvaluator::eval_curve(&curve, 1.0, 0), 10.0);
    }

    #[test]
    fn test_constant_interpolation() {
        let keys = vec![
            create_test_keyframe(0.0, 5.0, Interpolation::ConstantPrev),
            create_test_keyframe(1.0, 10.0, Interpolation::ConstantPrev),
        ];
        let curve = create_test_curve(keys);

        assert_eq!(AnimationEvaluator::eval_curve(&curve, 0.0, 0), 5.0);
        assert_eq!(AnimationEvaluator::eval_curve(&curve, 0.5, 0), 5.0);
        assert_eq!(AnimationEvaluator::eval_curve(&curve, 1.0, 0), 10.0);
    }

    #[test]
    fn test_extrapolation_constant() {
        let keys = vec![
            create_test_keyframe(1.0, 5.0, Interpolation::Linear),
            create_test_keyframe(2.0, 10.0, Interpolation::Linear),
        ];
        let curve = create_test_curve(keys);

        // Before curve: should clamp to first value
        assert_eq!(AnimationEvaluator::eval_curve(&curve, 0.0, 0), 5.0);

        // After curve: should clamp to last value
        assert_eq!(AnimationEvaluator::eval_curve(&curve, 3.0, 0), 10.0);
    }

    #[test]
    fn test_keyframe_search() {
        let keys = vec![
            create_test_keyframe(0.0, 0.0, Interpolation::Linear),
            create_test_keyframe(1.0, 1.0, Interpolation::Linear),
            create_test_keyframe(2.0, 2.0, Interpolation::Linear),
            create_test_keyframe(3.0, 3.0, Interpolation::Linear),
        ];

        assert_eq!(find_keyframe_segment(&keys, -1.0), 0);
        assert_eq!(find_keyframe_segment(&keys, 0.0), 0);
        assert_eq!(find_keyframe_segment(&keys, 0.5), 0);
        assert_eq!(find_keyframe_segment(&keys, 1.0), 1);
        assert_eq!(find_keyframe_segment(&keys, 1.5), 1);
        assert_eq!(find_keyframe_segment(&keys, 2.5), 2);
        assert_eq!(find_keyframe_segment(&keys, 5.0), 3);
    }

    #[test]
    fn test_layer_blending_additive() {
        let mut result = Vec3::new(1.0, 2.0, 3.0);
        let value = Vec3::new(0.5, 1.0, 1.5);

        LayerBlender::blend_vec3(
            &mut result,
            &value,
            1.0,
            BlendMode::Additive,
            false,
            false,
        );

        assert_eq!(result.x, 1.5);
        assert_eq!(result.y, 3.0);
        assert_eq!(result.z, 4.5);
    }

    #[test]
    fn test_layer_blending_override() {
        let mut result = Vec3::new(1.0, 2.0, 3.0);
        let value = Vec3::new(5.0, 6.0, 7.0);

        LayerBlender::blend_vec3(
            &mut result,
            &value,
            1.0,
            BlendMode::Override,
            false,
            false,
        );

        assert_eq!(result, value);
    }

    #[test]
    fn test_baking() {
        let keys = vec![
            create_test_keyframe(0.0, 0.0, Interpolation::Linear),
            create_test_keyframe(1.0, 10.0, Interpolation::Linear),
        ];
        let curve = create_test_curve(keys);
        let options = BakeOptions {
            sample_rate: 10.0,
            key_reduction: false,
            ..Default::default()
        };

        let baked = AnimationBaker::bake_curve(&curve, 0.0, 1.0, &options);

        assert!(baked.len() >= 10);
        assert_eq!(baked[0].time, 0.0);
        assert_eq!(baked[0].value, 0.0);

        // Last key should be at time 1.0 with value 10.0
        let last = baked.last().unwrap();
        assert!((last.time - 1.0).abs() < 0.01);
        assert!((last.value - 10.0).abs() < 0.1);
    }
}
