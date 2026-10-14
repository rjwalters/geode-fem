//! **Two-sided bound on the converged transmon `C_Σ`** (issue #1062).
//!
//! Every conforming (P1, P2) value of #1057 and #1058 is an upper bound on
//! the converged `C_Σ` of its polygonal geometry. This generator adds the
//! other side on the same meshes, with the complementary-energy certificate of
//! [`geode_core::assembly::electrostatic_dual`]:
//!
//! - **equilibrated flux** (Prager–Synge / Braess–Schöberl): the primal
//!   potential is equilibrated patch by patch into a Raviart–Thomas `RT_k`
//!   field that is divergence-free, normal-continuous off the sheets and
//!   neutral on the floating feedline, then certified;
//! - **Crouzeix–Raviart**: the lowest-order mixed (RT0) dual solve, an
//!   independent check that uses no primal field.
//!
//! It solves two mesh families, each checked against its committed primal
//! artifact:
//!
//! 1. **Uniform (#1057)**: X⁰ on the committed fixture and its red
//!    refinements: P1 at h, h/2, h/4, P2 at h, h/2 (scalar ε), and P1 at h,
//!    h/2, h/4 with the sapphire tensor. Every primal value must reproduce
//!    `c_sigma_convergence.toml`.
//! 2. **Adaptive (#1058)**: the ten P1 preparation cycles are replayed, then
//!    the seven held-out rounds of each optimized design (primary and
//!    resolution-corrected, with `theta` read from their `summary.toml`),
//!    and X⁰ on the primary design's meshes. Every P2 value must reproduce
//!    the committed `summary.toml`, so the meshes are the same.
//!
//! All meshes of one geometry share its polygonal surfaces (refinement does
//! not move them), so the largest certified lower bound and the smallest
//! conforming value over a whole family bracket the same number. X⁰'s
//! uniform and adaptive families are the same geometry.
//!
//! **Superseded anchor (issue #1066).** The two designs bracketed by the
//! default parts are the #1056 / #1058 `p2_cycles10*` designs, aimed at the
//! mis-derived 89.9 fF anchor (#1063, #1066); every anchor comparison in
//! `c_sigma_bracket.toml` (`anchor_ff`, `contains_anchor`,
//! `*_anchor_at_least_ff`) uses that superseded value. The X⁰ brackets do not
//! depend on any anchor and remain current.
//!
//! **Current anchor (issue #1075).** `BRACKET_PART=anchor_93p95` replays the
//! same ten preparation cycles and the seven held-out rounds of the two
//! re-anchored #1066 designs (`p2_cycles10_anchor_93p95*`, aimed at
//! `TransmonAnchor::devicelayout`, `C_Σ` ≈ 93.9537 fF, junction excluded),
//! asserts every P2 value against their `summary.toml`, certifies the same
//! lower bounds and writes `c_sigma_bracket_anchor_93p95.toml`. X⁰ is not
//! re-solved: the design increment is certified against X⁰'s committed
//! bracket in `c_sigma_bracket.toml`. Every comparison with the anchor there
//! carries #1066's resonator-hybridization caveat: the bare-qubit-consistent
//! target is below 93.9537 fF by an estimated 0.10–0.38 fF, ceiling about
//! 0.6 fF, so the anchor itself is uncertain at the width of the bracket.
//!
//! Run with (local, no cloud):
//!
//! ```text
//!   cargo run -p geode-core --release --example transmon_c_sigma_bracket
//! ```
//!
//! Environment: `BRACKET_PART` (`uniform`, `adaptive`, `all` or
//! `anchor_93p95`, default `all`), `BRACKET_KS` (RT orders equilibrated from P2, default `1,2`),
//! `BRACKET_K_LAST` (an extra order at each design's last held-out round,
//! default `3`; empty for none), `BRACKET_ROUNDS` (held-out rounds, default
//! 7), `BRACKET_OUT` (output TOML, default
//! `benchmarks/transmon_diffopt/c_sigma_bracket.toml`, or
//! `benchmarks/transmon_diffopt/c_sigma_bracket_anchor_93p95.toml` for
//! `anchor_93p95`). The TOML carries no timings and is bit-reproducible;
//! timings go to standard output.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use geode_core::adapt::refine::{BisectionMesh, RefineOpts};
use geode_core::assembly::electrostatic::{
    AmgPcgOptions, AmgPcgReport, CapacitanceMatrix, EPS_0, Electrode, assemble_electrostatic,
    assemble_electrostatic_p2_sheets, assemble_electrostatic_p2_sheets_tensor,
    assemble_electrostatic_tensor, extract_capacitance_amg_pcg_fields,
    extract_capacitance_p2_amg_pcg_fields, tet_bary_grads,
};
use geode_core::assembly::electrostatic_dual::{
    ConductorRole, DualBound, DualConductor, FaceTopology, Permittivity, PrimalPotential,
    certify_lower_bound, crouzeix_raviart_flux, equilibrated_flux,
};
use geode_core::mesh::red_refine::red_refine;
use geode_core::mesh::transmon::sapphire_eps_lab;
use geode_core::mesh::{
    MetalRole, TaggedTetMesh, TetMesh, TransmonFixture, read_transmon_smoke_fixture,
};
use geode_core::quantum::transmon::TransmonAnchor;
use geode_core::shape::transmon_morph::{TransmonMorphRoles, TransmonMorphSpec, parameter_fields};

/// The SUPERSEDED 89.9 fF anchor (#1066) the bracketed #1056 / #1058 designs
/// were aimed at. Not the current target: that is
/// [`TransmonAnchor::devicelayout`].
const ANCHOR_FF: f64 = 89.9;
/// #1057 committed values (fF): P1 h, h/2, h/4; P2 h, h/2; tensor P1.
const X0_P1_FF: [f64; 3] = [137.706752, 101.359397, 87.753422];
const X0_P2_FF: [f64; 2] = [90.044141, 83.015285];
const X0_TENSOR_FF: [f64; 3] = [136.684731, 102.375301, 89.493342];
/// #1058's preparation settings (both runs).
const PREP_CYCLES: usize = 10;
const PREP_MAX_TETS: usize = 1_000_000;
/// The #1056 / #1058 designs (SUPERSEDED 89.9 fF anchor).
const DESIGNS: [(&str, &str); 2] = [
    (
        "primary",
        "benchmarks/transmon_diffopt/refined_optimization/p2_cycles10",
    ),
    (
        "resolution_corrected",
        "benchmarks/transmon_diffopt/refined_optimization/p2_cycles10_resolution_corrected",
    ),
];
/// The re-anchored #1066 designs (current 93.9537 fF anchor), issue #1075.
const DESIGNS_93P95: [(&str, &str); 2] = [
    (
        "anchor_93p95",
        "benchmarks/transmon_diffopt/refined_optimization/p2_cycles10_anchor_93p95",
    ),
    (
        "anchor_93p95_resolution_corrected",
        "benchmarks/transmon_diffopt/refined_optimization/p2_cycles10_anchor_93p95_resolution_corrected",
    ),
];
/// #1066's estimate of how far the bare-qubit-consistent `C_Σ` target lies
/// BELOW the pure-LC anchor (g/2π = 50–100 MHz, first order), fF. Not
/// measured; quoted from the `[anchor]` caveat of the #1066 `summary.toml`.
const HYBRIDIZATION_SHIFT_ESTIMATE_FF: [f64; 2] = [0.10, 0.38];
/// #1043's same-mesh Palace-vs-LC offset read as a ceiling on that shift, fF.
const HYBRIDIZATION_SHIFT_CEILING_FF: f64 = 0.6;
const BRACKET_TOML: &str = "benchmarks/transmon_diffopt/c_sigma_bracket.toml";
const BRACKET_93P95_TOML: &str = "benchmarks/transmon_diffopt/c_sigma_bracket_anchor_93p95.toml";
const BRACKET_TENSOR_TOML: &str = "benchmarks/transmon_diffopt/c_sigma_bracket_tensor.toml";

fn env_list(name: &str, default: &str) -> Vec<usize> {
    std::env::var(name)
        .unwrap_or_else(|_| default.into())
        .split(',')
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.trim().parse().unwrap())
        .collect()
}

fn floor_fmt(x: f64, dp: usize) -> String {
    let p = 10f64.powi(dp as i32);
    format!("{:.dp$}", (x * p).floor() / p)
}

fn ceil_fmt(x: f64, dp: usize) -> String {
    let p = 10f64.powi(dp as i32);
    format!("{:.dp$}", (x * p).ceil() / p)
}

fn floor_to(x: f64, dp: usize) -> f64 {
    let p = 10f64.powi(dp as i32);
    (x * p).floor() / p
}

