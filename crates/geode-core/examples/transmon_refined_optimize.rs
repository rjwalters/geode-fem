//! Refine, optimize, then measure the SAME geometry on a held-out refinement series.
//! The initial coarse harmonic geometry map is prolonged exactly between meshes;
//! refinement is outside differentiation. A discrete target hit is not convergence:
//! after the optimization the final geometry (and `X0`) is re-solved on several
//! further flux-jump refinement rounds, and the series, its rigorous upper bound
//! and its (non-rigorous) extrapolation are written to `summary.toml`.
//! Run: cargo run -p geode-core --release --example transmon_refined_optimize
//! Environment: REFINED_OPT_CYCLES (default 8), REFINED_OPT_MAX_TETS (2 million),
//! REFINED_OPT_ORDER (1 or 2), REFINED_OPT_DIR (output directory),
//! REFINED_OPT_TARGET (optimizer target in fF, default the DeviceLayout anchor
//! `TransmonAnchor::devicelayout().c_sigma_farad`, about 93.9537 fF, issue #1066),
//! REFINED_OPT_HELD_OUT_ROUNDS (default 7), REFINED_OPT_HELD_OUT_MAX_TETS
//! (4 million), REFINED_OPT_HELD_OUT_X0 (1 = also solve `X0` per round).
//! No reference benchmark is overwritten.

use geode_core::adapt::refine::{BisectionMesh, RefineOpts};
use geode_core::assembly::electrostatic::{
    AmgPcgOptions, Electrode, assemble_electrostatic, assemble_electrostatic_p2_sheets,
    extract_capacitance_amg_pcg_fields, extract_capacitance_p2_amg_pcg_fields, tet_bary_grads,
};
use geode_core::mesh::{
    MetalRole, TaggedTetMesh, TetMesh, TransmonFixture, read_transmon_smoke_fixture,
};
use geode_core::quantum::transmon::TransmonAnchor;
use geode_core::shape::min_tet_volume_ratio;
use geode_core::shape::refined_capacitance::{
    p1_energy_shape_derivatives, p2_energy_shape_derivatives,
};
use geode_core::shape::transmon_morph::{TransmonMorphRoles, TransmonMorphSpec, parameter_fields};
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

const SCALE: [f64; 3] = [0.040959, 0.553015, 0.784636];

/// The anchor every result is compared against, in fF: the electrostatic
/// `C_Σ` (junction excluded) whose harmonic frequency with the junction's
/// `L_J ∥ C_J` is DeviceLayout's published 4.14 GHz (issue #1066). It is
/// derived in one place, `TransmonAnchor::devicelayout`. Runs before #1066
/// used a mis-derived 89.9 fF here; their artifacts are kept and labelled
/// superseded.
fn anchor_ff() -> f64 {
    TransmonAnchor::devicelayout().c_sigma_farad * 1e15
}

/// Optimizer target in fF: the anchor unless `REFINED_OPT_TARGET` offsets it
/// (the resolution-corrected run).
fn target() -> f64 {
    std::env::var("REFINED_OPT_TARGET").map_or(anchor_ff(), |v| v.parse().unwrap())
}

/// Harmonic qubit frequency (GHz) of an electrostatic `C_Σ` in fF, with the
/// anchor's `L_J ∥ C_J`. Decreasing in `C_Σ`, so a rigorous upper bound on
/// `C_Σ` is a rigorous lower bound on this frequency.
fn harmonic_ghz(c_sigma_ff: f64) -> f64 {
    TransmonAnchor::devicelayout().harmonic_frequency_of_c_sigma_hz(c_sigma_ff * 1e-15) / 1e9
}
/// `E_C/h` (GHz) of an electrostatic `C_Σ` in fF, junction capacitance included.
fn e_c_ghz(c_sigma_ff: f64) -> f64 {
    TransmonAnchor::devicelayout().e_c_hz_of_c_sigma(c_sigma_ff * 1e-15) / 1e9
}
fn floor_to(v: f64, digits: i32) -> f64 {
    let k = 10f64.powi(digits);
    (v * k).floor() / k
}
fn ceil_to(v: f64, digits: i32) -> f64 {
    let k = 10f64.powi(digits);
    (v * k).ceil() / k
}
fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name).map_or(default, |v| v.parse().unwrap())
}
fn moved(base: &TetMesh, fields: &[Vec<[f64; 3]>], theta: &[f64; 3]) -> TetMesh {
    let mut mesh = base.clone();
    for (i, x) in mesh.nodes.iter_mut().enumerate() {
        for (field, step) in fields.iter().zip(theta) {
            for (d, xd) in x.iter_mut().enumerate() {
                *xd += step * field[i][d];
            }
        }
    }
    mesh
}
fn fixture(tagged: &TaggedTetMesh, original: &TransmonFixture) -> TransmonFixture {
    TransmonFixture {
        mesh: tagged.mesh.clone(),
        tags: original.tags,
        tet_physical_tags: tagged.tet_physical_tags.clone(),
        boundary_triangles: tagged.boundary_triangles.clone(),
        triangle_physical_tags: tagged.triangle_physical_tags.clone(),
    }
}
fn electrodes(fx: &TransmonFixture) -> (Vec<Electrode>, Vec<u32>) {
    let components = fx.split_metal_conductors();
    let nodes = |r| {
        components
            .iter()
            .find(|c| c.role == r)
            .unwrap()
            .nodes
            .clone()
    };
    (
        vec![
            Electrode {
                name: "island".into(),
                nodes: nodes(MetalRole::Island),
                voltage: 1.0,
            },
            Electrode {
                name: "feedline".into(),
                nodes: nodes(MetalRole::Feedline),
                voltage: 0.0,
            },
        ],
        nodes(MetalRole::Ground),
    )
}
struct Solve {
    c: f64,
    gradient: Vec<f64>,
    phi: Vec<f64>,
    elapsed: f64,
    true_residual: f64,
    iterations: Vec<usize>,
}
fn solve(fx: &TransmonFixture, fields: &[Vec<[f64; 3]>]) -> Solve {
    let order = std::env::var("REFINED_OPT_ORDER")
        .unwrap_or("1".into())
        .parse()
        .unwrap();
    solve_with_order(fx, fields, order)
}
fn solve_with_order(fx: &TransmonFixture, fields: &[Vec<[f64; 3]>], order: usize) -> Solve {
    let start = Instant::now();
    let (conductors, ground) = electrodes(fx);
    let eps = fx.epsilon_r_scalar();
    let rho = vec![0.0; fx.mesh.n_tets()];
    let (full, report, phis) = if order == 1 {
        let sys = assemble_electrostatic(&fx.mesh, &eps, &rho, &conductors, &ground).unwrap();
        extract_capacitance_amg_pcg_fields(&sys, &conductors, &ground, AmgPcgOptions::default())
            .unwrap()
    } else {
        assert_eq!(order, 2);
        let components = fx.split_metal_conductors();
        let tris = |role| {
            components
                .iter()
                .find(|c| c.role == role)
                .unwrap()
                .triangles
                .as_slice()
        };
        let (sys, dofs) = assemble_electrostatic_p2_sheets(
            &fx.mesh,
            &eps,
            &rho,
            &conductors,
            &[tris(MetalRole::Island), tris(MetalRole::Feedline)],
            &ground,
            tris(MetalRole::Ground),
        )
        .unwrap();
        extract_capacitance_p2_amg_pcg_fields(&sys, &dofs, AmgPcgOptions::default()).unwrap()
    };
    assert!(report.rel_residual.iter().all(|r| *r <= 1e-9));
    let vf = -full.c[1][0] / full.c[1][1];
    let phi: Vec<_> = phis[0]
        .iter()
        .zip(&phis[1])
        .map(|(a, b)| a + vf * b)
        .collect();
    let c = full.c[0][0] + vf * full.c[0][1];
    let gradient = if order == 1 {
        p1_energy_shape_derivatives(&fx.mesh, &eps, &phi, fields)
    } else {
        p2_energy_shape_derivatives(&fx.mesh, &eps, &phi, fields)
    }
    .unwrap()
    .iter()
    .map(|v| v * 1e15)
    .collect();
    let phi = phi[..fx.mesh.n_nodes()].to_vec();
    Solve {
        c: c * 1e15,
        gradient,
        phi,
        elapsed: start.elapsed().as_secs_f64(),
        true_residual: report.rel_residual.iter().copied().fold(0.0, f64::max),
        iterations: report.iterations,
    }
}

