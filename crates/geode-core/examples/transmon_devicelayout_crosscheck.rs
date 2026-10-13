//! DeviceLayout.jl SingleTransmon cross-check: converged qubit and resonator
//! eigenfrequencies of the published **initial** geometry (capacitor_length
//! 620 µm, resonator total_length 5000 µm, published `[4.14, 5.591]` GHz)
//! and a forward check of the published **final** geometry (1217.678 µm,
//! 7243.955 µm, published `[3.005, 3.982]` GHz), from geode's A2 eigen path
//! and from Palace, both with the sapphire tensor (issue #1092).
//!
//! The meshes come from `reference/julia/generate_transmon_param.jl`, which
//! drives DeviceLayout's own `single_transmon()` builder with the two
//! parameters the blog optimizes. Each geometry has a resolution ladder:
//! the DeviceLayout mesh at first order (`h`), one and two uniform gmsh
//! refinements (`h/2`, `h/4`), Palace at second order on `h` and `h/2`, and
//! two Palace runs on DeviceLayout's own second-order mesh: the blog's exact
//! discretization (absorbing far field, 50 Ω readout ports, mesh_order 2,
//! solver_order 2) and the same mesh with the PEC-box model every other cell
//! uses, which isolates the boundary model from the resolution.
//!
//! # Two modes
//!
//! **Solve** (on the worker): one A2 tensor-ε eigensolve of a mesh file,
//! written as a small TOML record.
//!
//! ```sh
//! GEODE_SOLVE_MESH=/work/initial_o1.msh GEODE_SOLVE_LABEL=initial_h \
//! GEODE_SIGMA_GHZ=5.0 GEODE_NMODES=3 GEODE_RUNS_DIR=/work/geode_runs \
//!     cargo run -p geode-core --release --example transmon_devicelayout_crosscheck
//! ```
//!
//! **Assemble** (default, no solve): read the committed geode run records
//! (`benchmarks/transmon_devicelayout_crosscheck/geode_runs/*.toml`) and
//! the committed Palace cells
//! (`reference/fixtures/transmon_palace/results_crosscheck_1092/<cell>/`),
//! build the ladders, extrapolate, compare with the published values and
//! write `benchmarks/transmon_devicelayout_crosscheck/results.toml`
//! (override the directory with `$TRANSMON_DL_BENCH_DIR`). Checked by
//! `tests/transmon_devicelayout_crosscheck.rs`.

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
use geode_core::mesh::{TetMesh, TransmonFixture, read_transmon_fixture_from_bytes};
use geode_core::testing::TestBackend;
use geode_util::fixture::BackendInfo;

type B = TestBackend;

/// Metres per mesh unit (micrometre mesh).
const M_PER_UNIT: f64 = 1e-6;

/// The published values (AWS DeviceLayout.jl blog, SingleTransmon example).
const PUBLISHED: [(&str, f64, f64, f64, f64); 2] = [
    ("initial", 620.0, 5000.0, 4.14, 5.591),
    ("final", 1217.678, 7243.955, 3.005, 3.982),
];

const PALACE_DIR: &str = "reference/fixtures/transmon_palace/results_crosscheck_1092";

/// Palace modes with a port EPR below this are the port-sheet artifact
/// (#1045); they are dropped before the modes are ordered.
const PORT_SHEET_ARTIFACT_MAX_P: f64 = 1e-6;

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

/// Peak resident set size (GB) from `/proc/self/status` (Linux only).
fn peak_rss_gb() -> Option<f64> {
    let s = fs::read_to_string("/proc/self/status").ok()?;
    let line = s.lines().find(|l| l.starts_with("VmHWM:"))?;
    let kb: f64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kb / 1e6)
}

fn host_string() -> String {
    std::env::var("GEODE_HOST_NOTE").unwrap_or_else(|_| {
        let n = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(0);
        format!("{} {} cores", std::env::consts::ARCH, n)
    })
}

// ---------------------------------------------------------------------------
// Solve mode.
// ---------------------------------------------------------------------------

