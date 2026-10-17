//! **ICEM reference motor, initial design: forward torque reproduction**
//! (motor paper M2, issue #1168).
//!
//! Solves the nonlinear 2-D magnetostatic field of the ICEM PMSM's initial
//! design on the antiperiodic 60° sector with the conforming sliding band
//! (#1161), the M27 law (#1160) and the tagged mesh family (#1162), at
//! mechanical angles 0..19° (1° steps, each warm-started from the previous
//! angle) and current levels 0, 0.5 and 1 of `I_peak = 9.6777 A`. Torque per
//! modelled sector (axial length 0.1 m, SymMult = 1) by Arkkio over the
//! 44.0–45.0 mm gap band, with the inner/outer half-bands and Maxwell
//! stress on circles as cross-checks. Model set-up and every convention:
//! `tests/common/icem_forward.rs`.
//!
//! ```text
//!   cargo run -p geode-core --release --example icem_initial_torque -- \
//!       --level 1 [--currents 0,1,2] [--angles 0-19] [--law table|clamp426] \
//!       [--tol default|tight] [--area mesh|exact] [--spectrum] \
//!       [--coupling conforming|harmonic:START:STEP:END|harmonic:all] [--ends tied|free] \
//!       [--out FILE.json]
//! ```
//!
//! `--coupling harmonic:3:6:45` replaces the conforming band by the
//! reference's harmonic coupling (issue #1170): weak continuity of the sine
//! and cosine moments of orders 3, 9, ..., 45 only, with Lagrange
//! multipliers. `harmonic:all` uses every antiperiodic order below the
//! grid's Nyquist limit, which reproduces the conforming band. Records then
//! carry the coupling torque `T_coupling` as well. `--ends free` leaves the
//! coupling-circle end nodes out of the radial-edge pairs, as the reference
//! does (`CouplingMode::HarmonicFreeEnds`).
//!
//! Writes one JSON record per run (default
//! `benchmarks/motor_reference/forward/runs/L{level}_{law}_{tol}_{area}.json`);
//! `benchmarks/motor_reference/forward/analyze.py` turns the records into
//! the tables, convergence study and figure. Meshes L1–L3 are gitignored:
//! regenerate them with the `geometry/README.md` command.

#[path = "../tests/common/icem_forward.rs"]
mod icem;

use std::fmt::Write as _;
use std::path::PathBuf;
use std::time::Instant;

use geode_core::assembly::magnetostatic_sector::CouplingMode;
use icem::{AreaMode, I_PEAK_INITIAL, IcemModel, IronLaw, LEVELS, PHI0_INITIAL_DEG};

struct Args {
    level: usize,
    currents: Vec<usize>,
    angles: Vec<i64>,
    law: IronLaw,
    tight: bool,
    area: AreaMode,
    spectrum: bool,
    /// `None`: conforming; `Some(None)`: every order; `Some(Some(v))`.
    harmonics: Option<Option<Vec<usize>>>,
    coupling_name: String,
    free_ends: bool,
    out: Option<PathBuf>,
}

fn parse_list(s: &str) -> Vec<i64> {
    let mut v = Vec::new();
    for part in s.split(',') {
        if let Some((a, b)) = part.split_once('-') {
            let (a, b): (i64, i64) = (a.parse().expect("range"), b.parse().expect("range"));
            v.extend(a..=b);
        } else {
            v.push(part.parse().expect("integer"));
        }
    }
    v
}

