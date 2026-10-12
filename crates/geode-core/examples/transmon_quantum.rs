//! Transmon quantum-parameter benchmark generator (Epic #476 Phase C,
//! issue #505): extract the Maxwell capacitance matrix on the real
//! DeviceLayout `SingleTransmon` mesh (split into its three node-disjoint
//! metal conductors), derive `C_Σ`, `E_C`, and the Koch-exact `ω01`/`α`,
//! and emit `benchmarks/transmon_quantum/results.toml`.
//!
//! `E_C = e²/(2(C_Σ + C_J))` with the junction's lumped `C_J = 5.5 fF`
//! (issue #1066): the electrostatic `C_Σ` excludes the junction. The
//! anchors come from `TransmonAnchor::devicelayout`.
//!
//! Run with:
//!
//! ```text
//!   cargo run -p geode-core --example transmon_quantum --release
//! ```
//!
//! The electrostatic solve is on the full 133k-tet mesh, so it is a
//! release build. Override the output root with `$TRANSMON_QUANTUM_BENCH_DIR`.
//!
//! # Unit handling
//!
//! The DeviceLayout mesh is in **micrometres**; the electrostatic operator
//! carries `ε₀` in SI (F/m). We therefore scale the node coordinates to
//! **metres** (× `1e-6`) before assembly, so the extracted capacitance is
//! in SI farads directly (`C = φᵀ K φ`, `K ∝ ε₀ · length`).

use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

use geode_core::assembly::electrostatic::{
    Electrode, assemble_electrostatic, assemble_electrostatic_tensor, extract_capacitance,
};
use geode_core::mesh::transmon::{
    JUNCTION_CAPACITANCE_F, JUNCTION_INDUCTANCE_H, sapphire_eps_lab, sapphire_eps_scalar,
};
use geode_core::mesh::{MetalRole, TetMesh, read_transmon_smoke_fixture};
use geode_core::quantum::transmon::{
    TransmonAnchor, cross_kerr_from_epr, e_c_hz_from_c_sigma, e_j_hz_from_inductance,
    harmonic_frequency_hz, omega01_asymptotic_hz, self_kerr_from_epr, transmon_spectrum,
};

/// Metres per mesh unit (micrometre mesh).
const M_PER_UNIT: f64 = 1e-6;

// Issue #1051 mesh-convergence values, copied from
// `benchmarks/transmon_diffopt/c_sigma_convergence.toml` (`[tensor_eps]`,
// `[x0_extrapolation]`) and cross-checked against it by
// `tests/transmon_diffopt.rs`.
/// Tensor-ε P1 `C_Σ` at h, h/2, h/4 (fF).
const MESH_CONV_TENSOR_P1_FF: [f64; 3] = [136.684731, 102.375301, 89.493342];
/// Tensor-ε upper bound (P1 h/4, rounded up) (fF).
const MESH_CONV_TENSOR_UPPER_FF: f64 = 89.50;
/// Tensor-ε Richardson estimate band (fF).
const MESH_CONV_TENSOR_BAND_FF: [f64; 2] = [76.61, 81.75];
/// Scalar-ε upper bound (P2 h/2, rounded up) (fF).
const MESH_CONV_SCALAR_UPPER_FF: f64 = 83.02;
/// Scalar-ε estimate band (fF).
const MESH_CONV_SCALAR_BAND_FF: [f64; 2] = [74.14, 79.62];

/// Scale a mesh's node coordinates by `s` (in place on a clone).
fn scaled_mesh(mesh: &TetMesh, s: f64) -> TetMesh {
    let mut m = mesh.clone();
    for n in m.nodes.iter_mut() {
        n[0] *= s;
        n[1] *= s;
        n[2] *= s;
    }
    m
}

