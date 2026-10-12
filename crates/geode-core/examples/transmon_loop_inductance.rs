//! The transmon qubit mode's series loop inductance `L_s`, and the
//! loop-corrected bare-qubit `C_Σ` anchor it implies (issue #1079).
//!
//! #1078 found that the qubit mode keeps about 0.5 % of its inductive energy
//! outside the junction: a series loop inductance `L_s ≈ 0.08 nH` on the
//! committed fixture, rising under refinement. This generator settles what
//! `L_s` is, how large it is when converged, and what it does to the target.
//!
//! 1. **Port-patch test (physical or artifact).** The junction is a 1 µm
//!    wide wire across the 12 µm island-ground gap: a metal lead, the
//!    1 µm × 1 µm `lumped_element` patch, a second lead. Each variant moves
//!    the wire's triangles between the PEC and port groups (the rule of
//!    `reference/fixtures/transmon_palace/results_loop_1079/mkmesh.py`), so
//!    the geometry and the current path stay fixed and only the stretch of
//!    wire that carries the port sheet changes: patch length 1, 4.67 and
//!    12 µm, and a 1.83 µm patch moved to the ground end. geode solves each
//!    with the A2 port-voltage shunt (#1044); Palace solves the same meshes
//!    with its distributed lumped port, at first order and (three of them)
//!    at second order.
//! 2. **Topology test.** Where does `C_J` sit? In the series circuit
//!    (`C_J` across the junction, `L_s` outside it) the participation-based
//!    `L_J (1 − p)` falls with `C_J`; in the parallel reading
//!    `(L_J + L_s) ∥ (C_Σ + C_J)` it does not depend on `C_J`. geode and
//!    Palace solve `C_J` = 0, 5.5 and 20 fF.
//! 3. **Resolution.** Palace AMS on the fixture (`h`), on Palace's own
//!    uniform refinement (`h/2`, 1.38M ND unknowns) and at second order on
//!    `h`, plus #1078's gmsh-refined `h/2` cell. `h/4` (about 11M unknowns)
//!    does not fit the local Docker VM; the converged value is an
//!    extrapolation with a band (rule in `[convergence].rule`, asserted
//!    against [`DEVICELAYOUT_LOOP_INDUCTANCE_H`]). A set of local-refinement
//!    cells (spheres around the junction, boxes around the island) shows
//!    where the mesh sensitivity lives.
//! 4. **Anchor.** [`TransmonAnchor::devicelayout_bare_loop`] and its band,
//!    and the #1077 brackets re-evaluated against it.
//! 5. **#1043 offset.** How much of the 0.28 % same-mesh Palace-vs-LC qubit
//!    offset `L_s` accounts for, at `L_J` and at `2 L_J`.
//!
//! # Usage
//!
//! ```sh
//! cargo run -p geode-core --release --example transmon_loop_inductance
//! ```
//!
//! Writes `benchmarks/transmon_loop_inductance/results.toml` (override the
//! directory with `$TRANSMON_LOOP_BENCH_DIR`). Six A2 eigensolves on the
//! fixture, about 2.5 min on 28 local cores. The Palace cells are not re-run;
//! `reference/fixtures/transmon_palace/results_loop_1079/run.sh` reproduces
//! them.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use burn::tensor::Tensor;
use burn::tensor::backend::BackendTypes;
use faer::c64;

use geode_core::assembly::nedelec::{
    NedelecScatterMap, assemble_global_nedelec_with_full_tensors_sparse,
};
use geode_core::assembly::p1::upload_mesh;
use geode_core::eigen::projection::solve_transmon_eigenmodes_projected_with_port_model;
use geode_core::eigen::transmon::{
    JunctionPortModel, LumpedReactiveShunt, ReactiveElementNatural, TransmonPencil,
    lambda_shift_for_frequency_hz,
};
use geode_core::mesh::spiral::pec_interior_mask_from_triangles;
use geode_core::mesh::transmon::{JUNCTION_CAPACITANCE_F, JUNCTION_INDUCTANCE_H};
use geode_core::mesh::{TetMesh, TransmonFixture, read_transmon_smoke_fixture};
use geode_core::quantum::transmon::{
    DEVICELAYOUT_COUPLING_K, DEVICELAYOUT_COUPLING_K_BAND, DEVICELAYOUT_LOOP_INDUCTANCE_BAND_H,
    DEVICELAYOUT_LOOP_INDUCTANCE_H, DEVICELAYOUT_QUBIT_FREQUENCY_HZ,
    DEVICELAYOUT_RESONATOR_FREQUENCY_HZ, TransmonAnchor, coupled_lc_bare_pair_hz,
    coupled_lc_dressed_pair_hz, harmonic_frequency_hz, series_loop_inductance_from_participations,
};
use geode_core::testing::TestBackend;
use geode_util::fixture::BackendInfo;

type B = TestBackend;

/// Metres per mesh unit (micrometre mesh).
const M_PER_UNIT: f64 = 1e-6;
/// Shift of every geode solve (GHz) and modes requested (as #1078).
const SIGMA_GHZ: f64 = 5.0;
const N_MODES: usize = 3;

/// Port-patch variants: name and the wire's port `y` range (µm). The rule of
/// `mkmesh.py`: a wire triangle (metal or port, all vertices with
/// `|x| ≤ 0.5`, `0 ≤ y ≤ 12`, `z = 0`) is port when all its vertices lie in
/// the range (1e-3 µm tolerance), else metal.
const PATCHES: [(&str, f64, f64); 4] = [
    ("l1p00", 5.5, 6.5),
    ("l4p67", 11.0 / 3.0, 25.0 / 3.0),
    ("l12p00", 0.0, 12.0),
    ("g1p83", 0.0, 11.0 / 6.0),
];
/// Junction capacitances of the topology test (F).
const C_J_VARIANTS: [(&str, f64); 3] = [("cj0", 0.0), ("cj5p5", 5.5e-15), ("cj20", 20e-15)];

/// Palace modes with a port EPR below this are the port-sheet artifact.
const PORT_SHEET_ARTIFACT_MAX_P: f64 = 1e-6;

const PALACE_DIR: &str = "reference/fixtures/transmon_palace/results_loop_1079";
const PALACE_DIR_1078: &str = "reference/fixtures/transmon_palace/results_coupling_1078";
/// Palace port-patch cells: (variant, order-1 cell, order-2 cell).
const PALACE_PATCH: [(&str, &str, Option<&str>); 4] = [
    ("l1p00", "h_L14p86", Some("p2_L14p86")),
    ("l4p67", "l4p67_L14p86", None),
    ("l12p00", "l12p00_L14p86", Some("l12p00p2_L14p86")),
    ("g1p83", "g1p83_L14p86", Some("g1p83p2_L14p86")),
];
/// Palace topology cells: (C_J variant, cell).
const PALACE_CJ: [(&str, &str); 3] = [
    ("cj0", "cj0_L14p86"),
    ("cj5p5", "h_L14p86"),
    ("cj20", "cj20_L14p86"),
];
/// Global resolutions: (label, directory, cell, description).
const PALACE_GLOBAL: [(&str, &str, &str, &str); 4] = [
    (
        "h",
        PALACE_DIR,
        "h_L14p86",
        "the committed fixture, order 1, SuperLU",
    ),
    (
        "h2",
        PALACE_DIR,
        "u1_L14p86",
        "Palace's own uniform refinement of h (UniformLevels 1), order 1, AMS",
    ),
    (
        "h2_gmsh",
        PALACE_DIR_1078,
        "h2_L14p86",
        "#1078's gmsh uniform refinement of the fixture, order 1, AMS",
    ),
    (
        "h_p2",
        PALACE_DIR,
        "p2_L14p86",
        "the fixture at order 2, AMS",
    ),
];
/// Local-refinement cells: (cell, description). Spheres are centred on the
/// junction patch (0, 6, 0) µm; box "f" is the island finger and its gaps
/// (|x|, |z| ≤ 50, −50 ≤ y ≤ 680 µm), box "w" a wider one (|x|, |z| ≤ 150,
/// −150 ≤ y ≤ 800 µm).
const PALACE_LOCAL: [(&str, &str); 13] = [
    ("s10x1_L14p86", "sphere r = 10 um, 1 level"),
    ("s10x2_L14p86", "sphere r = 10 um, 2 levels"),
    ("s10x3_L14p86", "sphere r = 10 um, 3 levels"),
    ("s30x1_L14p86", "sphere r = 30 um, 1 level"),
    ("s30x2_L14p86", "sphere r = 30 um, 2 levels"),
    ("s30x3_L14p86", "sphere r = 30 um, 3 levels"),
    ("s100x1_L14p86", "sphere r = 100 um, 1 level"),
    ("s100x2_L14p86", "sphere r = 100 um, 2 levels"),
    ("s300x1_L14p86", "sphere r = 300 um, 1 level"),
    ("bfx1_L14p86", "finger box, 1 level"),
    ("bfx2_L14p86", "finger box, 2 levels"),
    ("bwx1_L14p86", "wide box, 1 level"),
    (
        "g100x1s30x2s10x3_L14p86",
        "graded: spheres r = 100 / 30 / 10 um at 1 / 2 / 3 levels",
    ),
];

