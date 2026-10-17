//! Slotless surface-PM machine cross-section: solve the air-gap field from
//! a radially-magnetized permanent-magnet band and compare against the
//! exact Zhu & Howe annular-harmonic oracle (Epic #448, Phase 2b).
//!
//! Run with:
//!
//! ```text
//!   cargo run -p geode-core --example slotless_pm --release
//! ```
//!
//! Prints the mid-gap `B_r(θ), B_θ(θ)` FEM-vs-oracle comparison and the
//! peak air-gap flux density — the tearsheet numbers for the machine
//! benchmark.

use geode_core::analytic::slotless_pm::{MU_0, SlotlessPm};
use geode_core::analytic::waveguide::{RadialGrading, disk_boundary_nodes, disk_tri_mesh_bands};
use geode_core::assembly::magnetostatic::{
    assemble_magnetostatic_pm, build_nu_r, radial_magnetization_source, recover_b_field,
};

const TAG_MAGNET: i32 = 1;

fn main() {
    // Four-band cross-section: bore / magnet / air-gap / outer air.
    let (r1, r2, r3, rout) = (0.030, 0.040, 0.045, 0.20);
    let p = 2u32;
    let b_rem = 1.2; // NdFeB remanence (T)
    let m0 = b_rem / MU_0;
    let radii = [0.0, r1, r2, r3, rout];
    let n_rad = [18usize, 20, 12, 26];
    let gradings = [RadialGrading::Uniform; 4];

    let (mesh, tags) = disk_tri_mesh_bands(&radii, 384, &n_rad, &gradings);
    println!(
        "slotless-PM cross-section: {} nodes, {} triangles",
        mesh.n_nodes(),
        mesh.n_tris()
    );

    // μ_rec = 1 everywhere (open-space oracle regime).
    let nu = build_nu_r(&tags, &[1.0, 1.0, 1.0, 1.0]);
    let j_z = vec![0.0; mesh.n_tris()];
    let m = radial_magnetization_source(&mesh, &tags, TAG_MAGNET, m0, p);
    let bc = disk_boundary_nodes(&mesh, rout);
    let sys = assemble_magnetostatic_pm(&mesh, &nu, &j_z, &m, &bc).expect("assemble PM system");
    let a_z = sys.solve().expect("solve PM system");
    let b = recover_b_field(&mesh, &a_z);

    let oracle = SlotlessPm::new(r1, r2, m0, p);
    let r_gap = 0.5 * (r2 + r3);

    // Sample the mid-gap contour and report the L2 error + peak field.
    let n_contour = 180;
    let mut num = 0.0;
    let mut den = 0.0;
    let mut peak = 0.0_f64;
    for i in 0..n_contour {
        let theta = std::f64::consts::TAU * i as f64 / n_contour as f64;
        let (px, py) = (r_gap * theta.cos(), r_gap * theta.sin());
        let t = locate(&mesh, px, py).expect("contour point inside mesh");
        let (br_o, bth_o) = oracle.exterior_field(r_gap, theta);
        let (c, s) = (theta.cos(), theta.sin());
        let br = b[t][0] * c + b[t][1] * s;
        let bth = -b[t][0] * s + b[t][1] * c;
        num += (br - br_o).powi(2) + (bth - bth_o).powi(2);
        den += br_o.powi(2) + bth_o.powi(2);
        peak = peak.max((br * br + bth * bth).sqrt());
    }
    let l2 = (num / den).sqrt();
    let peak_oracle = oracle.exterior_coeff().abs() * r_gap.powi(-3);
    println!("mid-gap radius r_gap = {r_gap:.4} m, {p}-pole-pair magnet");
    println!(
        "peak mid-gap |B|: FEM = {:.4} T, oracle = {:.4} T",
        peak, peak_oracle
    );
    println!(
        "mid-gap contour L2 (FEM vs Zhu-Howe oracle) = {:.3}%",
        l2 * 100.0
    );
    if l2 > 0.01 {
        println!(
            "(P1 piecewise-constant B: first-order convergent; ≤1% is the P2-Lagrange target)"
        );
    }
}

/// Locate the triangle containing `(px, py)` by barycentric test.
fn locate(mesh: &geode_core::analytic::waveguide::TriMesh, px: f64, py: f64) -> Option<usize> {
    for (t, tri) in mesh.tris.iter().enumerate() {
        let [x1, y1] = mesh.nodes[tri[0] as usize];
        let [x2, y2] = mesh.nodes[tri[1] as usize];
        let [x3, y3] = mesh.nodes[tri[2] as usize];
        let d = (y2 - y3) * (x1 - x3) + (x3 - x2) * (y1 - y3);
        let a = ((y2 - y3) * (px - x3) + (x3 - x2) * (py - y3)) / d;
        let bb = ((y3 - y1) * (px - x3) + (x1 - x3) * (py - y3)) / d;
        let c = 1.0 - a - bb;
        if a >= -1e-9 && bb >= -1e-9 && c >= -1e-9 {
            return Some(t);
        }
    }
    None
}