fn ceil_to(x: f64, dp: usize) -> f64 {
    let p = 10f64.powi(dp as i32);
    (x * p).ceil() / p
}

/// Sheet triangles and node sets of the three metal components.
#[derive(Clone)]
struct Sheets {
    tris: [Vec<[u32; 3]>; 3],
    nodes: [Vec<u32>; 3],
}

impl Sheets {
    fn of(fx: &TransmonFixture) -> Self {
        let comps = fx.split_metal_conductors();
        let get = |role: MetalRole| {
            let c: Vec<_> = comps.iter().filter(|c| c.role == role).collect();
            assert_eq!(c.len(), 1, "one {role:?} component");
            (c[0].triangles.clone(), c[0].nodes.clone())
        };
        let (ti, ni) = get(MetalRole::Island);
        let (tf, nf) = get(MetalRole::Feedline);
        let (tg, ng) = get(MetalRole::Ground);
        Self {
            tris: [ti, tf, tg],
            nodes: [ni, nf, ng],
        }
    }

    fn red_refined(&self, r: &geode_core::mesh::red_refine::RedRefinement) -> Self {
        Self {
            nodes: std::array::from_fn(|i| r.lift_sheet_nodes(&self.nodes[i], &self.tris[i])),
            tris: std::array::from_fn(|i| r.lift_triangles(&self.tris[i])),
        }
    }

    fn electrodes(&self) -> Vec<Electrode> {
        vec![
            Electrode {
                name: "island".into(),
                nodes: self.nodes[0].clone(),
                voltage: 1.0,
            },
            Electrode {
                name: "feedline".into(),
                nodes: self.nodes[1].clone(),
                voltage: 0.0,
            },
        ]
    }

    fn dual(&self) -> Vec<DualConductor> {
        let roles = [
            ("island", ConductorRole::Driven),
            ("feedline", ConductorRole::Floating),
            ("ground", ConductorRole::Ground),
        ];
        roles
            .iter()
            .zip(&self.tris)
            .map(|((name, role), t)| DualConductor {
                name: (*name).into(),
                role: *role,
                triangles: t.clone(),
            })
            .collect()
    }
}

#[derive(Clone, Copy)]
enum Eps<'a> {
    Scalar(&'a [f64]),
    Tensor(&'a [[[f64; 3]; 3]]),
}

impl<'a> Eps<'a> {
    fn dual(self) -> Permittivity<'a> {
        match self {
            Eps::Scalar(e) => Permittivity::Scalar(e),
            Eps::Tensor(e) => Permittivity::Tensor(e),
        }
    }
}

struct Primal {
    c_ff: f64,
    phi: Vec<f64>,
    order: usize,
    iters: Vec<usize>,
    res: f64,
}

fn reduce(cm: &CapacitanceMatrix, phis: &[Vec<f64>]) -> (f64, Vec<f64>) {
    let vf = -cm.c[1][0] / cm.c[1][1];
    let phi = phis[0]
        .iter()
        .zip(&phis[1])
        .map(|(a, b)| a + vf * b)
        .collect();
    (cm.c[0][0] + vf * cm.c[0][1], phi)
}

/// The conforming `C_Σ` (feedline floating) and its reduced potential.
fn primal(mesh: &TetMesh, eps: Eps<'_>, sheets: &Sheets, order: usize) -> Primal {
    let electrodes = sheets.electrodes();
    let ground = &sheets.nodes[2];
    let rho = vec![0.0; mesh.n_tets()];
    let opts = AmgPcgOptions::default();
    let (cm, rep, phis): (CapacitanceMatrix, AmgPcgReport, Vec<Vec<f64>>) = match (order, eps) {
        (1, Eps::Scalar(e)) => {
            let sys = assemble_electrostatic(mesh, e, &rho, &electrodes, ground).unwrap();
            extract_capacitance_amg_pcg_fields(&sys, &electrodes, ground, opts).unwrap()
        }
        (1, Eps::Tensor(e)) => {
            let sys = assemble_electrostatic_tensor(mesh, e, &rho, &electrodes, ground).unwrap();
            extract_capacitance_amg_pcg_fields(&sys, &electrodes, ground, opts).unwrap()
        }
        (2, Eps::Scalar(e)) => {
            let (sys, dofs) = assemble_electrostatic_p2_sheets(
                mesh,
                e,
                &rho,
                &electrodes,
                &[&sheets.tris[0], &sheets.tris[1]],
                ground,
                &sheets.tris[2],
            )
            .unwrap();
            extract_capacitance_p2_amg_pcg_fields(&sys, &dofs, opts).unwrap()
        }
        (2, Eps::Tensor(e)) => {
            let (sys, dofs) = assemble_electrostatic_p2_sheets_tensor(
                mesh,
                e,
                &rho,
                &electrodes,
                &[&sheets.tris[0], &sheets.tris[1]],
                ground,
                &sheets.tris[2],
            )
            .unwrap();
            extract_capacitance_p2_amg_pcg_fields(&sys, &dofs, opts).unwrap()
        }
        _ => panic!("orders 1 and 2 only"),
    };
    let res = rep.rel_residual.iter().copied().fold(0.0, f64::max);
    assert!(res <= 1e-9, "PCG true residual {res}");
    let (c, phi) = reduce(&cm, &phis);
    Primal {
        c_ff: c * 1e15,
        phi,
        order,
        iters: rep.iterations,
        res,
    }
}

/// One certified lower bound.
struct Lower {
    method: String,
    c_ff: f64,
    bound: DualBound,
}

impl Lower {
    /// Share of the driven charge the certificate charged for jumps,
    /// divergence and floating charge.
    fn penalty(&self) -> f64 {
        (self.bound.charges[0] - self.bound.numerator) / self.bound.charges[0]
    }
}

fn lowers(
    mesh: &TetMesh,
    eps: Eps<'_>,
    sheets: &Sheets,
    p: &Primal,
    ks: &[usize],
    with_cr: bool,
) -> Vec<Lower> {
    let topo = FaceTopology::new(mesh, &sheets.dual()).unwrap();
    let e = eps.dual();
    let mut out = Vec::new();
    let primal = if p.order == 1 {
        PrimalPotential::P1(&p.phi)
    } else {
        PrimalPotential::P2(&p.phi)
    };
    for &k in ks {
        let t = Instant::now();
        let flux = equilibrated_flux(mesh, &topo, e, primal, k).unwrap();
        let bound = certify_lower_bound(mesh, &topo, e, &flux).unwrap();
        let l = Lower {
            method: format!("eq_k{k}"),
            c_ff: bound.c_lower_f * 1e15,
            bound,
        };
        println!(
            "    lower eq RT{k}: {:.6} fF (penalty {:.2e}, {:.1}s)",
            l.c_ff,
            l.penalty(),
            t.elapsed().as_secs_f64()
        );
        assert!(l.c_ff <= p.c_ff, "lower {} above upper {}", l.c_ff, p.c_ff);
        out.push(l);
    }
    if with_cr {
        let t = Instant::now();
        let (flux, rep) = crouzeix_raviart_flux(mesh, &topo, e, AmgPcgOptions::default()).unwrap();
        let bound = certify_lower_bound(mesh, &topo, e, &flux).unwrap();
        let l = Lower {
            method: "cr".into(),
            c_ff: bound.c_lower_f * 1e15,
            bound,
        };
        println!(
            "    lower CR/RT0: {:.6} fF (CR energy {:.6} fF, {} free faces, PCG {:?}, penalty {:.2e}, {:.1}s)",
            l.c_ff,
            rep.c_cr_f * 1e15,
            rep.n_free,
            rep.pcg.iterations,
            l.penalty(),
            t.elapsed().as_secs_f64()
        );
        assert!(l.c_ff <= p.c_ff);
        out.push(l);
    }
    out
}

/// One mesh of a family.
struct Row {
    label: String,
    order: usize,
    n_tets: usize,
    upper_ff: f64,
    iters: Vec<usize>,
    res: f64,
    lowers: Vec<Lower>,
    /// Scalar-`ε` conforming value on the same mesh (tensor rows, #1091).
    scalar_upper: Option<f64>,
}

impl Row {
    fn best_lower(&self) -> Option<&Lower> {
        self.lowers.iter().max_by(|a, b| a.c_ff.total_cmp(&b.c_ff))
    }
}