// Flux-jump marking, excluding the actual conductor sheets where a normal
// displacement jump is physical. This is a heuristic indicator; the acceptance
// test is a separate solve on a more resolved discretization, not its value.
fn indicators(fx: &TransmonFixture, phi: &[f64]) -> Vec<f64> {
    let mesh = &fx.mesh;
    let eps = fx.epsilon_r_scalar();
    let mut pec = BTreeSet::new();
    for mut tri in fx.metal_triangles() {
        tri.sort_unstable();
        pec.insert(tri);
    }
    let mut faces: HashMap<[u32; 3], (usize, [f64; 3], f64)> = HashMap::new();
    let mut eta = vec![0.0; mesh.n_tets()];
    for (t, tet) in mesh.tets.iter().enumerate() {
        let coords = tet.map(|v| mesh.nodes[v as usize]);
        let grads = tet_bary_grads(&coords);
        let mut flux = [0.0; 3];
        for a in 0..4 {
            for (d, f) in flux.iter_mut().enumerate() {
                *f += eps[t] * phi[tet[a] as usize] * grads[a][d];
            }
        }
        for local in [[1, 2, 3], [0, 2, 3], [0, 1, 3], [0, 1, 2]] {
            let mut key = local.map(|a| tet[a]);
            key.sort_unstable();
            if pec.contains(&key) {
                continue;
            }
            let p = key.map(|v| mesh.nodes[v as usize]);
            let a: [f64; 3] = std::array::from_fn(|d| p[1][d] - p[0][d]);
            let b: [f64; 3] = std::array::from_fn(|d| p[2][d] - p[0][d]);
            let n = [
                a[1] * b[2] - a[2] * b[1],
                a[2] * b[0] - a[0] * b[2],
                a[0] * b[1] - a[1] * b[0],
            ];
            if let Some((other, f, e)) = faces.remove(&key) {
                let jump: f64 = (0..3).map(|d| (flux[d] - f[d]) * n[d]).sum();
                let area2 = n.iter().map(|x| x * x).sum::<f64>().sqrt();
                let value = jump * jump / area2.sqrt() / eps[t].max(e);
                eta[t] += value;
                eta[other] += value;
            } else {
                faces.insert(key, (t, flux, eps[t]));
            }
        }
    }
    eta
}
fn save(path: &Path, history: &[Value], status: &str, theta: &[f64; 3]) {
    fs::write(
        path,
        serde_json::to_string_pretty(&json!({"target_scalar_capacitance_ff":target(),"anchor_ff":anchor_ff(),"anchor_issue":1066,"optimization_order":std::env::var("REFINED_OPT_ORDER").unwrap_or("1".into()),
        "geometry_map":"fixed coarse junction-pinned harmonic fields; exact nested P1 prolongation",
        "convergence_threshold_relative":0.01,"status":status,"theta":theta,"history":history}))
        .unwrap(),
    )
    .unwrap();
}

