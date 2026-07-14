//! Transmon quantum-parameter benchmark generator (Epic #476 Phase C,
//! issue #505): extract the Maxwell capacitance matrix on the real
//! DeviceLayout `SingleTransmon` mesh (split into its three node-disjoint
//! metal conductors), derive `C_Σ`, `E_C`, and the Koch-exact `ω01`/`α`,
//! and emit `benchmarks/transmon_quantum/results.toml`.
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
use geode_core::mesh::transmon::{JUNCTION_INDUCTANCE_H, sapphire_eps_lab, sapphire_eps_scalar};
use geode_core::mesh::{MetalRole, TetMesh, read_transmon_smoke_fixture};
use geode_core::quantum::transmon::{
    capacitance_from_e_c_hz, cross_kerr_from_epr, e_c_hz_from_capacitance, e_j_hz_from_inductance,
    omega01_asymptotic_hz, self_kerr_from_epr, transmon_spectrum,
};

/// Metres per mesh unit (micrometre mesh).
const M_PER_UNIT: f64 = 1e-6;

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
    let e_c = e_c_hz_from_capacitance(c_sigma_phys);
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

    // Back-solved anchors (curator-confirmed EXPECTATIONS, not tuning
    // targets): E_C ≈ 0.2156 GHz ⇒ C_Σ ≈ 89.9 fF ⇒ E_J/E_C ≈ 51.
    let c_anchor = capacitance_from_e_c_hz(0.2156e9);

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
    let _ = writeln!(t, "spectrum_converged = {}", spec.converged);
    t.push('\n');

    t.push_str("[quantum.derived_omega01_band]\n");
    t.push_str("# HONEST bar, DERIVED (not retrofitted): omega01 ~ sqrt(E_C), so\n");
    t.push_str("# delta_omega01/omega01 ~= 1/2 delta_C_Sigma/C_Sigma.\n");
    let _ = writeln!(
        t,
        "c_matrix_accuracy = {c_accuracy}  # PR #481 oracle ~1% class"
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
    t.push_str("model_difference_terms = \"floating readout feedline (folded out via Schur reduction), junction self-C 5.5 fF (not in this electrostatic C_Sigma), exterior natural-Neumann truncation\"\n\n");

    t.push_str("[quantum.anchors]\n");
    t.push_str("# Curator-confirmed EXPECTATIONS to compare against (NOT tuning targets).\n");
    t.push_str("blog_omega01_ghz = 4.14\n");
    t.push_str("expected_e_c_ghz = 0.2156\n");
    let _ = writeln!(
        t,
        "expected_c_sigma_ff = {:.2}  # back-solved from E_C=0.2156 GHz",
        c_anchor * 1e15
    );
    t.push_str("expected_e_j_over_e_c = 51.0\n");
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

    // ---- Minev p_m 3-way reconciliation over the six committed modes ----
    // The six Phase-B modes + their stiffness-participation are committed in
    // benchmarks/transmon_eigen/results.toml. In the reactive-lumped-shunt
    // formulation the Minev energy-participation p_m = junction inductive /
    // total inductive energy is ALGEBRAICALLY the same quantity as the Phase
    // B stiffness participation p = xᵀ K_port x / xᵀ (K+K_port) x (both are
    // the junction inductive-energy fraction of the same eigenvector). So
    // the Minev p_m column EQUALS the committed participation column; the
    // reconciliation is against Palace's differently-normalized field
    // port-EPR. (The release test tests/transmon_quantum.rs re-derives p_m
    // from a fresh eigensolve to confirm the identity numerically.)
    // Committed values (transmon_eigen/results.toml + Palace port-EPR.csv):
    let modes: [(&str, f64, f64, f64); 6] = [
        // (name, f_ghz, phase_b_participation == minev p_m, palace port-EPR p[1])
        ("resonator", 5.153, 0.000, -4.706079891562e-4),
        ("mode_2", 15.465, 0.000, -1.710883210358e-5),
        ("junction_lc", 17.490, 1.000, 2.494782690465e-8),
        ("mode_4", 18.693, 0.000, -2.975892992981e-9),
        ("mode_5", 20.703, 0.000, -1.172141061263e-8),
        ("mode_6", 26.088, 0.000, -6.038680846706e-6),
    ];
    t.push_str("[epr_reconciliation]\n");
    t.push_str("# 3-way reconciliation over the six committed Phase-B field modes.\n");
    t.push_str("# minev_p_m == phase_b_participation ALGEBRAICALLY (reactive-shunt\n");
    t.push_str("# formulation makes junction-inductive-energy-fraction the same\n");
    t.push_str("# quantity); Palace port-EPR is a differently-normalized, SIGNED\n");
    t.push_str("# field diagnostic that ranks modes DIFFERENTLY (junction mode has\n");
    t.push_str("# the SMALLEST |p[1]|, resonator the LARGEST) — an honest\n");
    t.push_str("# non-comparability, reconciled by the frequency-agreement mode-ID.\n");
    t.push_str("modes = [\n");
    for (name, f, p_minev, p_palace) in &modes {
        let _ = writeln!(
            t,
            "  {{ name = \"{name}\", f_ghz = {f}, minev_p_m = {p_minev:.3}, phase_b_participation = {p_minev:.3}, palace_port_epr = {p_palace:.6e} }},"
        );
    }
    t.push_str("]\n");
    t.push_str(
        "minev_equals_phase_b = true  # algebraic identity, tested in tests/transmon_quantum.rs\n",
    );
    t.push_str(
        "palace_ranks_differently = true  # signed field diagnostic, not the energy fraction\n",
    );
    t.push_str("mode_id_basis = \"frequency agreement (17.4901 GHz junction mode in both solvers), NOT participation\"\n\n");

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
    println!("  anchors: blog omega01 4.14 GHz, E_C 0.2156 GHz, C_Sigma ~89.9 fF, E_J/E_C ~51");
}
