//! Small I/O helpers shared by the `geode-core` examples and integration
//! tests (issue #1547): JSON report writing, the SHA-256 hex digest used to
//! pin fixtures, the ICEM control-point curve dump, and the step-sweep
//! report shared by the two ICEM design-gradient examples (issue #1664).
//!
//! Pull it in with
//! `#[path = "../tests/common/example_io.rs"] mod example_io;` (examples) or
//! `#[path = "common/example_io.rs"] mod example_io;` (tests). Each binary
//! uses a subset, hence the `dead_code` allowance.
#![allow(dead_code)]

use std::fmt::Write as _;
use std::path::Path;

use geode_core::mesh::icem_controlpoints::{IcemCpDesign, sector_curves_cp_f64};
use geode_core::mesh::icem_parametric::CurveShape;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

/// Write `v` as pretty JSON plus a trailing newline to `out`, creating the
/// parent directory first.
pub fn write_json(out: &Path, v: &Value) {
    if let Some(d) = out.parent() {
        std::fs::create_dir_all(d).unwrap();
    }
    std::fs::write(out, serde_json::to_string_pretty(v).unwrap() + "\n").unwrap();
}

/// [`write_json`] to the required `--out FILE` argument, reporting the path
/// on stderr.
pub fn write_json_out(path: &Option<std::path::PathBuf>, v: &Value) {
    let p = path.as_deref().expect("--out FILE");
    write_json(p, v);
    eprintln!("wrote {}", p.display());
}

/// Lowercase hex SHA-256 digest of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The sector curves of a control-point design as a JSON array.
pub fn curves_json(d: &IcemCpDesign) -> Value {
    json!(
        sector_curves_cp_f64(d)
            .expect("construction")
            .iter()
            .map(|c| match &c.shape {
                CurveShape::Line { a, b } =>
                    json!({"name": c.name, "kind": "line", "a": a, "b": b}),
                CurveShape::Arc {
                    c: cc,
                    r,
                    phi_a,
                    phi_b,
                    a,
                    b,
                } => json!({"name": c.name, "kind": "arc", "c": cc, "r": r,
                    "phi_a": phi_a, "phi_b": phi_b, "a": a, "b": b}),
                CurveShape::Spline {
                    degree,
                    knots,
                    cps,
                    weights,
                } => json!({"name": c.name, "kind": "nurbs", "degree": degree, "knots": knots,
                    "cps": cps, "weights": weights, "a": cps[0], "b": cps[cps.len() - 1]}),
            })
            .collect::<Vec<_>>()
    )
}

/// Steps of the sweep in metres for the geometric components and `LENGTH`;
/// the angle uses `× 1e3` degrees and `ScaleR` `× 10`, as in #1177.
pub const HS: [f64; 13] = [
    1e-3, 3e-4, 1e-4, 3e-5, 1e-5, 3e-6, 1e-6, 3e-7, 1e-7, 3e-8, 1e-8, 3e-9, 1e-9,
];

/// Reference step of the Taylor window (× the parameter's unit factor).
pub const H_REF: f64 = 1e-5;

/// Unit factor of component `k`'s sweep steps: `1e3` for the operating angle
/// (degrees) and `10.0` for `ScaleR`, `1.0` otherwise. `angle_k` / `scale_k`
/// are the design indices of the angle and `ScaleR` components (the
/// parametric and control-point designs number them differently).
pub fn step_factor(k: usize, angle_k: usize, scale_k: usize) -> f64 {
    if k == angle_k {
        1e3
    } else if k == scale_k {
        10.0
    } else {
        1.0
    }
}

/// Markdown table header of a step-sweep summary, first column labelled
/// `first_col` ("component" / "parameter").
pub fn sweep_header(first_col: &str) -> String {
    format!(
        "| {first_col} | d mean (adjoint) | mean: best FD rel. err (h) | mean: Taylor window, decades | d std (adjoint) | std: best FD rel. err (h) | std: Taylor window, decades | rejected h |\n|---|---|---|---|---|---|---|---|"
    )
}

/// The bytes of mesh file `p`, panicking with the regeneration hint when the
/// gitignored L1-L3 meshes are missing.
pub fn mesh_bytes(p: &Path) -> Vec<u8> {
    std::fs::read(p).unwrap_or_else(|e| {
        panic!(
            "read {}: {e}. L1-L3 are gitignored; regenerate them with the \
             benchmarks/motor_reference/geometry/README.md command",
            p.display()
        )
    })
}

