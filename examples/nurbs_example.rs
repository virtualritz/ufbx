//! Example: NURBS Curve and Surface Evaluation
//!
//! Demonstrates how to evaluate and tessellate NURBS curves and surfaces.

#[cfg(feature = "nurbs")]
fn main() {
    use ufbx::nurbs::{
        NurbsBasis, NurbsCurve, NurbsEvaluator, NurbsSurface, NurbsTopology,
    };
    use ufbx::types::{Vec3, Vec4};

    println!("=== NURBS Curve Evaluation Example ===\n");

    // Create a simple quadratic Bezier curve
    let curve = NurbsCurve {
        name: "bezier_curve".to_string(),
        basis: NurbsBasis {
            order: 3, // Degree 2 (quadratic)
            topology: NurbsTopology::Open,
            knot_vector: vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
            t_min: 0.0,
            t_max: 1.0,
            spans: vec![0.0, 1.0],
            is_2d: false,
            num_wrap_control_points: 0,
            valid: true,
        },
        control_points: vec![
            Vec4::new(0.0, 0.0, 0.0, 1.0),    // Start point
            Vec4::new(0.5, 2.0, 0.0, 1.0),    // Control point (pulls curve up)
            Vec4::new(1.0, 0.0, 0.0, 1.0),    // End point
        ],
    };

    println!("Curve: {}", curve.name);
    println!("Degree: {}", curve.basis.degree());
    println!("Control points: {}", curve.control_points.len());
    println!();

    // Evaluate curve at specific parameter values
    println!("Evaluating curve at parameter values:");
    for i in 0..=10 {
        let t = i as f64 / 10.0;
        let point = NurbsEvaluator::evaluate_curve(&curve, t);

        if point.valid {
            println!(
                "  t={:.1}: position=({:.3}, {:.3}, {:.3}), tangent=({:.3}, {:.3}, {:.3})",
                t,
                point.position.x,
                point.position.y,
                point.position.z,
                point.derivative.x,
                point.derivative.y,
                point.derivative.z
            );
        }
    }

    println!();

    // Tessellate the curve to line segments
    println!("Tessellating curve with 8 segments per span:");
    match NurbsEvaluator::tessellate_curve(&curve, 8) {
        Ok(points) => {
            println!("Generated {} points:", points.len());
            for (i, p) in points.iter().enumerate() {
                println!("  Point {}: ({:.3}, {:.3}, {:.3})", i, p.x, p.y, p.z);
            }
        }
        Err(e) => println!("Tessellation failed: {}", e),
    }

    println!("\n=== NURBS Surface Evaluation Example ===\n");

    // Create a simple bilinear surface (4 corners)
    let surface = NurbsSurface {
        name: "bilinear_surface".to_string(),
        basis_u: NurbsBasis {
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
        basis_v: NurbsBasis {
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
        num_control_points_u: 2,
        num_control_points_v: 2,
        control_points: vec![
            Vec4::new(0.0, 0.0, 0.0, 1.0), // (0,0)
            Vec4::new(1.0, 0.0, 0.0, 1.0), // (1,0)
            Vec4::new(0.0, 1.0, 0.0, 1.0), // (0,1)
            Vec4::new(1.0, 1.0, 1.0, 1.0), // (1,1) - lifted corner
        ],
        span_subdivision_u: 4,
        span_subdivision_v: 4,
        flip_normals: false,
    };

    println!("Surface: {}", surface.name);
    println!(
        "Control points: {} x {} = {}",
        surface.num_control_points_u,
        surface.num_control_points_v,
        surface.control_points.len()
    );
    println!();

    // Evaluate surface at a grid of UV points
    println!("Evaluating surface at grid:");
    for v in 0..=2 {
        for u in 0..=2 {
            let u_val = u as f64 / 2.0;
            let v_val = v as f64 / 2.0;
            let point = NurbsEvaluator::evaluate_surface(&surface, u_val, v_val);

            if point.valid {
                println!(
                    "  uv=({:.1},{:.1}): pos=({:.2}, {:.2}, {:.2})",
                    u_val, v_val, point.position.x, point.position.y, point.position.z
                );
            }
        }
    }

    println!();

    // Tessellate surface to mesh
    println!("Tessellating surface to mesh (4x4 resolution):");
    match NurbsEvaluator::tessellate_surface(&surface, 4, 4) {
        Ok(mesh) => {
            println!("Generated mesh:");
            println!("  Vertices: {}", mesh.num_vertices);
            println!("  Faces: {}", mesh.num_faces);
            println!("  Triangles (approx): {}", mesh.num_triangles);
            println!("  Generated normals: {}", mesh.generated_normals);
            println!("  From tessellated NURBS: {}", mesh.from_tessellated_nurbs);
        }
        Err(e) => println!("Tessellation failed: {}", e),
    }
}

#[cfg(not(feature = "nurbs"))]
fn main() {
    println!("This example requires the 'nurbs' feature.");
    println!("Run with: cargo run --example nurbs_example --features nurbs");
}