fn parse_args() -> Args {
    let mut a = Args {
        level: 0,
        currents: vec![0, 1, 2],
        angles: (0..20).collect(),
        law: IronLaw::Table,
        tight: false,
        area: AreaMode::Mesh,
        spectrum: false,
        harmonics: None,
        coupling_name: "conforming".into(),
        free_ends: false,
        out: None,
    };
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < argv.len() {
        if argv[i] == "--spectrum" {
            a.spectrum = true;
            i += 1;
            continue;
        }
        let val = argv.get(i + 1).cloned().unwrap_or_default();
        match argv[i].as_str() {
            "--level" => a.level = val.parse().expect("--level N"),
            "--currents" => a.currents = parse_list(&val).into_iter().map(|x| x as usize).collect(),
            "--angles" => a.angles = parse_list(&val),
            "--law" => {
                a.law = match val.as_str() {
                    "table" => IronLaw::Table,
                    "clamp426" => IronLaw::RefClamp426,
                    o => panic!("unknown --law {o}"),
                }
            }
            "--tol" => {
                a.tight = match val.as_str() {
                    "default" => false,
                    "tight" => true,
                    o => panic!("unknown --tol {o}"),
                }
            }
            "--area" => {
                a.area = match val.as_str() {
                    "mesh" => AreaMode::Mesh,
                    "exact" => AreaMode::Exact,
                    o => panic!("unknown --area {o}"),
                }
            }
            "--coupling" => {
                if val == "conforming" {
                    a.harmonics = None;
                    a.coupling_name = "conforming".into();
                } else if val == "harmonic:all" {
                    a.harmonics = Some(None);
                    a.coupling_name = "hall".into();
                } else if let Some(spec) = val.strip_prefix("harmonic:") {
                    let p: Vec<usize> = spec
                        .split(':')
                        .map(|x| x.parse().expect("harmonic:START:STEP:END"))
                        .collect();
                    assert!(p.len() == 3 && p[1] > 0, "harmonic:START:STEP:END");
                    a.harmonics = Some(Some((p[0]..=p[2]).step_by(p[1]).collect()));
                    a.coupling_name = format!("h{}", p[2]);
                } else {
                    panic!("unknown --coupling {val}");
                }
            }
            "--ends" => {
                a.free_ends = match val.as_str() {
                    "tied" => false,
                    "free" => true,
                    o => panic!("unknown --ends {o}"),
                }
            }
            "--out" => a.out = Some(PathBuf::from(&val)),
            o => panic!("unknown argument {o}"),
        }
        i += 2;
    }
    a
}

/// Maxwell-stress circle radii (m): mid inner band, mid outer band, and
/// two near the band edges.
const MAXWELL_RADII: [f64; 4] = [0.04410, 0.04425, 0.04475, 0.04490];

/// Harmonic cut-offs of the `--spectrum` diagnostic (45 is the reference's
/// highest coupling harmonic, `boundaries.coupling_sine_and_cosine_harmonics`).
const SPECTRUM_CUTS: [usize; 5] = [9, 21, 45, 93, 189];
/// Rings of the `--spectrum` diagnostic across the 44.0-45.0 mm band.
const SPECTRUM_RINGS: usize = 20;