/// One component's step-sweep report: the markdown table row and the JSON
/// record (per-step rows, best FD, Taylor windows and, for the parametric
/// design, the convergence orders and unit). `f` is the component's unit
/// step factor and `rows` its `(h, J(θ + h), J(θ − h))` pairs at the scaled
/// steps; `unit` / `with_orders` reproduce the parametric JSON shape (the
/// control-point report passes `None` / `false`). `taylor_window` is the
/// design module's Taylor-window fit, passed in to keep this module
/// independent of it.
#[allow(clippy::too_many_arguments)]
pub fn sweep_record(
    md: &mut String,
    name: &str,
    k: usize,
    f: f64,
    adj: [f64; 2],
    j0: [f64; 2],
    rows: &[(f64, [f64; 2], [f64; 2])],
    rejected: &[(f64, String)],
    taylor_window: impl Fn(&[f64], &[f64], f64) -> Option<(f64, f64, f64)>,
    unit: Option<&str>,
    with_orders: bool,
) -> Value {
    let mut out_rows = Vec::new();
    let mut best = [(f64::INFINITY, 0.0); 2];
    let mut rems = [Vec::new(), Vec::new()];
    for &(h, plus, minus) in rows {
        let mut e = [0.0; 2];
        let fd = [
            (plus[0] - minus[0]) / (2.0 * h),
            (plus[1] - minus[1]) / (2.0 * h),
        ];
        let mut rem = [0.0; 2];
        for o in 0..2 {
            e[o] = (fd[o] - adj[o]).abs() / adj[o].abs();
            if e[o] < best[o].0 {
                best[o] = (e[o], h);
            }
            rem[o] = (plus[o] - j0[o] - h * adj[o]).abs();
            rems[o].push(rem[o]);
        }
        out_rows.push(json!({
            "h": h, "plus": plus.to_vec(), "minus": minus.to_vec(),
            "fd": fd, "rel_err": e.to_vec(),
            "remainder": [rem[0], rem[1]],
        }));
    }
    let hs: Vec<f64> = rows.iter().map(|(h, _, _)| *h).collect();
    let windows: Vec<Value> = (0..2)
        .map(|o| match taylor_window(&hs, &rems[o], H_REF * f) {
            Some((hi, lo, dec)) => json!({"h_hi": hi, "h_lo": lo, "decades": dec}),
            None => Value::Null,
        })
        .collect();
    let win = |o: usize| {
        windows[o]
            .as_object()
            .map(|w| {
                format!(
                    "{:.0e} .. {:.0e}, {:.1}",
                    w["h_hi"].as_f64().unwrap(),
                    w["h_lo"].as_f64().unwrap(),
                    w["decades"].as_f64().unwrap()
                )
            })
            .unwrap_or_else(|| "none".into())
    };
    let _ = writeln!(
        md,
        "| {name} | {:+.6e} | {:.1e} ({:.0e}) | {} | {:+.6e} | {:.1e} ({:.0e}) | {} | {} |",
        adj[0],
        best[0].0,
        best[0].1,
        win(0),
        adj[1],
        best[1].0,
        best[1].1,
        win(1),
        if rejected.is_empty() {
            String::new()
        } else {
            format!("{:?}", rejected.iter().map(|r| r.0).collect::<Vec<_>>())
        }
    );
    let mut rec = json!({
        "parameter": name, "k": k, "step_factor": f, "adjoint": adj.to_vec(), "j0": j0.to_vec(),
        "rows": out_rows,
        "best": [{"rel_err": best[0].0, "h": best[0].1}, {"rel_err": best[1].0, "h": best[1].1}],
        "taylor_window": windows,
        "rejected": rejected.iter().map(|(h, e)| json!({"h": h, "error": e})).collect::<Vec<_>>(),
    });
    if let Some(u) = unit {
        rec["unit"] = json!(u);
    }
    if with_orders {
        rec["orders"] = json!(
            (0..2)
                .map(|o| {
                    (1..hs.len())
                        .map(|i| (rems[o][i - 1] / rems[o][i]).ln() / (hs[i - 1] / hs[i]).ln())
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>()
        );
    }
    rec
}
