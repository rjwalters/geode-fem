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
        for c in 0..3 {
            p[c] += h * v[c];
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
        panic!("assemble mode lands with the optimization records");
    }
}