fn row_toml(t: &mut String, table: &str, r: &Row) {
    let _ = writeln!(t, "[[{table}]]");
    let _ = writeln!(t, "mesh = \"{}\"", r.label);
    let _ = writeln!(t, "order = \"P{}\"", r.order);
    let _ = writeln!(t, "n_tets = {}", r.n_tets);
    let _ = writeln!(
        t,
        "upper_ff = {}  # conforming primal, ceiled",
        ceil_fmt(r.upper_ff, 6)
    );
    let _ = writeln!(
        t,
        "pcg_iterations = {:?}\npcg_max_true_rel_residual = {:.3e}",
        r.iters, r.res
    );
    for l in &r.lowers {
        let _ = writeln!(
            t,
            "lower_{}_ff = {}  # certified, floored; certificate penalty {:.3e} of the island charge",
            l.method,
            floor_fmt(l.c_ff, 6),
            l.penalty()
        );
    }
    if let Some(b) = r.best_lower() {
        let _ = writeln!(
            t,
            "width_ff = {}  # upper (ceiled) minus best lower on this mesh (floored)",
            ceil_fmt(ceil_to(r.upper_ff, 6) - floor_to(b.c_ff, 6), 6)
        );
    }
    if let Some(su) = r.scalar_upper {
        let _ = writeln!(
            t,
            "scalar_upper_ff = {su:.9}  # scalar trace-averaged eps, same mesh and order (asserted against the committed value)"
        );
        let _ = writeln!(
            t,
            "scalar_minus_tensor_ff = {:.9}  # conforming scalar minus conforming tensor, same mesh: a discretization-level difference, not a bound",
            su - r.upper_ff
        );
    }
}

/// The family bracket: largest certified lower bound and smallest
/// conforming value over all meshes of one geometry.
struct Bracket {
    lower: f64,
    lower_from: String,
    upper: f64,
    upper_from: String,
}

fn bracket(rows: &[&Row]) -> Bracket {
    let (mut lower, mut lower_from) = (f64::NEG_INFINITY, String::new());
    let (mut upper, mut upper_from) = (f64::INFINITY, String::new());
    for r in rows {
        if r.upper_ff < upper {
            upper = r.upper_ff;
            upper_from = format!("{} P{}", r.label, r.order);
        }
        for l in &r.lowers {
            if l.c_ff > lower {
                lower = l.c_ff;
                lower_from = format!("{} P{} {}", r.label, r.order, l.method);
            }
        }
    }
    Bracket {
        lower,
        lower_from,
        upper,
        upper_from,
    }
}

/// The anchor-independent part of a family bracket; returns the rounded
/// `(lower, upper)`.
fn bracket_core_toml(t: &mut String, table: &str, b: &Bracket, note: &str) -> (f64, f64) {
    let (lo, hi) = (floor_to(b.lower, 2), ceil_to(b.upper, 2));
    let _ = writeln!(t, "\n[{table}]  # {note}");
    let _ = writeln!(
        t,
        "lower_ff = {lo:.2}  # RIGOROUS: largest certified lower bound of the family, floored"
    );
    let _ = writeln!(t, "lower_from = \"{}\"", b.lower_from);
    let _ = writeln!(
        t,
        "upper_ff = {hi:.2}  # RIGOROUS: smallest conforming value of the family, ceiled"
    );
    let _ = writeln!(t, "upper_from = \"{}\"", b.upper_from);
    let _ = writeln!(t, "width_ff = {:.2}", hi - lo);
    let _ = writeln!(
        t,
        "width_relative_to_lower = {}",
        ceil_fmt((hi - lo) / lo, 4)
    );
    (lo, hi)
}

/// A family bracket compared with the SUPERSEDED 89.9 fF anchor.
fn bracket_toml(t: &mut String, table: &str, b: &Bracket, note: &str) {
    let (lo, hi) = bracket_core_toml(t, table, b, note);
    let _ = writeln!(
        t,
        "contains_anchor = {}  # SUPERSEDED {ANCHOR_FF} fF anchor (#1066) inside [lower_ff, upper_ff]; not the current target",
        lo <= ANCHOR_FF && ANCHOR_FF <= hi
    );
    if hi < ANCHOR_FF {
        let _ = writeln!(
            t,
            "below_anchor_at_least_ff = {}",
            floor_fmt(ANCHOR_FF - hi, 2)
        );
    } else if lo > ANCHOR_FF {
        let _ = writeln!(
            t,
            "above_anchor_at_least_ff = {}",
            floor_fmt(lo - ANCHOR_FF, 2)
        );
    }
}

/// A family bracket compared with the CURRENT anchor (#1066), with the
/// resonator-hybridization caveat attached to every comparison (#1075).
fn bracket_toml_current(t: &mut String, table: &str, b: &Bracket, anchor: f64, note: &str) {
    let (lo, hi) = bracket_core_toml(t, table, b, note);
    let _ = writeln!(
        t,
        "contains_anchor = {}  # current {anchor:.4} fF pure-LC anchor inside [lower_ff, upper_ff]; NOT resolved at the hybridization shift (see [meta])",
        lo <= anchor && anchor <= hi
    );
    let _ = writeln!(
        t,
        "anchor_relative = [{}, {}]  # (family bound / anchor - 1) from the unrounded bounds, rounded outward",
        floor_fmt(b.lower / anchor - 1.0, 4),
        ceil_fmt(b.upper / anchor - 1.0, 4)
    );
    if hi < anchor {
        let _ = writeln!(
            t,
            "below_anchor_at_least_ff = {}",
            floor_fmt(anchor - hi, 2)
        );
    } else if lo > anchor {
        let _ = writeln!(
            t,
            "above_anchor_at_least_ff = {}",
            floor_fmt(lo - anchor, 2)
        );
    }
    let [s_lo, s_hi] = HYBRIDIZATION_SHIFT_ESTIMATE_FF;
    let meets = |a: f64, z: f64| lo <= z && a <= hi;
    let _ = writeln!(
        t,
        "meets_bare_qubit_estimate_band = {}  # [lower_ff, upper_ff] intersects [anchor - {s_hi:.2}, anchor - {s_lo:.2}]: an ESTIMATE band, g not measured",
        meets(anchor - s_hi, anchor - s_lo)
    );
    let _ = writeln!(
        t,
        "meets_bare_qubit_ceiling_band = {}  # [lower_ff, upper_ff] intersects [anchor - {HYBRIDIZATION_SHIFT_CEILING_FF:.1}, anchor]: the #1043 ceiling, not a bound",
        meets(anchor - HYBRIDIZATION_SHIFT_CEILING_FF, anchor)
    );
}

/// The design increment over X⁰: both are certified brackets of converged
/// values, so `C_design - C_X0` lies in
/// `[lower_design - upper_X0, upper_design - lower_X0]`, rounded outward.
fn increment_toml(t: &mut String, table: &str, b: &Bracket, x0: (f64, f64), note: &str) {
    let lo = floor_to(b.lower - x0.1, 2);
    let hi = ceil_to(b.upper - x0.0, 2);
    let _ = writeln!(t, "\n[{table}]  # {note}");
    let _ = writeln!(
        t,
        "lower_ff = {lo:.2}  # RIGOROUS: design family lower bound minus X0 family upper bound, floored"
    );
    let _ = writeln!(
        t,
        "upper_ff = {hi:.2}  # RIGOROUS: design family upper bound minus X0 family lower bound, ceiled"
    );
    let _ = writeln!(t, "width_ff = {:.2}", hi - lo);
}

/// X⁰'s family bounds from the committed `c_sigma_bracket.toml`: the largest
/// floored lower bound and the smallest ceiled upper bound over its uniform
/// and adaptive rows, checked against its `[bracket_x0]`.
fn committed_x0_bounds() -> (f64, f64) {
    let text = fs::read_to_string(BRACKET_TOML).unwrap();
    let v: toml::Value = text.parse().unwrap();
    let (mut lo, mut hi) = (f64::NEG_INFINITY, f64::INFINITY);
    for table in ["uniform_x0", "adaptive_x0"] {
        for r in v[table].as_array().unwrap() {
            hi = hi.min(r["upper_ff"].as_float().unwrap());
            for (k, x) in r.as_table().unwrap() {
                if k.starts_with("lower_") && k.ends_with("_ff") {
                    lo = lo.max(x.as_float().unwrap());
                }
            }
        }
    }
    let b = &v["bracket_x0"];
    assert!((floor_to(lo, 2) - b["lower_ff"].as_float().unwrap()).abs() < 1e-9);
    assert!((ceil_to(hi, 2) - b["upper_ff"].as_float().unwrap()).abs() < 1e-9);
    (lo, hi)
}

// ─── #1058 replay (verbatim from examples/transmon_refined_optimize.rs) ───

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

