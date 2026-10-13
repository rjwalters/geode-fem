//! Two-parameter re-optimization of DeviceLayout.jl's SingleTransmon
//! (issue #1092, part 2): the transmon `capacitor_length` and the readout
//! resonator `total_length`, toward the example's own targets
//! `targets_GHz = [3.0, 4.0]`, with geode's A2 tensor-ε eigen path and its
//! Hellmann–Feynman shape gradient.
//!
//! # The two parameters
//!
//! - **`capacitor_length` `c` (shape gradient).** DeviceLayout keeps the
//!   junction at the origin and builds the island upward from
//!   `y = junction_gap = 12 µm` to `y = 12 µm + c`. Everything above the
//!   island (the claw, the meander, the feedline with its air bridges)
//!   translates rigidly with `c`, and the exterior box re-centers by `c/2`.
//!   The part-1 meshes confirm this exactly: from the initial to the final
//!   geometry the readout ports move by `Δc` and both box faces by `Δc/2`.
//!   [`cap_velocity`] is that motion as a node velocity `∂X/∂c`: zero below
//!   the island, a linear stretch of the island band, a rigid translation
//!   above it, and linear blends to `1/2` at the two box faces through
//!   plain ground. It checks the mesh first: every metal outline node in a
//!   stretched or blended band must lie on a straight edge the motion maps
//!   onto itself. The eigenfrequency gradient is then the adjoint-free
//!   [`EigenSensitivity::deigenvalue_dtheta_tensor_eps`] contraction of the
//!   converged eigenvector with the exact element JVP.
//! - **`total_length` `L` (not a shape motion).** The meander's legs and
//!   turns are re-laid by DeviceLayout when `L` changes, so no fixed-topology
//!   node motion represents it. It is handled through the transmission-line
//!   relation `f_r ∝ 1/(L + δ)`, with `δ` measured once by a finite
//!   difference of two DeviceLayout meshes, and `∂f_q/∂L = 0` (the dressing
//!   shift through the detuning is below 0.1 %).
//!
//! # Modes
//!
//! **Solve** (on the worker): one A2 solve of a mesh with the shape gradients
//! of the two lowest modes, written as a TOML record. With
//! `GEODE_FD_H_UM=<h>` it also re-solves the mesh moved by `±h` along the
//! velocity and records the central difference beside the Hellmann–Feynman
//! value.
//!
//! ```sh
//! GEODE_SOLVE_MESH=/work/it0_o1.msh GEODE_SOLVE_LABEL=it0 GEODE_CAP_UM=620 \
//! GEODE_SIGMA_GHZ=4.0 GEODE_RUNS_DIR=/work/opt_runs \
//!     cargo run -p geode-core --release --example transmon_devicelayout_optimize
//! ```
//!
//! **Assemble** (default): read the committed records and Palace cells and
//! write `benchmarks/transmon_devicelayout_optimize/results.toml`.

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
use geode_core::eigen::projection::solve_transmon_eigenmodes_projected_with_port_model_full;
use geode_core::eigen::sensitivity::EigenSensitivity;
use geode_core::eigen::transmon::{
    JunctionPortModel, LumpedReactiveShunt, ReactiveElementNatural, TransmonMode, TransmonPencil,
    lambda_shift_for_frequency_hz,
};
use geode_core::mesh::spiral::pec_interior_mask_from_triangles;
use geode_core::mesh::transmon::{JUNCTION_CAPACITANCE_F, JUNCTION_INDUCTANCE_H};
use geode_core::mesh::{TetMesh, TransmonFixture, read_transmon_fixture_from_bytes};
use geode_core::testing::TestBackend;
use geode_util::fixture::BackendInfo;

type B = TestBackend;

/// Metres per mesh unit (micrometre mesh).
const M_PER_UNIT: f64 = 1e-6;

/// DeviceLayout's `junction_gap`: the island's lower edge sits at this `y`
/// (µm) for every `capacitor_length`.
const ISLAND_BOTTOM_UM: f64 = 12.0;
/// The stretched band ends this far (µm) below the island top `12 + c`,
/// below the claw, which overlaps the island's upper 97 µm.
const STRETCH_TOP_BELOW_ISLAND_TOP_UM: f64 = 132.0;
/// The upper blend starts this far (µm) above the island top, above the
/// feedline and its air bridges (the highest feature sits 1663 µm above the
/// island bottom offset in both part-1 geometries).
const TOP_BLEND_ABOVE_CAP_UM: f64 = 1700.0;
/// Half-widths (µm) of the island and of its ground cutout: the only metal
/// edges inside the stretched band.
const ISLAND_HALF_WIDTH_UM: f64 = 12.0;
const CUTOUT_HALF_WIDTH_UM: f64 = 42.0;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

fn vals_to_host(t: Tensor<B, 1>) -> Vec<f64> {
    t.into_data().iter::<f64>().collect()
}

fn peak_rss_gb() -> Option<f64> {
    let s = fs::read_to_string("/proc/self/status").ok()?;
    let line = s.lines().find(|l| l.starts_with("VmHWM:"))?;
    let kb: f64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kb / 1e6)
}