fn main() {
    let fx = read_transmon_smoke_fixture().expect("load transmon fixture");
    // Coordinates → metres so ε₀ (F/m) yields SI farads.
    let mesh_m = scaled_mesh(&fx.mesh, M_PER_UNIT);
    let n_tets = mesh_m.n_tets();

    // Split the metal group into ground / feedline / island conductors.
    let comps = fx.split_metal_conductors();
    let ground = comps
        .iter()
        .find(|c| c.role == MetalRole::Ground)
        .expect("ground component");
    let island = comps
        .iter()
        .find(|c| c.role == MetalRole::Island)
        .expect("island component");
    let feedline = comps
        .iter()
        .find(|c| c.role == MetalRole::Feedline)
        .expect("feedline component");

    // Conductors: {island, feedline}; ground plane = Dirichlet ground.
    let conductors = vec![
        Electrode {
            name: "island".into(),
            nodes: island.nodes.clone(),
            voltage: 1.0,
        },
        Electrode {
            name: "feedline".into(),
            nodes: feedline.nodes.clone(),
            voltage: 0.0,
        },
    ];
    let ground_nodes = ground.nodes.clone();
    let rho = vec![0.0; n_tets];

    // ---- Scalar-ε run (trace-averaged sapphire ε̄ on substrate) ----
    let eps_scalar = fx.epsilon_r_scalar();
    let sys_s =
        assemble_electrostatic(&mesh_m, &eps_scalar, &rho, &conductors, &ground_nodes).unwrap();
    let cm_s = extract_capacitance(
        &sys_s,
        &mesh_m,
        &eps_scalar,
        &conductors,
        &ground_nodes,
        &[],
    )
    .unwrap();

    // ---- Tensor-ε run (rotated sapphire lab tensor on substrate) ----
    let lab = sapphire_eps_lab();
    let identity = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    let substrate = fx.tags.substrate;
    let eps_tensor: Vec<[[f64; 3]; 3]> = fx
        .tet_physical_tags
        .iter()
        .map(|&t| if t == substrate { lab } else { identity })
        .collect();
    let sys_t =
        assemble_electrostatic_tensor(&mesh_m, &eps_tensor, &rho, &conductors, &ground_nodes)
            .unwrap();
    let cm_t = extract_capacitance(&sys_t, &mesh_m, &[], &conductors, &ground_nodes, &[]).unwrap();

    // ---- BC sensitivity: exterior far-field wall GROUNDED ----
    // The baseline leaves the exterior boundary natural (open Neumann); a
    // cheap sensitivity check adds the far-field wall to the ground set
    // (Dirichlet phi=0). If C_Σ moves a lot, the open truncation is a
    // dominant model term; if little, the island-to-ground-plane path
    // dominates and the open BC is benign.
    let ext_tris = fx.exterior_boundary_triangles();
    let mut ext_nodes: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
    for tri in &ext_tris {
        for &n in tri {
            ext_nodes.insert(n);
        }
    }
    let mut ground_plus_ext = ground.nodes.clone();
    ground_plus_ext.extend(ext_nodes.iter().copied());
    ground_plus_ext.sort_unstable();
    ground_plus_ext.dedup();
    let sys_ext =
        assemble_electrostatic_tensor(&mesh_m, &eps_tensor, &rho, &conductors, &ground_plus_ext)
            .unwrap();
    let cm_ext =
        extract_capacitance(&sys_ext, &mesh_m, &[], &conductors, &ground_plus_ext, &[]).unwrap();
    let c_island_grounded_ext = cm_ext.get("island", "island").unwrap();

    // ---- C_Σ across the junction (grounded transmon) ----
    // The junction bridges island → ground. The island's total
    // capacitance to ground is its Maxwell self-cap MINUS the feedline
    // coupling folded out (floating-trace reduction): with the feedline
    // left floating (not a driven terminal), C_Σ = C_island,island −
    // C_island,feedline² / C_feedline,feedline (Schur/ground reduction of
    // the floating conductor). We report both the raw self-cap and the
    // reduced C_Σ.
    let c_sigma = |cm: &geode_core::assembly::electrostatic::CapacitanceMatrix| -> (f64, f64) {
        let ii = cm.get("island", "island").unwrap();
        let itf = cm.get("island", "feedline").unwrap();
        let ff = cm.get("feedline", "feedline").unwrap();
        // Maxwell self-cap of the island (row sum to ground = C_ii + C_if,
        // with C_if ≤ 0 the mutual): the island-to-ground capacitance is
        // the Maxwell diagonal (already the self-cap to the grounded system).
        let raw = ii;
        // Floating-trace reduction (feedline not terminated): fold it out.
        let reduced = ii - (itf * itf) / ff;
        (raw, reduced)
    };
    let (_c_sig_raw_s, c_sig_red_s) = c_sigma(&cm_s);
    let (c_sig_raw_t, c_sig_red_t) = c_sigma(&cm_t);

    // Choose the tensor run's reduced C_Σ as the physical value (rotated
    // sapphire is the true substrate); report the scalar-vs-tensor delta.
    let c_sigma_phys = c_sig_red_t;
    let scalar_tensor_delta = (c_sig_red_s - c_sig_red_t).abs() / c_sig_red_t;

    // ---- Quantum parameters ----
    let e_j = e_j_hz_from_inductance(JUNCTION_INDUCTANCE_H);
    // Issue #1066: the junction self-capacitance is in parallel with C_Σ.
    let e_c = e_c_hz_from_c_sigma(c_sigma_phys, JUNCTION_CAPACITANCE_F);
    let harmonic =
        harmonic_frequency_hz(JUNCTION_INDUCTANCE_H, c_sigma_phys + JUNCTION_CAPACITANCE_F);
    let e_j_over_e_c = e_j / e_c;
    let spec = transmon_spectrum(e_j, e_c, 0.0, 40, 3);
    let w01 = spec.omega01_hz();
    let w01_asymp = omega01_asymptotic_hz(e_j, e_c);
    let alpha = spec.anharmonicity_hz();

    // Derived honest ω01 bar: ω01 ∝ √E_C, so δω01/ω01 ≈ ½ δC_Σ/C_Σ.
    // C-matrix validation accuracy is the ~1% class (PR #481 oracles), and
    // the scalar-vs-tensor spread is an additional ε-model term.
    let c_accuracy = 0.01_f64; // PR #481 oracle class
    let dw01_from_c = 0.5 * c_accuracy;
    let dw01_from_eps = 0.5 * scalar_tensor_delta;
    let w01_band_frac = dw01_from_c + dw01_from_eps;

    // The DeviceLayout anchor (issue #1066; an expectation to compare
    // against, not a tuning target here): 4.14 GHz harmonic with L_J ∥ C_J.
    let anchor = TransmonAnchor::devicelayout();

    // ---- Emit results.toml ----
    let mut t = String::with_capacity(8192);
    t.push_str("# Auto-generated by `cargo run -p geode-core --release \\\n");
    t.push_str("#   --example transmon_quantum`.\n");
    t.push_str("# Do NOT edit by hand — regenerate after any intentional change.\n");
    t.push_str("# Consumed by `tests/transmon_quantum.rs`.\n\n");

    t.push_str("[meta]\n");
    let _ = writeln!(
        t,
        "description = \"Transmon qubit parameters (Epic #476 Phase C): E_C from the Maxwell capacitance matrix on the real DeviceLayout SingleTransmon mesh (split into 3 node-disjoint metal conductors: ground+resonator / feedline / island), E_J from the junction inductance, and the Koch 2007 charge-basis-exact omega01/alpha. Grounded-transmon topology: junction bridges island to ground.\""
    );
    t.push_str("fixture = \"crates/geode-core/tests/fixtures/transmon_smoke.msh\"\n");
    let _ = writeln!(t, "n_nodes = {}", mesh_m.n_nodes());
    let _ = writeln!(t, "n_tets = {n_tets}");
    t.push_str("mesh_length_unit = \"micrometre\"  # coords scaled to metres for SI eps_0\n");
    let _ = writeln!(
        t,
        "junction_inductance_nh = {}",
        JUNCTION_INDUCTANCE_H * 1e9
    );
    let _ = writeln!(
        t,
        "junction_capacitance_ff = {}  # lumped C_J in parallel with C_Sigma; included in E_C (issue #1066)",
        JUNCTION_CAPACITANCE_F * 1e15
    );
    t.push_str("method = \"energy_reaction\"  # C_ij = phi_i^T K phi_j, full K\n");
    t.push_str(
        "boundary_condition = \"exterior natural (Neumann); ground plane Dirichlet phi=0\"\n",
    );
    let _ = writeln!(
        t,
        "sapphire_eps_scalar = {:.6}  # trace-averaged (9.3+9.3+11.5)/3",
        sapphire_eps_scalar()
    );
    t.push_str("sapphire_eps_tensor_diag = [9.3, 9.3, 11.5]  # rotated lab tensor (in-plane isotropic -> diagonal)\n\n");

    // Full C matrix (both ε models).
    t.push_str("[capacitance.tensor_eps]\n");
    t.push_str("# Rotated sapphire lab tensor on substrate (the physical model). Farads.\n");
    let _ = writeln!(
        t,
        "c_island_island_ff = {:.6}",
        cm_t.get("island", "island").unwrap() * 1e15
    );
    let _ = writeln!(
        t,
        "c_island_feedline_ff = {:.6}",
        cm_t.get("island", "feedline").unwrap() * 1e15
    );
    let _ = writeln!(
        t,
        "c_feedline_feedline_ff = {:.6}",
        cm_t.get("feedline", "feedline").unwrap() * 1e15
    );
    let _ = writeln!(
        t,
        "c_sigma_raw_ff = {:.6}  # island Maxwell self-cap to ground",
        c_sig_raw_t * 1e15
    );
    let _ = writeln!(
        t,
        "c_sigma_reduced_ff = {:.6}  # floating-feedline reduction C_ii - C_if^2/C_ff",
        c_sig_red_t * 1e15
    );
    let _ = writeln!(t, "max_rel_asymmetry = {:.3e}", cm_t.max_rel_asymmetry());
    t.push('\n');

    // Issue #1051: the mesh-convergence study of this C_Sigma. The numbers
    // are copied from benchmarks/transmon_diffopt/c_sigma_convergence.toml
    // (generator: examples/transmon_c_sigma_convergence.rs) and cross-checked
    // against it by tests/transmon_diffopt.rs.
    let c_t = cm_t.get("island", "island").unwrap() * 1e15;
    assert!(
        (c_t - MESH_CONV_TENSOR_P1_FF[0]).abs() < 1e-5,
        "the #1051 convergence study starts from a different coarse tensor C_Sigma"
    );
    t.push_str("[capacitance.mesh_convergence]\n");
    t.push_str("# Issue #1051. The C_Sigma above is NOT mesh-converged. Values copied from\n");
    t.push_str("# benchmarks/transmon_diffopt/c_sigma_convergence.toml (uniform red\n");
    t.push_str("# refinement of this mesh; every value is a conforming upper bound).\n");
    t.push_str("source = \"benchmarks/transmon_diffopt/c_sigma_convergence.toml\"\n");
    let _ = writeln!(
        t,
        "tensor_p1_c_sigma_ff = [{:.6}, {:.6}, {:.6}]  # h (this file), h/2, h/4",
        MESH_CONV_TENSOR_P1_FF[0], MESH_CONV_TENSOR_P1_FF[1], MESH_CONV_TENSOR_P1_FF[2]
    );
    let _ = writeln!(
        t,
        "tensor_upper_bound_ff = {MESH_CONV_TENSOR_UPPER_FF}  # P1 h/4, rounded up: the converged tensor C_Sigma is below this"
    );
    let _ = writeln!(
        t,
        "tensor_estimate_band_ff = [{}, {}]  # Richardson at p = 1 and at the observed order; an estimate, not a bound",
        MESH_CONV_TENSOR_BAND_FF[0], MESH_CONV_TENSOR_BAND_FF[1]
    );
    let _ = writeln!(
        t,
        "scalar_upper_bound_ff = {MESH_CONV_SCALAR_UPPER_FF}  # P2 h/2, rounded up"
    );
    let _ = writeln!(
        t,
        "scalar_estimate_band_ff = [{}, {}]  # an estimate, not a bound",
        MESH_CONV_SCALAR_BAND_FF[0], MESH_CONV_SCALAR_BAND_FF[1]
    );
    let e_c_factor = c_t / MESH_CONV_TENSOR_UPPER_FF;
    let _ = writeln!(
        t,
        "coarse_c_sigma_overstated_by_at_least_ff = {:.2}  # this file's tensor C_Sigma minus tensor_upper_bound_ff (floored)",
        ((c_t - MESH_CONV_TENSOR_UPPER_FF) * 100.0).floor() / 100.0
    );
    let _ = writeln!(
        t,
        "e_c_understated_by_at_least_pct = {:.1}  # E_C ~ 1/C_Sigma: (coarse C / upper bound - 1), floored",
        ((e_c_factor - 1.0) * 1000.0).floor() / 10.0
    );
    t.push_str("reading = \"measured: the converged C_Sigma of this geometry is below the 93.95 fF anchor of [quantum.anchors] (and below the superseded 89.9 fF anchor), so the 136.7 fF vs about 94 fF gap reported here is coarse-mesh discretization error, not a model difference. Every C_Sigma, E_C, E_J/E_C, omega01, alpha and harmonic frequency in this file is a coarse-mesh number. The scalar-vs-tensor delta is one too: it changes sign under refinement. What remains against the anchor is a gap of the opposite sign (converged C_Sigma below the anchor), whose cause is not measured here.\"\n\n");

    t.push_str("[capacitance.bc_sensitivity]\n");
    t.push_str("# Exterior far-field wall grounded (Dirichlet) vs the baseline\n");
    t.push_str("# open natural-Neumann truncation — the BC-model term.\n");
    let _ = writeln!(
        t,
        "c_island_open_ff = {:.6}  # baseline (exterior natural)",
        cm_t.get("island", "island").unwrap() * 1e15
    );
    let _ = writeln!(
        t,
        "c_island_grounded_ext_ff = {:.6}  # exterior wall grounded",
        c_island_grounded_ext * 1e15
    );
    let _ = writeln!(
        t,
        "bc_rel_delta = {:.4e}",
        (cm_t.get("island", "island").unwrap() - c_island_grounded_ext).abs()
            / cm_t.get("island", "island").unwrap()
    );
    t.push('\n');

    t.push_str("[capacitance.scalar_eps]\n");
    t.push_str("# Trace-averaged scalar eps on substrate (the approximation). Farads.\n");
    let _ = writeln!(
        t,
        "c_island_island_ff = {:.6}",
        cm_s.get("island", "island").unwrap() * 1e15
    );
    let _ = writeln!(
        t,
        "c_island_feedline_ff = {:.6}",
        cm_s.get("island", "feedline").unwrap() * 1e15
    );
    let _ = writeln!(
        t,
        "c_feedline_feedline_ff = {:.6}",
        cm_s.get("feedline", "feedline").unwrap() * 1e15
    );
    let _ = writeln!(t, "c_sigma_reduced_ff = {:.6}", c_sig_red_s * 1e15);
    let _ = writeln!(t, "max_rel_asymmetry = {:.3e}", cm_s.max_rel_asymmetry());
    t.push('\n');

    t.push_str("[capacitance.scalar_vs_tensor]\n");
    t.push_str("# The anisotropy delta the issue asks to quantify (NOT assumed a priori).\n");
    let _ = writeln!(t, "c_sigma_rel_delta = {scalar_tensor_delta:.4e}");
    t.push_str("note = \"The in-plane sapphire block is isotropic (eps1=eps2=9.3), so the rotated lab tensor is diagonal diag(9.3,9.3,11.5); the scalar-vs-tensor difference is the in-plane (9.3) vs out-of-plane (11.5) anisotropy seen by the mixed-orientation field between planar conductors, not a rotation-coupling effect.\"\n\n");

    // Quantum parameters.
    t.push_str("[quantum]\n");
    t.push_str("# h-convention throughout (E/h in Hz; divide by 1e9 for GHz).\n");
    t.push_str(
        "# E_C = e^2/(2 (C_Sigma + C_J)), C_J = meta.junction_capacitance_ff (issue #1066).\n",
    );
    let _ = writeln!(
        t,
        "c_sigma_ff = {:.4}  # tensor reduced (physical)",
        c_sigma_phys * 1e15
    );
    let _ = writeln!(t, "e_j_ghz = {:.6}", e_j / 1e9);
    let _ = writeln!(t, "e_c_ghz = {:.6}", e_c / 1e9);
    let _ = writeln!(t, "e_j_over_e_c = {e_j_over_e_c:.3}");
    let _ = writeln!(
        t,
        "omega01_koch_ghz = {:.6}  # gate: charge-basis exact",
        w01 / 1e9
    );
    let _ = writeln!(
        t,
        "omega01_asymptotic_ghz = {:.6}  # sqrt(8 E_J E_C) - E_C (sanity)",
        w01_asymp / 1e9
    );
    let _ = writeln!(
        t,
        "alpha_koch_ghz = {:.6}  # E_2 - 2 E_1 + E_0 (exact)",
        alpha / 1e9
    );
    let _ = writeln!(t, "alpha_minus_ec_ghz = {:.6}  # -E_C sanity", -e_c / 1e9);
    let _ = writeln!(
        t,
        "harmonic_lc_ghz = {:.6}  # 1/(2 pi sqrt(L_J (C_Sigma + C_J))) = sqrt(8 E_J E_C), what a linear eigensolve reports",
        harmonic / 1e9
    );
    let _ = writeln!(t, "spectrum_converged = {}", spec.converged);
    t.push('\n');

    t.push_str("[quantum.derived_omega01_band]\n");
    t.push_str("# HONEST bar, DERIVED (not retrofitted): omega01 ~ sqrt(E_C), so\n");
    t.push_str("# delta_omega01/omega01 ~= 1/2 delta_C_Sigma/C_Sigma.\n");
    let _ = writeln!(
        t,
        "c_matrix_accuracy = {c_accuracy}  # PR #481 oracle ~1% class; does NOT cover this mesh's discretization error (#1051)"
    );
    let _ = writeln!(
        t,
        "band_scope = \"this band is the PR #481 oracle class plus the eps-model term on the coarse mesh; it excludes the mesh discretization error, which [capacitance.mesh_convergence] measures to be far larger (at least {:.1}% of this file's C_Sigma, floored), so it is not an accuracy band for omega01\"",
        ((c_t - MESH_CONV_TENSOR_UPPER_FF) / c_t * 1000.0).floor() / 10.0
    );
    let _ = writeln!(t, "dw01_from_c_frac = {dw01_from_c:.4e}");
    let _ = writeln!(t, "dw01_from_eps_model_frac = {dw01_from_eps:.4e}");
    let _ = writeln!(t, "omega01_band_frac = {w01_band_frac:.4e}");
    let _ = writeln!(
        t,
        "omega01_band_ghz = [{:.4}, {:.4}]",
        w01 / 1e9 * (1.0 - w01_band_frac),
        w01 / 1e9 * (1.0 + w01_band_frac)
    );
    t.push_str("terms_not_in_this_c_sigma = \"floating readout feedline (folded out via Schur reduction), junction self-C 5.5 fF (not in this electrostatic C_Sigma), exterior natural-Neumann truncation\"\n");
    t.push_str("anchor_gap = \"the 136.7 fF vs about 94 fF gap is discretization error (issue #1051, [capacitance.mesh_convergence]), not the model terms above: the BC term is 6e-5, and the junction self-C is on neither side of the comparison (excluded from this C_Sigma and from the anchor C_Sigma, added to both in E_C, issue #1066)\"\n\n");

    t.push_str("[quantum.anchors]\n");
    t.push_str("# EXPECTATIONS to compare against (NOT tuning targets), issue #1066:\n");
    t.push_str("# DeviceLayout's published 4.14 GHz is the harmonic frequency of the\n");
    t.push_str("# linear L_J || (C_Sigma + C_J) eigenproblem Palace solves. Derived in\n");
    t.push_str("# geode_core::quantum::transmon::TransmonAnchor::devicelayout.\n");
    let _ = writeln!(
        t,
        "devicelayout_harmonic_ghz = {}  # published DeviceLayout SingleTransmon qubit frequency",
        anchor.frequency_hz / 1e9
    );
    let _ = writeln!(
        t,
        "expected_c_total_ff = {:.4}  # 1/((2 pi f)^2 L_J)",
        anchor.c_total_farad * 1e15
    );
    let _ = writeln!(
        t,
        "expected_c_sigma_ff = {:.4}  # c_total - C_J, junction excluded",
        anchor.c_sigma_farad * 1e15
    );
    let _ = writeln!(
        t,
        "expected_e_c_ghz = {:.6}  # e^2/(2 C_total h)",
        anchor.e_c_hz / 1e9
    );
    let _ = writeln!(
        t,
        "expected_e_j_over_e_c = {:.2}",
        anchor.e_j_hz / anchor.e_c_hz
    );
    t.push_str("superseded = \"the earlier anchor E_C = 0.2156 GHz, C_Sigma = 89.84-89.9 fF, E_J/E_C = 51 inverted the anharmonic sqrt(8 E_J E_C) - E_C at 4.14 GHz and left out C_J (issue #1066); it is not used\"\n");
    t.push_str("phase_b_c_sigma_note_ff = \"80-100\"\n\n");

    // Kerr closed forms at the physical E_C (qubit p≈1 example).
    t.push_str("[quantum.kerr_epr]\n");
    t.push_str("# EPR/BBQ dispersive closed forms at the extracted E_C.\n");
    let _ = writeln!(
        t,
        "self_kerr_p1_ghz = {:.6}  # -(E_C/2) p^2 at p=1",
        self_kerr_from_epr(e_c, 1.0) / 1e9
    );
    let _ = writeln!(
        t,
        "cross_kerr_p1_p001_ghz = {:.6}  # -2 E_C p_m p_n, p_m=1 p_n=0.01",
        cross_kerr_from_epr(e_c, 1.0, 0.01) / 1e9
    );
    t.push('\n');

    // ---- Minev p_m 3-way reconciliation over the seven committed modes ----
    // The Phase-B modes + their stiffness-participation are committed in
    // benchmarks/transmon_eigen/results.toml `[modes.*]`: the qubit mode, five
    // cavity modes and the 17.49 GHz port-sheet artifact (issue #1045; the
    // qubit row was absent before, and the artifact was named `junction_lc`).
    // In the reactive-lumped-shunt formulation the Minev energy-participation p_m = junction inductive /
    // total inductive energy is ALGEBRAICALLY the same quantity as the Phase
    // B stiffness participation p = xᵀ K_port x / xᵀ (K+K_port) x (both are
    // the junction inductive-energy fraction of the same eigenvector). So
    // the Minev p_m column EQUALS the committed participation column; the
    // reconciliation is against Palace's differently-normalized field
    // port-EPR. (The release test tests/transmon_quantum.rs re-derives p_m
    // from a fresh eigensolve to confirm the identity numerically.)
    // Committed values (transmon_eigen/results.toml + Palace port-EPR.csv; the
    // qubit row's Palace value is the Target = 2 GHz re-run of #1043,
    // reference/fixtures/transmon_palace/results_p1_target2/target2_L1/port-EPR.csv):
    let modes: [(&str, f64, f64, f64); 7] = [
        // (name, f_ghz, phase_b_participation == minev p_m, palace port-EPR p[1])
        ("qubit", 3.452751, 0.9942, 9.942420548984e-1),
        ("resonator", 5.153, 0.000, -4.706079891562e-4),
        ("mode_2", 15.465, 0.000, -1.710883210358e-5),
        ("port_sheet_artifact", 17.490, 1.000, 2.494782690465e-8),
        ("mode_4", 18.693, 0.000, -2.975892992981e-9),
        ("mode_5", 20.703, 0.000, -1.172141061263e-8),
        ("mode_6", 26.088, 0.000, -6.038680846706e-6),
    ];
    t.push_str("[epr_reconciliation]\n");
    t.push_str("# 3-way reconciliation over the seven committed Phase-B field modes:\n");
    t.push_str("# the qubit mode, five cavity modes and the port-sheet artifact\n");
    t.push_str("# (issue #1045; the artifact was named junction_lc and the qubit\n");
    t.push_str("# row was absent). minev_p_m == phase_b_participation ALGEBRAICALLY\n");
    t.push_str("# (reactive-shunt formulation makes junction-inductive-energy-fraction\n");
    t.push_str("# the same quantity). Palace port-EPR is a differently-normalized,\n");
    t.push_str("# SIGNED field diagnostic. It agrees with p on the qubit mode\n");
    t.push_str("# (0.99424 vs 0.9942, Palace at Target 2 GHz, #1043) and differs on\n");
    t.push_str("# the port-sheet artifact (2.49e-8 vs 1.000), which stores its energy\n");
    t.push_str("# in the port sheet without a port voltage. Mode ID is by frequency.\n");
    t.push_str("modes = [\n");
    for (name, f, p_minev, p_palace) in &modes {
        // Participations at their committed precision: 4 decimals for the
        // qubit (0.9942), 3 for the others.
        let p_txt = if *name == "qubit" {
            format!("{p_minev:.4}")
        } else {
            format!("{p_minev:.3}")
        };
        let _ = writeln!(
            t,
            "  {{ name = \"{name}\", f_ghz = {f}, minev_p_m = {p_txt}, phase_b_participation = {p_txt}, palace_port_epr = {p_palace:.6e} }},"
        );
    }
    t.push_str("]\n");
    t.push_str(
        "minev_equals_phase_b = true  # algebraic identity, tested in tests/transmon_quantum.rs\n",
    );
    t.push_str(
        "palace_ranks_differently = true  # on the port-sheet artifact only; the qubit agrees (0.9942 vs 0.99424)\n",
    );
    t.push_str("mode_id_basis = \"frequency agreement (3.4527 GHz qubit mode and 17.4901 GHz port-sheet artifact in both solvers), NOT participation\"\n\n");

    let out_root = std::env::var("TRANSMON_QUANTUM_BENCH_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../benchmarks/transmon_quantum")
        });
    fs::create_dir_all(&out_root).expect("create benchmark dir");
    let path = out_root.join("results.toml");
    fs::write(&path, &t).expect("write results.toml");

    println!("Transmon quantum benchmark written to {}", path.display());
    println!(
        "  C_Sigma (tensor, reduced) = {:.3} fF",
        c_sigma_phys * 1e15
    );
    println!(
        "  scalar-vs-tensor C_Sigma delta = {:.3}%",
        scalar_tensor_delta * 100.0
    );
    println!(
        "  E_J = {:.4} GHz,  E_C = {:.4} GHz,  E_J/E_C = {:.2}",
        e_j / 1e9,
        e_c / 1e9,
        e_j_over_e_c
    );
    println!(
        "  omega01 (Koch exact) = {:.4} GHz  (band +/-{:.2}%)",
        w01 / 1e9,
        w01_band_frac * 100.0
    );
    println!(
        "  alpha (Koch exact) = {:.4} GHz  (-E_C = {:.4} GHz)",
        alpha / 1e9,
        -e_c / 1e9
    );
    println!(
        "  anchor (#1066): harmonic {:.2} GHz, C_Sigma {:.4} fF, E_C {:.6} GHz; this mesh harmonic {:.4} GHz",
        anchor.frequency_hz / 1e9,
        anchor.c_sigma_farad * 1e15,
        anchor.e_c_hz / 1e9,
        harmonic / 1e9
    );
    println!(
        "  NOT mesh-converged (#1051): converged tensor C_Sigma < {MESH_CONV_TENSOR_UPPER_FF} fF"
    );
}