// The small coarse mesh carries exactly the same polygonal final geometry.
// Palace can change polynomial order/refinement independently of our optimizer.
fn export_palace(fx: &TransmonFixture, out: &std::path::Path, mesh_name: &str) {
    let components = fx.split_metal_conductors();
    let mut sheet_tag = HashMap::new();
    for comp in components {
        let tag = match comp.role {
            MetalRole::Ground => 101,
            MetalRole::Island => 102,
            MetalRole::Feedline => 103,
        };
        for mut tri in comp.triangles {
            tri.sort_unstable();
            sheet_tag.insert(tri, tag);
        }
    }
    let mut f = std::io::BufWriter::new(fs::File::create(out.join(mesh_name)).unwrap());
    writeln!(
        f,
        "$MeshFormat\n2.2 0 8\n$EndMeshFormat\n$Nodes\n{}",
        fx.mesh.n_nodes()
    )
    .unwrap();
    for (i, x) in fx.mesh.nodes.iter().enumerate() {
        writeln!(f, "{} {:.17e} {:.17e} {:.17e}", i + 1, x[0], x[1], x[2]).unwrap();
    }
    writeln!(
        f,
        "$EndNodes\n$Elements\n{}",
        fx.mesh.n_tets() + fx.boundary_triangles.len()
    )
    .unwrap();
    let mut id = 1;
    for (tri, &tag) in fx.boundary_triangles.iter().zip(&fx.triangle_physical_tags) {
        let mut key = *tri;
        key.sort_unstable();
        let tag = sheet_tag.get(&key).copied().unwrap_or(tag);
        writeln!(
            f,
            "{id} 2 2 {tag} {tag} {} {} {}",
            tri[0] + 1,
            tri[1] + 1,
            tri[2] + 1
        )
        .unwrap();
        id += 1;
    }
    for (tet, &tag) in fx.mesh.tets.iter().zip(&fx.tet_physical_tags) {
        writeln!(
            f,
            "{id} 4 2 {tag} {tag} {} {} {} {}",
            tet[0] + 1,
            tet[1] + 1,
            tet[2] + 1,
            tet[3] + 1
        )
        .unwrap();
        id += 1;
    }
    writeln!(f, "$EndElements").unwrap();
    f.flush().unwrap();
    for order in 1..=3 {
        let config = json!({"Problem":{"Type":"Electrostatic","Output":format!("palace_p{order}"),"Verbose":1},
            "Model":{"Mesh":mesh_name,"L0":1.0},
            "Domains":{"Materials":[{"Attributes":[fx.tags.substrate],"Permittivity":(9.3+9.3+11.5)/3.0},{"Attributes":[fx.tags.vacuum],"Permittivity":1.0}]},
            "Boundaries":{"Ground":{"Attributes":[101]},"Terminal":[{"Index":1,"Attributes":[102]},{"Index":2,"Attributes":[103]}]},
            "Solver":{"Order":order,"Electrostatic":{"Save":0},"Linear":{"Type":"BoomerAMG","Tol":1e-10,"MaxIts":2000}}});
        fs::write(
            out.join(format!("palace_p{order}.json")),
            serde_json::to_string_pretty(&config).unwrap() + "\n",
        )
        .unwrap();
    }
}
/// Exact nodal prolongation of the geometry-map fields to a refined mesh.
fn prolong(
    fields: &[Vec<[f64; 3]>],
    p: faer::sparse::SparseColMatRef<'_, usize, f64>,
) -> Vec<Vec<[f64; 3]>> {
    fields
        .iter()
        .map(|field| {
            let mut fine = vec![[0.0; 3]; p.nrows()];
            for (col, value) in field.iter().enumerate() {
                for q in p.col_ptr()[col]..p.col_ptr()[col + 1] {
                    let row = p.row_idx()[q];
                    for (d, &v) in value.iter().enumerate() {
                        fine[row][d] += p.val()[q] * v;
                    }
                }
            }
            fine
        })
        .collect()
}
/// Mark the largest indicators carrying half of the indicator sum.
fn dorfler(eta: &[f64]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..eta.len()).collect();
    order.sort_by(|&a, &b| eta[b].total_cmp(&eta[a]));
    let threshold = 0.5 * eta.iter().sum::<f64>();
    let mut sum = 0.0;
    let mut marked = Vec::new();
    for t in order {
        marked.push(t);
        sum += eta[t];
        if sum >= threshold {
            break;
        }
    }
    marked
}
struct Step {
    c: f64,
    theta: [f64; 3],
    ratio: f64,
}
struct Round {
    n_tets: usize,
    n_nodes: usize,
    ratio: f64,
    fin: Solve,
    x0: Option<Solve>,
}
struct Design {
    bbox: [[f64; 3]; 2],
    length: f64,
    width: f64,
}
/// Island bounding box (um) measured on the final adapted mesh.
fn design_dimensions(fx: &TransmonFixture) -> Design {
    let components = fx.split_metal_conductors();
    let island = components
        .iter()
        .find(|c| c.role == MetalRole::Island)
        .unwrap();
    let mut bbox = [[f64::INFINITY; 3], [f64::NEG_INFINITY; 3]];
    for &v in &island.nodes {
        for (d, x) in fx.mesh.nodes[v as usize].iter().enumerate() {
            bbox[0][d] = bbox[0][d].min(x * 1e6);
            bbox[1][d] = bbox[1][d].max(x * 1e6);
        }
    }
    Design {
        bbox,
        length: bbox[1][1] - bbox[0][1],
        width: bbox[1][0] - bbox[0][0],
    }
}

