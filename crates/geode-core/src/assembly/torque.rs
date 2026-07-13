//! Electromagnetic **torque extraction** from a piecewise-constant air-gap
//! flux density (Epic #448, Phase 3a).
//!
//! Torque is a *derived* quantity: a nonlinear (`B_r·B_θ`) integral of the
//! field over the gap, notoriously noisy on coarse meshes. This module
//! implements the two standard 2-D machine estimators and is validated
//! against the closed-form **torque-on-a-current-loop** oracle
//! (`T = m × B`) *before* it is trusted on a real machine solve, so a
//! torque miss on the machine can be attributed to the field or the mesh
//! rather than a buggy extractor.
//!
//! ## Estimators
//!
//! **Maxwell stress-tensor air-gap line integral.** Torque per axial length
//! `L` on a circular contour of radius `r_gap` through the air gap:
//!
//! ```text
//!   T = (L r_gap² / μ₀) ∫_0^{2π} B_r(θ) B_θ(θ) dθ
//! ```
//!
//! [`maxwell_stress_torque`] samples `(B_r, B_θ)` on the mid-gap contour by
//! locating the containing triangle for each sample point and evaluating the
//! per-triangle constant `B`, then integrates in `θ` with the periodic
//! trapezoid rule (exact for the truncated Fourier series of a smooth
//! periodic integrand).
//!
//! **Arkkio's volume-averaged variant (strongly preferred on coarse
//! meshes).** Instead of one contour, average the stress over the whole gap
//! annulus `[r_i, r_o]`:
//!
//! ```text
//!   T = (L / (μ₀ (r_o − r_i))) ∫∫_{gap} B_r B_θ r  dr dθ
//! ```
//!
//! [`arkkio_torque`] computes the per-triangle constant `B_r B_θ r`, weights
//! by triangle area, and sums over the gap-band triangles selected by their
//! band tag. Volume-averaging cancels the pointwise product noise, so this
//! is the recommended production estimator.
//!
//! ## Shared contour sampler
//!
//! [`locate_triangle`] (barycentric point-in-triangle search) and
//! [`sample_b_polar_on_contour`] (mid-gap ring sampler returning the polar
//! `(B_r, B_θ)` components) are `pub` here so the torque extractor, field
//! benchmarks, and examples share one implementation instead of each keeping
//! a private copy.

use crate::analytic::slotless_pm::MU_0;
use crate::analytic::waveguide::TriMesh;

/// Locate the triangle of `mesh` that contains the point `(px, py)` by a
/// barycentric point-in-triangle test, returning its 0-based triangle index
/// (or `None` if the point lies outside every triangle).
///
/// A small negative tolerance (`-1e-9`) admits points exactly on an edge or
/// vertex, so a contour sample that lands on a shared triangle boundary is
/// still assigned to a neighbouring triangle rather than being reported as
/// outside the mesh. When a point is shared by several triangles the first
/// in mesh order wins — fine for evaluating a per-triangle constant field,
/// which is (nearly) continuous across the gap.
pub fn locate_triangle(mesh: &TriMesh, px: f64, py: f64) -> Option<usize> {
    for (t, tri) in mesh.tris.iter().enumerate() {
        let [x1, y1] = mesh.nodes[tri[0] as usize];
        let [x2, y2] = mesh.nodes[tri[1] as usize];
        let [x3, y3] = mesh.nodes[tri[2] as usize];
        let d = (y2 - y3) * (x1 - x3) + (x3 - x2) * (y1 - y3);
        let a = ((y2 - y3) * (px - x3) + (x3 - x2) * (py - y3)) / d;
        let b = ((y3 - y1) * (px - x3) + (x1 - x3) * (py - y3)) / d;
        let c = 1.0 - a - b;
        let tol = -1e-9;
        if a >= tol && b >= tol && c >= tol {
            return Some(t);
        }
    }
    None
}

/// Rotate a Cartesian per-triangle field `[B_x, B_y]` into the local polar
/// frame at angle `theta`, returning `(B_r, B_θ)`:
/// `B_r =  B_x cosθ + B_y sinθ`, `B_θ = −B_x sinθ + B_y cosθ`.
#[inline]
pub fn cartesian_to_polar_b(bxy: [f64; 2], theta: f64) -> (f64, f64) {
    let (c, s) = (theta.cos(), theta.sin());
    let br = bxy[0] * c + bxy[1] * s;
    let bth = -bxy[0] * s + bxy[1] * c;
    (br, bth)
}