fn indicators(fx: &TransmonFixture, phi: &[f64]) -> Vec<f64> {
    use std::collections::{BTreeSet, HashMap};
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

/// The committed `summary.toml` of a #1058 run: theta and the held-out values.
struct Committed {
    theta: [f64; 3],
    c_ff: Vec<f64>,
    x0_c_ff: Vec<Option<f64>>,
    n_tets: Vec<usize>,
}

fn committed(dir: &Path) -> Committed {
    let text = fs::read_to_string(dir.join("summary.toml")).unwrap();
    let v: toml::Value = text.parse().unwrap();
    let th = v["optimization"]["theta"].as_array().unwrap();
    let theta = std::array::from_fn(|i| th[i].as_float().unwrap());
    let rounds = v["held_out"].as_array().unwrap();
    Committed {
        theta,
        c_ff: rounds
            .iter()
            .map(|r| r["c_ff"].as_float().unwrap())
            .collect(),
        x0_c_ff: rounds
            .iter()
            .map(|r| r.get("x0_c_ff").and_then(toml::Value::as_float))
            .collect(),
        n_tets: rounds
            .iter()
            .map(|r| r["n_tets"].as_integer().unwrap() as usize)
            .collect(),
    }
}

fn check(tag: &str, got: f64, want: f64) {
    // Committed with 9 decimals (adaptive) or 6 (uniform).
    assert!(
        (got - want).abs() <= 2e-6,
        "{tag}: replayed {got:.9} fF vs committed {want:.9} fF"
    );
}

#[allow(clippy::too_many_lines)]
fn uniform(fx: &TransmonFixture, ks: &[usize]) -> (Vec<Row>, Vec<Row>) {
    let mut mesh = fx.mesh.clone();
    let mut sheets = Sheets::of(fx);
    let mut eps = fx.epsilon_r_scalar();
    let lab = sapphire_eps_lab();
    let identity = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    let mut eps_t: Vec<[[f64; 3]; 3]> = fx
        .tet_physical_tags
        .iter()
        .map(|&t| {
            if t == fx.tags.substrate {
                lab
            } else {
                identity
            }
        })
        .collect();
    let (mut scalar, mut tensor) = (Vec::new(), Vec::new());
    for level in 0..3 {
        if level > 0 {
            let r = red_refine(&mesh);
            assert_eq!(r.n_flipped, 0);
            sheets = sheets.red_refined(&r);
            eps = r.lift_per_tet(&eps);
            eps_t = r.lift_per_tet(&eps_t);
            mesh = r.mesh;
        }
        let label = format!("uniform h/{}", 1 << level);
        let mut orders = vec![1];
        if level < 2 {
            orders.push(2);
        }
        for order in orders {
            let t = Instant::now();
            let p = primal(&mesh, Eps::Scalar(&eps), &sheets, order);
            println!(
                "  X0 {label} P{order}: {} tets, upper {:.6} fF ({:.1}s)",
                mesh.n_tets(),
                p.c_ff,
                t.elapsed().as_secs_f64()
            );
            let want = if order == 1 {
                X0_P1_FF[level]
            } else {
                X0_P2_FF[level]
            };
            assert!((p.c_ff - want).abs() < 1e-5, "{label} P{order} vs #1057");
            // P1 fluxes are piecewise constant: RT1 already holds ψ_a σ_h.
            let kk: Vec<usize> = if order == 1 { vec![1] } else { ks.to_vec() };
            let l = lowers(&mesh, Eps::Scalar(&eps), &sheets, &p, &kk, order == 1);
            scalar.push(Row {
                label: label.clone(),
                order,
                n_tets: mesh.n_tets(),
                upper_ff: p.c_ff,
                iters: p.iters.clone(),
                res: p.res,
                lowers: l,
                scalar_upper: None,
            });
        }
        let t = Instant::now();
        let p = primal(&mesh, Eps::Tensor(&eps_t), &sheets, 1);
        println!(
            "  X0 tensor {label} P1: upper {:.6} fF ({:.1}s)",
            p.c_ff,
            t.elapsed().as_secs_f64()
        );
        assert!(
            (p.c_ff - X0_TENSOR_FF[level]).abs() < 1e-5,
            "tensor {label}"
        );
        let l = lowers(&mesh, Eps::Tensor(&eps_t), &sheets, &p, &[1], true);
        tensor.push(Row {
            label,
            order: 1,
            n_tets: mesh.n_tets(),
            upper_ff: p.c_ff,
            iters: p.iters.clone(),
            res: p.res,
            lowers: l,
            scalar_upper: None,
        });
    }
    (scalar, tensor)
}

/// The per-tet sapphire lab tensor (substrate) and the identity (vacuum).
fn tensor_eps(fx: &TransmonFixture) -> Vec<[[f64; 3]; 3]> {
    let lab = sapphire_eps_lab();
    let identity = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    fx.tet_physical_tags
        .iter()
        .map(|&t| {
            if t == fx.tags.substrate {
                lab
            } else {
                identity
            }
        })
        .collect()
}

/// Rows per design, plus X⁰ rows on the meshes of every design whose
/// `summary.toml` has X⁰ values (when `with_x0`).
///
/// With `tensor` (issue #1091) the meshes are still built from the scalar
/// solves (the scalar P2 value of every round is asserted against the
/// committed one, and the marking uses it), and each row then carries the
/// **sapphire-tensor** P2 primal and its certified lower bounds, with the
/// scalar value of the same mesh in `scalar_upper`.
#[allow(clippy::too_many_lines, clippy::too_many_arguments)]
fn adaptive(
    fx_um: &TransmonFixture,
    design_list: &[(&str, &str)],
    with_x0: bool,
    ks: &[usize],
    k_last: &[usize],
    max_rounds: usize,
    tensor: bool,
) -> (Vec<(String, Vec<Row>)>, Vec<Row>) {
    let mut fx = fx_um.clone();
    for p in &mut fx.mesh.nodes {
        for x in p {
            *x *= 1e-6;
        }
    }
    let roles =
        TransmonMorphRoles::identify(&fx, &fx.mesh, TransmonMorphSpec::fixture_default(1e-6))
            .unwrap();
    let mut fields = parameter_fields(&fx.mesh, &roles).unwrap().to_vec();
    let tagged = TaggedTetMesh {
        mesh: fx.mesh.clone(),
        tet_physical_tags: fx.tet_physical_tags.clone(),
        boundary_triangles: fx.boundary_triangles.clone(),
        triangle_physical_tags: fx.triangle_physical_tags.clone(),
    };
    let mut state = BisectionMesh::new(tagged).unwrap();
    // Preparation cycles: P1 at theta = 0, flux-jump marking.
    for cycle in 0..=PREP_CYCLES {
        let current = fixture(state.mesh(), &fx);
        let sheets = Sheets::of(&current);
        let p = primal(
            &current.mesh,
            Eps::Scalar(&current.epsilon_r_scalar()),
            &sheets,
            1,
        );
        println!(
            "  prep cycle {cycle}: {} tets, P1 {:.9} fF",
            current.mesh.n_tets(),
            p.c_ff
        );
        if cycle == PREP_CYCLES {
            break;
        }
        let marked = dorfler(&indicators(&current, &p.phi));
        let mut trial = state.clone();
        let refined = trial.refine(&marked, &RefineOpts::default()).unwrap();
        assert!(refined.mesh.mesh.n_tets() <= PREP_MAX_TETS);
        state = trial;
        fields = prolong(&fields, refined.node_prolongation.as_ref());
    }
    let (prep_state, prep_fields) = (state, fields);
    let mut designs = Vec::new();
    let mut x0_rows = Vec::new();
    for &(name, dir) in design_list {
        let want = committed(Path::new(dir));
        let (mut state, mut fields) = (prep_state.clone(), prep_fields.clone());
        let mut rows = Vec::new();
        let rounds = max_rounds.min(want.c_ff.len() - 1);
        for round in 0..=rounds {
            let base = state.mesh().mesh.clone();
            assert_eq!(
                base.n_tets(),
                want.n_tets[round],
                "{name} round {round} mesh"
            );
            let mut at_final = fixture(state.mesh(), &fx);
            at_final.mesh = moved(&base, &fields, &want.theta);
            let sheets = Sheets::of(&at_final);
            let eps = at_final.epsilon_r_scalar();
            let t = Instant::now();
            let p = primal(&at_final.mesh, Eps::Scalar(&eps), &sheets, 2);
            println!(
                "  {name} round {round}: {} tets, upper {:.9} fF ({:.1}s)",
                base.n_tets(),
                p.c_ff,
                t.elapsed().as_secs_f64()
            );
            check(&format!("{name} round {round}"), p.c_ff, want.c_ff[round]);
            let mut kk = ks.to_vec();
            if round == rounds {
                kk.extend(k_last.iter().filter(|k| !ks.contains(k)));
            }
            let marked = (round < rounds).then(|| dorfler(&indicators(&at_final, &p.phi)));
            let row = if tensor {
                let scalar_c = p.c_ff;
                drop(p);
                let et = tensor_eps(&at_final);
                let t = Instant::now();
                let pt = primal(&at_final.mesh, Eps::Tensor(&et), &sheets, 2);
                println!(
                    "  {name} round {round} TENSOR: upper {:.9} fF (scalar - tensor {:+.6} fF, {:.1}s)",
                    pt.c_ff,
                    scalar_c - pt.c_ff,
                    t.elapsed().as_secs_f64()
                );
                let l = lowers(&at_final.mesh, Eps::Tensor(&et), &sheets, &pt, &kk, true);
                Row {
                    label: format!("{name} round {round}"),
                    order: 2,
                    n_tets: base.n_tets(),
                    upper_ff: pt.c_ff,
                    iters: pt.iters.clone(),
                    res: pt.res,
                    lowers: l,
                    scalar_upper: Some(scalar_c),
                }
            } else {
                let l = lowers(&at_final.mesh, Eps::Scalar(&eps), &sheets, &p, &kk, true);
                let row = Row {
                    label: format!("{name} round {round}"),
                    order: 2,
                    n_tets: base.n_tets(),
                    upper_ff: p.c_ff,
                    iters: p.iters.clone(),
                    res: p.res,
                    lowers: l,
                    scalar_upper: None,
                };
                drop(p);
                row
            };
            rows.push(row);
            if let Some(x0) = want.x0_c_ff[round].filter(|_| with_x0) {
                let at_x0 = fixture(state.mesh(), &fx);
                let s0 = Sheets::of(&at_x0);
                let e0 = at_x0.epsilon_r_scalar();
                let p0 = primal(&at_x0.mesh, Eps::Scalar(&e0), &s0, 2);
                println!("  X0 on {name} round {round}: upper {:.9} fF", p0.c_ff);
                check(&format!("X0 {name} round {round}"), p0.c_ff, x0);
                let mut kk0 = ks.to_vec();
                if round == rounds {
                    kk0.extend(k_last.iter().filter(|k| !ks.contains(k)));
                }
                let row0 = if tensor {
                    let scalar_c = p0.c_ff;
                    drop(p0);
                    let et0 = tensor_eps(&at_x0);
                    let pt0 = primal(&at_x0.mesh, Eps::Tensor(&et0), &s0, 2);
                    println!(
                        "  X0 on {name} round {round} TENSOR: upper {:.9} fF (scalar - tensor {:+.6} fF)",
                        pt0.c_ff,
                        scalar_c - pt0.c_ff
                    );
                    let l0 = lowers(&at_x0.mesh, Eps::Tensor(&et0), &s0, &pt0, &kk0, false);
                    Row {
                        label: format!("{name} round {round}"),
                        order: 2,
                        n_tets: at_x0.mesh.n_tets(),
                        upper_ff: pt0.c_ff,
                        iters: pt0.iters.clone(),
                        res: pt0.res,
                        lowers: l0,
                        scalar_upper: Some(scalar_c),
                    }
                } else {
                    let l0 = lowers(&at_x0.mesh, Eps::Scalar(&e0), &s0, &p0, &kk0, false);
                    Row {
                        label: format!("{name} round {round}"),
                        order: 2,
                        n_tets: at_x0.mesh.n_tets(),
                        upper_ff: p0.c_ff,
                        iters: p0.iters.clone(),
                        res: p0.res,
                        lowers: l0,
                        scalar_upper: None,
                    }
                };
                x0_rows.push(row0);
            }
            let Some(marked) = marked else { break };
            drop(at_final);
            let refined = state.refine(&marked, &RefineOpts::default()).unwrap();
            fields = prolong(&fields, refined.node_prolongation.as_ref());
        }
        designs.push((name.to_string(), rows));
    }
    (designs, x0_rows)
}

/// Issue #1091: X⁰ with the sapphire tensor at P2 on the uniform h and h/2
/// levels (the scalar P2 values are the committed #1057 ones).
fn uniform_tensor_p2(fx: &TransmonFixture, ks: &[usize]) -> Vec<Row> {
    let mut mesh = fx.mesh.clone();
    let mut sheets = Sheets::of(fx);
    let mut eps_t = tensor_eps(fx);
    let mut rows = Vec::new();
    for (level, &scalar_p2) in X0_P2_FF.iter().enumerate() {
        if level > 0 {
            let r = red_refine(&mesh);
            assert_eq!(r.n_flipped, 0);
            sheets = sheets.red_refined(&r);
            eps_t = r.lift_per_tet(&eps_t);
            mesh = r.mesh;
        }
        let label = format!("uniform h/{}", 1 << level);
        let t = Instant::now();
        let p = primal(&mesh, Eps::Tensor(&eps_t), &sheets, 2);
        println!(
            "  X0 tensor {label} P2: {} tets, upper {:.6} fF (scalar - tensor {:+.6} fF, {:.1}s)",
            mesh.n_tets(),
            p.c_ff,
            scalar_p2 - p.c_ff,
            t.elapsed().as_secs_f64()
        );
        let l = lowers(&mesh, Eps::Tensor(&eps_t), &sheets, &p, ks, false);
        rows.push(Row {
            label,
            order: 2,
            n_tets: mesh.n_tets(),
            upper_ff: p.c_ff,
            iters: p.iters.clone(),
            res: p.res,
            lowers: l,
            scalar_upper: Some(scalar_p2),
        });
    }
    rows
}

/// The unrounded scalar family bounds `(max lower, min upper)` of the rows
/// of `tables` in a committed bracket artifact.
fn committed_family(path: &str, tables: &[&str]) -> (f64, f64) {
    let text = fs::read_to_string(path).unwrap();
    let v: toml::Value = text.parse().unwrap();
    let (mut lo, mut hi) = (f64::NEG_INFINITY, f64::INFINITY);
    for table in tables {
        for r in v[*table].as_array().unwrap() {
            hi = hi.min(r["upper_ff"].as_float().unwrap());
            for (k, x) in r.as_table().unwrap() {
                if k.starts_with("lower_") && k.ends_with("_ff") {
                    lo = lo.max(x.as_float().unwrap());
                }
            }
        }
    }
    (lo, hi)
}

/// Least-squares fit `g(N) = g_inf + a N^-p` over `(N, g)` (grid in `p`,
/// linear in `(g_inf, a)`); returns `(p, g_inf, a, rms)`. An estimate.
fn power_fit(pts: &[(f64, f64)]) -> (f64, f64, f64, f64) {
    let mut best = (f64::NAN, f64::NAN, f64::NAN, f64::INFINITY);
    for i in 1..=4000 {
        let p = f64::from(i) * 1e-3;
        let (mut sx, mut sy, mut sxx, mut sxy) = (0.0, 0.0, 0.0, 0.0);
        let n = pts.len() as f64;
        for &(nn, g) in pts {
            let x = nn.powf(-p);
            sx += x;
            sy += g;
            sxx += x * x;
            sxy += x * g;
        }
        let a = (n * sxy - sx * sy) / (n * sxx - sx * sx);
        let g_inf = (sy - a * sx) / n;
        let rms = (pts
            .iter()
            .map(|&(nn, g)| (g_inf + a * nn.powf(-p) - g).powi(2))
            .sum::<f64>()
            / n)
            .sqrt();
        if rms < best.3 {
            best = (p, g_inf, a, rms);
        }
    }
    best
}

/// One reference `C_Σ` for the verdicts: a point and a band, fF.
struct Target {
    key: &'static str,
    point: f64,
    band: (f64, f64),
    note: &'static str,
}

/// The three anchors of #1066, #1078 and #1079, each with the band spanned
/// by the corners of the `k` (and `L_s`) bands through the two-node
/// `TransmonAnchor` constructors. The bands committed by #1078 / #1079
/// (`benchmarks/transmon_coupling/results.toml`, `.../transmon_loop_inductance/
/// results.toml`) also carry the resonator's odd harmonics, which only lower
/// the band's LOWER edge (to 93.826 / 93.087 fF); the upper edges agree to
/// 1e-4 fF. An "above" verdict reads only the upper edge, so it is the same
/// against either band (`tests/transmon_diffopt.rs` checks this).
fn targets() -> Vec<Target> {
    use geode_core::quantum::transmon::{
        DEVICELAYOUT_COUPLING_K_BAND, DEVICELAYOUT_LOOP_INDUCTANCE_BAND_H,
    };
    let ff = |a: TransmonAnchor| a.c_sigma_farad * 1e15;
    let dressed = ff(TransmonAnchor::devicelayout());
    let (k_lo, k_hi) = DEVICELAYOUT_COUPLING_K_BAND;
    let bare = [
        ff(TransmonAnchor::devicelayout_bare_with_coupling(k_lo)),
        ff(TransmonAnchor::devicelayout_bare_with_coupling(k_hi)),
    ];
    let (l_lo, l_hi) = DEVICELAYOUT_LOOP_INDUCTANCE_BAND_H;
    let mut loops = Vec::new();
    for k in [k_lo, k_hi] {
        for l in [l_lo, l_hi] {
            loops.push(ff(TransmonAnchor::devicelayout_bare_loop_with(k, l)));
        }
    }
    let span = |v: &[f64]| {
        (
            v.iter().copied().fold(f64::INFINITY, f64::min),
            v.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        )
    };
    vec![
        Target {
            key: "devicelayout",
            point: dressed,
            band: (dressed, dressed),
            note: "pure-LC C_Sigma of the published 4.14 GHz (#1066)",
        },
        Target {
            key: "devicelayout_bare",
            point: ff(TransmonAnchor::devicelayout_bare()),
            band: span(&bare),
            note: "bare-qubit, coupling k band (#1078)",
        },
        Target {
            key: "devicelayout_bare_loop",
            point: ff(TransmonAnchor::devicelayout_bare_loop()),
            band: span(&loops),
            note: "bare-qubit and loop inductance, k and L_s bands (#1079)",
        },
    ]
}

/// Verdict lines of a rounded bracket `[lo, hi]` against one target.
fn verdict_toml(t: &mut String, lo: f64, hi: f64, g: &Target) {
    let (b_lo, b_hi) = g.band;
    let v = if hi < b_lo {
        format!("below by at least {} fF", floor_fmt(b_lo - hi, 2))
    } else if lo > b_hi {
        format!("above by at least {} fF", floor_fmt(lo - b_hi, 2))
    } else if lo <= g.point && g.point <= hi {
        "contains the point".to_string()
    } else {
        "meets the band, not the point".to_string()
    };
    let _ = writeln!(
        t,
        "vs_{} = \"{v}\"  # point {:.4} fF, band [{:.4}, {:.4}] fF: {}",
        g.key, g.point, b_lo, b_hi, g.note
    );
}

/// `c_sigma_bracket_tensor.toml` (#1091): the sapphire-tensor brackets of X⁰
/// and both re-anchored designs, the scalar-vs-tensor gap and the restated
/// verdicts.
#[allow(clippy::too_many_lines)]
fn write_tensor(
    out: &Path,
    uni: &[Row],
    x0_rows: &[Row],
    designs: &[(String, Vec<Row>)],
    ks: &[usize],
    k_last: &[usize],
    max_rounds: usize,
) {
    let lab = sapphire_eps_lab();
    let diag_sapphire = [[9.3, 0.0, 0.0], [0.0, 9.3, 0.0], [0.0, 0.0, 11.5]];
    let rot_dev = (0..3)
        .flat_map(|i| (0..3).map(move |j| (i, j)))
        .map(|(i, j)| (lab[i][j] - diag_sapphire[i][j]).abs())
        .fold(0.0_f64, f64::max);
    let mut t = String::new();
    let _ = writeln!(
        t,
        "# Auto-generated by `BRACKET_PART=tensor cargo run -p geode-core --release --example transmon_c_sigma_bracket`.\n\
         # Do NOT edit by hand. Consumed by `tests/transmon_diffopt.rs`. No timings: bit-reproducible.\n\
         # Issue #1091: the SAPPHIRE-TENSOR C_Sigma (the permittivity model of the Palace/DeviceLayout anchor), certified on both\n\
         # sides on the meshes of c_sigma_bracket_anchor_93p95.toml (the meshes are rebuilt from the scalar solves, which are\n\
         # asserted against the committed values), with the scalar-minus-tensor gap per mesh and per family.\n\
         # TARGET STATUS (#1063, 2026-10-12): 4.14 GHz is DeviceLayout's INITIAL-geometry frequency, not a design target; the\n\
         # anchor verdicts below are against that misread target and are supporting material only.\n"
    );
    let _ = writeln!(t, "[meta]");
    let _ = writeln!(t, "issue = 1091");
    let _ = writeln!(t, "method_issue = 1062");
    let _ = writeln!(
        t,
        "generator = \"crates/geode-core/examples/transmon_c_sigma_bracket.rs\""
    );
    let _ = writeln!(t, "part = \"tensor\"");
    let _ = writeln!(
        t,
        "fixture = \"crates/geode-core/tests/fixtures/transmon_smoke.msh\""
    );
    let _ = writeln!(
        t,
        "quantity = \"island C_Sigma with the feedline floating and neutral (C_ii - C_if^2/C_ff), SAPPHIRE LAB TENSOR on the substrate, 1 in vacuum; the meshed polygonal geometry\""
    );
    let _ = writeln!(
        t,
        "tensor = \"R diag(9.3, 9.3, 11.5) R^T with R = MaterialAxes [[0.8,0.6,0],[-0.6,0.8,0],[0,0,1]] (geode_core::mesh::transmon::sapphire_eps_lab)\""
    );
    let _ = writeln!(
        t,
        "sapphire_rotation_is_identity = true  # R is a rotation about z and the in-plane principal values are equal, so R diag(a,a,b) R^T = diag(a,a,b) exactly"
    );
    let _ = writeln!(
        t,
        "sapphire_lab_max_abs_deviation_from_diag = {rot_dev:.3e}  # |sapphire_eps_lab - diag(9.3, 9.3, 11.5)| in f64: rounding only"
    );
    for (name, dir) in DESIGNS_93P95 {
        let _ = writeln!(t, "primal_{name} = \"{dir}/summary.toml\"");
    }
    let _ = writeln!(
        t,
        "scalar_bracket_source = \"c_sigma_bracket.toml [uniform_x0, adaptive_x0] for X0; c_sigma_bracket_anchor_93p95.toml for the designs (unrounded family bounds)\""
    );
    let _ = writeln!(
        t,
        "equilibration_orders_from_p2 = {ks:?}\nextra_order_at_last_round = {k_last:?}\nheld_out_rounds = {max_rounds}"
    );
    let _ = writeln!(t, "eps0 = {EPS_0:e}  # F/m; coordinates in metres");
    method_toml(&mut t);
    let _ = writeln!(
        t,
        "tensor_method = \"P2 tensor stiffness int grad N_i . eps grad N_j (exact 4-point rule, assembly::electrostatic::assemble_electrostatic_p2_sheets_tensor), AMG-PCG with the #1061/#1064 guards; the certificate and the equilibration are those of #1062 with eps a constant SPD tensor per tet (assembly::electrostatic_dual module docs, 'Anisotropic permittivity')\"\n"
    );
    for r in uni {
        row_toml(&mut t, "uniform_x0_tensor_p2", r);
    }
    for r in x0_rows {
        row_toml(&mut t, "adaptive_x0_tensor", r);
    }
    for (name, rows) in designs {
        for r in rows {
            row_toml(&mut t, &format!("adaptive_{name}_tensor"), r);
        }
    }
    // Family brackets.
    let x0_all: Vec<&Row> = uni.iter().chain(x0_rows).collect();
    let bx0 = bracket(&x0_all);
    bracket_core_toml(
        &mut t,
        "bracket_x0_tensor",
        &bx0,
        "X0, sapphire tensor: uniform P2 levels and the anchor_93p95 adaptive meshes (one geometry)",
    );
    let x0_s = committed_family(BRACKET_TOML, &["uniform_x0", "adaptive_x0"]);
    let gap_toml = |t: &mut String, table: &str, b: &Bracket, s: (f64, f64), note: &str| {
        // tensor - scalar of the converged values, certified (positive:
        // the tensor C_Sigma is the larger).
        let lo = floor_to(b.lower - s.1, 2);
        let hi = ceil_to(b.upper - s.0, 2);
        let _ = writeln!(t, "\n[{table}]  # {note}");
        let _ = writeln!(
            t,
            "scalar_lower_ff = {}\nscalar_upper_ff = {}",
            floor_fmt(s.0, 6),
            ceil_fmt(s.1, 6)
        );
        let _ = writeln!(
            t,
            "tensor_minus_scalar_lower_ff = {lo:.2}  # RIGOROUS: tensor family lower minus scalar family upper, floored"
        );
        let _ = writeln!(
            t,
            "tensor_minus_scalar_upper_ff = {hi:.2}  # RIGOROUS: tensor family upper minus scalar family lower, ceiled"
        );
        let _ = writeln!(
            t,
            "tensor_minus_scalar_relative = [{}, {}]  # of the scalar value: [(tensor lower - scalar upper) / scalar upper, (tensor upper - scalar lower) / scalar lower] from the UNROUNDED family bounds, rounded outward; not outward against the rounded fF values above",
            floor_fmt((b.lower - s.1) / s.1, 4),
            ceil_fmt((b.upper - s.0) / s.0, 4)
        );
    };
    gap_toml(&mut t, "gap_x0", &bx0, x0_s, "certified converged gap, X0");
    for (name, rows) in designs {
        let rr: Vec<&Row> = rows.iter().collect();
        let b = bracket(&rr);
        let (lo, hi) = bracket_core_toml(
            &mut t,
            &format!("bracket_{name}_tensor"),
            &b,
            "the re-anchored #1066 design, sapphire tensor, its held-out meshes",
        );
        for g in targets() {
            verdict_toml(&mut t, lo, hi, &g);
        }
        let s = committed_family(BRACKET_93P95_TOML, &[&format!("adaptive_{name}")]);
        gap_toml(
            &mut t,
            &format!("gap_{name}"),
            &b,
            s,
            "certified converged gap, this design",
        );
        let ilo = floor_to(b.lower - bx0.upper, 2);
        let ihi = ceil_to(b.upper - bx0.lower, 2);
        let _ = writeln!(
            t,
            "\n[increment_{name}_over_x0_tensor]  # certified C_design - C_X0, both sapphire tensor"
        );
        let _ = writeln!(
            t,
            "lower_ff = {ilo:.2}  # RIGOROUS: design tensor family lower minus X0 tensor family upper, floored"
        );
        let _ = writeln!(
            t,
            "upper_ff = {ihi:.2}  # RIGOROUS: design tensor family upper minus X0 tensor family lower, ceiled"
        );
        let _ = writeln!(t, "width_ff = {:.2}", ihi - ilo);
    }
    // Observed order of the same-mesh gap (an estimate, not a bound).
    let mut series: Vec<(&str, Vec<&Row>)> = vec![("x0", x0_rows.iter().collect())];
    for (name, rows) in designs {
        series.push((name.as_str(), rows.iter().collect()));
    }
    for (name, rows) in series {
        let pts: Vec<(f64, f64)> = rows
            .iter()
            .skip(2)
            .map(|r| (r.n_tets as f64, r.scalar_upper.unwrap() - r.upper_ff))
            .collect();
        let (p, g_inf, a, rms) = power_fit(&pts);
        let last = rows.last().unwrap();
        let s_last = last.scalar_upper.unwrap();
        let _ = writeln!(
            t,
            "\n[gap_order_{name}]  # ESTIMATE: least-squares g(N) = g_inf + a N^-p of the same-mesh scalar-minus-tensor conforming values, rounds 2-{max_rounds}, N = tets"
        );
        let _ = writeln!(
            t,
            "n_tets = {:?}",
            rows.iter().map(|r| r.n_tets).collect::<Vec<_>>()
        );
        let _ = writeln!(
            t,
            "same_mesh_gap_ff = [{}]  # scalar minus tensor (the Table 3 sign), rounds 0-{max_rounds}",
            rows.iter()
                .map(|r| format!("{:.6}", r.scalar_upper.unwrap() - r.upper_ff))
                .collect::<Vec<_>>()
                .join(", ")
        );
        let _ = writeln!(
            t,
            "same_mesh_gap_relative = [{}]  # of the scalar value",
            rows.iter()
                .map(|r| format!(
                    "{:.6}",
                    (r.scalar_upper.unwrap() - r.upper_ff) / r.scalar_upper.unwrap()
                ))
                .collect::<Vec<_>>()
                .join(", ")
        );
        let _ = writeln!(
            t,
            "observed_order_p = {p:.3}  # in N = tets (multiply by 3 for the order in h)"
        );
        let _ = writeln!(t, "extrapolated_gap_ff = {g_inf:.4}");
        let _ = writeln!(
            t,
            "extrapolated_gap_relative = {:.5}  # of the last scalar value",
            g_inf / s_last
        );
        let _ = writeln!(t, "fit_coefficient_a = {a:.6e}\nfit_rms_ff = {rms:.3e}");
    }
    fs::write(out, t).unwrap();
}

/// `c_sigma_bracket_anchor_93p95.toml` (#1075): the re-anchored #1066
/// designs, compared with the current anchor under the hybridization caveat,
/// and their certified increment over X⁰'s committed bracket.
fn write_anchor_93p95(
    out: &Path,
    designs: &[(String, Vec<Row>)],
    ks: &[usize],
    k_last: &[usize],
    max_rounds: usize,
) {
    let anchor = TransmonAnchor::devicelayout().c_sigma_farad * 1e15;
    let x0 = committed_x0_bounds();
    let [s_lo, s_hi] = HYBRIDIZATION_SHIFT_ESTIMATE_FF;
    let mut t = String::new();
    let _ = writeln!(
        t,
        "# Auto-generated by `BRACKET_PART=anchor_93p95 cargo run -p geode-core --release --example transmon_c_sigma_bracket`.\n\
         # Do NOT edit by hand. Consumed by `tests/transmon_diffopt.rs`. No timings: bit-reproducible.\n\
         # CURRENT ANCHOR (#1066): the bracketed designs are the re-anchored p2_cycles10_anchor_93p95* runs, aimed at\n\
         # geode_core::quantum::transmon::TransmonAnchor::devicelayout, C_Sigma {anchor:.4} fF (junction excluded).\n\
         # HYBRIDIZATION CAVEAT: that anchor is the pure-LC inversion of a resonator-hybridized 4.14 GHz mode; the bare-qubit-consistent\n\
         # target is BELOW it by an estimated {s_lo:.2}-{s_hi:.2} fF (ceiling about {HYBRIDIZATION_SHIFT_CEILING_FF} fF). Each bracket is a certified statement about the\n\
         # converged electrostatic C_Sigma of its polygonal geometry; an anchor comparison finer than that shift is not resolved.\n"
    );
    let _ = writeln!(t, "[meta]");
    let _ = writeln!(t, "issue = 1075");
    let _ = writeln!(t, "method_issue = 1062");
    let _ = writeln!(
        t,
        "generator = \"crates/geode-core/examples/transmon_c_sigma_bracket.rs\""
    );
    let _ = writeln!(t, "part = \"anchor_93p95\"");
    let _ = writeln!(
        t,
        "fixture = \"crates/geode-core/tests/fixtures/transmon_smoke.msh\""
    );
    let _ = writeln!(
        t,
        "quantity = \"island C_Sigma with the feedline floating and neutral (C_ii - C_if^2/C_ff), scalar trace-averaged sapphire eps; the meshed polygonal geometry\""
    );
    for (name, dir) in DESIGNS_93P95 {
        let _ = writeln!(t, "primal_{name} = \"{dir}/summary.toml\"");
    }
    let _ = writeln!(
        t,
        "anchor_ff = {anchor:?}  # CURRENT (#1066): pure-LC C_Sigma of the published 4.14 GHz, junction excluded"
    );
    let _ = writeln!(
        t,
        "hybridization_shift_estimate_ff = [{s_lo:.2}, {s_hi:.2}]  # ESTIMATE (#1066, g/2pi = 50-100 MHz, g not measured): the bare-qubit target lies BELOW anchor_ff by this much"
    );
    let _ = writeln!(
        t,
        "hybridization_shift_ceiling_ff = {HYBRIDIZATION_SHIFT_CEILING_FF}  # #1043 same-mesh Palace-vs-LC offset (0.281 %): a ceiling, not a bound"
    );
    let _ = writeln!(
        t,
        "x0_bracket_source = \"benchmarks/transmon_diffopt/c_sigma_bracket.toml [bracket_x0] (anchor-independent; not re-solved here)\""
    );
    let _ = writeln!(
        t,
        "x0_family_lower_ff = {}  # largest certified X0 lower bound of that artifact (floored there)",
        floor_fmt(x0.0, 6)
    );
    let _ = writeln!(
        t,
        "x0_family_upper_ff = {}  # smallest X0 conforming value of that artifact (ceiled there)",
        ceil_fmt(x0.1, 6)
    );
    let _ = writeln!(t, "equilibration_orders_from_p2 = {ks:?}");
    let _ = writeln!(t, "extra_order_at_last_round = {k_last:?}");
    let _ = writeln!(t, "held_out_rounds = {max_rounds}");
    let _ = writeln!(t, "eps0 = {EPS_0:e}  # F/m; coordinates in metres\n");
    method_toml(&mut t);
    for (name, rows) in designs {
        for r in rows {
            row_toml(&mut t, &format!("adaptive_{name}"), r);
        }
    }
    for (name, rows) in designs {
        let rows: Vec<&Row> = rows.iter().collect();
        bracket_toml_current(
            &mut t,
            &format!("bracket_{name}"),
            &bracket(&rows),
            anchor,
            "the optimized geometry of this #1066 run (aimed at the CURRENT anchor), its held-out meshes",
        );
    }
    for (name, rows) in designs {
        let rows: Vec<&Row> = rows.iter().collect();
        increment_toml(
            &mut t,
            &format!("increment_{name}_over_x0"),
            &bracket(&rows),
            x0,
            "certified C_design - C_X0 of the converged values; X0 from c_sigma_bracket.toml",
        );
    }
    fs::write(out, t).unwrap();
}

/// The `[method]` table, shared by both artifacts.
fn method_toml(t: &mut String) {
    let _ = writeln!(t, "[method]");
    let _ = writeln!(
        t,
        "upper = \"conforming P1/P2 Galerkin energy (primal minimum principle): every value is an upper bound\""
    );
    let _ = writeln!(
        t,
        "certificate = \"for any piecewise-polynomial D: C >= eps0 N^2 / int D.eps^-1 D with N = Q_island - |Q_feedline| - sum_F int|normal jump| - sum_T int|div D| (Thomson's principle plus Cauchy-Schwarz, 0 <= phi* <= 1); evaluated with exact quadrature, L1 terms bounded by |F|^1/2 ||.||_L2\""
    );
    let _ = writeln!(
        t,
        "eq = \"lower_eq_kK: Prager-Synge/Braess-Schoeberl patch equilibration of the conforming potential into RT_K on the sheet-cut mesh (div sigma = grad psi_a . sigma_h per patch), then certified\""
    );
    let _ = writeln!(
        t,
        "cr = \"lower_cr: Crouzeix-Raviart nonconforming solve = lowest-order mixed RT0 dual flux (Marini), AMG-PCG, then certified; independent of the primal field\""
    );
    let _ = writeln!(
        t,
        "rounding = \"lower bounds floored, upper bounds ceiled (6 decimals per mesh, 2 decimals per family); plain f64, not interval arithmetic: worst-case summation error of the certificate about 1e-10 relative at 3.3M tets, far below the rounding\""
    );
    let _ = writeln!(
        t,
        "family = \"refinement does not move the polygonal surfaces, so every mesh of one geometry bounds the same converged value; the family bracket is [max lower, min upper]\"\n"
    );
}

#[allow(clippy::too_many_lines)]
fn main() {
    let t0 = Instant::now();
    let part = std::env::var("BRACKET_PART").unwrap_or_else(|_| "all".into());
    let ks = env_list("BRACKET_KS", "1,2");
    let k_last = env_list("BRACKET_K_LAST", "3");
    let max_rounds = env_list("BRACKET_ROUNDS", "7")[0];
    let out = PathBuf::from(std::env::var("BRACKET_OUT").unwrap_or_else(|_| BRACKET_TOML.into()));
    let fx = read_transmon_smoke_fixture().expect("load transmon fixture");
    let mut scaled = fx.clone();
    for p in &mut scaled.mesh.nodes {
        for x in p {
            *x *= 1e-6;
        }
    }
    let (mut uni_s, mut uni_t) = (Vec::new(), Vec::new());
    if part == "uniform" || part == "all" {
        println!("uniform (#1057) family, X0:");
        (uni_s, uni_t) = uniform(&scaled, &ks);
    }
    let (mut designs, mut x0_adapt) = (Vec::new(), Vec::new());
    if part == "adaptive" || part == "all" {
        println!("adaptive (#1058) families:");
        (designs, x0_adapt) = adaptive(&fx, &DESIGNS, true, &ks, &k_last, max_rounds, false);
    }
    if part == "tensor" {
        println!("sapphire-tensor X0 uniform P2 (#1091):");
        let uni = uniform_tensor_p2(&scaled, &ks);
        println!("sapphire-tensor adaptive families (#1091), meshes of #1066:");
        let (designs, x0_rows) =
            adaptive(&fx, &DESIGNS_93P95, true, &ks, &k_last, max_rounds, true);
        let out = PathBuf::from(
            std::env::var("BRACKET_OUT").unwrap_or_else(|_| BRACKET_TENSOR_TOML.into()),
        );
        write_tensor(&out, &uni, &x0_rows, &designs, &ks, &k_last, max_rounds);
        println!(
            "wrote {} ({:.1} s total)",
            out.display(),
            t0.elapsed().as_secs_f64()
        );
        return;
    }
    if part == "anchor_93p95" {
        println!("adaptive (#1066) families at the current anchor (#1075):");
        let (designs, _) = adaptive(&fx, &DESIGNS_93P95, false, &ks, &k_last, max_rounds, false);
        let out = PathBuf::from(
            std::env::var("BRACKET_OUT").unwrap_or_else(|_| BRACKET_93P95_TOML.into()),
        );
        write_anchor_93p95(&out, &designs, &ks, &k_last, max_rounds);
        println!(
            "wrote {} ({:.1} s total)",
            out.display(),
            t0.elapsed().as_secs_f64()
        );
        return;
    }

    let mut t = String::new();
    let _ = writeln!(
        t,
        "# Auto-generated by `cargo run -p geode-core --release --example transmon_c_sigma_bracket`.\n\
         # Do NOT edit by hand. Consumed by `tests/transmon_diffopt.rs`. No timings: bit-reproducible."
    );
    let _ = writeln!(
        t,
        "# SUPERSEDED ANCHOR (issue #1066): every anchor comparison below (anchor_ff, contains_anchor, *_anchor_at_least_ff)\n\
         # uses the mis-derived {ANCHOR_FF} fF that the bracketed #1056/#1058 p2_cycles10* designs were aimed at (#1063, #1066).\n\
         # The X0 brackets are anchor-independent and current. The current anchor is geode_core::quantum::transmon::TransmonAnchor::devicelayout:\n\
         # C_Sigma {:.4} fF (junction excluded); its re-anchored p2_cycles10_anchor_93p95* designs are bracketed in c_sigma_bracket_anchor_93p95.toml (#1075).\n",
        TransmonAnchor::devicelayout().c_sigma_farad * 1e15
    );
    let _ = writeln!(t, "[meta]");
    let _ = writeln!(t, "issue = 1062");
    let _ = writeln!(
        t,
        "generator = \"crates/geode-core/examples/transmon_c_sigma_bracket.rs\""
    );
    let _ = writeln!(
        t,
        "fixture = \"crates/geode-core/tests/fixtures/transmon_smoke.msh\""
    );
    let _ = writeln!(
        t,
        "quantity = \"island C_Sigma with the feedline floating and neutral (C_ii - C_if^2/C_ff), scalar trace-averaged sapphire eps unless the table says tensor; the meshed polygonal geometry\""
    );
    let _ = writeln!(
        t,
        "anchor_ff = {ANCHOR_FF}  # SUPERSEDED (#1066): the target of the bracketed designs, not the current anchor"
    );
    let _ = writeln!(t, "part = \"{part}\"");
    let _ = writeln!(t, "equilibration_orders_from_p2 = {ks:?}");
    let _ = writeln!(t, "extra_order_at_last_round = {k_last:?}");
    let _ = writeln!(t, "held_out_rounds = {max_rounds}");
    let _ = writeln!(t, "eps0 = {EPS_0:e}  # F/m; coordinates in metres\n");
    method_toml(&mut t);
    for r in &uni_s {
        row_toml(&mut t, "uniform_x0", r);
    }
    for r in &uni_t {
        row_toml(&mut t, "uniform_x0_tensor", r);
    }
    for (name, rows) in &designs {
        for r in rows {
            row_toml(&mut t, &format!("adaptive_{name}"), r);
        }
    }
    for r in &x0_adapt {
        row_toml(&mut t, "adaptive_x0", r);
    }
    if !uni_s.is_empty() || !x0_adapt.is_empty() {
        let rows: Vec<&Row> = uni_s.iter().chain(&x0_adapt).collect();
        bracket_toml(
            &mut t,
            "bracket_x0",
            &bracket(&rows),
            "X0, scalar eps: the uniform and the primary-adaptive families are one geometry; anchor-independent, current",
        );
    }
    if !uni_t.is_empty() {
        let rows: Vec<&Row> = uni_t.iter().collect();
        bracket_toml(
            &mut t,
            "bracket_x0_tensor",
            &bracket(&rows),
            "X0, sapphire lab tensor, P1 uniform levels only",
        );
    }
    for (name, rows) in &designs {
        let rows: Vec<&Row> = rows.iter().collect();
        bracket_toml(
            &mut t,
            &format!("bracket_{name}"),
            &bracket(&rows),
            "the optimized geometry of this #1058 run (aimed at the SUPERSEDED 89.9 fF anchor), its held-out meshes",
        );
    }
    fs::write(&out, t).unwrap();
    println!(
        "wrote {} ({:.1} s total)",
        out.display(),
        t0.elapsed().as_secs_f64()
    );
}