/// Whether a scanned free-fit rate is an interior minimum of the scan.
fn interior_rate(alpha: f64) -> bool {
    alpha > 7.5e-4 && alpha < 2.99975
}
/// Least-squares `c = c_inf + a x`; returns `(c_inf, sum of squared residuals)`.
fn line_fit(x: &[f64], c: &[f64]) -> (f64, f64) {
    let m = x.len() as f64;
    let (xm, cm) = (x.iter().sum::<f64>() / m, c.iter().sum::<f64>() / m);
    let sxx: f64 = x.iter().map(|v| (v - xm).powi(2)).sum();
    let sxc: f64 = x.iter().zip(c).map(|(v, w)| (v - xm) * (w - cm)).sum();
    let a = sxc / sxx;
    let c_inf = cm - a * xm;
    let sse = x
        .iter()
        .zip(c)
        .map(|(v, w)| (w - c_inf - a * v).powi(2))
        .sum();
    (c_inf, sse)
}
/// `c_inf` of the least-squares fit `c = c_inf + a n^-alpha` at a fixed rate.
fn power_fit(n: &[f64], c: &[f64], alpha: f64) -> (f64, f64) {
    let x: Vec<f64> = n.iter().map(|v| v.powf(-alpha)).collect();
    line_fit(&x, c)
}
/// What a held-out series says about the converged value of one geometry.
/// Only `upper_bound_ff` is rigorous. Everything else is an estimate.
struct Extrapolation {
    decrements: Vec<f64>,
    ratios: Vec<f64>,
    /// Last solved value rounded UP to 0.01 fF.
    upper_bound_ff: f64,
    /// Geometric tail `c_last - d_last r / (1 - r)` at the largest and the
    /// smallest observed decrement ratio (low, high), unrounded.
    geometric: [f64; 2],
    /// Free-rate fits `c_inf + a n^-alpha`: (first round used, alpha, c_inf).
    /// A fit whose rate sits on an end of the scan found no interior
    /// minimum; it is recorded but kept out of `band`.
    free_fits: Vec<(usize, f64, f64)>,
    /// Fixed slower rates on rounds 2..: (alpha, c_inf).
    slow_fits: Vec<(f64, f64)>,
    /// Outward-rounded (0.01 fF) union of `geometric` and `free_fits`.
    band: [f64; 2],
}
impl Extrapolation {
    fn midpoint(&self) -> f64 {
        0.5 * (self.geometric[0] + self.geometric[1])
    }
    fn json(&self) -> Value {
        json!({"decrements_ff":self.decrements,"decrement_ratios":self.ratios,"upper_bound_ff":self.upper_bound_ff,
            "geometric_tail_ff":self.geometric,"geometric_tail_midpoint_ff":self.midpoint(),
            "free_rate_fits":self.free_fits.iter().map(|(k,a,c)| json!({"first_round":k,"alpha":a,"c_inf_ff":c,"in_band":interior_rate(*a)})).collect::<Vec<_>>(),
            "slow_rate_fits":self.slow_fits.iter().map(|(a,c)| json!({"alpha":a,"c_inf_ff":c})).collect::<Vec<_>>(),
            "estimate_band_ff":self.band})
    }
}
/// Needs at least five rounds with strictly decreasing, contracting values;
/// otherwise no extrapolation is attempted (the series itself is still saved).
fn extrapolate(n: &[f64], c: &[f64]) -> Option<Extrapolation> {
    if c.len() < 5 {
        return None;
    }
    let decrements: Vec<f64> = c.windows(2).map(|w| w[0] - w[1]).collect();
    let ratios: Vec<f64> = decrements.windows(2).map(|w| w[1] / w[0]).collect();
    if decrements.iter().any(|d| *d <= 0.0) || ratios.iter().any(|r| *r >= 1.0) {
        return None;
    }
    let (last, d_last) = (*c.last().unwrap(), *decrements.last().unwrap());
    let tail = |r: f64| last - d_last * r / (1.0 - r);
    let r_min = ratios.iter().copied().fold(f64::INFINITY, f64::min);
    let r_max = ratios.iter().copied().fold(0.0, f64::max);
    let geometric = [tail(r_max), tail(r_min)];
    let mut free_fits = Vec::new();
    for first in 2..=c.len().saturating_sub(4) {
        // Scan the rate; three unknowns, so at least four rounds per fit.
        let best = (1..=6000)
            .map(|i| {
                let alpha = 5e-4 * i as f64;
                let (c_inf, sse) = power_fit(&n[first..], &c[first..], alpha);
                (alpha, c_inf, sse)
            })
            .min_by(|a, b| a.2.total_cmp(&b.2))
            .unwrap();
        free_fits.push((first, best.0, best.1));
    }
    let slow_fits = [1.0 / 3.0, 0.5]
        .iter()
        .map(|&alpha| (alpha, power_fit(&n[2..], &c[2..], alpha).0))
        .collect();
    let all = geometric
        .iter()
        .copied()
        .chain(free_fits.iter().filter(|f| interior_rate(f.1)).map(|f| f.2));
    let lo = all.clone().fold(f64::INFINITY, f64::min);
    let hi = all.fold(f64::NEG_INFINITY, f64::max);
    Some(Extrapolation {
        decrements,
        ratios,
        upper_bound_ff: (last * 100.0).ceil() / 100.0,
        geometric,
        free_fits,
        slow_fits,
        band: [(lo * 100.0).floor() / 100.0, (hi * 100.0).ceil() / 100.0],
    })
}

