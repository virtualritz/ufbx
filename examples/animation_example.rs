//! Animation Evaluation Example
//!
//! This example demonstrates how to use the ufbx animation evaluation system
//! to evaluate curves, blend layers, and bake animations.

use ufbx::animation::{AnimationEvaluator, AnimationBaker, BakeOptions, LayerBlender};
use ufbx::types::*;

/// Example: Evaluate a simple animation curve
fn example_curve_evaluation() {
    // Create a simple animation curve with linear interpolation
    let keyframes = vec![
        Keyframe {
            time: 0.0,
            value: 0.0,
            interpolation: Interpolation::Linear,
            left: Tangent { dx: 0.0, dy: 0.0 },
            right: Tangent { dx: 0.0, dy: 0.0 },
        },
        Keyframe {
            time: 1.0,
            value: 10.0,
            interpolation: Interpolation::Linear,
            left: Tangent { dx: 0.0, dy: 0.0 },
            right: Tangent { dx: 0.0, dy: 0.0 },
        },
        Keyframe {
            time: 2.0,
            value: 5.0,
            interpolation: Interpolation::Linear,
            left: Tangent { dx: 0.0, dy: 0.0 },
            right: Tangent { dx: 0.0, dy: 0.0 },
        },
    ];

    let curve = AnimCurve {
        element: Element {
            name: FbxString::new("position_x"),
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
        min_value: 0.0,
        max_value: 10.0,
        min_time: 0.0,
        max_time: 2.0,
    };

    // Evaluate at different times
    println!("Curve Evaluation:");
    for time in &[0.0, 0.5, 1.0, 1.5, 2.0] {
        let value = AnimationEvaluator::eval_curve(&curve, *time, 0);
        println!("  t={:.1}: value={:.2}", time, value);
    }
}

/// Example: Bake an animation to fixed sample rate
fn example_animation_baking() {
    // Create a cubic curve
    let keyframes = vec![
        Keyframe {
            time: 0.0,
            value: 0.0,
            interpolation: Interpolation::Cubic,
            left: Tangent { dx: 0.0, dy: 0.0 },
            right: Tangent { dx: 0.333, dy: 2.0 },
        },
        Keyframe {
            time: 1.0,
            value: 5.0,
            interpolation: Interpolation::Cubic,
            left: Tangent { dx: 0.333, dy: 2.0 },
            right: Tangent { dx: 0.333, dy: -2.0 },
        },
        Keyframe {
            time: 2.0,
            value: 0.0,
            interpolation: Interpolation::Cubic,
            left: Tangent { dx: 0.333, dy: -2.0 },
            right: Tangent { dx: 0.0, dy: 0.0 },
        },
    ];

    let curve = AnimCurve {
        element: Element {
            name: FbxString::new("rotation_y"),
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
        min_value: 0.0,
        max_value: 5.0,
        min_time: 0.0,
        max_time: 2.0,
    };

    // Bake to 10 FPS
    let options = BakeOptions {
        sample_rate: 10.0,
        key_reduction: true,
        key_reduction_threshold: 0.01,
        ..Default::default()
    };

    let baked = AnimationBaker::bake_curve(&curve, 0.0, 2.0, &options);

    println!("\nBaked Animation ({} keys):", baked.len());
    for key in &baked {
        println!("  t={:.2}: value={:.3}", key.time, key.value);
    }
}

/// Example: Layer blending
fn example_layer_blending() {
    use ufbx::animation::BlendMode;

    let mut base = Vec3::new(10.0, 20.0, 30.0);
    let additive = Vec3::new(5.0, -5.0, 0.0);

    println!("\nLayer Blending:");
    println!("  Base: {:?}", base);
    println!("  Additive layer: {:?}", additive);

    LayerBlender::blend_vec3(
        &mut base,
        &additive,
        1.0,
        BlendMode::Additive,
        false,
        false,
    );

    println!("  Result: {:?}", base);
}

fn main() {
    println!("=== Animation Evaluation Examples ===\n");

    example_curve_evaluation();
    example_animation_baking();
    example_layer_blending();
}