/// #1077 (#1075) certified brackets of the two re-anchored designs (fF,
/// unrounded family bounds of
/// `benchmarks/transmon_diffopt/c_sigma_bracket_anchor_93p95.toml`), as in
/// `examples/transmon_coupling_g.rs`.
const BRACKET_1066_DESIGN_FF: (f64, f64) = (92.758828, 92.998561);
const BRACKET_CORRECTED_FF: (f64, f64) = (93.891607, 94.136104);

/// Island finger and junction wire of the fixture (µm), for the analytic
/// order-of-magnitude check: finger width, gap to ground and length; wire
/// width and length.
const FINGER_W_UM: f64 = 24.0;
const FINGER_GAP_UM: f64 = 30.0;
const FINGER_LEN_UM: f64 = 620.0;
const WIRE_W_UM: f64 = 1.0;
const WIRE_LEN_UM: f64 = 12.0;
const MU0: f64 = 1.256_637_062_12e-6;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

fn vals_to_host(t: Tensor<B, 1>) -> Vec<f64> {
    t.into_data().iter::<f64>().collect()
}

fn edge_tables(mesh: &TetMesh) -> (Vec<[u32; 6]>, Vec<[i8; 6]>) {
    let te = mesh.tet_edges();
    (
        te.iter()
            .map(|row| std::array::from_fn(|i| row[i].0))
            .collect(),
        te.iter()
            .map(|row| std::array::from_fn(|i| row[i].1))
            .collect(),
    )
}

/// The fixture with the wire's port stretch set to `[y_lo, y_hi]` (µm).
/// Returns the variant and the number of retagged triangles.
fn with_patch(fx: &TransmonFixture, y_lo: f64, y_hi: f64) -> (TransmonFixture, usize) {
    let mut out = fx.clone();
    let (metal, port) = (fx.tags.metal, fx.tags.lumped_element);
    let mut moved = 0;
    for (i, tri) in fx.boundary_triangles.iter().enumerate() {
        let t = fx.triangle_physical_tags[i];
        if t != metal && t != port {
            continue;
        }
        let v: Vec<[f64; 3]> = tri.iter().map(|&n| fx.mesh.nodes[n as usize]).collect();
        let wire = v.iter().all(|p| {
            p[0].abs() <= 0.5 + 1e-6 && p[1] >= -1e-6 && p[1] <= 12.0 + 1e-6 && p[2].abs() <= 1e-6
        });
        if !wire {
            continue;
        }
        let inside = v.iter().all(|p| p[1] >= y_lo - 1e-3 && p[1] <= y_hi + 1e-3);
        let want = if inside { port } else { metal };
        if want != t {
            moved += 1;
        }
        out.triangle_physical_tags[i] = want;
    }
    (out, moved)
}

/// One solved (or read) qubit-mode record at `L_J` = 14.86 nH.
#[derive(Clone, Debug)]
struct Cell {
    name: String,
    f_ghz: Vec<f64>,
    p: Vec<f64>,
    c_j_farad: f64,
    nd: Option<usize>,
    solve_s: Option<f64>,
    port_faces: Option<usize>,
    port_length_um: Option<f64>,
}

impl Cell {
    fn p_sum(&self) -> f64 {
        self.p.iter().take(3).map(|x| x.abs()).sum()
    }
    /// `L_J (1 − Σp)` (nH): #1078's estimator.
    fn l_parallel_nh(&self) -> f64 {
        JUNCTION_INDUCTANCE_H * 1e9 * (1.0 - self.p_sum())
    }
    /// Series `L_s` (nH) over the three lowest modes,
    /// [`series_loop_inductance_from_participations`].
    fn l_series_nh(&self) -> f64 {
        let modes: Vec<(f64, f64)> = self
            .f_ghz
            .iter()
            .zip(&self.p)
            .take(3)
            .map(|(&f, &p)| (f * 1e9, p.abs()))
            .collect();
        series_loop_inductance_from_participations(JUNCTION_INDUCTANCE_H, self.c_j_farad, &modes)
            * 1e9
    }
}

struct Geode {
    k_vals: Vec<f64>,
    m_vals: Vec<f64>,
    scatter: NedelecScatterMap,
    edges: Vec<[u32; 2]>,
}

impl Geode {
    fn new(fx: &TransmonFixture) -> Self {
        let edges = fx.mesh.edges();
        let (tet_edge_idx, tet_edge_sign) = edge_tables(&fx.mesh);
        let eps = fx.epsilon_tensor_r();
        let scatter = NedelecScatterMap::new(&tet_edge_idx);
        let (nodes_t, tets_t) = upload_mesh::<B>(&fx.mesh, &device());
        let identity: [[c64; 3]; 3] = std::array::from_fn(|i| {
            std::array::from_fn(|j| c64::new(if i == j { 1.0 } else { 0.0 }, 0.0))
        });
        let nu = vec![identity; fx.mesh.n_tets()];
        let sys = assemble_global_nedelec_with_full_tensors_sparse::<B>(
            nodes_t,
            tets_t,
            &tet_edge_sign,
            &scatter,
            &eps,
            &nu,
        );
        for (name, v) in [
            ("K", vals_to_host(sys.k_im_vals)),
            ("M", vals_to_host(sys.m_im_vals)),
        ] {
            let m = v.iter().fold(0.0_f64, |a, b| a.max(b.abs()));
            assert!(m <= 1e-9, "{name} imaginary part {m} is not negligible");
        }
        Self {
            k_vals: vals_to_host(sys.k_re_vals),
            m_vals: vals_to_host(sys.m_re_vals),
            scatter,
            edges,
        }
    }

    /// One A2 solve of a fixture variant at `L_J` = 14.86 nH and `C_J`.
    fn solve(&self, name: &str, v: &TransmonFixture, c_j: f64) -> Cell {
        let t = Instant::now();
        let metal = v.metal_triangles();
        let exterior = v.exterior_boundary_triangles();
        let mask =
            pec_interior_mask_from_triangles(&self.edges, &[metal.as_slice(), exterior.as_slice()]);
        let jport = v.lumped_element_port();
        let element = ReactiveElementNatural::from_si(JUNCTION_INDUCTANCE_H, c_j, M_PER_UNIT);
        let pencil = TransmonPencil {
            scatter: &self.scatter,
            k_vals: &self.k_vals,
            m_vals: &self.m_vals,
            edges: &self.edges,
            mesh: &v.mesh,
            shunt: LumpedReactiveShunt {
                faces: &jport.faces,
                length: jport.length,
                width: jport.width,
                element,
            },
            interior_mask: &mask,
        };
        let sigma = lambda_shift_for_frequency_hz(SIGMA_GHZ * 1e9, M_PER_UNIT);
        let (mut modes, diag) = solve_transmon_eigenmodes_projected_with_port_model(
            &pencil,
            JunctionPortModel::PortVoltage { e_hat: jport.e_hat },
            sigma,
            N_MODES,
            M_PER_UNIT,
        )
        .expect("A2 bulk-projected eigensolve");
        assert_eq!(modes.len(), N_MODES);
        assert!(
            diag.mode_residual_rels.iter().all(|&r| r <= 1e-6),
            "{name}: residuals {:?}",
            diag.mode_residual_rels
        );
        modes.sort_by(|a, b| a.frequency_hz.total_cmp(&b.frequency_hz));
        let cell = Cell {
            name: name.to_string(),
            f_ghz: modes.iter().map(|m| m.frequency_ghz()).collect(),
            p: modes.iter().map(|m| m.participation).collect(),
            c_j_farad: c_j,
            nd: None,
            solve_s: Some(t.elapsed().as_secs_f64()),
            port_faces: Some(jport.faces.len()),
            port_length_um: Some(jport.length),
        };
        println!(
            "  geode {name}: f = {:.6} GHz, sum p = {:.6}, L_J(1-p) = {:.5} nH, L_s = {:.5} nH ({:.1}s)",
            cell.f_ghz[0],
            cell.p_sum(),
            cell.l_parallel_nh(),
            cell.l_series_nh(),
            cell.solve_s.unwrap()
        );
        cell
    }
}

