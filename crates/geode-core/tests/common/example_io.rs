//! Small I/O helpers shared by the `geode-core` examples and integration
//! tests (issue #1547): JSON report writing, the SHA-256 hex digest used to
//! pin fixtures, and the ICEM control-point curve dump.
//!
//! Pull it in with
//! `#[path = "../tests/common/example_io.rs"] mod example_io;` (examples) or
//! `#[path = "common/example_io.rs"] mod example_io;` (tests). Each binary
//! uses a subset, hence the `dead_code` allowance.
#![allow(dead_code)]

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