fn git_head() -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "--short=8", "HEAD"])
        .current_dir(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

// ---------------------------------------------------------------------------
// The capacitor_length node velocity.
// ---------------------------------------------------------------------------

/// The band layout of [`cap_velocity`] for one mesh (µm).
#[derive(Debug, Clone, Copy)]
struct Bands {
    y_min: f64,
    y0: f64,
    y1: f64,
    yt: f64,
    y_max: f64,
}

impl Bands {
    fn of(mesh: &TetMesh, cap_um: f64) -> Self {
        let (mut y_min, mut y_max) = (f64::INFINITY, f64::NEG_INFINITY);
        for p in &mesh.nodes {
            y_min = y_min.min(p[1]);
            y_max = y_max.max(p[1]);
        }
        let b = Self {
            y_min,
            y0: ISLAND_BOTTOM_UM,
            y1: ISLAND_BOTTOM_UM + cap_um - STRETCH_TOP_BELOW_ISLAND_TOP_UM,
            yt: cap_um + TOP_BLEND_ABOVE_CAP_UM,
            y_max,
        };
        assert!(
            b.y_min < -100.0 && b.y1 > b.y0 + 100.0 && b.yt + 100.0 < b.y_max,
            "band layout does not fit this mesh: {b:?}"
        );
        b
    }

    /// `∂y/∂c` at height `y`.
    fn vy(&self, y: f64) -> f64 {
        if y < 0.0 {
            0.5 * y / self.y_min
        } else if y <= self.y0 {
            0.0
        } else if y < self.y1 {
            (y - self.y0) / (self.y1 - self.y0)
        } else if y <= self.yt {
            1.0
        } else {
            1.0 - 0.5 * (y - self.yt) / (self.y_max - self.yt)
        }
    }
}

/// Nodes on the outline of the metal sheet: the edges of `z = 0` metal
/// triangles that belong to exactly one such triangle.
fn metal_outline_nodes(fx: &TransmonFixture) -> Vec<usize> {
    use std::collections::HashMap;
    let mut count: HashMap<(u32, u32), u32> = HashMap::new();
    for tri in fx.metal_triangles() {
        if tri
            .iter()
            .any(|&n| fx.mesh.nodes[n as usize][2].abs() > 1e-9)
        {
            continue;
        }
        for (a, b) in [(tri[0], tri[1]), (tri[1], tri[2]), (tri[2], tri[0])] {
            *count.entry((a.min(b), a.max(b))).or_insert(0) += 1;
        }
    }
    let mut nodes: Vec<usize> = count
        .into_iter()
        .filter(|&(_, c)| c == 1)
        .flat_map(|((a, b), _)| [a as usize, b as usize])
        .collect();
    nodes.sort_unstable();
    nodes.dedup();
    nodes
}

/// The node velocity `∂X/∂c` of DeviceLayout's `capacitor_length` on this
/// mesh, after checking that the motion is exact for its geometry:
///
/// - in the stretched band `(y0, y1)` every metal outline node lies on the
///   island's or the cutout's vertical sides (`|x| = 12` or `42`), which a
///   `y`-only stretch maps onto themselves;
/// - in the two blends every outline node lies on a box wall;
/// - no off-plane metal (air bridges) and none of the junction port's nodes
///   sit where the motion is not rigid.
///
/// Returns the velocity and the number of outline nodes checked.
fn cap_velocity(fx: &TransmonFixture, cap_um: f64) -> (Vec<[f64; 3]>, Bands, usize) {
    let mesh = &fx.mesh;
    let b = Bands::of(mesh, cap_um);
    let (mut x_min, mut x_max) = (f64::INFINITY, f64::NEG_INFINITY);
    for p in &mesh.nodes {
        x_min = x_min.min(p[0]);
        x_max = x_max.max(p[0]);
    }
    let on = |v: f64, w: f64| (v - w).abs() < 1e-6;
    let outline = metal_outline_nodes(fx);
    for &n in &outline {
        let [x, y, _] = mesh.nodes[n];
        let wall = on(x, x_min) || on(x, x_max) || on(y, b.y_min) || on(y, b.y_max);
        if y > b.y0 + 1e-6 && y < b.y1 - 1e-6 {
            let ax = x.abs();
            assert!(
                on(ax, ISLAND_HALF_WIDTH_UM) || on(ax, CUTOUT_HALF_WIDTH_UM) || wall,
                "metal outline node {n} at ({x}, {y}) inside the stretched band is not on a vertical side"
            );
        }
        if y < -1e-6 || y > b.yt + 1e-6 {
            assert!(
                wall,
                "metal outline node {n} at ({x}, {y}) inside a blend is not on a box wall"
            );
        }
    }
    for tri in fx.metal_triangles() {
        for &n in &tri {
            let [_, y, z] = mesh.nodes[n as usize];
            if z.abs() > 1e-9 {
                assert!(
                    y >= b.y1 && y <= b.yt,
                    "off-plane metal node {n} at y = {y} outside the rigid band"
                );
            }
        }
    }
    let vel: Vec<[f64; 3]> = mesh.nodes.iter().map(|p| [0.0, b.vy(p[1]), 0.0]).collect();
    let jport = fx.lumped_element_port();
    for f in &jport.faces {
        for &n in f {
            assert_eq!(vel[n as usize][1], 0.0, "junction port node {n} moves");
        }
    }
    (vel, b, outline.len())
}

// ---------------------------------------------------------------------------
// One A2 solve on a (possibly moved) mesh.
// ---------------------------------------------------------------------------

struct Solved {
    modes: Vec<TransmonMode>,
    max_residual_rel: f64,
    lanczos_iterations: usize,
    n_edges: usize,
    n_interior: usize,
    mask: Vec<bool>,
    edges: Vec<[u32; 2]>,
}

fn solve_a2(fx: &TransmonFixture, sigma_ghz: f64, n_modes: usize) -> Solved {
    let mesh = &fx.mesh;
    let edges = mesh.edges();
    let te = mesh.tet_edges();
    let idx: Vec<[u32; 6]> = te.iter().map(|r| std::array::from_fn(|i| r[i].0)).collect();
    let sgn: Vec<[i8; 6]> = te.iter().map(|r| std::array::from_fn(|i| r[i].1)).collect();
    let eps = fx.epsilon_tensor_r();
    let scatter = NedelecScatterMap::new(&idx);
    let (nodes_t, tets_t) = upload_mesh::<B>(mesh, &device());
    let identity: [[c64; 3]; 3] = std::array::from_fn(|i| {
        std::array::from_fn(|j| c64::new(if i == j { 1.0 } else { 0.0 }, 0.0))
    });
    let nu = vec![identity; mesh.n_tets()];
    let sys = assemble_global_nedelec_with_full_tensors_sparse::<B>(
        nodes_t, tets_t, &sgn, &scatter, &eps, &nu,
    );
    let k_vals = vals_to_host(sys.k_re_vals);
    let m_vals = vals_to_host(sys.m_re_vals);
    let metal = fx.metal_triangles();
    let exterior = fx.exterior_boundary_triangles();
    let mask = pec_interior_mask_from_triangles(&edges, &[metal.as_slice(), exterior.as_slice()]);
    let jport = fx.lumped_element_port();
    let element =
        ReactiveElementNatural::from_si(JUNCTION_INDUCTANCE_H, JUNCTION_CAPACITANCE_F, M_PER_UNIT);
    let pencil = TransmonPencil {
        scatter: &scatter,
        k_vals: &k_vals,
        m_vals: &m_vals,
        edges: &edges,
        mesh,
        shunt: LumpedReactiveShunt {
            faces: &jport.faces,
            length: jport.length,
            width: jport.width,
            element,
        },
        interior_mask: &mask,
    };
    let sigma = lambda_shift_for_frequency_hz(sigma_ghz * 1e9, M_PER_UNIT);
    let (mut modes, diag) = solve_transmon_eigenmodes_projected_with_port_model_full(
        &pencil,
        JunctionPortModel::PortVoltage { e_hat: jport.e_hat },
        sigma,
        n_modes,
        M_PER_UNIT,
    )
    .expect("A2 bulk-projected eigensolve");
    assert_eq!(modes.len(), n_modes);
    let max_residual_rel = diag
        .mode_residual_rels
        .iter()
        .copied()
        .fold(0.0_f64, f64::max);
    assert!(
        max_residual_rel <= 1e-6,
        "residuals {:?}",
        diag.mode_residual_rels
    );
    modes.sort_by(|a, b| a.report.frequency_hz.total_cmp(&b.report.frequency_hz));
    let n_interior = mask.iter().filter(|&&m| m).count();
    Solved {
        modes,
        max_residual_rel,
        lanczos_iterations: diag.iterations,
        n_edges: edges.len(),
        n_interior,
        mask,
        edges,
    }
}

fn moved(fx: &TransmonFixture, vel: &[[f64; 3]], h: f64) -> TransmonFixture {
    let mut m = fx.clone();
    for (p, v) in m.mesh.nodes.iter_mut().zip(vel) {
        for (x, vc) in p.iter_mut().zip(v) {
            *x += h * vc;
        }
    }
    m
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

fn solve_mode(
    mesh_path: &Path,
    label: &str,
    cap_um: f64,
    total_um: f64,
    sigma_ghz: f64,
    fd_h_um: Option<f64>,
    runs_dir: &Path,
) {
    let t_all = Instant::now();
    let bytes = fs::read(mesh_path).unwrap_or_else(|e| panic!("{}: {e}", mesh_path.display()));
    let fx = read_transmon_fixture_from_bytes(&bytes).expect("transmon mesh");
    println!(
        "{label}: {} ({} nodes, {} tets), c = {cap_um} um, L = {total_um} um",
        mesh_path.display(),
        fx.mesh.nodes.len(),
        fx.mesh.n_tets()
    );
    let (vel, bands, n_outline) = cap_velocity(&fx, cap_um);
    let n_moving = vel.iter().filter(|v| v[1] != 0.0).count();

    let t_solve = Instant::now();
    let n_modes = 3;
    let s = solve_a2(&fx, sigma_ghz, n_modes);
    let solve_s = t_solve.elapsed().as_secs_f64();
    let lambdas: Vec<f64> = s.modes.iter().map(|m| m.report.lambda).collect();
    let f: Vec<f64> = s.modes.iter().map(|m| m.report.frequency_ghz()).collect();
    let p: Vec<f64> = s.modes.iter().map(|m| m.report.participation).collect();
    assert!(
        p[0] > 0.9 && p[1] < 0.1,
        "{label}: mode 0 must be the qubit and mode 1 the resonator: f {f:?} p {p:?}"
    );

    // Hellmann–Feynman shape gradient along ∂X/∂c, sapphire tensor.
    let t_grad = Instant::now();
    let eps_real: Vec<[[f64; 3]; 3]> = fx
        .epsilon_tensor_r()
        .iter()
        .map(|e| std::array::from_fn(|i| std::array::from_fn(|j| e[i][j].re)))
        .collect();
    let unused = vec![f64::NAN; fx.mesh.n_tets()];
    let mut dfdc = Vec::new();
    for (i, mode) in s.modes.iter().take(2).enumerate() {
        let sens = EigenSensitivity {
            mesh: &fx.mesh,
            edges: &s.edges,
            interior_mask: &s.mask,
            eps_r: &unused,
            lambdas: &lambdas,
            mode_index: i,
            eigenvector: &mode.vector,
            min_rel_gap: 1e-3,
        };
        let dl = sens
            .deigenvalue_dtheta_tensor_eps(&eps_real, &vel)
            .expect("Hellmann–Feynman gradient");
        dfdc.push(f[i] * dl / (2.0 * lambdas[i]));
    }
    let grad_s = t_grad.elapsed().as_secs_f64();
    println!(
        "  f = {}  p = {}  df/dc = {} GHz/um  (solve {solve_s:.1} s, gradient {grad_s:.2} s)",
        arr(&f, 6),
        arr(&p, 6),
        arr(&dfdc, 8)
    );

    // Optional central-difference check along the same motion.
    let fd = fd_h_um.map(|h| {
        let fp = solve_a2(&moved(&fx, &vel, h), sigma_ghz, n_modes);
        let fm = solve_a2(&moved(&fx, &vel, -h), sigma_ghz, n_modes);
        let d: Vec<f64> = (0..2)
            .map(|i| {
                (fp.modes[i].report.frequency_ghz() - fm.modes[i].report.frequency_ghz())
                    / (2.0 * h)
            })
            .collect();
        println!("  FD (h = {h} um): df/dc = {}", arr(&d, 8));
        (h, d)
    });
    let rss = peak_rss_gb();

    let backend = BackendInfo::of::<B>(&device());
    let mut t = String::new();
    t.push_str("# geode A2 tensor-eps solve + Hellmann-Feynman capacitor_length gradient (issue #1092 part 2),\n");
    t.push_str(
        "# written by examples/transmon_devicelayout_optimize.rs in solve mode. Do NOT edit.\n",
    );
    let _ = writeln!(t, "\n[meta]");
    let _ = writeln!(t, "label = \"{label}\"");
    let _ = writeln!(
        t,
        "mesh_file = \"{}\"",
        mesh_path.file_name().unwrap().to_string_lossy()
    );
    let _ = writeln!(
        t,
        "host = \"{}\"",
        std::env::var("GEODE_HOST_NOTE").unwrap_or_else(|_| std::env::consts::ARCH.to_string())
    );
    let _ = writeln!(t, "backend = \"{}\"", backend.name);
    let _ = writeln!(t, "float_dtype = \"{}\"", backend.float_dtype);
    let _ = writeln!(t, "generated_at_commit = \"{}\"", git_head());
    let _ = writeln!(t, "\n[run]");
    let _ = writeln!(t, "capacitor_length_um = {cap_um}");
    let _ = writeln!(t, "total_length_um = {total_um}");
    let _ = writeln!(t, "n_nodes = {}", fx.mesh.nodes.len());
    let _ = writeln!(t, "n_tets = {}", fx.mesh.n_tets());
    let _ = writeln!(t, "n_edges = {}", s.n_edges);
    let _ = writeln!(t, "n_interior_edges = {}", s.n_interior);
    let _ = writeln!(t, "sigma_ghz = {sigma_ghz}");
    let _ = writeln!(t, "f_ghz = {}", arr(&f, 6));
    let _ = writeln!(t, "p = {}", arr(&p, 6));
    let _ = writeln!(t, "max_residual_rel = {:.3e}", s.max_residual_rel);
    let _ = writeln!(t, "lanczos_iterations = {}", s.lanczos_iterations);
    let _ = writeln!(
        t,
        "df_dc_ghz_per_um = {}  # Hellmann-Feynman, qubit and resonator",
        arr(&dfdc, 9)
    );
    let _ = writeln!(
        t,
        "velocity_bands_um = [{:.3}, {:.3}, {:.3}, {:.3}, {:.3}]  # y_min, y0, y1, yt, y_max",
        bands.y_min, bands.y0, bands.y1, bands.yt, bands.y_max
    );
    let _ = writeln!(t, "velocity_moving_nodes = {n_moving}");
    let _ = writeln!(t, "velocity_outline_nodes_checked = {n_outline}");
    if let Some((h, d)) = &fd {
        let _ = writeln!(t, "fd_h_um = {h}");
        let _ = writeln!(t, "fd_df_dc_ghz_per_um = {}", arr(d, 9));
    }
    let _ = writeln!(t, "solve_s = {solve_s:.1}");
    let _ = writeln!(t, "gradient_s = {grad_s:.2}");
    let _ = writeln!(t, "wall_s = {:.1}", t_all.elapsed().as_secs_f64());
    let _ = writeln!(t, "peak_rss_gb = {:.2}", rss.unwrap_or(-1.0));
    fs::create_dir_all(runs_dir).expect("runs dir");
    let path = runs_dir.join(format!("{label}.toml"));
    fs::write(&path, t).expect("write record");
    println!("wrote {}", path.display());
}

fn main() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let out_dir = std::env::var("TRANSMON_DL_OPT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| repo.join("benchmarks/transmon_devicelayout_optimize"));
    let env_f = |k: &str| -> Option<f64> {
        std::env::var(k)
            .ok()
            .map(|s| s.parse().unwrap_or_else(|_| panic!("{k}")))
    };
    if let Ok(mesh) = std::env::var("GEODE_SOLVE_MESH") {
        let label = std::env::var("GEODE_SOLVE_LABEL").expect("GEODE_SOLVE_LABEL");
        let runs = std::env::var("GEODE_RUNS_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| out_dir.join("runs"));
        solve_mode(
            Path::new(&mesh),
            &label,
            env_f("GEODE_CAP_UM").expect("GEODE_CAP_UM"),
            env_f("GEODE_TOTAL_UM").expect("GEODE_TOTAL_UM"),
            env_f("GEODE_SIGMA_GHZ").unwrap_or(4.0),
            env_f("GEODE_FD_H_UM"),
            &runs,
        );
    } else {
        assemble_mode(&repo, &out_dir);
    }
}

// ---------------------------------------------------------------------------
// Assemble mode.
// ---------------------------------------------------------------------------

/// The targets of DeviceLayout's `run_optimization` (example source).
const TARGETS_GHZ: [f64; 2] = [3.0, 4.0];
/// The published designs and the PRIMA run's cost.
const INITIAL_UM: [f64; 2] = [620.0, 5000.0];
const PUBLISHED_FINAL_UM: [f64; 2] = [1217.678, 7243.955];
const PUBLISHED_FINAL_GHZ: [f64; 2] = [3.005, 3.982];
const PRIMA_PALACE_RUNS: usize = 9;
/// DeviceLayout's stopping rule: `sum((f - t)^2 / t^2) / 2 < reltol^2`.
const DL_RELTOL: f64 = 1e-2;
/// The Newton loop's own stopping rule.
const NEWTON_ERR_STOP: f64 = 1e-6;

const PALACE_OPT_DIR: &str = "reference/fixtures/transmon_palace/results_optimize_1092";
const PALACE_XCHECK_DIR: &str = "reference/fixtures/transmon_palace/results_crosscheck_1092";

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

fn csv_col(path: &Path, col: usize) -> Vec<f64> {
    let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    text.lines()
        .skip(1)
        .filter_map(|l| l.split(',').nth(col))
        .map(|s| s.trim().parse::<f64>().expect("csv column"))
        .collect()
}

/// The two lowest physical modes of a Palace cell: junction port EPR (the
/// last port-EPR column) at least 1e-6, in frequency order.
fn palace_pair(dir: &Path) -> [f64; 2] {
    let f = csv_col(&dir.join("eig.csv"), 1);
    let epr = dir.join("port-EPR.csv");
    let text = fs::read_to_string(&epr).unwrap_or_else(|e| panic!("{}: {e}", epr.display()));
    let ncol = text.lines().next().unwrap().split(',').count();
    let p = csv_col(&epr, ncol - 1);
    let kept: Vec<f64> = f
        .iter()
        .zip(&p)
        .filter(|(_, p)| p.abs() >= 1e-6)
        .map(|(&f, _)| f)
        .collect();
    assert!(
        kept.len() >= 2 && kept[0] < kept[1],
        "{}: {kept:?}",
        dir.display()
    );
    [kept[0], kept[1]]
}

fn time_wall_s(path: &Path) -> f64 {
    let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let line = text
        .lines()
        .find(|l| l.contains("Elapsed (wall clock) time"))
        .expect("wall clock line");
    let v = line.rsplit(' ').next().unwrap();
    let parts: Vec<f64> = v.split(':').map(|s| s.parse().unwrap()).collect();
    match parts.len() {
        3 => parts[0] * 3600.0 + parts[1] * 60.0 + parts[2],
        2 => parts[0] * 60.0 + parts[1],
        _ => parts[0],
    }
}

fn dl_error(f: [f64; 2]) -> f64 {
    (0..2)
        .map(|i| (f[i] - TARGETS_GHZ[i]).powi(2) / TARGETS_GHZ[i].powi(2))
        .sum::<f64>()
        / 2.0
}

/// One step of the Newton iteration of `driver.py` (the same arithmetic):
/// in DeviceLayout's variables `x1 = √(620/c)`, `x2 = 5000/L`, with the
/// Jacobian `[[S_q ∂f_q/∂c · dc/dx1, 0], [S_r ∂f_r/∂c · dc/dx1,
/// −F_r/(L + δ) · dL/dx2]]`, `S = F / f_h`, clamped to the box
/// `[0.6, 1.4]²` and rounded to 1 nm.
fn newton_step(
    theta: [f64; 2],
    big_f: [f64; 2],
    f_h: [f64; 2],
    dfdc: [f64; 2],
    delta: f64,
) -> [f64; 2] {
    let [c, l] = theta;
    let (x1, x2) = ((620.0 / c).sqrt(), 5000.0 / l);
    let dc_dx1 = -2.0 * 620.0 / x1.powi(3);
    let dl_dx2 = -5000.0 / x2.powi(2);
    let s = [big_f[0] / f_h[0], big_f[1] / f_h[1]];
    let jq_1 = s[0] * dfdc[0] * dc_dx1;
    let jr_1 = s[1] * dfdc[1] * dc_dx1;
    let jr_2 = -big_f[1] / (l + delta) * dl_dx2;
    let dx1 = -(big_f[0] - TARGETS_GHZ[0]) / jq_1;
    let dx2 = -((big_f[1] - TARGETS_GHZ[1]) + jr_1 * dx1) / jr_2;
    let x1n = (x1 + dx1).clamp(0.6, 1.4);
    let x2n = (x2 + dx2).clamp(0.6, 1.4);
    let r = |v: f64| (v * 1e3).round() / 1e3;
    [r(620.0 / (x1n * x1n)), r(5000.0 / x2n)]
}

struct Rec {
    theta: [f64; 2],
    f: [f64; 2],
    dfdc: [f64; 2],
    wall_s: f64,
    doc: toml::Value,
}

fn rec(runs: &Path, label: &str) -> Rec {
    let d = toml_doc(&runs.join(format!("{label}.toml")));
    assert_eq!(
        tget(&d, "meta.float_dtype").as_str(),
        Some("F64"),
        "{label}"
    );
    let f = tarr(&d, "run.f_ghz");
    let g = tarr(&d, "run.df_dc_ghz_per_um");
    Rec {
        theta: [
            tf(&d, "run.capacitor_length_um"),
            tf(&d, "run.total_length_um"),
        ],
        f: [f[0], f[1]],
        dfdc: [g[0], g[1]],
        wall_s: tf(&d, "run.wall_s"),
        doc: d,
    }
}

fn pct(x: f64, r: f64) -> f64 {
    100.0 * (x / r - 1.0)
}

fn assemble_mode(repo: &Path, out_dir: &Path) {
    let runs = out_dir.join("runs");
    let mut t = String::new();
    t.push_str("# Auto-generated by `cargo run -p geode-core --release --example transmon_devicelayout_optimize`.\n");
    t.push_str("# Do NOT edit by hand; regenerate after any intentional change.\n");
    t.push_str("# Issue #1092 part 2. Checked by tests/transmon_devicelayout_optimize.rs.\n\n");

    // --- Gradient checks and the transmission-line calibration.
    let fd0 = rec(&runs, "fd_initial");
    let fd_h = tf(&fd0.doc, "run.fd_h_um");
    let fd = tarr(&fd0.doc, "run.fd_df_dc_ghz_per_um");
    let l0 = rec(&runs, "calibL_5000");
    let l1 = rec(&runs, "calibL_5250");
    let c1 = rec(&runs, "calibC_650");
    assert_eq!(
        l0.f, fd0.f,
        "the two initial-geometry solves are the same mesh"
    );
    let delta = (l1.f[1] * l1.theta[1] - l0.f[1] * l0.theta[1]) / (l0.f[1] - l1.f[1]);
    let remesh_fd = [
        (c1.f[0] - l0.f[0]) / (c1.theta[0] - l0.theta[0]),
        (c1.f[1] - l0.f[1]) / (c1.theta[0] - l0.theta[0]),
    ];
    let hf_mean = [
        0.5 * (l0.dfdc[0] + c1.dfdc[0]),
        0.5 * (l0.dfdc[1] + c1.dfdc[1]),
    ];
    let dfq_dl_remesh = (l1.f[0] - l0.f[0]) / (l1.theta[1] - l0.theta[1]);

    let backend = tget(&fd0.doc, "meta.backend").as_str().unwrap().to_string();
    t.push_str("[meta]\n");
    t.push_str("description = \"Two-parameter re-optimization of DeviceLayout.jl's SingleTransmon (capacitor_length, resonator total_length) toward the example's targets [3.0, 4.0] GHz: Newton in DeviceLayout's own optimizer variables with geode's A2 tensor-eps solve and Hellmann-Feynman capacitor_length shape gradient on DeviceLayout's order-1 mesh, the resonator length through the transmission line; against DeviceLayout's discretization (Palace on its mesh_order 2 mesh, solver_order 2, its configfile) and against the converged frequencies (one step, verified by a resolution ladder)\"\n");
    t.push_str("issue = 1092\n");
    let _ = writeln!(t, "backend = \"{backend}\"  # of the geode solve records");
    t.push_str("float_dtype = \"F64\"\n");
    let _ = writeln!(t, "generated_at_commit = \"{}\"", git_head());
    t.push_str("targets_ghz = [3.0, 4.0]  # DeviceLayout.jl v1.15.0 examples/SingleTransmon/SingleTransmon.jl run_optimization(targets_GHz)\n");
    t.push_str("geometry_generator = \"reference/julia/generate_transmon_param.jl (DeviceLayout.jl v1.15.0, gmsh_jll 4.15.2, Julia 1.10.11, macOS arm64); every iterate meshed by DeviceLayout itself\"\n");
    t.push_str("compute_host = \"AWS r8g.8xlarge (Graviton4, 32 vCPU, 247 GiB), Ubuntu 24.04 arm64; Palace fba6a5b (v0.17.0-99), 32 MPI ranks; geode ndarray f64\"\n");

    t.push_str("\n[gradient_check]\n");
    t.push_str("method = \"Hellmann-Feynman d f / d c along the capacitor_length node velocity (eigen::sensitivity::EigenSensitivity::deigenvalue_dtheta_tensor_eps, sapphire tensor), against a central difference of two A2 re-solves of the same mesh moved by +-h along the same velocity\"\n");
    let _ = writeln!(t, "mesh = \"initial geometry, DeviceLayout order-1 mesh\"");
    let _ = writeln!(
        t,
        "hf_df_dc_ghz_per_um = [{:.9}, {:.9}]",
        fd0.dfdc[0], fd0.dfdc[1]
    );
    let _ = writeln!(t, "fd_h_um = {fd_h}");
    let _ = writeln!(t, "fd_df_dc_ghz_per_um = [{:.9}, {:.9}]", fd[0], fd[1]);
    let _ = writeln!(
        t,
        "qubit_rel_diff = {:.3e}",
        (fd0.dfdc[0] - fd[0]).abs() / fd[0].abs()
    );
    let _ = writeln!(
        t,
        "gradient_s = {:.2}  # one contraction; the FD needs two more solves",
        tf(&fd0.doc, "run.gradient_s")
    );
    let _ = writeln!(
        t,
        "velocity_outline_nodes_checked = {}",
        tf(&fd0.doc, "run.velocity_outline_nodes_checked") as i64
    );
    t.push_str("\n[remesh_noise]\n");
    t.push_str("note = \"DeviceLayout re-meshes every design, so the order-1 objective is not a smooth function of the parameters: the remeshed finite difference differs from the fixed-topology (Hellmann-Feynman) derivative by more than the derivative itself. This is why the iteration runs Newton on the high-fidelity objective with the geode gradient as its Jacobian model, rather than an inner optimization on the order-1 solves\"\n");
    let _ = writeln!(
        t,
        "c_620_to_650_remesh_fd_ghz_per_um = [{:.9}, {:.9}]",
        remesh_fd[0], remesh_fd[1]
    );
    let _ = writeln!(
        t,
        "c_620_to_650_hf_mean_ghz_per_um = [{:.9}, {:.9}]",
        hf_mean[0], hf_mean[1]
    );
    let _ = writeln!(
        t,
        "l_5000_to_5250_remesh_dfq_dl_ghz_per_um = {dfq_dl_remesh:.9}  # physical dressing shift about 1e-6"
    );
    t.push_str("\n[transmission_line]\n");
    t.push_str("model = \"f_r (L + delta) = const at fixed capacitor_length: delta from the two order-1 solves at L = 5000 and 5250 um (c = 620 um); d f_q / d L = 0\"\n");
    let _ = writeln!(t, "f_r_ghz = [{:.6}, {:.6}]", l0.f[1], l1.f[1]);
    let _ = writeln!(t, "delta_um = {delta:.3}");

    // --- Newton on DeviceLayout's discretization.
    let pal = repo.join(PALACE_OPT_DIR);
    let mut thetas = vec![INITIAL_UM];
    let mut big = vec![palace_pair(
        &repo.join(PALACE_XCHECK_DIR).join("dl_initial"),
    )];
    let mut cells = vec!["dl_initial (part 1)".to_string()];
    let mut pwall = vec![time_wall_s(
        &repo.join(PALACE_XCHECK_DIR).join("dl_initial/time.txt"),
    )];
    let mut geo = vec![rec(&runs, "dl0")];
    let mut k = 1;
    while runs.join(format!("dl{k}.toml")).exists() {
        let cell = pal.join(format!("dl_dl{k}"));
        thetas.push(rec(&runs, &format!("dl{k}")).theta);
        big.push(palace_pair(&cell));
        cells.push(format!("dl_dl{k}"));
        pwall.push(time_wall_s(&cell.join("time.txt")));
        geo.push(rec(&runs, &format!("dl{k}")));
        k += 1;
    }
    let n = thetas.len();
    for i in 0..n - 1 {
        assert_eq!(
            geo[i].theta, thetas[i],
            "geode record {i} at the step's design"
        );
        let next = newton_step(thetas[i], big[i], geo[i].f, geo[i].dfdc, delta);
        for j in 0..2 {
            assert!(
                (next[j] - thetas[i + 1][j]).abs() <= 1.5e-3,
                "step {i} -> {}: re-derived {next:?} vs recorded {:?}",
                i + 1,
                thetas[i + 1]
            );
        }
    }
    let errs: Vec<f64> = big.iter().map(|&f| dl_error(f)).collect();
    let met = errs
        .iter()
        .position(|&e| e < DL_RELTOL * DL_RELTOL)
        .expect("criterion met");
    assert!(
        errs[n - 1] < NEWTON_ERR_STOP,
        "the loop ran to its own stop"
    );
    let col = |f: &dyn Fn(usize) -> f64, prec: usize| arr(&(0..n).map(f).collect::<Vec<_>>(), prec);
    t.push_str("\n[newton_devicelayout]\n");
    t.push_str("objective = \"DeviceLayout's discretization: Palace on its mesh_order 2 mesh at solver_order 2 with its configfile verbatim (absorbing far field, 50 ohm readout ports, junction L || C, sapphire tensor with loss tangent); the two lowest modes\"\n");
    t.push_str("jacobian = \"c column: geode's Hellmann-Feynman d f_h / d c on the order-1 mesh of the same design, scaled by S = F / f_h; L column: the transmission line. Step 0 reuses the part-1 Palace run at the initial geometry\"\n");
    let _ = writeln!(t, "capacitor_length_um = {}", col(&|i| thetas[i][0], 3));
    let _ = writeln!(t, "total_length_um = {}", col(&|i| thetas[i][1], 3));
    let _ = writeln!(
        t,
        "x1 = {}  # sqrt(620 / c)",
        col(&|i| (620.0 / thetas[i][0]).sqrt(), 6)
    );
    let _ = writeln!(t, "x2 = {}  # 5000 / L", col(&|i| 5000.0 / thetas[i][1], 6));
    let _ = writeln!(t, "f_q_ghz = {}", col(&|i| big[i][0], 6));
    let _ = writeln!(t, "f_r_ghz = {}", col(&|i| big[i][1], 6));
    let _ = writeln!(
        t,
        "err = {}  # sum((f - t)^2 / t^2) / 2",
        arr(&errs.iter().map(|e| *e).collect::<Vec<_>>(), 9)
    );
    let _ = writeln!(t, "geode_f_h_q_ghz = {}", col(&|i| geo[i].f[0], 6));
    let _ = writeln!(t, "geode_f_h_r_ghz = {}", col(&|i| geo[i].f[1], 6));
    let _ = writeln!(
        t,
        "geode_df_dc_q_ghz_per_um = {}",
        col(&|i| geo[i].dfdc[0], 9)
    );
    let _ = writeln!(
        t,
        "geode_df_dc_r_ghz_per_um = {}",
        col(&|i| geo[i].dfdc[1], 9)
    );
    let _ = writeln!(
        t,
        "palace_cells = [{}]",
        cells
            .iter()
            .map(|c| format!("\"{c}\""))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let _ = writeln!(t, "palace_wall_s = {}", arr(&pwall, 1));
    let _ = writeln!(t, "geode_wall_s = {}", col(&|i| geo[i].wall_s, 1));
    let _ = writeln!(
        t,
        "devicelayout_criterion = {:.0e}  # reltol^2 of run_optimization",
        DL_RELTOL * DL_RELTOL
    );
    let _ = writeln!(t, "criterion_met_at_step = {met}");
    let _ = writeln!(
        t,
        "palace_runs_at_criterion = {}  # including the initial-geometry run, as DeviceLayout counts",
        met + 1
    );
    let _ = writeln!(
        t,
        "geode_solves_at_criterion = {}  # one order-1 solve + gradient per Newton point",
        met + 1
    );
    let _ = writeln!(
        t,
        "palace_runs_total = {n}  # to err < {NEWTON_ERR_STOP:.0e}"
    );
    let _ = writeln!(t, "prima_palace_runs = {PRIMA_PALACE_RUNS}");
    let at = thetas[met];
    let last = thetas[n - 1];
    let _ = writeln!(
        t,
        "published_final_um = [{}, {}]",
        PUBLISHED_FINAL_UM[0], PUBLISHED_FINAL_UM[1]
    );
    let _ = writeln!(
        t,
        "published_final_f_ghz = [{}, {}]  # DeviceLayout's own Palace run",
        PUBLISHED_FINAL_GHZ[0], PUBLISHED_FINAL_GHZ[1]
    );
    let dlf = palace_pair(&repo.join(PALACE_XCHECK_DIR).join("dl_final"));
    let _ = writeln!(
        t,
        "this_palace_at_published_final_ghz = [{:.6}, {:.6}]  # part-1 dl_final: same discretization, this Palace build",
        dlf[0], dlf[1]
    );
    let _ = writeln!(
        t,
        "this_palace_at_published_final_err = {:.3e}",
        dl_error(dlf)
    );
    let _ = writeln!(
        t,
        "at_criterion_minus_published_um = [{:.3}, {:.3}]",
        at[0] - PUBLISHED_FINAL_UM[0],
        at[1] - PUBLISHED_FINAL_UM[1]
    );
    let _ = writeln!(
        t,
        "at_criterion_over_published_pct = [{:.3}, {:.3}]",
        pct(at[0], PUBLISHED_FINAL_UM[0]),
        pct(at[1], PUBLISHED_FINAL_UM[1])
    );
    let _ = writeln!(
        t,
        "optimum_minus_published_um = [{:.3}, {:.3}]",
        last[0] - PUBLISHED_FINAL_UM[0],
        last[1] - PUBLISHED_FINAL_UM[1]
    );
    let _ = writeln!(
        t,
        "optimum_over_published_pct = [{:.3}, {:.3}]",
        pct(last[0], PUBLISHED_FINAL_UM[0]),
        pct(last[1], PUBLISHED_FINAL_UM[1])
    );
    // The c offset the Palace-build difference at the published final explains:
    // (F_this - F_published) / (dF/dc) with dF/dc = S * HF at the optimum.
    let s_last = big[n - 1][0] / geo[n - 1].f[0];
    let dc_build = (dlf[0] - PUBLISHED_FINAL_GHZ[0]) / (-(s_last * geo[n - 1].dfdc[0]));
    let _ = writeln!(
        t,
        "palace_build_offset_explains_dc_um = {dc_build:.1}  # (this Palace - published) at the published final, over S d f_h / d c at the optimum"
    );

    let _ = fs::create_dir_all(out_dir);
    write_conv(&mut t, repo, out_dir, delta);
    let path = out_dir.join("results.toml");
    fs::write(&path, t).expect("write results");
    println!("wrote {}", path.display());
}

/// The converged-target step(s) and their verification ladders.
fn write_conv(t: &mut String, repo: &Path, out_dir: &Path, delta: f64) {
    let runs = out_dir.join("runs");
    let x = toml_doc(&repo.join("benchmarks/transmon_devicelayout_crosscheck/results.toml"));
    let mut from = PUBLISHED_FINAL_UM;
    let mut f_conv = [
        tf(&x, "convergence.final.f_q_ghz"),
        tf(&x, "convergence.final.f_r_ghz"),
    ];
    let mut step = 1;
    while runs.join(format!("cv{step}_at.toml")).exists() {
        let g = rec(&runs, &format!("cv{step}_at"));
        assert_eq!(
            g.theta, from,
            "cv{step}: gradient solve at the step's start"
        );
        let to = newton_step(from, f_conv, g.f, g.dfdc, delta);
        let tag = format!("cv{step}");
        let lad = ladder(repo, out_dir, &tag);
        let _ = writeln!(t, "\n[conv.{tag}]");
        t.push_str("objective = \"the converged eigenfrequencies (PEC box, the part-1 rule: order-1 and order-2 Palace series extrapolated at the asymptotic order 1)\"\n");
        let _ = writeln!(t, "from_um = [{:.3}, {:.3}]", from[0], from[1]);
        let _ = writeln!(
            t,
            "from_converged_ghz = [{:.4}, {:.4}]",
            f_conv[0], f_conv[1]
        );
        let _ = writeln!(t, "from_geode_f_h_ghz = [{:.6}, {:.6}]", g.f[0], g.f[1]);
        let _ = writeln!(
            t,
            "from_geode_df_dc_ghz_per_um = [{:.9}, {:.9}]",
            g.dfdc[0], g.dfdc[1]
        );
        let _ = writeln!(t, "to_um = [{:.3}, {:.3}]", to[0], to[1]);
        let _ = writeln!(
            t,
            "to_minus_published_final_um = [{:.3}, {:.3}]",
            to[0] - PUBLISHED_FINAL_UM[0],
            to[1] - PUBLISHED_FINAL_UM[1]
        );
        lad.write(t, &tag, to);
        // The next step starts from the estimate as written (4 decimals),
        // as step 1 starts from the part-1 artifact's 4-decimal values.
        let r4 = |v: f64| (v * 1e4).round() / 1e4;
        f_conv = [r4(lad.f_q), r4(lad.f_r)];
        from = to;
        step += 1;
    }
}

struct Ladder {
    p1q: Vec<f64>,
    p1r: Vec<f64>,
    p2q: Vec<f64>,
    p2r: Vec<f64>,
    geode: Vec<[f64; 2]>,
    f_q: f64,
    f_r: f64,
    band_q: [f64; 2],
    band_r: [f64; 2],
    walls: Vec<f64>,
}

fn ladder(repo: &Path, out_dir: &Path, tag: &str) -> Ladder {
    let pal = repo.join(PALACE_OPT_DIR);
    let cell = |s: &str| pal.join(format!("{tag}_{s}"));
    let p1: Vec<[f64; 2]> = ["h_p1", "r1_p1", "r2_p1"]
        .iter()
        .map(|s| palace_pair(&cell(s)))
        .collect();
    let p2: Vec<[f64; 2]> = ["h_p2", "r1_p2"]
        .iter()
        .map(|s| palace_pair(&cell(s)))
        .collect();
    let walls: Vec<f64> = ["h_p1", "r1_p1", "r2_p1", "h_p2", "r1_p2"]
        .iter()
        .map(|s| time_wall_s(&cell(s).join("time.txt")))
        .collect();
    let gruns = out_dir.join(format!("ladder_{tag}/geode_runs"));
    let geode: Vec<[f64; 2]> = ["h", "h2"]
        .iter()
        .map(|s| {
            let d = toml_doc(&gruns.join(format!("{tag}_{s}.toml")));
            let f = tarr(&d, "run.f_ghz");
            [f[0], f[1]]
        })
        .collect();
    let p1q: Vec<f64> = p1.iter().map(|v| v[0]).collect();
    let p1r: Vec<f64> = p1.iter().map(|v| v[1]).collect();
    let p2q: Vec<f64> = p2.iter().map(|v| v[0]).collect();
    let p2r: Vec<f64> = p2.iter().map(|v| v[1]).collect();
    let obs = |f: &[f64]| {
        let r = (f[0] - f[1]) / (f[1] - f[2]);
        assert!(r > 1.0, "{tag}: not contracting {f:?}");
        let p = r.log2();
        (p, f[2] + (f[2] - f[1]) / (2f64.powf(p) - 1.0))
    };
    let (oq, q_obs) = obs(&p1q);
    let (or_, r_obs) = obs(&p1r);
    let q2_obs = p2q[1] + (p2q[1] - p2q[0]) / (2f64.powf(oq) - 1.0);
    let r2_obs = p2r[1] + (p2r[1] - p2r[0]) / (2f64.powf(or_) - 1.0);
    let q1 = 2.0 * p1q[2] - p1q[1];
    let r1 = 2.0 * p1r[2] - p1r[1];
    let q2 = 2.0 * p2q[1] - p2q[0];
    let r2 = 2.0 * p2r[1] - p2r[0];
    let lo = |v: &[f64]| v.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = |v: &[f64]| v.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let all_q = [q1, q2, q_obs, q2_obs];
    let all_r = [r1, r2, r_obs, r2_obs];
    Ladder {
        f_q: 0.5 * (q1 + q2),
        f_r: 0.5 * (r1 + r2),
        band_q: [lo(&all_q), hi(&all_q)],
        band_r: [lo(&all_r), hi(&all_r)],
        p1q,
        p1r,
        p2q,
        p2r,
        geode,
        walls,
    }
}

impl Ladder {
    fn write(&self, t: &mut String, tag: &str, at: [f64; 2]) {
        let _ = writeln!(t, "\n[conv.{tag}.ladder]");
        let _ = writeln!(t, "design_um = [{:.3}, {:.3}]", at[0], at[1]);
        let _ = writeln!(t, "p1_f_q_ghz = {}  # h, h/2, h/4", arr(&self.p1q, 6));
        let _ = writeln!(t, "p1_f_r_ghz = {}", arr(&self.p1r, 6));
        let _ = writeln!(t, "p2_f_q_ghz = {}  # h, h/2", arr(&self.p2q, 6));
        let _ = writeln!(t, "p2_f_r_ghz = {}", arr(&self.p2r, 6));
        let _ = writeln!(
            t,
            "geode_f_q_ghz = [{:.6}, {:.6}]  # h, h/2",
            self.geode[0][0], self.geode[1][0]
        );
        let _ = writeln!(
            t,
            "geode_f_r_ghz = [{:.6}, {:.6}]",
            self.geode[0][1], self.geode[1][1]
        );
        let same: Vec<f64> = (0..2)
            .flat_map(|i| {
                [
                    pct(self.geode[i][0], [self.p1q[0], self.p1q[1]][i]),
                    pct(self.geode[i][1], [self.p1r[0], self.p1r[1]][i]),
                ]
            })
            .collect();
        let _ = writeln!(
            t,
            "geode_over_palace_pct = {}  # q h, r h, q h/2, r h/2",
            arr(&same, 4)
        );
        let _ = writeln!(
            t,
            "palace_wall_s = {}  # h_p1, r1_p1, r2_p1, h_p2, r1_p2",
            arr(&self.walls, 1)
        );
        let _ = writeln!(t, "f_q_ghz = {:.4}", self.f_q);
        let _ = writeln!(
            t,
            "f_q_band_ghz = [{:.4}, {:.4}]",
            self.band_q[0], self.band_q[1]
        );
        let _ = writeln!(t, "f_r_ghz = {:.4}", self.f_r);
        let _ = writeln!(
            t,
            "f_r_band_ghz = [{:.4}, {:.4}]",
            self.band_r[0], self.band_r[1]
        );
        let _ = writeln!(
            t,
            "over_target_pct = [{:.3}, {:.3}]",
            pct(self.f_q, TARGETS_GHZ[0]),
            pct(self.f_r, TARGETS_GHZ[1])
        );
        let _ = writeln!(
            t,
            "err = {:.3e}  # DeviceLayout's measure on the converged pair",
            dl_error([self.f_q, self.f_r])
        );
        let _ = writeln!(
            t,
            "targets_in_band = [{}, {}]",
            self.band_q[0] <= TARGETS_GHZ[0] && TARGETS_GHZ[0] <= self.band_q[1],
            self.band_r[0] <= TARGETS_GHZ[1] && TARGETS_GHZ[1] <= self.band_r[1]
        );
    }
}