struct Summary<'a> {
    target: f64,
    theta: [f64; 3],
    steps: &'a [Step],
    checks: &'a [(f64, f64, f64)],
    coarse: &'a Solve,
    design: &'a Design,
    rounds: &'a [Round],
    ex_final: Option<&'a Extrapolation>,
    ex_x0: Option<&'a Extrapolation>,
    cycles: usize,
}
fn list(values: &[f64], digits: usize) -> String {
    let items: Vec<String> = values.iter().map(|v| format!("{v:.digits$}")).collect();
    format!("[{}]", items.join(", "))
}
fn write_extrapolation(f: &mut impl Write, table: &str, reference: f64, ex: &Extrapolation) {
    writeln!(f, "\n[{table}]").unwrap();
    writeln!(f, "decrements_ff = {}", list(&ex.decrements, 6)).unwrap();
    writeln!(f, "decrement_ratios = {}", list(&ex.ratios, 4)).unwrap();
    writeln!(
        f,
        "upper_bound_ff = {:.2}  # RIGOROUS: last solved value rounded up; the converged value of this geometry is below it",
        ex.upper_bound_ff
    )
    .unwrap();
    writeln!(
        f,
        "geometric_tail_ff = {}  # ESTIMATE: c_last - d_last r/(1-r) at the largest and smallest observed ratio",
        list(&ex.geometric, 6)
    )
    .unwrap();
    writeln!(f, "geometric_tail_midpoint_ff = {:.6}", ex.midpoint()).unwrap();
    for (first, alpha, c_inf) in &ex.free_fits {
        writeln!(
            f,
            "[[{table}.free_rate_fit]]  # ESTIMATE: least squares c_inf + a n_tets^-alpha, alpha scanned in steps of 5e-4\nfirst_round = {first}\nalpha = {alpha:.4}\nc_inf_ff = {c_inf:.6}\nin_band = {}",
            interior_rate(*alpha)
        )
        .unwrap();
    }
    for (alpha, c_inf) in &ex.slow_fits {
        writeln!(
            f,
            "[[{table}.slow_rate_fit]]  # SENSITIVITY: the same fit on rounds 2.. at an assumed slower rate\nalpha = {alpha:.6}\nc_inf_ff = {c_inf:.6}"
        )
        .unwrap();
    }
    // Relative to the round-0 value the optimizer saw: by how much it
    // overstates the estimated converged value (low end floored, high end ceiled).
    let over = |est: f64| (reference - est) / est;
    writeln!(
        f,
        "[{table}.estimate]  # NOT a bound: union of the geometric tail and the free-rate fits, rounded outward\nband_ff = {}\nround_0_overstates_estimate_relative = [{:.4}, {:.4}]",
        list(&ex.band, 2),
        (over(ex.band[1]) * 1e4).floor() / 1e4,
        (over(ex.band[0]) * 1e4).ceil() / 1e4
    )
    .unwrap();
    // Issue #1066 item 7: what each number implies for the qubit, as an
    // observation. E_C includes C_J; the harmonic frequency is
    // 1/(2 pi sqrt(L_J (C_Sigma + C_J))). Both decrease with C_Sigma, so the
    // rigorous upper bound on C_Sigma gives rigorous LOWER bounds here
    // (rounded down); estimate bands are rounded outward.
    writeln!(
        f,
        "[{table}.qubit]  # observation only: E_C = e^2/(2(C_Sigma + C_J)) and f = 1/(2 pi sqrt(L_J (C_Sigma + C_J))) with the [anchor] L_J, C_J\n\
         round_0_e_c_ghz = {:.6}\n\
         round_0_harmonic_ghz = {:.6}\n\
         converged_e_c_at_least_ghz = {:.6}  # RIGOROUS, from upper_bound_ff, rounded down\n\
         converged_harmonic_at_least_ghz = {:.4}  # RIGOROUS, from upper_bound_ff, rounded down\n\
         estimate_e_c_ghz = [{:.6}, {:.6}]  # ESTIMATE, from band_ff, rounded outward\n\
         estimate_harmonic_ghz = [{:.4}, {:.4}]  # ESTIMATE, from band_ff, rounded outward\n\
         slow_rate_harmonic_ghz = {}  # SENSITIVITY, from the slow-rate fits, alpha = 1/3 and 1/2, rounded to 1e-4",
        e_c_ghz(reference),
        harmonic_ghz(reference),
        floor_to(e_c_ghz(ex.upper_bound_ff), 6),
        floor_to(harmonic_ghz(ex.upper_bound_ff), 4),
        floor_to(e_c_ghz(ex.band[1]), 6),
        ceil_to(e_c_ghz(ex.band[0]), 6),
        floor_to(harmonic_ghz(ex.band[1]), 4),
        ceil_to(harmonic_ghz(ex.band[0]), 4),
        list(
            &ex.slow_fits
                .iter()
                .map(|(_, c)| harmonic_ghz(*c))
                .collect::<Vec<_>>(),
            4
        )
    )
    .unwrap();
}
/// Deterministic headline summary (no timings): the artifact the pin test reads.
fn write_summary(out: &Path, s: &Summary<'_>) {
    let mut f = std::io::BufWriter::new(fs::File::create(out.join("summary.toml")).unwrap());
    let order = std::env::var("REFINED_OPT_ORDER").unwrap_or("1".into());
    let (first, last) = (&s.rounds[0], s.rounds.last().unwrap());
    let a = TransmonAnchor::devicelayout();
    let anchor = anchor_ff();
    writeln!(
        f,
        "# Auto-generated by `cargo run -p geode-core --release --example transmon_refined_optimize`\n\
         # (see the README next to the run directory for the environment of this run).\n\
         # Do NOT edit by hand. Consumed by `tests/transmon_diffopt.rs`. No timings: bit-reproducible.\n\n\
         [meta]\n\
         issue = 1066\n\
         method_issue = 1056\n\
         generator = \"crates/geode-core/examples/transmon_refined_optimize.rs\"\n\
         fixture = \"crates/geode-core/tests/fixtures/transmon_smoke.msh\"\n\
         quantity = \"scalar-eps island capacitance with a neutral floating feedline (C_ii - C_if^2/C_ff), energy method, f64; no junction self-capacitance\"\n\
         anchor_ff = {anchor:?}\n\
         optimizer_target_ff = {:?}\n\
         resolution_corrected = {}\n\
         element_order = {order}\n\
         p1_preparation_cycles = {}\n\
         solver = \"AMG-PCG, relative residual 1e-10, true residual recomputed and asserted <= 1e-9\"\n\
         bounds = \"the held-out spaces are nested and conforming and the capacitance is an energy minimum, so every solved value is an upper bound on the converged value of the same polygonal geometry; there is no rigorous lower bound, and every extrapolation below is an estimate\"",
        s.target,
        s.target != anchor,
        s.cycles
    )
    .unwrap();
    writeln!(
        f,
        "\n[anchor]  # issue #1066: the published DeviceLayout qubit frequency read as the harmonic frequency of the linear L_J || (C_Sigma + C_J) eigenproblem; derived in geode_core::quantum::transmon::TransmonAnchor::devicelayout\n\
         frequency_ghz = {:?}\n\
         l_j_nh = {:?}\n\
         c_j_ff = {:?}\n\
         c_total_ff = {:?}  # 1/((2 pi f)^2 L_J)\n\
         c_sigma_target_ff = {:?}  # c_total_ff - c_j_ff, junction excluded\n\
         e_c_ghz = {:?}  # e^2/(2 C_total h)\n\
         e_j_ghz = {:?}\n\
         superseded_anchor_ff = 89.9  # mis-derived (#505 inverted the anharmonic formula at 4.14 GHz and left out C_J); runs against it are kept and labelled superseded",
        a.frequency_hz / 1e9,
        a.l_j_henry * 1e9,
        a.c_j_farad * 1e15,
        a.c_total_farad * 1e15,
        a.c_sigma_farad * 1e15,
        a.e_c_hz / 1e9,
        a.e_j_hz / 1e9
    )
    .unwrap();
    writeln!(
        f,
        "\n[optimization]\nn_tets = {}\nn_nodes = {}\ninitial_ff = {:.9}\noptimized_ff = {:.9}\naccepted_steps = {}\ntheta = {:?}\nmin_volume_ratio_over_accepted_steps = {:.5}  # rounded down\nfinal_min_volume_ratio = {:.5}  # rounded down",
        first.n_tets,
        first.n_nodes,
        s.steps[0].c,
        s.steps.last().unwrap().c,
        s.steps.len() - 1,
        s.theta,
        (s.steps.iter().map(|t| t.ratio).fold(f64::INFINITY, f64::min) * 1e5).floor() / 1e5,
        (s.steps.last().unwrap().ratio * 1e5).floor() / 1e5
    )
    .unwrap();
    for (k, t) in s.steps.iter().enumerate() {
        writeln!(
            f,
            "[[optimization.step]]\nstep = {k}\nc_ff = {:.9}\ntheta = {:?}\nmin_volume_ratio = {:.5}",
            t.c,
            t.theta,
            (t.ratio * 1e5).floor() / 1e5
        )
        .unwrap();
    }
    writeln!(
        f,
        "\n[design]  # island sheet nodes of the final adapted mesh\nisland_length_um = {:.6}\npad_width_um = {:.6}",
        s.design.length, s.design.width
    )
    .unwrap();
    writeln!(
        f,
        "\n[coarse_reference]  # the same theta on the committed 133k-tet fixture mesh (coarse geometry-map fields)\nc_ff = {:.9}\ngradient_ff = {}",
        s.coarse.c,
        list(&s.coarse.gradient, 9)
    )
    .unwrap();
    for (i, (analytic, fd, rel)) in s.checks.iter().enumerate() {
        writeln!(
            f,
            "\n[[gradient_check]]  # central difference of fully re-solved capacitances, h = 1e-4\nparameter = {i}\nanalytic_ff = {analytic:.9}\nfd_ff = {fd:.9}\nrelative_error = {rel:.3e}"
        )
        .unwrap();
    }
    for (k, r) in s.rounds.iter().enumerate() {
        let shift: Vec<f64> = r
            .fin
            .gradient
            .iter()
            .zip(&first.fin.gradient)
            .map(|(g, g0)| (g - g0) / g0 + 0.0)
            .collect();
        writeln!(
            f,
            "\n[[held_out]]  # round 0 is the optimization mesh; each round marks half the flux-jump indicator of the final design\nround = {k}\nn_tets = {}\nn_nodes = {}\nmin_volume_ratio = {:.5}\nc_ff = {:.9}\nrelative_shift_from_round_0 = {:.6e}\ngradient_ff = {}\ngradient_relative_shift_from_round_0 = {}\npcg_iterations = {:?}\npcg_max_true_rel_residual = {:.3e}",
            r.n_tets,
            r.n_nodes,
            (r.ratio * 1e5).floor() / 1e5,
            r.fin.c,
            (r.fin.c - first.fin.c) / first.fin.c,
            list(&r.fin.gradient, 9),
            list(&shift, 6),
            r.fin.iterations,
            r.fin.true_residual
        )
        .unwrap();
        if let Some(x0) = &r.x0 {
            writeln!(
                f,
                "x0_c_ff = {:.9}\nx0_gradient_ff = {}\nx0_pcg_max_true_rel_residual = {:.3e}\ndesign_increment_ff = {:.9}",
                x0.c,
                list(&x0.gradient, 9),
                x0.true_residual,
                r.fin.c - x0.c
            )
            .unwrap();
        }
    }
    writeln!(
        f,
        "\n[held_out_result]\nrounds = {}\nlast_round_n_tets = {}\nlast_round_c_ff = {:.9}\nlast_round_relative_shift_from_round_0 = {:.6e}\nlast_round_relative_to_anchor = {:.6e}",
        s.rounds.len() - 1,
        last.n_tets,
        last.fin.c,
        (last.fin.c - first.fin.c) / first.fin.c,
        (last.fin.c - anchor) / anchor
    )
    .unwrap();
    if let Some(ex) = s.ex_final {
        // Rigorous, one-sided: the converged value is below the last solved
        // value. Below the anchor that gives a minimum shortfall (rounded
        // down); above it, only a maximum excess (rounded up).
        if last.fin.c < anchor {
            writeln!(
                f,
                "converged_below_anchor_at_least_relative = {:.4}  # RIGOROUS, from the unrounded last value, rounded down",
                ((anchor - last.fin.c) / anchor * 1e4).floor() / 1e4
            )
            .unwrap();
        } else {
            writeln!(
                f,
                "converged_above_anchor_at_most_relative = {:.4}  # RIGOROUS upper side only, rounded up; there is no rigorous lower bound, so nothing here proves the converged value is near the anchor",
                ((last.fin.c - anchor) / anchor * 1e4).ceil() / 1e4
            )
            .unwrap();
        }
        writeln!(
            f,
            "estimate_relative_to_anchor = [{:.4}, {:.4}]  # ESTIMATE band against the anchor, rounded outward",
            ((ex.band[0] - anchor) / anchor * 1e4).floor() / 1e4,
            ((ex.band[1] - anchor) / anchor * 1e4).ceil() / 1e4
        )
        .unwrap();
        if s.target == anchor {
            writeln!(
                f,
                "resolution_corrected_target_ff = {:.6}  # anchor * round-0 value / geometric-tail midpoint: the round-0 target whose extrapolation is expected near the anchor",
                anchor * first.fin.c / ex.midpoint()
            )
            .unwrap();
        }
        write_extrapolation(&mut f, "final_design_extrapolation", first.fin.c, ex);
    }
    if let Some(ex) = s.ex_x0 {
        write_extrapolation(&mut f, "x0_extrapolation", first.x0.as_ref().unwrap().c, ex);
    }
    f.flush().unwrap();
}
fn main() {
    let cycles: usize = std::env::var("REFINED_OPT_CYCLES")
        .unwrap_or("8".into())
        .parse()
        .unwrap();
    let max_tets: usize = std::env::var("REFINED_OPT_MAX_TETS")
        .unwrap_or("2000000".into())
        .parse()
        .unwrap();
    let out = PathBuf::from(
        std::env::var("REFINED_OPT_DIR")
            .unwrap_or("benchmarks/transmon_diffopt/refined_optimization".into()),
    );
    fs::create_dir_all(&out).unwrap();
    let mut fx = read_transmon_smoke_fixture().unwrap();
    for p in &mut fx.mesh.nodes {
        for x in p {
            *x *= 1e-6;
        }
    }
    let roles =
        TransmonMorphRoles::identify(&fx, &fx.mesh, TransmonMorphSpec::fixture_default(1e-6))
            .unwrap();
    let mut fields = parameter_fields(&fx.mesh, &roles).unwrap().to_vec();
    let coarse_fields = fields.clone();
    let tagged = TaggedTetMesh {
        mesh: fx.mesh.clone(),
        tet_physical_tags: fx.tet_physical_tags.clone(),
        boundary_triangles: fx.boundary_triangles.clone(),
        triangle_physical_tags: fx.triangle_physical_tags.clone(),
    };
    let mut state = BisectionMesh::new(tagged).unwrap();
    let mut history = Vec::new();
    let mut theta = [0.0; 3];
    let mut previous: Option<f64> = None;
    let result_path = out.join("results.json");
    for cycle in 0..=cycles {
        let mut current = fixture(state.mesh(), &fx);
        current.mesh = moved(&state.mesh().mesh, &fields, &theta);
        let solved = solve_with_order(&current, &fields, 1);
        let shift = previous.map(|c| (solved.c - c) / c);
        println!(
            "cycle {cycle}: {} tets, C={:.9} fF, gradient={:?}, shift={shift:?}, {:.1}s",
            current.mesh.n_tets(),
            solved.c,
            solved.gradient,
            solved.elapsed
        );
        history.push(json!({"kind":"refinement","cycle":cycle,"n_tets":current.mesh.n_tets(),"n_nodes":current.mesh.n_nodes(),"theta":theta,"c_ff":solved.c,"gradient_ff":solved.gradient,"relative_shift":shift,"solve_s":solved.elapsed,"true_relative_residual":solved.true_residual,"iterations":solved.iterations}));
        save(&result_path, &history, "running", &theta);
        if cycle == cycles {
            break;
        }
        let eta = indicators(&current, &solved.phi);
        let marked = dorfler(&eta);
        let mut trial_state = state.clone();
        let refined = trial_state.refine(&marked, &RefineOpts::default()).unwrap();
        println!(
            "refined {} marked -> {} tets",
            marked.len(),
            refined.mesh.mesh.n_tets()
        );
        if refined.mesh.mesh.n_tets() > max_tets {
            save(
                &result_path,
                &history,
                "refinement_budget_exceeded_before_next_solve",
                &theta,
            );
            break;
        }
        state = trial_state;
        fields = prolong(&fields, refined.node_prolongation.as_ref());
        previous = Some(solved.c);
    }
    // Optimization is deliberately after the resolution study, on a fixed mesh.
    // A separate validation pass below must still establish design stability.
    let target = target();
    let mut last = None;
    let mut cached = None;
    let mut steps: Vec<Step> = Vec::new();
    for step in 0..20 {
        let mut current = fixture(state.mesh(), &fx);
        current.mesh = moved(&state.mesh().mesh, &fields, &theta);
        let s = cached.take().unwrap_or_else(|| solve(&current, &fields));
        let ratio = min_tet_volume_ratio(&state.mesh().mesh, &current.mesh);
        println!(
            "opt {step}: C={:.9}, theta={theta:?}, ratio={ratio:.5}",
            s.c
        );
        history.push(json!({"kind":"optimization","step":step,"c_ff":s.c,"gradient_ff":s.gradient,"theta":theta,"min_volume_ratio":ratio,"solve_s":s.elapsed,"true_relative_residual":s.true_residual,"iterations":s.iterations}));
        save(&result_path, &history, "optimizing", &theta);
        steps.push(Step {
            c: s.c,
            theta,
            ratio,
        });
        if (s.c - target).abs() < 0.005 {
            last = Some(s);
            break;
        }
        let norm: f64 = s
            .gradient
            .iter()
            .zip(SCALE)
            .map(|(g, w)| (g * w).powi(2))
            .sum();
        let mut delta: [f64; 3] =
            std::array::from_fn(|i| (target - s.c) * s.gradient[i] * SCALE[i].powi(2) / norm);
        let trust = delta
            .iter()
            .zip(SCALE)
            .map(|(d, w)| (d / w).abs())
            .fold(0.0_f64, f64::max);
        if trust > 0.3 {
            for d in &mut delta {
                *d *= 0.3 / trust;
            }
        }
        let mut accepted = false;
        for _ in 0..14 {
            let trial = std::array::from_fn(|i| theta[i] + delta[i]);
            let m = moved(&state.mesh().mesh, &fields, &trial);
            if min_tet_volume_ratio(&state.mesh().mesh, &m) >= 0.25 {
                let mut candidate = current.clone();
                candidate.mesh = m;
                let t = solve(&candidate, &fields);
                if (t.c - target).abs() < (s.c - target).abs() {
                    theta = trial;
                    cached = Some(t);
                    accepted = true;
                    break;
                }
            }
            for d in &mut delta {
                *d *= 0.5;
            }
        }
        if !accepted {
            break;
        }
    }

    let mut export_fx = fx.clone();
    export_fx.mesh = moved(&fx.mesh, &coarse_fields, &theta);
    export_palace(&export_fx, &out, "final_coarse.msh");
    let coarse_check = solve(&export_fx, &coarse_fields);
    history.push(json!({"kind":"final_coarse_cross_solver_reference","c_ff":coarse_check.c,"gradient_ff":coarse_check.gradient}));
    let Some(last) = last else {
        save(&result_path, &history, "optimization_stalled", &theta);
        return;
    };
    let mut final_fx = fixture(state.mesh(), &fx);
    final_fx.mesh = moved(&state.mesh().mesh, &fields, &theta);
    let adaptive_export = out.join("adaptive");
    fs::create_dir_all(&adaptive_export).unwrap();
    export_palace(&final_fx, &adaptive_export, "final_adapted.msh");
    let design = design_dimensions(&final_fx);
    fs::write(
        out.join("design.json"),
        serde_json::to_string_pretty(&json!({"island_bbox_um":design.bbox,"island_length_um":design.length,"pad_width_um":design.width,
            "note":"Written by the generator from the island sheet nodes of the final adapted mesh (the exported polygonal geometry), not from accumulated optimization parameters."})).unwrap() + "\n",
    )
    .unwrap();
    // FD uses fully re-solved perturbed fields, not a frozen-field energy.
    let mut checks = Vec::new();
    for i in 0..3 {
        let h = 1e-4;
        let mut tp = theta;
        tp[i] += h;
        let mut tm = theta;
        tm[i] -= h;
        let mut plus = final_fx.clone();
        plus.mesh = moved(&state.mesh().mesh, &fields, &tp);
        let mut minus = final_fx.clone();
        minus.mesh = moved(&state.mesh().mesh, &fields, &tm);
        let fd = (solve(&plus, &fields).c - solve(&minus, &fields).c) / (2.0 * h);
        let rel = (fd - last.gradient[i]).abs() / fd.abs().max(1e-12);
        assert!(rel < 1e-3, "final FD parameter {i}: {rel}");
        history.push(json!({"kind":"final_gradient_check","parameter":i,"h":h,"analytic_ff":last.gradient[i],"fd_ff":fd,"relative_error":rel}));
        checks.push((last.gradient[i], fd, rel));
        save(&result_path, &history, "validating", &theta);
    }
    drop(final_fx);
    // Held-out series: keep refining the final geometry, independently of the
    // optimization. These solves never update theta and do not feed its target.
    // Round 0 is the optimization mesh; marking uses the final design's field.
    let max_rounds = env_usize("REFINED_OPT_HELD_OUT_ROUNDS", 7);
    let series_max_tets = env_usize("REFINED_OPT_HELD_OUT_MAX_TETS", 4_000_000);
    let with_x0 = env_usize("REFINED_OPT_HELD_OUT_X0", 1) == 1;
    let mut rounds: Vec<Round> = Vec::new();
    for round in 0..=max_rounds {
        let base = state.mesh().mesh.clone();
        let mut at_final = fixture(state.mesh(), &fx);
        at_final.mesh = moved(&base, &fields, &theta);
        let ratio = min_tet_volume_ratio(&base, &at_final.mesh);
        let fin = solve(&at_final, &fields);
        let x0 = with_x0.then(|| solve(&fixture(state.mesh(), &fx), &fields));
        println!(
            "held-out round {round}: {} tets, final C={:.9}, X0 C={:?}, ratio={ratio:.5}, {:.1}s",
            base.n_tets(),
            fin.c,
            x0.as_ref().map(|s| s.c),
            fin.elapsed
        );
        let side = |s: &Solve| json!({"c_ff":s.c,"gradient_ff":s.gradient,"solve_s":s.elapsed,"true_relative_residual":s.true_residual,"iterations":s.iterations});
        history.push(json!({"kind":"held_out_round","round":round,"n_tets":base.n_tets(),"n_nodes":base.n_nodes(),"min_volume_ratio":ratio,
            "relative_shift_from_round_0":rounds.first().map(|r: &Round| (fin.c - r.fin.c) / r.fin.c),
            "final_design":side(&fin),"x0":x0.as_ref().map(side)}));
        save(&result_path, &history, "held_out_series_running", &theta);
        let marked = (round < max_rounds).then(|| dorfler(&indicators(&at_final, &fin.phi)));
        rounds.push(Round {
            n_tets: base.n_tets(),
            n_nodes: base.n_nodes(),
            ratio,
            fin,
            x0,
        });
        let Some(marked) = marked else { break };
        drop(at_final);
        let refined = state.refine(&marked, &RefineOpts::default()).unwrap();
        if refined.mesh.mesh.n_tets() > series_max_tets {
            println!(
                "held-out: next round would have {} tets, above the cap; stop",
                refined.mesh.mesh.n_tets()
            );
            break;
        }
        fields = prolong(&fields, refined.node_prolongation.as_ref());
    }
    let n: Vec<f64> = rounds.iter().map(|r| r.n_tets as f64).collect();
    let c_final: Vec<f64> = rounds.iter().map(|r| r.fin.c).collect();
    let ex_final = extrapolate(&n, &c_final);
    let ex_x0 = with_x0
        .then(|| {
            extrapolate(
                &n,
                &rounds
                    .iter()
                    .map(|r| r.x0.as_ref().unwrap().c)
                    .collect::<Vec<_>>(),
            )
        })
        .flatten();
    history.push(json!({"kind":"held_out_summary","final_design":ex_final.as_ref().map(Extrapolation::json),"x0":ex_x0.as_ref().map(Extrapolation::json),
        "note":"upper_bound_ff is rigorous (nested conforming spaces, energy minimum, rounded up); every other number is an estimate with the stated method, not a bound"}));
    save(
        &result_path,
        &history,
        "target_reached_on_optimization_mesh_held_out_series_recorded",
        &theta,
    );
    write_summary(
        &out,
        &Summary {
            target,
            theta,
            steps: &steps,
            checks: &checks,
            coarse: &coarse_check,
            design: &design,
            rounds: &rounds,
            ex_final: ex_final.as_ref(),
            ex_x0: ex_x0.as_ref(),
            cycles,
        },
    );
}