// ---------------------------------------------------------------------------
// Palace.
// ---------------------------------------------------------------------------

fn csv_col1(path: &Path) -> Vec<f64> {
    let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    text.lines()
        .skip(1)
        .filter_map(|l| l.split(',').nth(1))
        .map(|s| s.trim().parse::<f64>().expect("column 1"))
        .collect()
}

/// The number of ND unknowns from a Palace log line
/// `" H1 (p = 1): 29705, ND (p = 1): 183891, RT (p = 1): 292418"`.
fn palace_nd(log: &Path) -> usize {
    let text = fs::read_to_string(log).unwrap_or_else(|e| panic!("{}: {e}", log.display()));
    let line = text
        .lines()
        .find(|l| l.contains("ND (p = "))
        .unwrap_or_else(|| panic!("{}: no ND line", log.display()));
    let after = &line[line.find("ND (p = ").unwrap()..];
    let after = &after[after.find("): ").unwrap() + 3..];
    after
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>()
        .parse()
        .expect("ND count")
}

/// The LumpedPort C (F) of a cell's committed config.
fn palace_c_j(cfg: &Path) -> f64 {
    let text = fs::read_to_string(cfg).unwrap_or_else(|e| panic!("{}: {e}", cfg.display()));
    let i = text.find("\"C\":").expect("LumpedPort C");
    text[i + 4..]
        .trim_start()
        .split([',', '\n'])
        .next()
        .unwrap()
        .trim()
        .parse()
        .expect("C value")
}

fn palace_cell(repo: &Path, dir: &str, name: &str) -> Cell {
    let d = repo.join(dir).join(name);
    let f_all = csv_col1(&d.join("eig.csv"));
    let p_all = csv_col1(&d.join("port-EPR.csv"));
    assert_eq!(f_all.len(), p_all.len(), "{name}");
    // Drop the port-sheet artifact (#1045): a mode with |p| < 1e-6 (2.5e-8
    // at 17.49 GHz on the fixture; it moves to 9.2 GHz at C_J = 20 fF).
    let (f, p): (Vec<f64>, Vec<f64>) = f_all
        .iter()
        .zip(&p_all)
        .filter(|(_, p)| p.abs() >= PORT_SHEET_ARTIFACT_MAX_P)
        .map(|(&f, &p)| (f, p))
        .unzip();
    assert!(f.len() >= 3, "{name}: {f_all:?} {p_all:?}");
    assert!(
        f[0] < 4.5 && f[1] > 5.0,
        "{name}: qubit-like mode first, resonator second: {f:?}"
    );
    Cell {
        name: name.to_string(),
        f_ghz: f,
        p: p.iter().map(|x| x.abs()).collect(),
        c_j_farad: palace_c_j(&d.join("palace_config.json")),
        nd: Some(palace_nd(&d.join("palace_run.log"))),
        solve_s: None,
        port_faces: None,
        port_length_um: None,
    }
}

// ---------------------------------------------------------------------------
// Helpers.
// ---------------------------------------------------------------------------

fn sig3(x: f64, mode: i32) -> f64 {
    let s = 10f64.powi(3 - 1 - x.log10().floor() as i32);
    match mode {
        -1 => (x * s).floor() / s,
        1 => (x * s).ceil() / s,
        _ => (x * s).round() / s,
    }
}

fn floor_dp(x: f64, dp: i32) -> f64 {
    let s = 10f64.powi(dp);
    (x * s).floor() / s
}

fn ceil_dp(x: f64, dp: i32) -> f64 {
    let s = 10f64.powi(dp);
    (x * s).ceil() / s
}

