//! 3-D vector magnetostatic inductance-extraction benchmark generator
//! (Epic #475, issue #504 — Palace `Magnetostatic` parity): solve the
//! solid-coax `L'` oracle and the coaxial-loop-pair mutual-inductance
//! oracle, extract the Maxwell inductance matrices, and emit the committed
//! benchmark fixture `benchmarks/magnetostatic_inductance/results.toml`.
//!
//! Run with:
//!
//! ```text
//!   cargo run -p geode-core --example magnetostatic_inductance --release
//! ```
//!
//! Emits `benchmarks/magnetostatic_inductance/results.toml` — the baseline
//! the `tests/magnetostatic_inductance.rs` oracles guard, plus the pending
//! Palace `terminal-M` cross-check slot (`status = "pending_operator_run"`,
//! same toolchain-gap convention as the electrostatic / patch-antenna /
//! spiral-inductor benchmarks). Override the output root with
//! `$MAGNETOSTATIC_BENCH_DIR`.

use std::f64::consts::PI;
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

use geode_core::assembly::magnetostatic3d::{
    CurrentTerminal, MU_0, assemble_magnetostatic3d, axial_current_density, extract_inductance,
    loop_current_density, measure_axial_current, measure_loop_current,
};
use geode_core::assembly::nedelec::pec_interior_edge_mask;
use geode_core::mesh::magnetostatic_fixtures::{
    cylinder_cap_nodes, cylinder_pec_interior_mask, loop_pair_mesh, solid_coax_mesh,
};

/// Complete elliptic integrals `K(m)`, `E(m)` (`m = k²`) via the AGM /
/// descending-Landen recurrence (≈1e-12).
fn ellipk_e(m: f64) -> (f64, f64) {
    let mut a = 1.0;
    let mut b = (1.0 - m).sqrt();
    let mut c = m.sqrt();
    let mut sum = 0.0;
    let mut pow2 = 0.5;
    for _ in 0..60 {
        sum += pow2 * c * c;
        let an = 0.5 * (a + b);
        let bn = (a * b).sqrt();
        c = 0.5 * (a - b);
        a = an;
        b = bn;
        pow2 *= 2.0;
        if c.abs() < 1e-16 {
            break;
        }
    }
    let k = PI / (2.0 * a);
    (k, k * (1.0 - sum))
}

/// Maxwell mutual inductance of two coaxial circular filaments (radii
/// `r1`,`r2`, axial separation `d`).
fn maxwell_mutual(r1: f64, r2: f64, d: f64) -> f64 {
    let k2 = 4.0 * r1 * r2 / ((r1 + r2).powi(2) + d * d);
    let k = k2.sqrt();
    let (bigk, bige) = ellipk_e(k2);
    MU_0 * (r1 * r2).sqrt() * ((2.0 / k - k) * bigk - (2.0 / k) * bige)
}

