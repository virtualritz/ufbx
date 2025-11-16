# C to Rust Transformation Examples

## Example 1: Basis Weight Calculation

### Original C Code
```c
static ufbxi_forceinline ufbx_real ufbxi_nurbs_weight(
    const ufbx_real_list *knots,
    size_t knot,
    size_t degree,
    ufbx_real u)
{
    if (knot >= knots->count) return 0.0f;
    if (knots->count - knot < degree) return 0.0f;
    ufbx_real prev_u = knots->data[knot];
    ufbx_real next_u = knots->data[knot + degree];
    if (prev_u >= next_u) return 0.0f;
    if (u <= prev_u) return 0.0f;
    if (u >= next_u) return 1.0f;
    return (u - prev_u) / (next_u - prev_u);
}
```

### Idiomatic Rust
```rust
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
```

**Changes:**
- `const ufbx_real_list*` → `&[Real]` (slice reference)
- `knots->data[i]` → `knots[i]` (direct indexing)
- `knots->count` → `knots.len()` (method call)
- Removed `static ufbxi_forceinline` (Rust inlines automatically)

---

## Example 2: Curve Point Evaluation

### Original C Code
```c
ufbx_abi ufbxi_noinline ufbx_curve_point
ufbx_evaluate_nurbs_curve(const ufbx_nurbs_curve *curve, ufbx_real u)
{
    ufbx_curve_point result = { false };

    ufbx_assert(curve);
    if (!curve) return result;

    ufbx_real weights[UFBXI_MAX_NURBS_ORDER];
    ufbx_real derivs[UFBXI_MAX_NURBS_ORDER];
    size_t base = ufbx_evaluate_nurbs_basis(&curve->basis, u,
        weights, UFBXI_MAX_NURBS_ORDER, derivs, UFBXI_MAX_NURBS_ORDER);
    if (base == SIZE_MAX) return result;

    ufbx_vec4 p = { 0 };
    ufbx_vec4 d = { 0 };

    size_t order = curve->basis.order;
    if (order > UFBXI_MAX_NURBS_ORDER) return result;
    if (curve->control_points.count == 0) return result;

    for (size_t i = 0; i < order; i++) {
        size_t ix = (base + i) % curve->control_points.count;
        ufbx_vec4 cp = curve->control_points.data[ix];
        ufbx_real weight = weights[i] * cp.w;
        ufbx_real deriv = derivs[i] * cp.w;

        p.x += cp.x * weight;
        p.y += cp.y * weight;
        p.z += cp.z * weight;
        p.w += weight;

        d.x += cp.x * deriv;
        d.y += cp.y * deriv;
        d.z += cp.z * deriv;
        d.w += deriv;
    }

    ufbx_real rcp_w = 1.0f / p.w;
    result.valid = true;
    result.position.x = p.x * rcp_w;
    result.position.y = p.y * rcp_w;
    result.position.z = p.z * rcp_w;
    result.derivative.x = (d.x - d.w*result.position.x) * rcp_w;
    result.derivative.y = (d.y - d.w*result.position.y) * rcp_w;
    result.derivative.z = (d.z - d.w*result.position.z) * rcp_w;
    return result;
}
```

### Idiomatic Rust
```rust
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

    // Accumulate weighted control points
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

    // Perspective divide
    if p.w.abs() < 1e-10 {
        return result;
    }

    let rcp_w = 1.0 / p.w;
    result.valid = true;
    result.position.x = p.x * rcp_w;
    result.position.y = p.y * rcp_w;
    result.position.z = p.z * rcp_w;

    // Quotient rule for derivatives
    result.derivative.x = (d.x - d.w * result.position.x) * rcp_w;
    result.derivative.y = (d.y - d.w * result.position.y) * rcp_w;
    result.derivative.z = (d.z - d.w * result.position.z) * rcp_w;

    result
}
```

**Changes:**
- Removed `ufbx_assert(curve)` → Rust doesn't allow null references
- `SIZE_MAX` → `None` in `Option<usize>`
- Stack arrays initialized: `[0.0; MAX_ORDER]`
- `if (base == SIZE_MAX)` → `match` with `Option`
- `curve->control_points.count` → `curve.control_points.len()`
- Struct field access: `->` → `.`
- Added safety check: `p.w.abs() < 1e-10` before division

---

## Example 3: Tessellation Context

### Original C Code
```c
typedef struct {
    ufbx_error error;
    ufbx_tessellate_curve_opts opts;
    const ufbx_nurbs_curve *curve;
    ufbxi_allocator ator_tmp;
    ufbxi_allocator ator_result;
    ufbxi_buf result;
    ufbx_line_curve line;
    ufbxi_line_curve_imp *imp;
} ufbxi_tessellate_curve_context;

ufbxi_nodiscard static ufbxi_noinline int
ufbxi_tessellate_nurbs_curve_imp(ufbxi_tessellate_curve_context *tc)
{
    if (tc->opts.span_subdivision <= 0) {
        tc->opts.span_subdivision = 4;
    }
    size_t num_sub = tc->opts.span_subdivision;

    const ufbx_nurbs_curve *curve = tc->curve;
    // ... more code
}
```

### Idiomatic Rust
```rust
pub fn tessellate_curve(
    curve: &NurbsCurve,
    segments_per_span: usize,
) -> Result<Vec<Vec3>> {
    let num_sub = if segments_per_span > 0 {
        segments_per_span
    } else {
        4 // Default subdivision
    };

    // ... implementation
}
```

**Changes:**
- No need for context struct (simplified API)
- Removed custom allocators (Vec handles memory)
- Removed error struct (return Result<T, Error>)
- Direct parameters instead of options struct
- Automatic memory management (no manual buffer management)

---

## Example 4: Error Handling

### Original C Code
```c
ufbxi_check_err_msg(&tc->error,
    curve->basis.valid && curve->control_points.count > 0,
    "Bad NURBS geometry");
```

### Idiomatic Rust
```rust
if !curve.basis.valid || curve.control_points.is_empty() {
    return Err(Error::BadNurbs {
        description: "Invalid NURBS curve basis or empty control points".to_string()
    });
}
```

**Changes:**
- Macro → direct `if` + early return
- Error stored in context → `Result<T, Error>`
- Error code → descriptive error variant

---

## Example 5: Binary Search for Knot Span

### Original C Code
```c
ufbxi_macro_lower_bound_eq(ufbx_real, 8, &knot, knots.data, 0, knots.count - 1,
    ( a[1] <= u ), ( a[0] <= u && u < a[1] ));
```

### Idiomatic Rust
```rust
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
```

**Changes:**
- Macro → explicit loop
- More readable with clear conditions
- Safe integer arithmetic (checked subtraction)
- Bounds checking via conditionals

---

## Key Transformation Patterns

### Memory Management
- **C:** Manual allocators, buffers, reference counting
- **Rust:** Vec, Box, Arc (automatic RAII)

### Error Handling
- **C:** Error codes, check macros, context structs
- **Rust:** Result<T, E>, ? operator, early returns

### Null Safety
- **C:** Null pointer checks (`if (!ptr)`)
- **Rust:** Option<T>, references can't be null

### Type Safety
- **C:** Void pointers, casts, unions
- **Rust:** Generic types, enums, pattern matching

### Bounds Checking
- **C:** Manual checks, assertions
- **Rust:** Automatic with slices, or explicit with get()

### Const Correctness
- **C:** const pointers (`const T*`)
- **Rust:** Immutable references (`&T`) by default

### Inline Hints
- **C:** `static inline`, `forceinline`
- **Rust:** Automatic inlining, #[inline] if needed