fn arr(v: &[f64], prec: usize) -> String {
    format!(
        "[{}]",
        v.iter()
            .map(|x| format!("{x:.prec$}"))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn arr_e(v: &[f64]) -> String {
    format!(
        "[{}]",
        v.iter()
            .map(|x| format!("{x:.4e}"))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn strs(v: &[&str]) -> String {
    format!(
        "[{}]",
        v.iter()
            .map(|s| format!("\"{s}\""))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn git_head() -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "--short=8", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".into())
}

fn toml_doc(path: &Path) -> toml::Value {
    let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    toml::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn tget<'a>(v: &'a toml::Value, path: &str) -> &'a toml::Value {
    let mut cur = v;
    for k in path.split('.') {
        cur = cur
            .get(k)
            .unwrap_or_else(|| panic!("missing key {path} (at {k})"));
    }
    cur
}

fn tf(v: &toml::Value, path: &str) -> f64 {
    let x = tget(v, path);
    x.as_float()
        .or_else(|| x.as_integer().map(|i| i as f64))
        .unwrap_or_else(|| panic!("{path} is not a number"))
}

fn tarr(v: &toml::Value, path: &str) -> Vec<f64> {
    tget(v, path)
        .as_array()
        .unwrap_or_else(|| panic!("{path} is not an array"))
        .iter()
        .map(|x| {
            x.as_float()
                .or_else(|| x.as_integer().map(|i| i as f64))
                .unwrap()
        })
        .collect()
}

/// Complete elliptic integral of the first kind `K(k)` by the AGM.
fn ellip_k(k: f64) -> f64 {
    let (mut a, mut b) = (1.0_f64, (1.0 - k * k).sqrt());
    for _ in 0..40 {
        let (an, bn) = (0.5 * (a + b), (a * b).sqrt());
        a = an;
        b = bn;
    }
    std::f64::consts::PI / (2.0 * a)
}

/// The bare qubit frequency (Hz) of DeviceLayout's dressed pair in the
/// multi-node Maxwell model with the λ/4 fundamental and odd harmonics
/// (`ratio · f_r`, coupling `k_ratio · k`), as
/// `examples/transmon_coupling_g.rs` `bare_multimode_ghz`; solved here by
/// Newton on the two-node inversion with a characteristic-polynomial
/// residual.
fn bare_multimode_hz(k: f64, harmonics: &[(f64, f64)]) -> f64 {
    let (d1, d2) = (
        DEVICELAYOUT_QUBIT_FREQUENCY_HZ / 1e9,
        DEVICELAYOUT_RESONATOR_FREQUENCY_HZ / 1e9,
    );
    let (q0, r0) = coupled_lc_bare_pair_hz(d1 * 1e9, d2 * 1e9, k);
    // det(F - f^2 C) with node-grounded F = diag(f_i^2) in normalized
    // capacitance form C = I - offdiag(k_ij): the Maxwell normal modes solve
    // det(diag(f_i^2) - f^2 (I - K)) = 0 for the star coupling to node 0.
    let det = |fq: f64, fr: f64, f: f64| -> f64 {
        let mut nodes = vec![(fq * fq, 0.0), (fr * fr, k)];
        nodes.extend(harmonics.iter().map(|&(r, kr)| ((r * fr).powi(2), kr * k)));
        let f2 = f * f;
        // Star topology: det = (a0) prod_j a_j - f2^2 sum_j k_j^2 prod_{i != j} a_i,
        // a0 = fq^2 - f2, a_j = f_j^2 - f2.
        let a0 = nodes[0].0 - f2;
        let a: Vec<f64> = nodes[1..].iter().map(|n| n.0 - f2).collect();
        let prod: f64 = a.iter().product();
        let mut s = 0.0;
        for j in 0..a.len() {
            let others: f64 = a
                .iter()
                .enumerate()
                .filter(|&(i, _)| i != j)
                .map(|(_, x)| x)
                .product();
            s += nodes[1 + j].1.powi(2) * others;
        }
        a0 * prod - f2 * f2 * s
    };
    let mut x = [q0 / 1e9, r0 / 1e9];
    for _ in 0..60 {
        let r = [det(x[0], x[1], d1), det(x[0], x[1], d2)];
        let h = 1e-7;
        let j: Vec<[f64; 2]> = (0..2)
            .map(|a| {
                let mut row = [0.0; 2];
                for (b, rb) in row.iter_mut().enumerate() {
                    let mut xp = x;
                    xp[b] += h;
                    let rp = [det(xp[0], xp[1], d1), det(xp[0], xp[1], d2)];
                    *rb = (rp[a] - r[a]) / h;
                }
                row
            })
            .collect();
        let dt = j[0][0] * j[1][1] - j[0][1] * j[1][0];
        let step = [
            (-r[0] * j[1][1] + r[1] * j[0][1]) / dt,
            (-r[1] * j[0][0] + r[0] * j[1][0]) / dt,
        ];
        x = [x[0] + step[0], x[1] + step[1]];
        if step[0].abs().max(step[1].abs()) < 1e-13 {
            break;
        }
    }
    x[0] * 1e9
}

fn c_sigma_ff(f_hz: f64, l_s_h: f64) -> f64 {
    TransmonAnchor::from_harmonic_with_loop_inductance(
        f_hz,
        JUNCTION_INDUCTANCE_H,
        l_s_h,
        JUNCTION_CAPACITANCE_F,
    )
    .c_sigma_farad
        * 1e15
}

fn main() {
    let t_all = Instant::now();
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let out_dir = std::env::var("TRANSMON_LOOP_BENCH_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| repo.join("benchmarks/transmon_loop_inductance"));
    let fx = read_transmon_smoke_fixture().expect("transmon fixture");
    let backend = BackendInfo::of::<B>(&device());
    let l_j_nh = JUNCTION_INDUCTANCE_H * 1e9;

    // ---- 1-2. geode solves. ----
    println!("geode A2 solves at L_J = {l_j_nh:.3} nH:");
    let t_geode = Instant::now();
    let g = Geode::new(&fx);
    let mut geode_patch = Vec::new();
    let mut moved = Vec::new();
    for (name, lo, hi) in PATCHES {
        let (v, n) = with_patch(&fx, lo, hi);
        moved.push(n);
        geode_patch.push(g.solve(name, &v, JUNCTION_CAPACITANCE_F));
    }
    assert_eq!(moved[0], 0, "the l1p00 variant is the committed fixture");
    let geode_cj: Vec<Cell> = C_J_VARIANTS
        .iter()
        .map(|&(name, cj)| {
            if cj == JUNCTION_CAPACITANCE_F {
                let mut c = geode_patch[0].clone();
                c.name = name.to_string();
                c
            } else {
                g.solve(name, &fx, cj)
            }
        })
        .collect();
    let geode_s = t_geode.elapsed().as_secs_f64();

    // ---- Palace cells. ----
    let pal = |dir: &str, name: &str| palace_cell(&repo, dir, name);
    let palace_patch: Vec<(Cell, Option<Cell>)> = PALACE_PATCH
        .iter()
        .map(|&(_, c1, c2)| (pal(PALACE_DIR, c1), c2.map(|c| pal(PALACE_DIR, c))))
        .collect();
    let palace_cj: Vec<Cell> = PALACE_CJ.iter().map(|&(_, c)| pal(PALACE_DIR, c)).collect();
    let global: Vec<Cell> = PALACE_GLOBAL
        .iter()
        .map(|&(_, d, c, _)| pal(d, c))
        .collect();
    let local: Vec<Cell> = PALACE_LOCAL
        .iter()
        .map(|&(c, _)| pal(PALACE_DIR, c))
        .collect();
    for (i, &(name, cj)) in C_J_VARIANTS.iter().enumerate() {
        assert!(
            (palace_cj[i].c_j_farad - cj).abs() < 1e-20,
            "{name}: Palace C {}",
            palace_cj[i].c_j_farad
        );
    }

    // ---- Patch test. ----
    let spread = |v: &[f64]| {
        let lo = v.iter().copied().fold(f64::INFINITY, f64::min);
        let hi = v.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        (hi - lo) / v[0]
    };
    let geode_patch_ls: Vec<f64> = geode_patch.iter().map(Cell::l_series_nh).collect();
    let palace_patch_ls: Vec<f64> = palace_patch.iter().map(|(c, _)| c.l_series_nh()).collect();
    let palace_patch_p2: Vec<(&str, f64)> = PALACE_PATCH
        .iter()
        .zip(&palace_patch)
        .filter_map(|(&(v, _, _), (_, c2))| c2.as_ref().map(|c| (v, c.l_series_nh())))
        .collect();
    let p2_ls: Vec<f64> = palace_patch_p2.iter().map(|x| x.1).collect();
    let geode_vs_palace_patch: Vec<f64> = geode_patch_ls
        .iter()
        .zip(&palace_patch_ls)
        .map(|(a, b)| a / b - 1.0)
        .collect();
    let patch_lengths: Vec<f64> = geode_patch
        .iter()
        .map(|c| c.port_length_um.unwrap())
        .collect();
    let length_ratio = patch_lengths.iter().copied().fold(0.0_f64, f64::max)
        / patch_lengths.iter().copied().fold(f64::INFINITY, f64::min);
    let (s_geode, s_palace, s_p2) = (
        spread(&geode_patch_ls),
        spread(&palace_patch_ls),
        spread(&p2_ls),
    );
    let patch_physical = s_geode < 0.02 && s_palace < 0.02 && s_p2 < 0.02;
    println!(
        "patch: L_s spread geode {s_geode:.4}, Palace {s_palace:.4}, Palace p2 {s_p2:.4} over a {length_ratio:.0}x patch length range"
    );

    // ---- Topology test. ----
    // Under the series circuit the series L_s is the same for every C_J and
    // L_J (1 - p) is not; under the parallel reading it is the other way
    // round.
    let g_par: Vec<f64> = geode_cj.iter().map(Cell::l_parallel_nh).collect();
    let g_ser: Vec<f64> = geode_cj.iter().map(Cell::l_series_nh).collect();
    let p_par: Vec<f64> = palace_cj.iter().map(Cell::l_parallel_nh).collect();
    let p_ser: Vec<f64> = palace_cj.iter().map(Cell::l_series_nh).collect();
    let rel_spread = |v: &[f64]| {
        let lo = v.iter().copied().fold(f64::INFINITY, f64::min);
        let hi = v.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        (hi - lo) / v[1]
    };
    let topo_series_spread = rel_spread(&g_ser).max(rel_spread(&p_ser));
    let topo_parallel_spread = rel_spread(&g_par).min(rel_spread(&p_par));
    println!(
        "topology: series L_s spread {topo_series_spread:.2e}, parallel L_J(1-p) spread {topo_parallel_spread:.3}"
    );
    let topology = if topo_series_spread < 2e-3 && topo_parallel_spread > 0.1 {
        "series"
    } else {
        "undetermined"
    };

    // ---- Resolution and the converged estimate. ----
    let gls: Vec<f64> = global.iter().map(Cell::l_series_nh).collect();
    let (l_h, l_h2, l_h2g, l_p2) = (gls[0], gls[1], gls[2], gls[3]);
    assert!(
        l_h < l_h2 && l_h2 < l_p2 && l_h < l_h2g,
        "L_s does not rise monotonically under refinement: {gls:?}"
    );
    let conv = toml_doc(&repo.join("benchmarks/transmon_diffopt/c_sigma_convergence.toml"));
    let cs = tarr(&conv, "tensor_eps.c_sigma_ff");
    let static_order = ((cs[0] - cs[1]) / (cs[1] - cs[2])).log2();
    let rich = |a: f64, b: f64, q: f64| b + (b - a) / (2f64.powf(q) - 1.0);
    let est_order1 = rich(l_h, l_h2, 1.0);
    let est_static = rich(l_h, l_h2, static_order);
    let est_order2 = rich(l_h, l_h2, 2.0);
    let alt_gmsh = [
        rich(l_h, l_h2g, 1.0),
        rich(l_h, l_h2g, static_order),
        rich(l_h, l_h2g, 2.0),
    ];
    let observed_h_increment = l_h2 / l_h - 1.0;
    let finest = gls.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let central_rule = sig3(0.5 * (est_order1 + est_static), 0);
    let band_rule = (sig3(finest, -1), sig3(est_order1, 1));
    let (ls_lo, ls_hi) = DEVICELAYOUT_LOOP_INDUCTANCE_BAND_H;
    assert!(
        (central_rule * 1e-9 - DEVICELAYOUT_LOOP_INDUCTANCE_H).abs() < 1e-15
            && (band_rule.0 * 1e-9 - ls_lo).abs() < 1e-15
            && (band_rule.1 * 1e-9 - ls_hi).abs() < 1e-15,
        "quantum::transmon loop constants ({DEVICELAYOUT_LOOP_INDUCTANCE_H}, {ls_lo}, {ls_hi}) do \
         not follow the estimates: want {central_rule} nH, [{}, {}] nH",
        band_rule.0,
        band_rule.1
    );
    println!(
        "resolution: L_s h {l_h:.5}, h/2 {l_h2:.5} (gmsh {l_h2g:.5}), p2 {l_p2:.5}; estimates {est_order1:.5} \
         (order 1), {est_static:.5} (order {static_order:.3}), {est_order2:.5} (order 2); \
         L_s = {central_rule} [{}, {}] nH",
        band_rule.0, band_rule.1
    );

    // Analytic order of magnitude: the finger as a CPW of width w and gap s,
    // fed at one end and charged uniformly along its length (L' l / 3), plus
    // the wire as an isolated thin strip.
    let kk = FINGER_W_UM / (FINGER_W_UM + 2.0 * FINGER_GAP_UM);
    let l_per_m = MU0 / 4.0 * ellip_k((1.0 - kk * kk).sqrt()) / ellip_k(kk);
    let l_finger_nh = l_per_m * FINGER_LEN_UM * 1e-6 / 3.0 * 1e9;
    let l_wire_nh = MU0 / (2.0 * std::f64::consts::PI)
        * WIRE_LEN_UM
        * 1e-6
        * ((2.0 * WIRE_LEN_UM / WIRE_W_UM).ln() + 0.5)
        * 1e9;

    // ---- Anchor. ----
    let bare = TransmonAnchor::devicelayout_bare();
    let dressed = TransmonAnchor::devicelayout();
    let anchor = TransmonAnchor::devicelayout_bare_loop();
    assert_eq!(anchor.loop_inductance_henry, DEVICELAYOUT_LOOP_INDUCTANCE_H);
    assert!((anchor.frequency_hz - bare.frequency_hz).abs() < 1e-6);
    let round = anchor.harmonic_frequency_of_c_sigma_hz(anchor.c_sigma_farad);
    assert!((round - anchor.frequency_hz).abs() / anchor.frequency_hz < 1e-12);
    let c_anchor = anchor.c_sigma_farad * 1e15;
    let c_bare = bare.c_sigma_farad * 1e15;
    let c_dressed = dressed.c_sigma_farad * 1e15;
    let (k_lo, k_hi) = DEVICELAYOUT_COUPLING_K_BAND;
    // The 3λ/4 node and the odd harmonics, from #1078's three-mode fit.
    let coup = toml_doc(&repo.join("benchmarks/transmon_coupling/results.toml"));
    let k3_over_k = tf(&coup, "anchor.three_mode_k3_over_k");
    let r3 = tf(&coup, "anchor.three_mode_f3_over_fr");
    let odd: Vec<(f64, f64)> = (1..=7)
        .map(|j| ((2 * j + 1) as f64 * r3 / 3.0, k3_over_k))
        .collect();
    let fq_odd_hi = bare_multimode_hz(k_hi, &odd);
    // Cross-check of the multimode solve against #1078's number.
    let c_odd_hi_l0 = c_sigma_ff(fq_odd_hi, 0.0);
    let c_odd_hi_1078 = tf(&coup, "anchor.odd_harmonics_bare_c_sigma_at_k_high_ff");
    assert!(
        (c_odd_hi_l0 - c_odd_hi_1078).abs() < 1e-5,
        "multimode bare {c_odd_hi_l0} vs #1078 {c_odd_hi_1078}"
    );
    let (fq_k_lo, _) = coupled_lc_bare_pair_hz(
        DEVICELAYOUT_QUBIT_FREQUENCY_HZ,
        DEVICELAYOUT_RESONATOR_FREQUENCY_HZ,
        k_lo,
    );
    let target_band = (c_sigma_ff(fq_odd_hi, ls_hi), c_sigma_ff(fq_k_lo, ls_lo));
    let c_band_ls_only = (
        c_sigma_ff(bare.frequency_hz, ls_hi),
        c_sigma_ff(bare.frequency_hz, ls_lo),
    );
    let c_same_mesh = c_sigma_ff(bare.frequency_hz, l_h * 1e-9);
    let c_h2 = c_sigma_ff(bare.frequency_hz, l_h2 * 1e-9);
    let c_p2 = c_sigma_ff(bare.frequency_hz, l_p2 * 1e-9);
    // The #1078 parallel-reading recipe, for comparison: C_total L/(L_J + L).
    let parallel_recipe_shift = bare.c_total_farad * 1e15 * DEVICELAYOUT_LOOP_INDUCTANCE_H
        / (JUNCTION_INDUCTANCE_H + DEVICELAYOUT_LOOP_INDUCTANCE_H);
    assert!(target_band.0 < c_anchor && c_anchor < target_band.1);
    assert!(target_band.1 < c_bare);
    let verdict = |b: (f64, f64)| -> &'static str {
        if b.0 > target_band.1 {
            "above"
        } else if b.1 < target_band.0 {
            "below"
        } else if b.0 <= target_band.0 && b.1 >= target_band.1 {
            "contains"
        } else {
            "overlaps"
        }
    };
    println!(
        "anchor: C_Sigma {c_anchor:.6} fF (bare {c_bare:.6}, dressed {c_dressed:.6}), band [{:.4}, {:.4}]",
        target_band.0, target_band.1
    );

    // ---- #1043 offset accounting (same mesh, L_J and 2 L_J). ----
    let c_h_static = tarr(&coup, "static_proxy.c_sigma_ff")[0] * 1e-15;
    let k_h = tf(&coup, "fit.maxwell2_all.k");
    let f_r_h = tf(&coup, "fit.maxwell2_all.f_r_ghz") * 1e9;
    let eig = toml_doc(&repo.join("benchmarks/transmon_eigen/results.toml"));
    let off_pct_1043 = tf(&eig, "oracles.palace_target2.qubit_palace_vs_lumped_pct");
    let off2_pct_1043 = tf(&eig, "oracles.palace_target2.l2_qubit_palace_vs_lumped_pct");
    let palace_l2_ghz = tf(&eig, "oracles.palace_target2.l2_qubit_f_ghz");
    let ls_h = l_h * 1e-9;
    let account = |l_j: f64, f_palace_hz: f64| {
        let f_lc = harmonic_frequency_hz(l_j, c_h_static + JUNCTION_CAPACITANCE_F);
        let a = TransmonAnchor::from_harmonic_with_loop_inductance(
            f_lc,
            l_j,
            ls_h,
            JUNCTION_CAPACITANCE_F,
        );
        let f_s = a.harmonic_frequency_of_c_sigma_hz(c_h_static);
        let (f_d, _) = coupled_lc_dressed_pair_hz(f_s, f_r_h, k_h);
        (
            f_lc,
            f_s,
            f_d,
            f_palace_hz / f_lc - 1.0,
            f_s / f_lc - 1.0,
            f_d / f_s - 1.0,
            f_palace_hz / f_d - 1.0,
        )
    };
    let acc1 = account(JUNCTION_INDUCTANCE_H, global[0].f_ghz[0] * 1e9);
    let acc2 = account(2.0 * JUNCTION_INDUCTANCE_H, palace_l2_ghz * 1e9);
    assert!(
        (acc1.3 * 100.0 - off_pct_1043).abs() < 5e-3
            && (acc2.3 * 100.0 - off2_pct_1043).abs() < 5e-3,
        "recomputed offsets {} / {} % vs #1043's {off_pct_1043} / {off2_pct_1043} %",
        acc1.3 * 100.0,
        acc2.3 * 100.0
    );
    let ls_share = acc1.4 / acc1.3;
    println!(
        "#1043: offset {:.4} %, L_s {:.4} %, hybridization {:.4} %, residual {:.4} %; at 2 L_J {:.4} / {:.4} / {:.4} / {:.4} %",
        acc1.3 * 100.0,
        acc1.4 * 100.0,
        acc1.5 * 100.0,
        acc1.6 * 100.0,
        acc2.3 * 100.0,
        acc2.4 * 100.0,
        acc2.5 * 100.0,
        acc2.6 * 100.0
    );

    // ---- Emit. ----
    let mut t = String::with_capacity(32 * 1024);
    t.push_str("# Auto-generated by `cargo run -p geode-core --release --example transmon_loop_inductance`.\n");
    t.push_str("# Do NOT edit by hand; regenerate after any intentional change.\n");
    t.push_str("# Issue #1079. Checked by tests/transmon_eigenmode.rs (loop_* tests).\n\n");
    t.push_str("[meta]\n");
    t.push_str("description = \"The transmon qubit mode's series loop inductance L_s on the committed 133k-tet DeviceLayout SingleTransmon fixture at L_J = 14.86 nH: a port-patch test (the junction wire's port stretch moved between PEC and port, geode A2 and Palace), a C_J topology test, a Palace resolution study (h, h/2, order 2, local refinement), the converged estimate with a band, the loop-corrected bare-qubit C_Sigma anchor, the #1077 bracket verdicts against it, and the share of #1043's same-mesh Palace-vs-LC offset that L_s accounts for.\"\n");
    t.push_str("issue = 1079\n");
    t.push_str("fixture = \"crates/geode-core/tests/fixtures/transmon_smoke.msh\"\n");
    let _ = writeln!(t, "n_tets = {}", fx.mesh.n_tets());
    let _ = writeln!(t, "generated_at_commit = \"{}\"", git_head());
    backend.push_meta(&mut t);
    t.push_str("geode_source = \"solved\"\n");
    let _ = writeln!(t, "geode_wall_s = {geode_s:.1}");
    let _ = writeln!(t, "wall_clock_s = {:.1}", t_all.elapsed().as_secs_f64());
    t.push_str("host = \"local CPU (Apple M3 Ultra, 28 cores, 96 GiB); local docker for Palace (VM 24 CPUs / 11.66 GiB); no cloud\"\n");
    let _ = writeln!(t, "l_j_nh = {l_j_nh:.3}");
    let _ = writeln!(t, "c_j_ff = {:.1}", JUNCTION_CAPACITANCE_F * 1e15);
    let _ = writeln!(t, "palace_results_dir = \"{PALACE_DIR}\"");
    t.push('\n');

    t.push_str("[model]\n");
    t.push_str("circuit = \"island capacitance C_Sigma to ground, in series with the loop inductance L_s and the junction L_J || C_J: (2 pi f)^2 C_Sigma (L_s + L_J / (1 - (2 pi f)^2 L_J C_J)) = 1 (TransmonAnchor::from_harmonic_with_loop_inductance)\"\n");
    t.push_str("estimator = \"L_s = L_J (1 - sum p_m) / sum p_m (1 - x_m)^2, x_m = (2 pi f_m)^2 L_J C_J, over the three lowest modes (qubit, lambda/4, 3 lambda/4) (quantum::transmon::series_loop_inductance_from_participations); assumes the qubit branch's inductive energy is shared among those modes only. l_parallel_nh = L_J (1 - sum p_m) is #1078's [loop_inductance] estimator\"\n");
    t.push_str(
        "participation = \"geode: junction share of x^T K_tot x (A2); Palace: |port-EPR p[1]|\"\n",
    );
    t.push('\n');

    t.push_str("[patch]\n");
    t.push_str("method = \"the junction is a 1 um wide wire across the 12 um island-ground gap (lead, 1 um x 1 um lumped_element patch, lead). Each variant moves the wire's triangles between PEC and port (mkmesh.py rule); geometry, mesh and current path are unchanged, only the stretch of wire carrying the port sheet changes. A port-model artifact would follow the patch; a geometric inductance does not\"\n");
    let _ = writeln!(t, "variants = {}", strs(&PATCHES.map(|p| p.0)));
    let _ = writeln!(
        t,
        "port_y_um = [{}]",
        PATCHES
            .iter()
            .map(|p| format!("[{:.4}, {:.4}]", p.1, p.2))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let _ = writeln!(t, "port_length_um = {}", arr(&patch_lengths, 4));
    let _ = writeln!(
        t,
        "port_faces = [{}]",
        geode_patch
            .iter()
            .map(|c| c.port_faces.unwrap().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    );
    let _ = writeln!(
        t,
        "retagged_triangles = [{}]",
        moved
            .iter()
            .map(|n| n.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    );
    let _ = writeln!(t, "length_ratio = {length_ratio:.1}");
    let _ = writeln!(
        t,
        "geode_f_q_ghz = {}",
        arr(
            &geode_patch.iter().map(|c| c.f_ghz[0]).collect::<Vec<_>>(),
            6
        )
    );
    let _ = writeln!(
        t,
        "geode_p_sum = {}",
        arr(&geode_patch.iter().map(Cell::p_sum).collect::<Vec<_>>(), 6)
    );
    let _ = writeln!(
        t,
        "geode_l_parallel_nh = {}",
        arr(
            &geode_patch
                .iter()
                .map(Cell::l_parallel_nh)
                .collect::<Vec<_>>(),
            5
        )
    );
    let _ = writeln!(t, "geode_l_s_nh = {}", arr(&geode_patch_ls, 5));
    let _ = writeln!(
        t,
        "geode_solve_s = {}",
        arr(
            &geode_patch
                .iter()
                .map(|c| c.solve_s.unwrap())
                .collect::<Vec<_>>(),
            1
        )
    );
    let _ = writeln!(t, "palace_cells = {}", strs(&PALACE_PATCH.map(|p| p.1)));
    let _ = writeln!(
        t,
        "palace_f_q_ghz = {}",
        arr(
            &palace_patch
                .iter()
                .map(|(c, _)| c.f_ghz[0])
                .collect::<Vec<_>>(),
            6
        )
    );
    let _ = writeln!(t, "palace_l_s_nh = {}", arr(&palace_patch_ls, 5));
    let _ = writeln!(
        t,
        "geode_over_palace_minus_1 = {}",
        arr_e(&geode_vs_palace_patch)
    );
    let _ = writeln!(
        t,
        "palace_p2_variants = {}",
        strs(&palace_patch_p2.iter().map(|x| x.0).collect::<Vec<_>>())
    );
    let _ = writeln!(
        t,
        "palace_p2_cells = {}",
        strs(&PALACE_PATCH.iter().filter_map(|p| p.2).collect::<Vec<_>>())
    );
    let _ = writeln!(t, "palace_p2_l_s_nh = {}", arr(&p2_ls, 5));
    let _ = writeln!(
        t,
        "spread_geode = {s_geode:.5}  # (max - min) / committed patch"
    );
    let _ = writeln!(t, "spread_palace = {s_palace:.5}");
    let _ = writeln!(t, "spread_palace_p2 = {s_p2:.5}");
    let _ = writeln!(
        t,
        "verdict = \"{}\"",
        if patch_physical {
            "physical"
        } else {
            "port-dependent"
        }
    );
    let _ = writeln!(
        t,
        "reading = \"{}\"",
        if patch_physical {
            "L_s stays within 2 % while the port patch changes length 12x and moves to the ground end, at order 1 (geode and Palace) and order 2 (Palace): it is not a port-model artifact. It is the geometric inductance of the qubit's current path, the 1 um wire and the 620 um island finger with its ground return (see [convergence].local_reading and [analytic]). The remaining 1 % drift with the patch bounds the port model's share of L_s"
        } else {
            "L_s follows the port patch by more than 2 %: part of it is a port-model artifact"
        }
    );
    t.push('\n');

    t.push_str("[topology]\n");
    t.push_str("method = \"C_J at 0, 5.5 and 20 fF on the committed patch. In the series circuit (C_J across the junction, L_s outside it) the series L_s is the same for every C_J while L_J (1 - p) falls with C_J; in the parallel reading (L_J + L_s) || (C_Sigma + C_J) L_J (1 - p) is the same for every C_J\"\n");
    let _ = writeln!(t, "c_j_ff = {}", arr(&C_J_VARIANTS.map(|c| c.1 * 1e15), 1));
    let _ = writeln!(t, "palace_cells = {}", strs(&PALACE_CJ.map(|c| c.1)));
    let _ = writeln!(
        t,
        "geode_f_q_ghz = {}",
        arr(&geode_cj.iter().map(|c| c.f_ghz[0]).collect::<Vec<_>>(), 6)
    );
    let _ = writeln!(t, "geode_l_parallel_nh = {}", arr(&g_par, 5));
    let _ = writeln!(t, "geode_l_s_nh = {}", arr(&g_ser, 5));
    let _ = writeln!(
        t,
        "palace_f_q_ghz = {}",
        arr(&palace_cj.iter().map(|c| c.f_ghz[0]).collect::<Vec<_>>(), 6)
    );
    let _ = writeln!(t, "palace_l_parallel_nh = {}", arr(&p_par, 5));
    let _ = writeln!(t, "palace_l_s_nh = {}", arr(&p_ser, 5));
    let _ = writeln!(
        t,
        "series_spread = {topo_series_spread:.3e}  # (max - min) / 5.5 fF value, worse of geode and Palace"
    );
    let _ = writeln!(
        t,
        "parallel_spread = {topo_parallel_spread:.4}  # the same for L_J (1 - p), smaller of the two"
    );
    let _ = writeln!(
        t,
        "verdict = \"{topology}\"  # C_J sits across the junction, L_s outside it"
    );
    t.push('\n');

    t.push_str("[convergence]\n");
    let _ = writeln!(t, "global_labels = {}", strs(&PALACE_GLOBAL.map(|g| g.0)));
    let _ = writeln!(t, "global_cells = {}", strs(&PALACE_GLOBAL.map(|g| g.2)));
    let _ = writeln!(t, "global_dirs = {}", strs(&PALACE_GLOBAL.map(|g| g.1)));
    let _ = writeln!(
        t,
        "global_description = {}",
        strs(&PALACE_GLOBAL.map(|g| g.3))
    );
    let _ = writeln!(
        t,
        "global_nd = [{}]",
        global
            .iter()
            .map(|c| c.nd.unwrap().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    );
    let _ = writeln!(
        t,
        "global_f_q_ghz = {}",
        arr(&global.iter().map(|c| c.f_ghz[0]).collect::<Vec<_>>(), 6)
    );
    let _ = writeln!(
        t,
        "global_f_r_ghz = {}",
        arr(&global.iter().map(|c| c.f_ghz[1]).collect::<Vec<_>>(), 6)
    );
    let _ = writeln!(
        t,
        "global_p_sum = {}",
        arr(&global.iter().map(Cell::p_sum).collect::<Vec<_>>(), 6)
    );
    let _ = writeln!(
        t,
        "global_l_parallel_nh = {}",
        arr(
            &global.iter().map(Cell::l_parallel_nh).collect::<Vec<_>>(),
            5
        )
    );
    let _ = writeln!(t, "global_l_s_nh = {}", arr(&gls, 5));
    let _ = writeln!(
        t,
        "observed_h_increment = {observed_h_increment:.4}  # L_s(h/2) / L_s(h) - 1"
    );
    let _ = writeln!(
        t,
        "static_c_sigma_order = {static_order:.4}  # benchmarks/transmon_diffopt/c_sigma_convergence.toml [tensor_eps], h -> h/2 -> h/4"
    );
    t.push_str("expected_order = \"inference from theory, as c_sigma_convergence.toml: the sheet-edge current and charge singularities make energy-type quantities converge as O(h) under uniform refinement; the static C_Sigma shows order 1.41 over its three levels (pre-asymptotic)\"\n");
    let _ = writeln!(t, "estimate_order1_nh = {est_order1:.5}");
    let _ = writeln!(t, "estimate_static_order_nh = {est_static:.5}");
    let _ = writeln!(
        t,
        "estimate_order2_nh = {est_order2:.5}  # below the measured order-2 value: too fast, not used"
    );
    let _ = writeln!(
        t,
        "gmsh_h2_estimates_nh = {}  # orders 1, static, 2 with #1078's gmsh h/2 instead",
        arr(&alt_gmsh, 5)
    );
    let _ = writeln!(t, "finest_measured_nh = {finest:.5}");
    t.push_str("rule = \"L_s = midpoint of estimate_order1_nh and estimate_static_order_nh (3 s.f.); band = [finest_measured_nh floored, estimate_order1_nh ceiled] (3 s.f.). An extrapolation with a band, not a bound: L_s rises at every measured level (h -> h/2, order 1 -> 2), which puts the low end at the finest value; the order-1 rate is the slowest the theory allows\"\n");
    let _ = writeln!(
        t,
        "l_s_nh = {central_rule}  # quantum::transmon::DEVICELAYOUT_LOOP_INDUCTANCE_H"
    );
    let _ = writeln!(
        t,
        "l_s_band_nh = [{}, {}]  # DEVICELAYOUT_LOOP_INDUCTANCE_BAND_H",
        band_rule.0, band_rule.1
    );
    t.push_str("h4 = \"not run: Palace h/4 is about 11M ND unknowns. At h/2 (1.38M) Palace estimates a 10.0 GB peak (u1_L14p86/palace_run.log) in the 11.66 GiB Docker VM, so h/4 at about 8x the unknowns needs several times the VM and more than the host had free (96 GiB, about 22 GiB free during this work). geode's direct LU at 1.16M unknowns needed 92 GB (benchmarks/transmon_bench_cpu/results.toml). Order 2 at h/2 (about 7M) is out for the same reason. The converged value is therefore an estimate\"\n");
    let _ = writeln!(t, "local_cells = {}", strs(&PALACE_LOCAL.map(|c| c.0)));
    let _ = writeln!(
        t,
        "local_description = {}",
        strs(&PALACE_LOCAL.map(|c| c.1))
    );
    let _ = writeln!(
        t,
        "local_nd = [{}]",
        local
            .iter()
            .map(|c| c.nd.unwrap().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    );
    let _ = writeln!(
        t,
        "local_f_q_ghz = {}",
        arr(&local.iter().map(|c| c.f_ghz[0]).collect::<Vec<_>>(), 6)
    );
    let _ = writeln!(
        t,
        "local_l_s_nh = {}",
        arr(&local.iter().map(Cell::l_series_nh).collect::<Vec<_>>(), 5)
    );
    let local_ls: Vec<f64> = local.iter().map(Cell::l_series_nh).collect();
    let rise = |name: &str| {
        let i = PALACE_LOCAL
            .iter()
            .position(|c| c.0 == name)
            .expect("local cell");
        (local_ls[i] - l_h) / (l_h2 - l_h)
    };
    let (r10, r100, r300) = (
        rise("s10x3_L14p86"),
        rise("s100x2_L14p86"),
        rise("s300x1_L14p86"),
    );
    let _ = writeln!(
        t,
        "local_fraction_of_global_rise = [{r10:.3}, {r100:.3}, {r300:.3}]  # (L_s - L_s(h)) / (L_s(h/2) - L_s(h)) for s10x3, s100x2, s300x1"
    );
    let _ = writeln!(
        t,
        "local_reading = \"Refining only within 10 um of the junction (the wire and the finger's base) recovers {:.0} % of the h -> h/2 rise after three levels; refining out to 100 or 300 um recovers {:.0} % and {:.0} %. The mesh sensitivity is the current crowding along the wire and the edges of the finger and the ground around it, which the port patch does not change ([patch]). Local refinement is not a convergence sequence (it overshoots the global h/2 value), so these cells are an observation and do not enter the estimate\"",
        r10 * 100.0,
        r100 * 100.0,
        r300 * 100.0
    );

    t.push('\n');

    t.push_str("[analytic]\n");
    t.push_str("method = \"order of magnitude only, not used: the 24 um x 620 um island finger as a coplanar line (gap 30 um) fed at one end and charged uniformly along its length, L' l / 3 with L' = (mu0/4) K(k')/K(k), k = w/(w + 2s); plus the 12 um x 1 um wire as an isolated thin strip, (mu0 l / 2 pi)(ln(2 l / w) + 1/2)\"\n");
    let _ = writeln!(t, "finger_l_per_um_ph = {:.4}", l_per_m * 1e-6 * 1e12);
    let _ = writeln!(t, "finger_nh = {l_finger_nh:.4}");
    let _ = writeln!(t, "wire_nh = {l_wire_nh:.4}");
    let _ = writeln!(t, "total_nh = {:.4}", l_finger_nh + l_wire_nh);
    t.push('\n');

    t.push_str("[anchor]\n");
    t.push_str("variant = \"TransmonAnchor::devicelayout_bare_loop: the resonator-grounded qubit frequency of devicelayout_bare, read through the series circuit with L_s (devicelayout and devicelayout_bare are unchanged)\"\n");
    t.push_str("transfer_assumption = \"as #1078's k: one L_s describes DeviceLayout's run, and the anchor takes the converged L_s (the physical inductance of this geometry). If DeviceLayout's 4.14 GHz came from a mesh as coarse as the fixture, same_mesh_c_sigma_ff applies instead\"\n");
    let _ = writeln!(t, "k = {DEVICELAYOUT_COUPLING_K}");
    let _ = writeln!(t, "k_band = [{k_lo}, {k_hi}]");
    let _ = writeln!(t, "l_s_nh = {}", DEVICELAYOUT_LOOP_INDUCTANCE_H * 1e9);
    let _ = writeln!(t, "l_s_band_nh = [{}, {}]", ls_lo * 1e9, ls_hi * 1e9);
    let _ = writeln!(t, "bare_f_q_ghz = {:.6}", anchor.frequency_hz / 1e9);
    let _ = writeln!(
        t,
        "c_sigma_ff = {c_anchor:.6}  # TransmonAnchor::devicelayout_bare_loop"
    );
    let _ = writeln!(t, "c_total_ff = {:.6}", anchor.c_total_farad * 1e15);
    let _ = writeln!(t, "e_c_ghz = {:.6}", anchor.e_c_hz / 1e9);
    let _ = writeln!(t, "e_j_ghz = {:.6}", anchor.e_j_hz / 1e9);
    let _ = writeln!(
        t,
        "bare_c_sigma_ff = {c_bare:.6}  # TransmonAnchor::devicelayout_bare"
    );
    let _ = writeln!(
        t,
        "dressed_c_sigma_ff = {c_dressed:.6}  # TransmonAnchor::devicelayout"
    );
    let _ = writeln!(t, "shift_from_bare_ff = {:.6}", c_bare - c_anchor);
    let _ = writeln!(t, "shift_from_dressed_ff = {:.6}", c_dressed - c_anchor);
    let _ = writeln!(
        t,
        "c_sigma_band_ls_only_ff = [{:.4}, {:.4}]  # central k, L_s band",
        floor_dp(c_band_ls_only.0, 4),
        ceil_dp(c_band_ls_only.1, 4)
    );
    let _ = writeln!(
        t,
        "c_sigma_band_ff = [{:.4}, {:.4}]  # outward-rounded: (k high with odd harmonics, L_s high) to (k low two-node, L_s low)",
        floor_dp(target_band.0, 4),
        ceil_dp(target_band.1, 4)
    );
    let _ = writeln!(t, "c_sigma_band_low_unrounded_ff = {:.6}", target_band.0);
    let _ = writeln!(t, "c_sigma_band_high_unrounded_ff = {:.6}", target_band.1);
    let _ = writeln!(t, "same_mesh_l_s_nh = {l_h:.5}");
    let _ = writeln!(
        t,
        "same_mesh_c_sigma_ff = {c_same_mesh:.6}  # central k, L_s of the fixture"
    );
    let _ = writeln!(
        t,
        "h2_mesh_c_sigma_ff = {c_h2:.6}  # central k, L_s of Palace h/2"
    );
    let _ = writeln!(
        t,
        "p2_c_sigma_ff = {c_p2:.6}  # central k, L_s of Palace order 2 on h"
    );
    let _ = writeln!(
        t,
        "parallel_recipe_shift_ff = {parallel_recipe_shift:.6}  # #1078's C_total L_s / (L_J + L_s) at this L_s, for comparison with shift_from_bare_ff"
    );
    t.push('\n');

    t.push_str("[verdicts]\n");
    t.push_str("brackets_source = \"#1077 (#1075): benchmarks/transmon_diffopt/c_sigma_bracket_anchor_93p95.toml, unrounded family bounds (as benchmarks/transmon_coupling/results.toml [verdicts])\"\n");
    t.push_str("target = \"TransmonAnchor::devicelayout_bare_loop, [anchor].c_sigma_band_ff\"\n");
    for (name, b) in [
        ("design_1066", BRACKET_1066_DESIGN_FF),
        ("resolution_corrected", BRACKET_CORRECTED_FF),
    ] {
        let _ = writeln!(t, "{name}_bracket_ff = [{:.6}, {:.6}]", b.0, b.1);
        let _ = writeln!(t, "{name}_vs_loop_target = \"{}\"", verdict(b));
        let _ = writeln!(
            t,
            "{name}_minus_loop_central_ff = [{:.4}, {:.4}]  # outward-rounded",
            floor_dp(b.0 - c_anchor, 4),
            ceil_dp(b.1 - c_anchor, 4)
        );
        let _ = writeln!(
            t,
            "{name}_minus_loop_band_ff = [{:.4}, {:.4}]  # outward-rounded, against the whole target band",
            floor_dp(b.0 - target_band.1, 4),
            ceil_dp(b.1 - target_band.0, 4)
        );
    }
    t.push('\n');

    t.push_str("[offset_1043]\n");
    t.push_str("method = \"same mesh (the fixture), the qubit at L_J and at 2 L_J: the lumped LC frequency 1/(2 pi sqrt(L_J (C_Sigma + C_J))) with geode's static C_Sigma on h; then the series circuit with the fixture's L_s; then the Maxwell two-node hybridization with #1078's same-mesh k and f_r; the residual is Palace over that. Shares are relative frequency offsets and add to first order\"\n");
    let _ = writeln!(
        t,
        "c_sigma_h_ff = {:.6}  # benchmarks/transmon_coupling/results.toml [static_proxy].c_sigma_ff[0]",
        c_h_static * 1e15
    );
    let _ = writeln!(t, "l_s_h_nh = {l_h:.5}  # [convergence].global_l_s_nh[0]");
    let _ = writeln!(t, "k_h = {k_h:.6}");
    let _ = writeln!(t, "f_r_h_ghz = {:.6}", f_r_h / 1e9);
    let _ = writeln!(
        t,
        "issue_1043_offset_pct = {off_pct_1043}  # benchmarks/transmon_eigen/results.toml [oracles.palace_target2]"
    );
    let _ = writeln!(t, "issue_1043_offset_2lj_pct = {off2_pct_1043}");
    for (tag, a) in [("", &acc1), ("_2lj", &acc2)] {
        let _ = writeln!(t, "lumped_f_ghz{tag} = {:.6}", a.0 / 1e9);
        let _ = writeln!(t, "series_f_ghz{tag} = {:.6}", a.1 / 1e9);
        let _ = writeln!(t, "dressed_f_ghz{tag} = {:.6}", a.2 / 1e9);
        let _ = writeln!(t, "offset_pct{tag} = {:.4}", a.3 * 100.0);
        let _ = writeln!(t, "loop_pct{tag} = {:.4}", a.4 * 100.0);
        let _ = writeln!(t, "hybridization_pct{tag} = {:.4}", a.5 * 100.0);
        let _ = writeln!(t, "residual_pct{tag} = {:.4}", a.6 * 100.0);
    }
    let _ = writeln!(
        t,
        "loop_share = {ls_share:.4}  # loop_pct / offset_pct at L_J"
    );
    let _ = writeln!(
        t,
        "loop_plus_hybridization_share = {:.4}",
        (acc1.4 + acc1.5) / acc1.3
    );
    let _ = writeln!(
        t,
        "reading = \"On the same mesh the fixture's L_s accounts for {:.0} % of the {:.3} % offset and the qubit-resonator hybridization for {:.0} %; the residual {:.4} % ({:.0} %) is what is left for solver and permittivity-model differences, which the anchor docs had named as the whole cause. At 2 L_J the L_s part halves ({:.4} %), as a series inductance predicts, and the residual stays at {:.4} %, as an L_J-independent capacitance difference would\"",
        ls_share * 100.0,
        acc1.3 * 100.0,
        acc1.5 / acc1.3 * 100.0,
        acc1.6 * 100.0,
        acc1.6 / acc1.3 * 100.0,
        acc2.4 * 100.0,
        acc2.6 * 100.0
    );

    fs::create_dir_all(&out_dir).expect("output dir");
    let path = out_dir.join("results.toml");
    fs::write(&path, t).expect("write results");
    println!(
        "wrote {} ({:.1}s)",
        path.display(),
        t_all.elapsed().as_secs_f64()
    );
}
