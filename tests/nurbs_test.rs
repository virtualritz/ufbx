#[cfg(feature = "nurbs")]
mod nurbs_tests {
    use ufbx::nurbs::{
        NurbsBasis, NurbsCurve, NurbsEvaluator, NurbsTopology,
    };
    use ufbx::types::{Vec3, Vec4};

    #[test]
    fn test_linear_curve() {
        // Create a simple linear curve from (0,0,0) to (1,0,0)
        let curve = NurbsCurve {
            name: "line".to_string(),
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

        // Evaluate at start
        let p0 = NurbsEvaluator::evaluate_curve(&curve, 0.0);
        assert!(p0.valid);
        assert!((p0.position.x - 0.0).abs() < 1e-6);
        assert!((p0.position.y - 0.0).abs() < 1e-6);
        assert!((p0.position.z - 0.0).abs() < 1e-6);

        // Evaluate at end
        let p1 = NurbsEvaluator::evaluate_curve(&curve, 1.0);
        assert!(p1.valid);
        assert!((p1.position.x - 1.0).abs() < 1e-6);
        assert!((p1.position.y - 0.0).abs() < 1e-6);
        assert!((p1.position.z - 0.0).abs() < 1e-6);

        // Evaluate at middle
        let pmid = NurbsEvaluator::evaluate_curve(&curve, 0.5);
        assert!(pmid.valid);
        assert!((pmid.position.x - 0.5).abs() < 1e-6);
        assert!((pmid.position.y - 0.0).abs() < 1e-6);
        assert!((pmid.position.z - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_curve_tessellation() {
        let curve = NurbsCurve {
            name: "line".to_string(),
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
        assert!(!points.is_empty());
        assert_eq!(points.len(), 1); // One span with 1 point

        // First point should be at start
        assert!((points[0].x - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_quadratic_bezier() {
        // Quadratic Bezier curve (degree 2)
        let curve = NurbsCurve {
            name: "bezier".to_string(),
            basis: NurbsBasis {
                order: 3,
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
                Vec4::new(0.0, 0.0, 0.0, 1.0),
                Vec4::new(0.5, 1.0, 0.0, 1.0), // Control point
                Vec4::new(1.0, 0.0, 0.0, 1.0),
            ],
        };

        // Evaluate at start and end
        let p0 = NurbsEvaluator::evaluate_curve(&curve, 0.0);
        assert!(p0.valid);
        assert!((p0.position.x - 0.0).abs() < 1e-6);

        let p1 = NurbsEvaluator::evaluate_curve(&curve, 1.0);
        assert!(p1.valid);
        assert!((p1.position.x - 1.0).abs() < 1e-6);

        // Middle point should be higher
        let pmid = NurbsEvaluator::evaluate_curve(&curve, 0.5);
        assert!(pmid.valid);
        assert!((pmid.position.x - 0.5).abs() < 1e-6);
        assert!(pmid.position.y > 0.4); // Should be pulled up by control point
    }
}