fn solve_mode(mesh_path: &Path, label: &str, sigma_ghz: f64, n_modes: usize, runs_dir: &Path) {
    let t_all = Instant::now();
    let bytes = fs::read(mesh_path).unwrap_or_else(|e| panic!("{}: {e}", mesh_path.display()));
    let fx: TransmonFixture = read_transmon_fixture_from_bytes(&bytes).expect("transmon mesh");
    let n_tets = fx.mesh.n_tets();
    let n_nodes = fx.mesh.nodes.len();
    println!(
        "{label}: {} ({n_nodes} nodes, {n_tets} tets)",
        mesh_path.display()
    );

    let t_asm = Instant::now();
    let edges = fx.mesh.edges();
    let (tet_edge_idx, tet_edge_sign) = edge_tables(&fx.mesh);
    let eps = fx.epsilon_tensor_r();
    let scatter = NedelecScatterMap::new(&tet_edge_idx);
    let (nodes_t, tets_t) = upload_mesh::<B>(&fx.mesh, &device());
    let identity: [[c64; 3]; 3] = std::array::from_fn(|i| {
        std::array::from_fn(|j| c64::new(if i == j { 1.0 } else { 0.0 }, 0.0))
    });
    let nu = vec![identity; n_tets];
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
    let k_vals = vals_to_host(sys.k_re_vals);
    let m_vals = vals_to_host(sys.m_re_vals);
    let assembly_s = t_asm.elapsed().as_secs_f64();
    println!("  assembled {} edges in {assembly_s:.1} s", edges.len());

    let t_solve = Instant::now();
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
        mesh: &fx.mesh,
        shunt: LumpedReactiveShunt {
            faces: &jport.faces,
            length: jport.length,
            width: jport.width,
            element,
        },
        interior_mask: &mask,
    };
    let sigma = lambda_shift_for_frequency_hz(sigma_ghz * 1e9, M_PER_UNIT);
    let (mut modes, diag) = solve_transmon_eigenmodes_projected_with_port_model(
        &pencil,
        JunctionPortModel::PortVoltage { e_hat: jport.e_hat },
        sigma,
        n_modes,
        M_PER_UNIT,
    )
    .expect("A2 bulk-projected eigensolve");
    let solve_s = t_solve.elapsed().as_secs_f64();
    assert_eq!(modes.len(), n_modes);
    let max_res = diag
        .mode_residual_rels
        .iter()
        .copied()
        .fold(0.0_f64, f64::max);
    assert!(
        max_res <= 1e-6,
        "{label}: residuals {:?}",
        diag.mode_residual_rels
    );
    modes.sort_by(|a, b| a.frequency_hz.total_cmp(&b.frequency_hz));
    for m in &modes {
        println!(
            "  f = {:.6} GHz  p = {:.6}",
            m.frequency_ghz(),
            m.participation
        );
    }
    let rss = peak_rss_gb();
    println!(
        "  solve {solve_s:.1} s, peak RSS {} GB, total {:.1} s",
        rss.map_or_else(|| "n/a".to_string(), |r| format!("{r:.2}")),
        t_all.elapsed().as_secs_f64()
    );

    let backend = BackendInfo::of::<B>(&device());
    let mut t = String::new();
    let _ = writeln!(
        t,
        "# geode A2 tensor-eps eigensolve record (issue #1092), written by"
    );
    let _ = writeln!(
        t,
        "# examples/transmon_devicelayout_crosscheck.rs in solve mode. Do NOT edit."
    );
    let _ = writeln!(t, "\n[meta]");
    let _ = writeln!(t, "label = \"{label}\"");
    let _ = writeln!(
        t,
        "mesh_file = \"{}\"",
        mesh_path.file_name().unwrap().to_string_lossy()
    );
    let _ = writeln!(t, "host = \"{}\"", host_string());
    let _ = writeln!(t, "backend = \"{}\"", backend.name);
    let _ = writeln!(t, "float_dtype = \"{}\"", backend.float_dtype);
    let _ = writeln!(t, "generated_at_commit = \"{}\"", git_head());
    let _ = writeln!(t, "\n[run]");
    let _ = writeln!(t, "n_nodes = {n_nodes}");
    let _ = writeln!(t, "n_tets = {n_tets}");
    let _ = writeln!(t, "n_edges = {}", edges.len());
    let _ = writeln!(
        t,
        "n_interior_edges = {}",
        mask.iter().filter(|&&m| m).count()
    );
    let _ = writeln!(t, "sigma_ghz = {sigma_ghz}");
    let _ = writeln!(t, "n_modes = {n_modes}");
    let _ = writeln!(t, "port_faces = {}", jport.faces.len());
    let _ = writeln!(t, "port_length_um = {:.6}", jport.length);
    let _ = writeln!(t, "port_width_um = {:.6}", jport.width);
    let _ = writeln!(t, "l_j_nh = {:.3}", JUNCTION_INDUCTANCE_H * 1e9);
    let _ = writeln!(t, "c_j_ff = {:.3}", JUNCTION_CAPACITANCE_F * 1e15);
    let _ = writeln!(
        t,
        "f_ghz = [{}]",
        modes
            .iter()
            .map(|m| format!("{:.6}", m.frequency_ghz()))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let _ = writeln!(
        t,
        "p = [{}]",
        modes
            .iter()
            .map(|m| format!("{:.6}", m.participation))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let _ = writeln!(t, "max_residual_rel = {max_res:.3e}");
    let _ = writeln!(t, "lanczos_iterations = {}", diag.iterations);
    let _ = writeln!(t, "assembly_s = {assembly_s:.1}");
    let _ = writeln!(t, "solve_s = {solve_s:.1}");
    let _ = writeln!(t, "wall_s = {:.1}", t_all.elapsed().as_secs_f64());
    match rss {
        Some(r) => {
            let _ = writeln!(t, "peak_rss_gb = {r:.2}");
        }
        None => {
            let _ = writeln!(t, "peak_rss_gb = -1.0  # not available on this host");
        }
    }
    fs::create_dir_all(runs_dir).expect("runs dir");
    let path = runs_dir.join(format!("{label}.toml"));
    fs::write(&path, t).expect("write run record");
    println!("wrote {}", path.display());
}

// ---------------------------------------------------------------------------
// Assemble mode: readers.
// ---------------------------------------------------------------------------

fn git_head() -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "--short=8", "HEAD"])
        .current_dir(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
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

fn ti(v: &toml::Value, path: &str) -> i64 {
    tget(v, path)
        .as_integer()
        .unwrap_or_else(|| panic!("{path} is not an integer"))
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

/// `" H1 (p = 1): 29705, ND (p = 1): 183891, ..."` → the ND count.
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

/// Wall seconds from a GNU `time -v` record.
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

/// Palace's estimated peak memory (GB) from the run log, if printed.
fn palace_peak_gb(log: &Path) -> f64 {
    let text = fs::read_to_string(log).unwrap_or_else(|e| panic!("{}: {e}", log.display()));
    let line = match text
        .lines()
        .filter(|l| l.contains("Estimated peak per-node memory usage"))
        .next_back()
    {
        Some(l) => l,
        None => return -1.0,
    };
    let after = &line[line.find("Total ").unwrap() + 6..];
    let num: String = after
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    let v: f64 = num.parse().unwrap();
    match after.chars().nth(num.len()) {
        Some('G') => v,
        Some('M') => v / 1e3,
        Some('K') => v / 1e6,
        _ => v / 1e9,
    }
}

/// One cell of the ladder: a geode run record or a Palace output directory.
#[derive(Clone, Debug)]
struct Cell {
    name: String,
    /// The two lowest physical modes (the qubit-like and resonator-like).
    f_q: f64,
    f_r: f64,
    p_q: f64,
    p_r: f64,
    n_unknowns: usize,
    wall_s: f64,
    peak_gb: f64,
}

fn geode_cell(runs: &Path, label: &str) -> Cell {
    let d = toml_doc(&runs.join(format!("{label}.toml")));
    let f = tarr(&d, "run.f_ghz");
    let p = tarr(&d, "run.p");
    assert!(f.len() >= 2, "{label}: {f:?}");
    assert_eq!(
        tget(&d, "meta.float_dtype").as_str(),
        Some("F64"),
        "{label}: f64 record"
    );
    Cell {
        name: label.to_string(),
        f_q: f[0],
        f_r: f[1],
        p_q: p[0],
        p_r: p[1],
        n_unknowns: ti(&d, "run.n_interior_edges") as usize,
        wall_s: tf(&d, "run.wall_s"),
        peak_gb: tf(&d, "run.peak_rss_gb"),
    }
}

fn palace_cell(repo: &Path, name: &str) -> Cell {
    let d = repo.join(PALACE_DIR).join(name);
    let f_all = csv_col(&d.join("eig.csv"), 1);
    let epr = d.join("port-EPR.csv");
    // The DeviceLayout-faithful cells have three ports; the junction is the
    // last column. The PEC-box cells have one.
    let text = fs::read_to_string(&epr).unwrap_or_else(|e| panic!("{}: {e}", epr.display()));
    let ncol = text.lines().next().unwrap().split(',').count();
    let p_all = csv_col(&epr, ncol - 1);
    assert_eq!(f_all.len(), p_all.len(), "{name}");
    let (f, p): (Vec<f64>, Vec<f64>) = f_all
        .iter()
        .zip(&p_all)
        .filter(|(_, p)| p.abs() >= PORT_SHEET_ARTIFACT_MAX_P)
        .map(|(&f, &p)| (f, p.abs()))
        .unzip();
    assert!(f.len() >= 2, "{name}: {f_all:?} {p_all:?}");
    assert!(f[0] < f[1], "{name}: {f:?}");
    Cell {
        name: name.to_string(),
        f_q: f[0],
        f_r: f[1],
        p_q: p[0],
        p_r: p[1],
        n_unknowns: palace_nd(&d.join("palace_run.log")),
        wall_s: time_wall_s(&d.join("time.txt")),
        peak_gb: palace_peak_gb(&d.join("palace_run.log")),
    }
}

// ---------------------------------------------------------------------------
// Assemble mode: ladders.
// ---------------------------------------------------------------------------

/// Richardson extrapolation of a three-level sequence `(h, h/2, h/4)`:
/// the observed order `p = log2((f0 − f1)/(f1 − f2))` and
/// `f∞ = f2 + (f2 − f1)/(2^p − 1)`.
fn richardson3(f: [f64; 3]) -> (f64, f64) {
    let r = (f[0] - f[1]) / (f[1] - f[2]);
    assert!(r > 1.0, "not monotone contracting: {f:?}");
    let p = r.log2();
    (p, f[2] + (f[2] - f[1]) / (2f64.powf(p) - 1.0))
}

/// Two-level extrapolation at an assumed order.
fn richardson2(f: [f64; 2], p: f64) -> f64 {
    f[1] + (f[1] - f[0]) / (2f64.powf(p) - 1.0)
}

fn pct(x: f64, r: f64) -> f64 {
    100.0 * (x / r - 1.0)
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

fn arri(v: &[usize]) -> String {
    format!(
        "[{}]",
        v.iter()
            .map(|x| x.to_string())
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

fn names(v: &[Cell]) -> Vec<&str> {
    v.iter().map(|c| c.name.as_str()).collect()
}

/// Everything the assemble mode derives for one geometry.
struct Ladder {
    tag: &'static str,
    geode: Vec<Cell>,
    palace_p1: Vec<Cell>,
    palace_p2: Vec<Cell>,
    dl: Cell,
    dlpec: Cell,
}

fn write_ladder(t: &mut String, l: &Ladder, pub_q: f64, pub_r: f64) {
    let tag = l.tag;
    let fq = |v: &[Cell]| v.iter().map(|c| c.f_q).collect::<Vec<_>>();
    let fr = |v: &[Cell]| v.iter().map(|c| c.f_r).collect::<Vec<_>>();
    let pq = |v: &[Cell]| v.iter().map(|c| c.p_q).collect::<Vec<_>>();
    let pr = |v: &[Cell]| v.iter().map(|c| c.p_r).collect::<Vec<_>>();
    let nu = |v: &[Cell]| v.iter().map(|c| c.n_unknowns).collect::<Vec<_>>();
    let ws = |v: &[Cell]| v.iter().map(|c| c.wall_s).collect::<Vec<_>>();
    let gb = |v: &[Cell]| v.iter().map(|c| c.peak_gb).collect::<Vec<_>>();

    // geode.
    let _ = writeln!(t, "\n[geode.{tag}]");
    let _ = writeln!(t, "labels = {}", strs(&names(&l.geode)));
    let _ = writeln!(t, "n_interior_edges = {}", arri(&nu(&l.geode)));
    let _ = writeln!(t, "f_q_ghz = {}", arr(&fq(&l.geode), 6));
    let _ = writeln!(t, "f_r_ghz = {}", arr(&fr(&l.geode), 6));
    let _ = writeln!(t, "p_q = {}", arr(&pq(&l.geode), 6));
    let _ = writeln!(t, "p_r = {}", arr(&pr(&l.geode), 6));
    let _ = writeln!(t, "wall_s = {}", arr(&ws(&l.geode), 1));
    let _ = writeln!(t, "peak_rss_gb = {}", arr(&gb(&l.geode), 2));

    // Palace, order 1 and 2.
    let _ = writeln!(t, "\n[palace.{tag}]");
    let _ = writeln!(t, "p1_cells = {}", strs(&names(&l.palace_p1)));
    let _ = writeln!(t, "p1_nd = {}", arri(&nu(&l.palace_p1)));
    let _ = writeln!(t, "p1_f_q_ghz = {}", arr(&fq(&l.palace_p1), 6));
    let _ = writeln!(t, "p1_f_r_ghz = {}", arr(&fr(&l.palace_p1), 6));
    let _ = writeln!(t, "p1_p_q = {}", arr(&pq(&l.palace_p1), 6));
    let _ = writeln!(t, "p1_wall_s = {}", arr(&ws(&l.palace_p1), 1));
    let _ = writeln!(t, "p1_peak_gb = {}", arr(&gb(&l.palace_p1), 2));
    let _ = writeln!(t, "p2_cells = {}", strs(&names(&l.palace_p2)));
    let _ = writeln!(t, "p2_nd = {}", arri(&nu(&l.palace_p2)));
    let _ = writeln!(t, "p2_f_q_ghz = {}", arr(&fq(&l.palace_p2), 6));
    let _ = writeln!(t, "p2_f_r_ghz = {}", arr(&fr(&l.palace_p2), 6));
    let _ = writeln!(t, "p2_p_q = {}", arr(&pq(&l.palace_p2), 6));
    let _ = writeln!(t, "p2_wall_s = {}", arr(&ws(&l.palace_p2), 1));
    let _ = writeln!(t, "p2_peak_gb = {}", arr(&gb(&l.palace_p2), 2));

    // Same-mesh geode vs Palace (order 1).
    let same: Vec<f64> = l
        .geode
        .iter()
        .zip(&l.palace_p1)
        .map(|(g, p)| pct(g.f_q, p.f_q))
        .collect();
    let same_r: Vec<f64> = l
        .geode
        .iter()
        .zip(&l.palace_p1)
        .map(|(g, p)| pct(g.f_r, p.f_r))
        .collect();
    let _ = writeln!(t, "\n[same_mesh.{tag}]");
    let _ = writeln!(t, "levels = {}", strs(&names(&l.geode)));
    let _ = writeln!(t, "geode_over_palace_f_q_pct = {}", arr(&same, 4));
    let _ = writeln!(t, "geode_over_palace_f_r_pct = {}", arr(&same_r, 4));
    let _ = writeln!(
        t,
        "note = \"on the DeviceLayout h meshes Palace adds elements by local bisection of under-resolved interior boundaries (its log: 4946 initial, 5944 final), so its h mesh is slightly finer than geode's and the resonator differs by about 0.03 %; the refined meshes need no bisection and both codes solve the same discrete problem (0.0000 %). Rerunning the initial h cell with AMS instead of SuperLU gives the same 5.176055 GHz\""
    );

    // DeviceLayout's own discretization and the same-mesh PEC-box control.
    let _ = writeln!(t, "\n[devicelayout_run.{tag}]");
    let _ = writeln!(t, "cell = \"{}\"", l.dl.name);
    let _ = writeln!(
        t,
        "model = \"DeviceLayout's configfile: mesh_order 2, solver_order 2, first-order absorbing far field, two 50 ohm readout lumped ports, the junction L || C, sapphire tensor with loss tangent\""
    );
    let _ = writeln!(t, "nd = {}", l.dl.n_unknowns);
    let _ = writeln!(t, "f_q_ghz = {:.6}", l.dl.f_q);
    let _ = writeln!(t, "f_r_ghz = {:.6}", l.dl.f_r);
    let _ = writeln!(t, "p_q = {:.6}", l.dl.p_q);
    let _ = writeln!(t, "wall_s = {:.1}", l.dl.wall_s);
    let _ = writeln!(t, "published_f_q_ghz = {pub_q}");
    let _ = writeln!(t, "published_f_r_ghz = {pub_r}");
    let _ = writeln!(t, "f_q_over_published_pct = {:.4}", pct(l.dl.f_q, pub_q));
    let _ = writeln!(t, "f_r_over_published_pct = {:.4}", pct(l.dl.f_r, pub_r));
    let _ = writeln!(t, "pec_box_cell = \"{}\"", l.dlpec.name);
    let _ = writeln!(
        t,
        "pec_box_model = \"the same second-order mesh and solver order with the PEC-box model of every other cell: exterior boundary PEC, readout ports omitted\""
    );
    let _ = writeln!(t, "pec_box_f_q_ghz = {:.6}", l.dlpec.f_q);
    let _ = writeln!(t, "pec_box_f_r_ghz = {:.6}", l.dlpec.f_r);
    let _ = writeln!(
        t,
        "boundary_model_f_q_pct = {:.4}  # DeviceLayout model over PEC box, same mesh and order",
        pct(l.dl.f_q, l.dlpec.f_q)
    );
    let _ = writeln!(
        t,
        "boundary_model_f_r_pct = {:.4}",
        pct(l.dl.f_r, l.dlpec.f_r)
    );
    let p2h = &l.palace_p2[0];
    let _ = writeln!(
        t,
        "mesh_order_f_q_pct = {:.4}  # PEC box on DeviceLayout's second-order mesh over PEC box on the first-order mesh, both solver order 2",
        pct(l.dlpec.f_q, p2h.f_q)
    );
    let _ = writeln!(t, "mesh_order_f_r_pct = {:.4}", pct(l.dlpec.f_r, p2h.f_r));

    // Convergence.
    let p1q: Vec<f64> = fq(&l.palace_p1);
    let p1r: Vec<f64> = fr(&l.palace_p1);
    assert_eq!(p1q.len(), 3, "{tag}: three order-1 Palace levels");
    let (oq, fq_obs) = richardson3([p1q[0], p1q[1], p1q[2]]);
    let (or_, fr_obs) = richardson3([p1r[0], p1r[1], p1r[2]]);
    let p2q = fq(&l.palace_p2);
    let p2r = fr(&l.palace_p2);
    assert_eq!(p2q.len(), 2, "{tag}: two order-2 Palace levels");
    // Every level rises under refinement (the conforming discretization
    // over-estimates the island capacitance, #1062), so each series
    // approaches from below. The thin-sheet edge singularity (E ~ r^-1/2)
    // limits the asymptotic eigenvalue rate to O(h) at any order on
    // uniform meshes; the order-1 ladder still shows a pre-asymptotic
    // 0.6. The two rates give the two ends of the band.
    let fq_p2_obs = richardson2([p2q[0], p2q[1]], oq);
    let fr_p2_obs = richardson2([p2r[0], p2r[1]], or_);
    let fq_p1_o1 = richardson2([p1q[1], p1q[2]], ASYMPTOTIC_ORDER);
    let fr_p1_o1 = richardson2([p1r[1], p1r[2]], ASYMPTOTIC_ORDER);
    let fq_p2_o1 = richardson2([p2q[0], p2q[1]], ASYMPTOTIC_ORDER);
    let fr_p2_o1 = richardson2([p2r[0], p2r[1]], ASYMPTOTIC_ORDER);
    // The same order-1 extrapolation of the eigenvalue variable λ ∝ f²
    // instead of f: sqrt(2 f₂² − f₁²), lower than the frequency one for a
    // rising series. It widens the band's low side (#1106 review).
    let lam1 = |f: [f64; 2]| (2.0 * f[1] * f[1] - f[0] * f[0]).sqrt();
    let fq_p1_lam = lam1([p1q[1], p1q[2]]);
    let fr_p1_lam = lam1([p1r[1], p1r[2]]);
    let fq_p2_lam = lam1([p2q[0], p2q[1]]);
    let fr_p2_lam = lam1([p2r[0], p2r[1]]);
    let all_q = [fq_p1_o1, fq_p2_o1, fq_obs, fq_p2_obs, fq_p1_lam, fq_p2_lam];
    let all_r = [fr_p1_o1, fr_p2_o1, fr_obs, fr_p2_obs, fr_p1_lam, fr_p2_lam];
    let lo = |v: &[f64]| v.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = |v: &[f64]| v.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let (q_lo, q_hi) = (lo(&all_q), hi(&all_q));
    let (r_lo, r_hi) = (lo(&all_r), hi(&all_r));
    let fq_est = 0.5 * (fq_p1_o1 + fq_p2_o1);
    let fr_est = 0.5 * (fr_p1_o1 + fr_p2_o1);
    let _ = writeln!(t, "\n[convergence.{tag}]");
    let _ = writeln!(
        t,
        "model = \"PEC box (exterior boundary PEC, readout ports omitted), junction L || C, sapphire tensor; first-order meshes h, h/2, h/4 are the DeviceLayout mesh and its uniform gmsh refinements\""
    );
    let _ = writeln!(t, "p1_levels = [\"h\", \"h/2\", \"h/4\"]");
    let _ = writeln!(t, "p1_f_q_ghz = {}", arr(&p1q, 6));
    let _ = writeln!(t, "p1_f_r_ghz = {}", arr(&p1r, 6));
    let _ = writeln!(t, "p1_observed_order_q = {oq:.4}");
    let _ = writeln!(t, "p1_observed_order_r = {or_:.4}");
    let _ = writeln!(
        t,
        "p1_richardson_f_q_ghz = {fq_obs:.6}  # three-level, at the observed order"
    );
    let _ = writeln!(t, "p1_richardson_f_r_ghz = {fr_obs:.6}");
    let _ = writeln!(
        t,
        "p1_order1_f_q_ghz = {fq_p1_o1:.6}  # h/2, h/4 at the asymptotic order 1"
    );
    let _ = writeln!(t, "p1_order1_f_r_ghz = {fr_p1_o1:.6}");
    let _ = writeln!(t, "p2_levels = [\"h\", \"h/2\"]");
    let _ = writeln!(t, "p2_f_q_ghz = {}", arr(&p2q, 6));
    let _ = writeln!(t, "p2_f_r_ghz = {}", arr(&p2r, 6));
    let _ = writeln!(
        t,
        "p2_richardson_f_q_ghz = {fq_p2_obs:.6}  # at the order-1 ladder's observed order"
    );
    let _ = writeln!(t, "p2_richardson_f_r_ghz = {fr_p2_obs:.6}");
    let _ = writeln!(
        t,
        "p2_order1_f_q_ghz = {fq_p2_o1:.6}  # at the asymptotic order 1"
    );
    let _ = writeln!(t, "p2_order1_f_r_ghz = {fr_p2_o1:.6}");
    let _ = writeln!(
        t,
        "asymptotic_order = {ASYMPTOTIC_ORDER:.1}  # thin-sheet edge singularity, E ~ r^-1/2: eigenvalue error O(h) on uniform meshes at any element order"
    );
    let _ = writeln!(
        t,
        "p1_order1_lambda_f_q_ghz = {fq_p1_lam:.6}  # order 1 in the eigenvalue variable: sqrt(2 f(h/4)^2 - f(h/2)^2)"
    );
    let _ = writeln!(t, "p1_order1_lambda_f_r_ghz = {fr_p1_lam:.6}");
    let _ = writeln!(
        t,
        "p2_order1_lambda_f_q_ghz = {fq_p2_lam:.6}  # order 1 in the eigenvalue variable: sqrt(2 f(h/2)^2 - f(h)^2)"
    );
    let _ = writeln!(t, "p2_order1_lambda_f_r_ghz = {fr_p2_lam:.6}");
    let _ = writeln!(
        t,
        "extrapolated_variable = \"the frequency f for the estimate (p1_order1_*, p2_order1_*, *_richardson_*); the eigenvalue variable lambda ~ f^2 at order 1 (*_order1_lambda_*) is also in the band and sets its low side\""
    );
    let _ = writeln!(
        t,
        "rule = \"every level rises under refinement, so each series approaches from below; estimate = mean of the order-1 and order-2 series, the frequency extrapolated at the asymptotic order 1; band = [min, max] over those two, the same two series in the eigenvalue variable at order 1 (lower), and the same two series in f at the order-1 ladder's observed (pre-asymptotic, lower) order (higher): an extrapolation with a band, not a bound\""
    );
    let _ = writeln!(
        t,
        "rule_history = \"#1092 part 1 replaced the work-in-progress rule (Richardson at the observed order only, band down to the finest measured levels, initial-qubit estimate 4.63 GHz) by this one, after the independent static route ([static_cross_check.initial]) put the initial qubit at 4.45 GHz and capped it at 4.477 GHz; the band's low side gained the eigenvalue-variable extrapolations in the #1106 review\""
    );
    let _ = writeln!(
        t,
        "band_upper_side = \"kept wide on purpose: the observed-order values are an extrapolation-only upper scenario and are not capped by the circuit model; for the initial geometry [static_cross_check.initial] gives the capped band\""
    );
    let _ = writeln!(t, "f_q_ghz = {fq_est:.4}");
    let _ = writeln!(t, "f_q_band_ghz = [{q_lo:.4}, {q_hi:.4}]");
    let _ = writeln!(t, "f_r_ghz = {fr_est:.4}");
    let _ = writeln!(t, "f_r_band_ghz = [{r_lo:.4}, {r_hi:.4}]");
    let _ = writeln!(t, "published_f_q_ghz = {pub_q}");
    let _ = writeln!(t, "published_f_r_ghz = {pub_r}");
    let _ = writeln!(t, "f_q_over_published_pct = {:.3}", pct(fq_est, pub_q));
    let _ = writeln!(t, "f_r_over_published_pct = {:.3}", pct(fr_est, pub_r));
    let _ = writeln!(
        t,
        "f_q_band_over_published_pct = [{:.3}, {:.3}]",
        pct(q_lo, pub_q),
        pct(q_hi, pub_q)
    );
    let _ = writeln!(
        t,
        "f_r_band_over_published_pct = [{:.3}, {:.3}]",
        pct(r_lo, pub_r),
        pct(r_hi, pub_r)
    );
    let _ = writeln!(
        t,
        "published_below_band = [{}, {}]",
        pub_q < q_lo,
        pub_r < r_lo
    );
    let _ = writeln!(
        t,
        "devicelayout_run_over_published_pct = [{:.3}, {:.3}]",
        pct(l.dl.f_q, pub_q),
        pct(l.dl.f_r, pub_r)
    );
    let _ = writeln!(
        t,
        "devicelayout_run_under_converged_pct = [{:.3}, {:.3}]  # DeviceLayout's discretization against the estimate",
        pct(l.dl.f_q, fq_est),
        pct(l.dl.f_r, fr_est)
    );
    let _ = writeln!(
        t,
        "h_over_converged_pct = [{:.3}, {:.3}]  # the DeviceLayout mesh at order 1 against the estimate",
        pct(p1q[0], fq_est),
        pct(p1r[0], fr_est)
    );
    let _ = writeln!(
        t,
        "geode_h2_over_converged_pct = [{:.3}, {:.3}]",
        pct(l.geode[1].f_q, fq_est),
        pct(l.geode[1].f_r, fr_est)
    );

    if tag == "initial" {
        write_static_check(t, fq_est, fr_est, q_lo, q_hi);
    }
}

/// Order of the asymptotic eigenvalue rate used for the lower band edge.
const ASYMPTOTIC_ORDER: f64 = 1.0;

/// The certified sapphire-tensor `C_Σ` of the initial geometry (#1091).
const C_SIGMA_BRACKET: &str = "benchmarks/transmon_diffopt/c_sigma_bracket_tensor.toml";

/// An independent static route to the initial geometry's qubit frequency:
/// the certified tensor `C_Σ` bracket of X⁰ (#1091; the same DeviceLayout
/// initial geometry, meshed by the earlier generator) read through the
/// two-node circuit of #1078/#1079 (`L_J + L_s`, `C_Σ + C_J`, coupling `k`
/// to a resonator at the converged estimate), over every corner of the
/// `C_Σ`, `L_s` and `k` bands.
fn write_static_check(t: &mut String, fq_est: f64, fr_est: f64, q_lo: f64, q_hi: f64) {
    use geode_core::quantum::transmon::{
        DEVICELAYOUT_COUPLING_K_BAND, DEVICELAYOUT_LOOP_INDUCTANCE_BAND_H,
        coupled_lc_dressed_pair_hz, harmonic_frequency_hz,
    };
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let b = toml_doc(&repo.join(C_SIGMA_BRACKET));
    let c_lo = tf(&b, "bracket_x0_tensor.lower_ff");
    let c_hi = tf(&b, "bracket_x0_tensor.upper_ff");
    let lc = |c_ff: f64, l_s: f64| {
        harmonic_frequency_hz(
            JUNCTION_INDUCTANCE_H + l_s,
            c_ff * 1e-15 + JUNCTION_CAPACITANCE_F,
        ) / 1e9
    };
    let lc_only = [lc(c_hi, 0.0), lc(c_lo, 0.0)];
    let mut dressed = Vec::new();
    for c in [c_lo, c_hi] {
        for l_s in [
            DEVICELAYOUT_LOOP_INDUCTANCE_BAND_H.0,
            DEVICELAYOUT_LOOP_INDUCTANCE_BAND_H.1,
        ] {
            for k in [
                DEVICELAYOUT_COUPLING_K_BAND.0,
                DEVICELAYOUT_COUPLING_K_BAND.1,
            ] {
                let (f_lo, _) = coupled_lc_dressed_pair_hz(lc(c, l_s) * 1e9, fr_est * 1e9, k);
                dressed.push(f_lo / 1e9);
            }
        }
    }
    let d_lo = dressed.iter().copied().fold(f64::INFINITY, f64::min);
    let d_hi = dressed.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let _ = writeln!(t, "\n[static_cross_check.initial]");
    let _ = writeln!(
        t,
        "source = \"{C_SIGMA_BRACKET} [bracket_x0_tensor]: certified sapphire-tensor C_Sigma of X0, the DeviceLayout initial geometry meshed by the earlier generator (crates/geode-core/tests/fixtures/transmon_smoke.msh)\""
    );
    let _ = writeln!(
        t,
        "c_sigma_bracket_ff = [{c_lo}, {c_hi}]  # RIGOROUS for that meshed geometry"
    );
    let _ = writeln!(
        t,
        "model = \"two-node Maxwell circuit (quantum::transmon::coupled_lc_dressed_pair_hz): bare qubit 1/(2 pi sqrt((L_J + L_s)(C_Sigma + C_J))), coupling k to a resonator at this geometry's converged f_r estimate; every corner of the C_Sigma, L_s (DEVICELAYOUT_LOOP_INDUCTANCE_BAND_H) and k (DEVICELAYOUT_COUPLING_K_BAND) bands. The circuit terms are estimates (#1078, #1079), so this is an estimate, not a bound\""
    );
    let _ = writeln!(
        t,
        "lc_only_f_q_band_ghz = [{:.4}, {:.4}]  # L_J alone, k = 0",
        lc_only[0], lc_only[1]
    );
    let _ = writeln!(t, "dressed_f_q_band_ghz = [{d_lo:.4}, {d_hi:.4}]");
    let _ = writeln!(
        t,
        "resonator_f_r_ghz = {fr_est:.4}  # the convergence estimate"
    );
    let _ = writeln!(t, "inside_eigen_band = {}", d_lo >= q_lo && d_hi <= q_hi);
    // Sign argument: the dressed qubit is the lower root, so k > 0 only
    // lowers it, and L_s >= 0 only lowers the bare frequency; with the
    // certified lower bound on C_Sigma the circuit caps the qubit at the
    // L_J-only value of c_lo.
    let cap = lc_only[1];
    let _ = writeln!(
        t,
        "circuit_cap_f_q_ghz = {cap:.4}  # 1/(2 pi sqrt(L_J (C_Sigma_lower + C_J))): k > 0 and L_s >= 0 only lower the qubit, so the circuit model caps it here"
    );
    let _ = writeln!(
        t,
        "f_q_band_capped_ghz = [{q_lo:.4}, {:.4}]  # the eigen band with its upper side capped by the circuit",
        q_hi.min(cap)
    );
    let _ = writeln!(
        t,
        "geometry_caveat = \"the C_Sigma bracket is certified on transmon_smoke.msh, the earlier generator's mesh of the same initial geometry, not on this PR's DeviceLayout mesh; the two agree on Palace at h (3.4527 vs 3.4520 GHz) but their h/2 levels differ (3.9528 GHz from Palace's own refinement of the fixture, #1081, vs 3.8771 GHz from gmsh's refinement here), because the two refinements are different meshes\""
    );
    let _ = writeln!(
        t,
        "eigen_estimate_minus_static_mid_pct = {:.3}",
        pct(fq_est, 0.5 * (d_lo + d_hi))
    );
}

fn assemble_mode(repo: &Path, out_dir: &Path) {
    let runs = out_dir.join("geode_runs");
    let mut ladders = Vec::new();
    for &(tag, _, _, _, _) in &PUBLISHED {
        let g = |s: &str| geode_cell(&runs, &format!("{tag}_{s}"));
        let p = |s: &str| palace_cell(repo, &format!("{tag}_{s}"));
        ladders.push(Ladder {
            tag,
            geode: vec![g("h"), g("h2")],
            palace_p1: vec![p("h_p1"), p("r1_p1"), p("r2_p1")],
            palace_p2: vec![p("h_p2"), p("r1_p2")],
            dl: palace_cell(repo, &format!("dl_{tag}")),
            dlpec: palace_cell(repo, &format!("dlpec_{tag}")),
        });
    }

    let mut t = String::new();
    t.push_str("# Auto-generated by `cargo run -p geode-core --release --example transmon_devicelayout_crosscheck`.\n");
    t.push_str("# Do NOT edit by hand; regenerate after any intentional change.\n");
    t.push_str("# Issue #1092. Checked by tests/transmon_devicelayout_crosscheck.rs.\n\n");
    t.push_str("[meta]\n");
    t.push_str("description = \"DeviceLayout.jl SingleTransmon cross-check: converged qubit and resonator eigenfrequencies of the published initial geometry (capacitor_length 620 um, resonator total_length 5000 um) and a forward check of the published final geometry (1217.678 um, 7243.955 um), from geode's A2 tensor-eps eigen path and from Palace, on meshes generated by DeviceLayout.jl itself with those two parameters; a resolution ladder per geometry; Palace at DeviceLayout's own discretization (mesh_order 2, solver_order 2, absorbing far field, 50 ohm readout ports) beside the PEC-box model; Richardson estimates with bands against the published values.\"\n");
    t.push_str("issue = 1092\n");
    // The backend and float dtype of the geode solves this artifact reads
    // (every record must agree; the assembly itself runs no solve).
    let mut backends = Vec::new();
    for &(tag, _, _, _, _) in &PUBLISHED {
        for s in ["h", "h2"] {
            let d = toml_doc(&runs.join(format!("{tag}_{s}.toml")));
            backends.push((
                tget(&d, "meta.backend").as_str().unwrap().to_string(),
                tget(&d, "meta.float_dtype").as_str().unwrap().to_string(),
            ));
        }
    }
    assert!(
        backends.windows(2).all(|w| w[0] == w[1]),
        "geode records disagree on backend: {backends:?}"
    );
    let _ = writeln!(
        t,
        "backend = \"{}\"  # of the geode solve records",
        backends[0].0
    );
    let _ = writeln!(t, "float_dtype = \"{}\"", backends[0].1);
    let _ = writeln!(t, "generated_at_commit = \"{}\"", git_head());
    t.push_str("geometry_generator = \"reference/julia/generate_transmon_param.jl (DeviceLayout.jl v1.15.0, gmsh_jll 4.15.2, Julia 1.10.11, macOS arm64); refinements and MSH 2.2 conversions with gmsh 4.15.2-git on the same host\"\n");
    t.push_str(
        "palace_results_dir = \"reference/fixtures/transmon_palace/results_crosscheck_1092\"\n",
    );
    t.push_str("geode_runs_dir = \"benchmarks/transmon_devicelayout_crosscheck/geode_runs\"\n");
    t.push_str("compute_host = \"AWS r8g.8xlarge (Graviton4, 32 vCPU, 247 GiB), Ubuntu 24.04 arm64; Palace fba6a5b (v0.17.0-99) built from reference/palace/docker/Dockerfile on the worker; geode ndarray f64\"\n");
    t.push_str("geode_model = \"A2 port-voltage shunt (#1044) with the bulk gradient projector, L_J = 14.86 nH || C_J = 5.5 fF, PEC on metal and on the exterior boundary, readout ports omitted (open), sapphire tensor diag(9.3, 9.3, 11.5)\"\n");
    t.push_str("\n[published]\n");
    t.push_str("source = \"AWS DeviceLayout.jl blog, SingleTransmon example (refs.bib devicelayout-blog); DeviceLayout.jl v1.15.0 examples/SingleTransmon/SingleTransmon.jl\"\n");
    t.push_str("discretization = \"mesh_order 2, solver_order 2 (compute_eigenfrequencies defaults), Palace Eigenmode N = 2, Target 2.5 GHz, Tol 1e-6; the blog warns the values are discretization-sensitive\"\n");
    for &(tag, cap, tot, fq, fr) in &PUBLISHED {
        let _ = writeln!(t, "{tag}_capacitor_length_um = {cap}");
        let _ = writeln!(t, "{tag}_total_length_um = {tot}");
        let _ = writeln!(t, "{tag}_f_ghz = [{fq}, {fr}]");
    }
    t.push_str("optimizer = \"PRIMA (derivative-free), 9 Palace runs, 'within 1% of our target frequencies' (blog); the targets are in the example source, not the blog: run_optimization(targets_GHz = [3.0, 4.0], reltol = 1e-2), PRIMA from x = [1, 1] with cap_length = 620 um / x1^2 and total_length = 5000 um / x2, bounds [0.6, 1.4], rhobeg 0.2, stopping when the mean squared relative error is below reltol^2 (DeviceLayout.jl v1.15.0 examples/SingleTransmon/SingleTransmon.jl)\"\n");
    t.push_str("targets_ghz = [3.0, 4.0]  # from the example source\n");

    for (l, &(_, _, _, fq, fr)) in ladders.iter().zip(&PUBLISHED) {
        write_ladder(&mut t, l, fq, fr);
    }

    fs::create_dir_all(out_dir).expect("out dir");
    let path = out_dir.join("results.toml");
    fs::write(&path, t).expect("write results");
    println!("wrote {}", path.display());
}

fn main() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let out_dir = std::env::var("TRANSMON_DL_BENCH_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| repo.join("benchmarks/transmon_devicelayout_crosscheck"));
    if let Ok(mesh) = std::env::var("GEODE_SOLVE_MESH") {
        let label = std::env::var("GEODE_SOLVE_LABEL").expect("GEODE_SOLVE_LABEL");
        let sigma: f64 = std::env::var("GEODE_SIGMA_GHZ")
            .ok()
            .map(|s| s.parse().expect("GEODE_SIGMA_GHZ"))
            .unwrap_or(5.0);
        let n: usize = std::env::var("GEODE_NMODES")
            .ok()
            .map(|s| s.parse().expect("GEODE_NMODES"))
            .unwrap_or(3);
        let runs = std::env::var("GEODE_RUNS_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| out_dir.join("geode_runs"));
        solve_mode(Path::new(&mesh), &label, sigma, n, &runs);
    } else {
        assemble_mode(&repo, &out_dir);
    }
}