fn main() {
    // ─────────────────────────────────────────────────────────────────
    // Oracle 1: solid coaxial cable, per-unit-length external+internal L'.
    // Closed form: L' = μ₀/(2π) ln(b/a)  +  μ₀/(8π).
    // ─────────────────────────────────────────────────────────────────
    let (ca, cb, clen) = (1.0_f64, 3.0_f64, 1.0_f64);
    let coax = solid_coax_mesh(ca, cb, clen, 64, 32, 3);
    let (coax_edges, coax_mask) = cylinder_pec_interior_mask(&coax.mesh, cb, clen);
    let coax_mu = vec![1.0; coax.mesh.n_tets()];
    let coax_sys = assemble_magnetostatic3d(&coax.mesh, &coax_mu, &coax_mask).unwrap();
    let coax_j = axial_current_density(&coax.mesh, ca, 1.0);
    let coax_i = measure_axial_current(&coax.mesh, &coax_j, clen);
    let coax_term = CurrentTerminal {
        name: "coax".into(),
        j: coax_j,
        current: coax_i,
        exempt_nodes: cylinder_cap_nodes(&coax.mesh, clen),
    };
    let coax_l = extract_inductance(
        &coax_sys,
        &coax.mesh,
        std::slice::from_ref(&coax_term),
        1e-6,
    )
    .unwrap();
    let coax_lp = coax_l.l[0][0] / clen;
    let l_ext = MU_0 / (2.0 * PI) * (cb / ca).ln();
    let l_int = MU_0 / (8.0 * PI);
    let l_closed = l_ext + l_int;

    // ─────────────────────────────────────────────────────────────────
    // Oracle 2: two coaxial loops, off-diagonal Maxwell mutual M.
    // ─────────────────────────────────────────────────────────────────
    let (lr1, lz1, lr2, lz2) = (1.0_f64, 2.0_f64, 1.0_f64, 3.0_f64);
    let (rbox, llen, rtube) = (6.0_f64, 5.0_f64, 0.18_f64);
    let lp = loop_pair_mesh(lr1, lz1, lr2, lz2, rbox, llen, 40, 18, 18);
    let tolb = 1e-6 * rbox;
    let tolz = 1e-6 * llen;
    let on_bdry: Vec<bool> = lp
        .mesh
        .nodes
        .iter()
        .map(|p| {
            let r = (p[0] * p[0] + p[1] * p[1]).sqrt();
            (r - rbox).abs() < tolb || p[2].abs() < tolz || (p[2] - llen).abs() < tolz
        })
        .collect();
    let lp_edges = lp.mesh.edges();
    let lp_mask = pec_interior_edge_mask(&lp_edges, &on_bdry);
    let lp_mu = vec![1.0; lp.mesh.n_tets()];
    let lp_sys = assemble_magnetostatic3d(&lp.mesh, &lp_mu, &lp_mask).unwrap();
    let j1 = loop_current_density(&lp.mesh, lr1, lz1, rtube, 1.0);
    let j2 = loop_current_density(&lp.mesh, lr2, lz2, rtube, 1.0);
    let i1 = measure_loop_current(&lp.mesh, &j1);
    let i2 = measure_loop_current(&lp.mesh, &j2);
    let lp_terms = vec![
        CurrentTerminal {
            name: "loop1".into(),
            j: j1,
            current: i1,
            exempt_nodes: vec![],
        },
        CurrentTerminal {
            name: "loop2".into(),
            j: j2,
            current: i2,
            exempt_nodes: vec![],
        },
    ];
    let lp_l = extract_inductance(&lp_sys, &lp.mesh, &lp_terms, 5e-2).unwrap();
    let m_fem = lp_l.l[0][1];
    let m_exact = maxwell_mutual(lr1, lr2, (lz2 - lz1).abs());
    let l11 = lp_l.l[0][0];
    let l22 = lp_l.l[1][1];
    // Thin-wire self-inductance band (Neumann approximation).
    let l_self_band = MU_0 * lr1 * ((8.0 * lr1 / rtube).ln() - 2.0);

    // ─────────────────────────────────────────────────────────────────
    // Emit results.toml.
    // ─────────────────────────────────────────────────────────────────
    let mut out = String::new();
    let _ = writeln!(
        out,
        "# Auto-generated by `cargo run -p geode-core --release \\\n\
         #   --example magnetostatic_inductance`.\n\
         # Do NOT edit by hand — regenerate after any intentional change.\n\
         # Consumed by `tests/magnetostatic_inductance.rs`.\n"
    );
    let _ = writeln!(out, "[meta]");
    let _ = writeln!(
        out,
        "description = \"3-D vector magnetostatic solver + Maxwell inductance-matrix extraction (Epic #475, Palace Magnetostatic parity): lowest-order Nedelec edge solve of curl(nu curl A)=J with per-element nu_r=1/mu_r, tree-cotree gauge, RHS-driven unit-current terminals; N solves -> L_ij = A_i^T K A_j/(I_i I_j) (full K). Programmatically-meshed solid-coax L' oracle at <=1% (external+internal closed form) and coaxial-loop-pair off-diagonal mutual M vs the Maxwell elliptic-integral formula.\""
    );
    let _ = writeln!(
        out,
        "method = \"energy_reaction\"  # L_ij = A_i^T K A_j / (I_i I_j), full pre-gauge K"
    );
    let _ = writeln!(
        out,
        "cross_check = \"flux_linkage\"  # Phi_i = A_i^T b_i / I_i = L_ii I_i"
    );
    let _ = writeln!(
        out,
        "gauge = \"tree_cotree\"  # eigen::gauge::TreeCotreeGauge (PR #508); B=curl A gauge-invariant for the source problem"
    );
    let _ = writeln!(out, "element = \"nedelec1_tet\"");
    let _ = writeln!(out, "permeability_h_per_m = {:.10e}  # mu_0", MU_0);

    let _ = writeln!(out, "\n[oracles.coax]");
    let _ = writeln!(
        out,
        "# Solid conductor radius a inside PEC shield radius b."
    );
    let _ = writeln!(
        out,
        "# L'/length = mu0/(2pi) ln(b/a) + mu0/(8pi) (external + internal)."
    );
    let _ = writeln!(out, "inner_radius_a = {ca}");
    let _ = writeln!(out, "outer_radius_b = {cb}");
    let _ = writeln!(out, "nodes = {}", coax.mesh.n_nodes());
    let _ = writeln!(out, "edges = {}", coax_edges.len());
    let _ = writeln!(out, "tets = {}", coax.mesh.n_tets());
    let _ = writeln!(out, "l_per_length_fem = {coax_lp:.9e}");
    let _ = writeln!(out, "l_per_length_closed = {l_closed:.9e}");
    let _ = writeln!(out, "l_external_exact = {l_ext:.9e}  # mu0/(2pi) ln(b/a)");
    let _ = writeln!(out, "l_internal_exact = {l_int:.9e}  # mu0/(8pi)");
    let _ = writeln!(
        out,
        "rel_err = {:.6e}  # bar: <= 1e-2 vs closed form",
        (coax_lp - l_closed).abs() / l_closed
    );
    let _ = writeln!(
        out,
        "flux_linkage_per_length = {:.9e}  # cross-check (= l_per_length_fem)",
        coax_l.flux_linkage_diag[0] / clen
    );

    let _ = writeln!(out, "\n[oracles.loop_pair]");
    let _ = writeln!(
        out,
        "# Two coaxial circular loops; off-diagonal Maxwell mutual M."
    );
    let _ = writeln!(
        out,
        "# M = mu0 sqrt(r1 r2) [(2/k - k)K(k) - (2/k)E(k)], k^2=4 r1 r2/((r1+r2)^2+d^2)."
    );
    let _ = writeln!(out, "loop_radius = {lr1}");
    let _ = writeln!(out, "separation = {}", (lz2 - lz1).abs());
    let _ = writeln!(out, "tube_minor_radius = {rtube}");
    let _ = writeln!(out, "box_radius = {rbox}");
    let _ = writeln!(out, "nodes = {}", lp.mesh.n_nodes());
    let _ = writeln!(out, "edges = {}", lp_edges.len());
    let _ = writeln!(out, "tets = {}", lp.mesh.n_tets());
    let _ = writeln!(
        out,
        "measured_current_1 = {i1:.6e}  # discrete threading current"
    );
    let _ = writeln!(out, "measured_current_2 = {i2:.6e}");
    let _ = writeln!(out, "m_mutual_fem = {m_fem:.9e}");
    let _ = writeln!(out, "m_mutual_exact = {m_exact:.9e}");
    let _ = writeln!(
        out,
        "m_rel_err = {:.6e}  # honest band ~5% at this resolution: fat-tube (r_tube/R=0.18) filament idealization + PEC-box truncation dominate; sign + order + symmetry exact. Not a tight 1% gate (cf. the two-sphere-box off-diagonal of benchmarks/electrostatic/results.toml).",
        (m_fem - m_exact).abs() / m_exact.abs()
    );
    let _ = writeln!(out, "l_self_1_fem = {l11:.9e}");
    let _ = writeln!(out, "l_self_2_fem = {l22:.9e}");
    let _ = writeln!(
        out,
        "l_self_thinwire_band = {l_self_band:.9e}  # mu0 R[ln(8R/a)-2], Neumann approx band (NOT a tight bar)"
    );
    let _ = writeln!(
        out,
        "max_rel_asymmetry = {:.3e}  # hard check: < 1e-9",
        lp_l.max_rel_asymmetry()
    );

    let _ = writeln!(out, "\n[oracles.palace]");
    let _ = writeln!(out, "status = \"pending_operator_run\"");
    let _ = writeln!(
        out,
        "note = \"Palace's Magnetostatic module extracts the same terminal inductance matrix (terminal-M.csv). Palace is not installed on the generation machine (Docker build recipe only, per #476's verified note), so this slot stays pending until an operator runs Palace. Same toolchain-gap convention as the Palace slot of benchmarks/electrostatic/results.toml and the FastHenry slot of benchmarks/spiral_inductor/results.toml. Tolerance band when populated: 1% on the coax diagonal L', honest few-% on the loop-pair off-diagonal M.\""
    );

    let root = std::env::var("MAGNETOSTATIC_BENCH_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .join("benchmarks")
                .join("magnetostatic_inductance")
        });
    fs::create_dir_all(&root).unwrap();
    let path = root.join("results.toml");
    fs::write(&path, out).unwrap();

    println!(
        "coax L'_fem={coax_lp:.6e} closed={l_closed:.6e} rel={:.4e}",
        (coax_lp - l_closed).abs() / l_closed
    );
    println!(
        "loop M_fem={m_fem:.6e} exact={m_exact:.6e} rel={:.4e}",
        (m_fem - m_exact).abs() / m_exact.abs()
    );
    println!("wrote {}", path.display());
}