/// Sample the piecewise-constant flux density `b` on a circular contour of
/// radius `r_gap`, at `n_contour` equal-angle points `θ_i = 2πi/n_contour`,
/// returning the polar components `(B_r(θ_i), B_θ(θ_i))`.
///
/// For each sample point the containing triangle is located with
/// [`locate_triangle`] and its constant `B` rotated into the polar frame.
///
/// # Panics
///
/// Panics if any contour point falls outside the mesh (the caller passed an
/// `r_gap` that is not inside the meshed domain) or if `b.len()` does not
/// equal the triangle count.
pub fn sample_b_polar_on_contour(
    mesh: &TriMesh,
    b: &[[f64; 2]],
    r_gap: f64,
    n_contour: usize,
) -> Vec<(f64, f64)> {
    assert_eq!(
        b.len(),
        mesh.n_tris(),
        "sample_b_polar_on_contour: b length {} != triangle count {}",
        b.len(),
        mesh.n_tris()
    );
    assert!(n_contour >= 3, "sample_b_polar_on_contour: need ≥3 samples");
    (0..n_contour)
        .map(|i| {
            let theta = std::f64::consts::TAU * i as f64 / n_contour as f64;
            let (px, py) = (r_gap * theta.cos(), r_gap * theta.sin());
            let t = locate_triangle(mesh, px, py).unwrap_or_else(|| {
                panic!(
                    "sample_b_polar_on_contour: contour point r_gap={r_gap} θ={theta} \
                     is outside the mesh"
                )
            });
            cartesian_to_polar_b(b[t], theta)
        })
        .collect()
}

/// Maxwell stress-tensor air-gap **line-integral** torque per axial length
/// `L`, on a circular contour of radius `r_gap` sampled at `n_contour`
/// equal-angle points:
///
/// ```text
///   T = (L r_gap² / μ₀) ∫_0^{2π} B_r(θ) B_θ(θ) dθ
///     ≈ (L r_gap² / μ₀) · Δθ · Σ_i B_r(θ_i) B_θ(θ_i),   Δθ = 2π/n_contour
/// ```
///
/// The periodic trapezoid rule (a plain average times `2π`) is spectrally
/// accurate for the smooth periodic integrand `B_r B_θ`, so quadrature error
/// is negligible against the field-recovery error on any reasonable
/// `n_contour`.
///
/// `b` is the per-triangle constant flux density `[B_x, B_y]` (e.g. from
/// [`crate::assembly::magnetostatic::recover_b_field`]); `r_gap` must lie
/// inside the meshed air gap.
///
/// # Panics
///
/// Panics if a contour point falls outside the mesh, if `b.len()` ≠ triangle
/// count, or if `n_contour < 3`.
pub fn maxwell_stress_torque(
    mesh: &TriMesh,
    b: &[[f64; 2]],
    r_gap: f64,
    l_axial: f64,
    n_contour: usize,
) -> f64 {
    let samples = sample_b_polar_on_contour(mesh, b, r_gap, n_contour);
    maxwell_stress_torque_from_samples(&samples, r_gap, l_axial)
}

/// Pure θ-quadrature core of [`maxwell_stress_torque`]: given the polar field
/// samples `(B_r(θ_i), B_θ(θ_i))` at `n = samples.len()` equal-angle points
/// on the contour of radius `r_gap`, return
/// `(L r_gap²/μ₀) · Δθ · Σ_i B_r B_θ` with `Δθ = 2π/n` (the periodic
/// trapezoid rule).
///
/// Exposed separately so a benchmark can feed *analytic* contour samples
/// (evaluated at the exact contour points) and isolate the quadrature +
/// prefactor from any triangle-location / piecewise-constant sampling error.
///
/// # Panics
///
/// Panics if fewer than 3 samples are supplied.
pub fn maxwell_stress_torque_from_samples(samples: &[(f64, f64)], r_gap: f64, l_axial: f64) -> f64 {
    assert!(
        samples.len() >= 3,
        "maxwell_stress_torque_from_samples: need ≥3 samples"
    );
    let dtheta = std::f64::consts::TAU / samples.len() as f64;
    let integral: f64 = samples.iter().map(|&(br, bth)| br * bth).sum::<f64>() * dtheta;
    l_axial * r_gap * r_gap / MU_0 * integral
}