fn main() {
    let args = parse_args();
    let reference = icem::load_reference();
    let exact = icem::load_exact_areas();
    let path = icem::mesh_path(args.level);
    let bytes = std::fs::read(&path).unwrap_or_else(|e| {
        panic!(
            "read {}: {e}. L1-L3 are gitignored; regenerate them with the \
             benchmarks/motor_reference/geometry/README.md command",
            path.display()
        )
    });
    let t_setup = Instant::now();
    let mut model = IcemModel::new(
        args.level,
        &bytes,
        &reference,
        args.law,
        args.area,
        Some(&exact),
    )
    .expect("model");
    let setup_s = t_setup.elapsed().as_secs_f64();
    let rep = model.band.report().clone();
    let orders: Option<Vec<usize>> = args.harmonics.as_ref().map(|h| match h {
        Some(v) => v.clone(),
        None => model.band.all_harmonic_orders(),
    });
    assert!(
        !args.free_ends || orders.is_some(),
        "--ends free needs a harmonic --coupling"
    );
    let coupling_name = if args.free_ends {
        format!("{}f", args.coupling_name)
    } else {
        args.coupling_name.clone()
    };
    if let Some(o) = &orders {
        model.coupling = if args.free_ends {
            CouplingMode::HarmonicFreeEnds(o.clone())
        } else {
            CouplingMode::Harmonic(o.clone())
        };
    }
    let tol_name = if args.tight { "tight" } else { "default" };
    let area_name = match args.area {
        AreaMode::Mesh => "mesh",
        AreaMode::Exact => "exact",
    };
    let cfg = icem::newton_config(args.tight);
    let n_maxwell = 8 * rep.n_coupling;
    println!(
        "ICEM initial design, L{}: {} nodes, {} triangles ({} reoriented on read), N = {} \
         coupling steps ({} per degree), r_c = {:.6} m, rotor/stator edge pairs {}/{}, \
         law {}, rel_tol {:e}, J areas from {}, Hc = {:.6e} A/m, setup {:.2} s",
        args.level,
        model.tagged.mesh.n_nodes(),
        model.tagged.mesh.n_tris(),
        model.tagged.reoriented_triangles,
        rep.n_coupling,
        model.steps_per_degree(),
        rep.r_c,
        rep.n_rotor_pairs,
        rep.n_stator_pairs,
        args.law.name(),
        cfg.rel_tol,
        area_name,
        icem::magnet_hc(),
        setup_s
    );
    match &orders {
        None => println!("  coupling: conforming sliding band"),
        Some(o) => println!(
            "  coupling: harmonic, {} ends, {} orders ({} sin/cos modes): {:?}",
            if args.free_ends { "free" } else { "tied" },
            o.len(),
            o.iter().map(|&n| if n == 0 { 1 } else { 2 }).sum::<usize>(),
            o
        ),
    }
    if let IronLaw::RefClamp426 = args.law {
        let (h, b) = icem::m27_table(&reference);
        let l = icem::ClampedLowFieldLaw::new(
            geode_core::assembly::magnetostatic_nonlinear::TabulatedBhLaw::from_hb(&h, &b).unwrap(),
        );
        println!(
            "  clamp426 law: pchip endpoint slope m0 = {:.6} m/H, nu(B <= 1e-5 T) = {:.6} m/H",
            l.m0(),
            l.nu_clamp()
        );
    }
    let mut rows = String::new();
    let t_all = Instant::now();
    for &ci in &args.currents {
        let i_peak = LEVELS[ci] * I_PEAK_INITIAL;
        let mut prev: Option<Vec<f64>> = None;
        println!("  current level {} (I_peak = {:.9} A)", LEVELS[ci], i_peak);
        println!(
            "    deg | T_arkkio (N m) | T_inner     | T_outer     | T_maxwell(44.25) | T_coupling  | its | \
             status | |R|          | Bmax iron | wall (s)"
        );
        for &deg in &args.angles {
            let (s, wall) = model
                .solve_degree(
                    i_peak,
                    PHI0_INITIAL_DEG.to_radians(),
                    deg,
                    &cfg,
                    prev.as_deref(),
                )
                .unwrap_or_else(|e| panic!("L{} I{} {deg} deg: {e}", args.level, ci));
            let ta = model.arkkio(&s, 2);
            let ti = model.arkkio(&s, 0);
            let to = model.arkkio(&s, 1);
            let tm: Vec<f64> = MAXWELL_RADII
                .iter()
                .map(|&r| model.maxwell(&s, r, n_maxwell))
                .collect();
            let spectrum = if args.spectrum {
                let (full, part) =
                    model.harmonic_torque(&s, SPECTRUM_RINGS, 4 * rep.n_coupling, &SPECTRUM_CUTS);
                println!(
                    "          spectrum: ring-average {full:.9}, n <= {SPECTRUM_CUTS:?}: {}",
                    part.iter()
                        .map(|t| format!("{t:.9}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                );
                format!(
                    ", \"spectrum_full\": {full:.17e}, \"spectrum_cuts\": {SPECTRUM_CUTS:?}, \"spectrum_partial\": [{}]",
                    part.iter()
                        .map(|t| format!("{t:.17e}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            } else {
                String::new()
            };
            let tc = s.coupling_torque.map(|t| t * icem::L_AXIAL);
            let tc_json = match tc {
                Some(t) => format!("{t:.17e}"),
                None => "null".into(),
            };
            let res = *s.solution.residual_norms.last().unwrap();
            let bmax = model.iron_b_max(&s);
            println!(
                "    {deg:3} | {ta:14.9} | {ti:11.8} | {to:11.8} | {:16.9} | {:>11} | {:3} | {:6} | \
                 {res:.3e} | {bmax:9.4} | {wall:.3}",
                tm[1],
                tc.map(|t| format!("{t:.8}")).unwrap_or_else(|| "-".into()),
                s.solution.iterations,
                icem::status_name(s.status()),
            );
            if !rows.is_empty() {
                rows.push_str(",\n");
            }
            write!(
                rows,
                "    {{\"current_index\": {ci}, \"level_fraction\": {}, \"i_peak_A\": {i_peak:.17e}, \
                 \"angle_deg\": {deg}, \"T_arkkio\": {ta:.17e}, \"T_arkkio_inner\": {ti:.17e}, \
                 \"T_arkkio_outer\": {to:.17e}, \"T_coupling\": {tc_json}, \
                 \"constraint_violation\": {:.3e}, \"T_maxwell\": [{}], \"newton_iterations\": {}, \
                 \"status\": \"{}\", \"residual\": {res:.6e}, \"tolerance\": {:.6e}, \
                 \"iron_b_max_T\": {bmax:.6}, \"wall_s\": {wall:.4}{spectrum}}}",
                LEVELS[ci],
                s.constraint_violation,
                tm.iter().map(|t| format!("{t:.17e}")).collect::<Vec<_>>().join(", "),
                s.solution.iterations,
                icem::status_name(s.status()),
                s.solution.tolerance,
            )
            .unwrap();
            prev = Some(s.solution.a_z.clone());
        }
    }
    let total = t_all.elapsed().as_secs_f64();
    println!("  total solve wall time {total:.2} s");

    let mut areas = String::new();
    for g in 0..12 {
        if g > 0 {
            areas.push_str(", ");
        }
        write!(areas, "{:.17e}", model.group_areas[g]).unwrap();
    }
    let json = format!(
        "{{\n  \"issue\": {},\n  \"coupling\": \"{}\",\n  \"coupling_orders\": {},\n  \"level\": {},\n  \"nodes\": {},\n  \"triangles\": {},\n  \
         \"reoriented_triangles\": {},\n  \"n_coupling\": {},\n  \"law\": \"{}\",\n  \
         \"rel_tol\": {:e},\n  \"area_mode\": \"{}\",\n  \"group_areas_m2\": [{}],\n  \
         \"magnet_hc_A_per_m\": {:.17e},\n  \"maxwell_radii_m\": [{}],\n  \"maxwell_samples\": {},\n  \
         \"setup_s\": {:.4},\n  \"total_solve_s\": {:.4},\n  \"rows\": [\n{}\n  ]\n}}\n",
        if orders.is_some() { 1170 } else { 1168 },
        coupling_name,
        match &orders {
            Some(o) => format!("{o:?}"),
            None => "null".into(),
        },
        args.level,
        model.tagged.mesh.n_nodes(),
        model.tagged.mesh.n_tris(),
        model.tagged.reoriented_triangles,
        rep.n_coupling,
        args.law.name(),
        cfg.rel_tol,
        area_name,
        areas,
        icem::magnet_hc(),
        MAXWELL_RADII.map(|r| format!("{r}")).join(", "),
        n_maxwell,
        setup_s,
        total,
        rows
    );
    let out = args.out.unwrap_or_else(|| {
        if orders.is_some() {
            icem::motor_reference_dir().join(format!(
                "forward_harmonic/runs/L{}_{}_{}_{}_{}.json",
                args.level,
                coupling_name,
                args.law.name(),
                tol_name,
                area_name
            ))
        } else {
            icem::motor_reference_dir().join(format!(
                "forward/runs/L{}_{}_{}_{}.json",
                args.level,
                args.law.name(),
                tol_name,
                area_name
            ))
        }
    });
    std::fs::create_dir_all(out.parent().unwrap()).unwrap();
    std::fs::write(&out, json).unwrap();
    println!("  wrote {}", out.display());
}