/// Arkkio volume-averaged air-gap torque per axial length `L`, summed over
/// the gap-band triangles (those with `tags[t] == gap_tag`) lying in the
/// annulus `[r_inner, r_outer]`:
///
/// ```text
///   T = (L / (μ₀ (r_o − r_i))) ∫∫_{gap} B_r B_θ r dr dθ
///     ≈ (L / (μ₀ (r_o − r_i))) Σ_{t ∈ gap} area_t · r_t · B_r(t) B_θ(t)
/// ```
///
/// where `r_t`, `θ_t` are the centroid radius / angle of triangle `t` and
/// `B_r(t), B_θ(t)` its per-triangle constant field in the polar frame at
/// `θ_t`. Averaging over the whole annulus cancels the pointwise `B_r B_θ`
/// noise that makes the single-contour line integral jittery on coarse
/// meshes, so this is the preferred estimator.
///
/// `b` is the per-triangle constant flux density; `tags` the band tags from
/// [`crate::analytic::waveguide::disk_tri_mesh_bands`]; `gap_tag` the band
/// index of the air gap.
///
/// # Panics
///
/// Panics if `b.len()` or `tags.len()` ≠ triangle count, or if
/// `r_outer <= r_inner`.
pub fn arkkio_torque(
    mesh: &TriMesh,
    tags: &[i32],
    b: &[[f64; 2]],
    gap_tag: i32,
    r_inner: f64,
    r_outer: f64,
    l_axial: f64,
) -> f64 {
    assert_eq!(
        b.len(),
        mesh.n_tris(),
        "arkkio_torque: b length {} != triangle count {}",
        b.len(),
        mesh.n_tris()
    );
    assert_eq!(
        tags.len(),
        mesh.n_tris(),
        "arkkio_torque: tags length {} != triangle count {}",
        tags.len(),
        mesh.n_tris()
    );
    assert!(
        r_outer > r_inner,
        "arkkio_torque: r_outer ({r_outer}) must exceed r_inner ({r_inner})"
    );

    let mut acc = 0.0;
    for (t, tri) in mesh.tris.iter().enumerate() {
        if tags[t] != gap_tag {
            continue;
        }
        let c0 = mesh.nodes[tri[0] as usize];
        let c1 = mesh.nodes[tri[1] as usize];
        let c2 = mesh.nodes[tri[2] as usize];
        let cx = (c0[0] + c1[0] + c2[0]) / 3.0;
        let cy = (c0[1] + c1[1] + c2[1]) / 3.0;
        let r = (cx * cx + cy * cy).sqrt();
        let theta = cy.atan2(cx);
        let area =
            0.5 * ((c1[0] - c0[0]) * (c2[1] - c0[1]) - (c1[1] - c0[1]) * (c2[0] - c0[0])).abs();
        let (br, bth) = cartesian_to_polar_b(b[t], theta);
        acc += area * r * br * bth;
    }
    l_axial / (MU_0 * (r_outer - r_inner)) * acc
}

/// Vector potential of a set of straight axial line currents at a field
/// point, plus a uniform external field's contribution, in the 2-D
/// (per-unit-length) reduction.
///
/// A line current `I` on the axis through `(x₀, y₀)` has
/// `A_z(x,y) = −(μ₀ I)/(2π) · ln r`, `r = |(x,y) − (x₀,y₀)|`, giving the
/// circulating field `B = ∇×(A_z ẑ) = (∂A_z/∂y, −∂A_z/∂x)`. This is used to
/// build the analytic loop field for the `T = m × B` oracle test; it lives
/// in the crate (not just the test) so it is exercised by doctests and can
/// be reused by future machine-torque examples.
///
/// `sources` is a slice of `(x₀, y₀, I)` line currents. `b_uniform` is the
/// uniform external field `[B_x, B_y]` added on top (superposition, exact for
/// linear materials). Returns the total `B = [B_x, B_y]` at `(px, py)`.
pub fn line_currents_plus_uniform_b(
    sources: &[(f64, f64, f64)],
    b_uniform: [f64; 2],
    px: f64,
    py: f64,
) -> [f64; 2] {
    let mut bx = b_uniform[0];
    let mut by = b_uniform[1];
    for &(x0, y0, i) in sources {
        let dx = px - x0;
        let dy = py - y0;
        let r2 = dx * dx + dy * dy;
        // B of a line current: B = (μ₀ I)/(2π r) θ̂, i.e.
        //   B_x = −(μ₀ I)/(2π) · dy/r²,  B_y = (μ₀ I)/(2π) · dx/r².
        let k = MU_0 * i / (std::f64::consts::TAU * r2);
        bx += -k * dy;
        by += k * dx;
    }
    [bx, by]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analytic::waveguide::{RadialGrading, disk_tri_mesh_bands};

    #[test]
    fn cartesian_polar_roundtrip() {
        let theta = 0.7;
        let (br, bth) = cartesian_to_polar_b([1.0, 2.0], theta);
        // Inverse rotation recovers the Cartesian components.
        let (c, s) = (theta.cos(), theta.sin());
        let bx = br * c - bth * s;
        let by = br * s + bth * c;
        assert!((bx - 1.0).abs() < 1e-12 && (by - 2.0).abs() < 1e-12);
    }

    #[test]
    fn locate_triangle_inside_and_outside() {
        // Two-band disk; a point at the origin is inside, a point far
        // outside the outer radius is not.
        let (mesh, _tags) =
            disk_tri_mesh_bands(&[0.0, 0.5, 1.0], 12, &[2, 2], &[RadialGrading::Uniform; 2]);
        assert!(locate_triangle(&mesh, 0.0, 0.0).is_some());
        assert!(locate_triangle(&mesh, 10.0, 10.0).is_none());
    }
}
