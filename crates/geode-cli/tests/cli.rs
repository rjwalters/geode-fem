//! End-to-end CLI contract tests for `geode` (issue #673 Phase 1, Epic
//! #680): `--version`, `check` success + structured failures, eigen-spec
//! validation (issue #681), extract-spec validation and its `L₀`
//! convergence gate (issue #682), open-boundary / wave-port spec
//! validation (issue #683), `--outdir` field export on a closed
//! lumped-port spec and its fail-fast `io` error (issue #684),
//! `--touchstone` output and rejections plus `check`'s resource estimate
//! (issue #703), a
//! no-double-space guard on every `invalid_spec` message, `--backend`
//! confirmation, and the
//! structured `solve_failed` error for a non-converging iterative solve
//! (one iteration). The eigen and extract solves themselves are
//! exercised by `tests/sphere_pec_golden.rs` and
//! `tests/slcfet_extract_golden.rs`.

use std::path::PathBuf;
use std::process::{Command, Output};

#[path = "support/scratch.rs"]
mod scratch_support;
#[path = "support/touchstone.rs"]
mod touchstone_support;

use scratch_support::{Scratch, ScratchFile};

#[path = "support/cli.rs"]
mod cli_support;
use cli_support::geode;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn smoke_spec() -> String {
    fixtures()
        .join("spiral_golden_smoke.json")
        .display()
        .to_string()
}

fn smoke_mesh() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../geode-core/tests/fixtures/spiral_3p5_smoke.msh")
        .canonicalize()
        .expect("smoke mesh exists")
}

/// Fresh scratch dir for one test, removed on drop (also on panic).
fn scratch(name: &str) -> Scratch {
    Scratch::new("geode-cli-test-", name)
}

/// The smoke spec with `edit` applied, written to a scratch file (mesh
/// path made absolute so the spec can live anywhere).
fn edited_spec(name: &str, edit: impl FnOnce(&mut serde_json::Value)) -> ScratchFile {
    let raw = std::fs::read_to_string(smoke_spec()).unwrap();
    let mut v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    v["mesh"]["path"] = smoke_mesh().display().to_string().into();
    edit(&mut v);
    let text = serde_json::to_string_pretty(&v).unwrap();
    ScratchFile::write("geode-cli-test-", name, "spec.json", text)
}

fn json(out: &Output) -> serde_json::Value {
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "stdout is not JSON ({e}): {}",
            String::from_utf8_lossy(&out.stdout)
        )
    })
}

/// Every `invalid_spec` message is one clean sentence: a run of spaces
/// is the signature of a multi-line string literal that lost its `\`
/// continuation (issue #684 regression guard, applied by
/// [`assert_error`] to every `invalid_spec` case in this file).
fn assert_no_double_space(msg: &str) {
    assert!(!msg.contains("  "), "stray whitespace: {msg:?}");
}

/// The JSON shape of an error report (wherever it was written): kind,
/// status, command and code, plus [`assert_no_double_space`] on every
/// `invalid_spec` message.
fn assert_error_value(v: &serde_json::Value, command: &str, code: &str) {
    assert_eq!(v["kind"], "error");
    assert_eq!(v["status"], "error");
    assert_eq!(v["command"], command);
    assert_eq!(v["error"]["code"], code, "report: {v:#}");
    if code == "invalid_spec" {
        assert_no_double_space(v["error"]["message"].as_str().expect("error message"));
    }
}

fn assert_error(out: &Output, command: &str, code: &str) -> serde_json::Value {
    assert!(!out.status.success(), "expected failure");
    let v = json(out);
    assert_error_value(&v, command, code);
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("error:"),
        "stderr carries the human-readable error"
    );
    v
}

#[test]
fn version_prints_crate_version_and_git_sha() {
    let out = geode(&["--version"]);
    assert!(out.status.success());
    let s = String::from_utf8_lossy(&out.stdout);
    let want_prefix = format!("geode {} (", env!("CARGO_PKG_VERSION"));
    assert!(s.starts_with(&want_prefix), "got {s:?}");
    assert!(s.trim_end().ends_with(')'), "got {s:?}");
    let sha = &s.trim_end()[want_prefix.len()..s.trim_end().len() - 1];
    assert!(!sha.is_empty(), "empty sha in {s:?}");
}

#[test]
fn check_valid_spec_reports_dofs_without_solving() {
    let out = geode(&["check", &smoke_spec()]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v = json(&out);
    assert_eq!(v["schema_version"], 1);
    assert_eq!(v["kind"], "check");
    assert_eq!(v["status"], "ok");
    let mesh = &v["mesh"];
    let n_edges = mesh["n_edges"].as_u64().unwrap();
    let n_interior = mesh["n_interior"].as_u64().unwrap();
    assert!(n_edges > 10_000);
    assert!(
        0 < n_interior && n_interior < n_edges,
        "PEC eliminated some edges"
    );
    assert_eq!(mesh["length_unit_m"], 1e-6);

    // Regions: substrate/dielectric from the spec, air/air_buffer default vacuum.
    let regions = v["regions"].as_array().unwrap();
    let source = |name: &str| {
        regions
            .iter()
            .find(|r| r["physical_group"] == name)
            .unwrap_or_else(|| panic!("region {name}"))["eps_r_source"]
            .clone()
    };
    assert_eq!(source("substrate"), "spec");
    assert_eq!(source("dielectric"), "spec");
    assert_eq!(source("air"), "default_vacuum");
    assert_eq!(source("air_buffer"), "default_vacuum");

    // Port geometry derived from the tagged faces.
    let port = &v["ports"][0];
    assert_eq!(port["physical_group"], "port");
    assert_eq!(port["geometry_derived"], true);
    assert!(port["width"].as_f64().unwrap() > 0.0);
    assert!(port["length"].as_f64().unwrap() > 0.0);

    // Frequencies echoed in Hz and natural k0 (1 GHz on a µm mesh).
    let f0 = &v["frequencies"][0];
    assert_eq!(f0["frequency_hz"], 1e9);
    assert!((f0["k0"].as_f64().unwrap() - 2.095845021951682e-5).abs() < 1e-15);

    // Leontovich σ converted to natural units: 5.8e7 · η₀ · 1e-6 ≈ 2.185e4.
    let sigma = v["leontovich"][0]["conductivity_natural"].as_f64().unwrap();
    assert!((sigma - 2.185e4).abs() / 2.185e4 < 1e-3);

    // Resource estimate (issue #703): direct, complex pencil, one LU per
    // frequency; finite / positive (no ground truth at CI scale).
    let r = &v["resources"];
    assert_eq!(r["solver_mode"], "direct");
    assert_eq!(r["scalar"], "complex");
    assert!(
        r["nnz_a"].as_u64().unwrap() > n_edges,
        "diagonal + couplings"
    );
    assert_eq!(r["n_factorizations"], 4);
    assert_eq!(r["n_rhs_per_frequency"], 1);
    for k in [
        "peak_memory_gb",
        "wall_time_s",
        "wall_time_per_factorization_s",
    ] {
        let x = r[k].as_f64().unwrap();
        assert!(x.is_finite() && x > 0.0, "{k} = {x}");
    }
    assert!(r["flops_per_iteration"].is_null() && r["flops_max"].is_null());
    assert_anchor_ratio(r);
    assert_eq!(r["peak_memory_confidence"], "order_of_magnitude");
    assert_eq!(r["wall_time_confidence"], "conservative_below_anchor");
    let basis = r["calibration_basis"].as_str().unwrap();
    assert!(
        basis.contains("2026-07-15") && basis.contains("20467522"),
        "{basis}"
    );
}

/// Issue #713: `anchor_nnz_ratio` is `nnz_a` over the direct-LU anchor's
/// `nnz(A)` (20 467 522), and every in-repo fixture is far below it.
fn assert_anchor_ratio(r: &serde_json::Value) {
    let ratio = r["anchor_nnz_ratio"].as_f64().unwrap();
    let want = r["nnz_a"].as_u64().unwrap() as f64 / 20_467_522.0;
    assert!((ratio - want).abs() <= 1e-15 * want, "{ratio} vs {want}");
    assert!(
        ratio > 0.0 && ratio < 1.0,
        "fixture below the anchor: {ratio}"
    );
    assert_eq!(r["above_anchor"], false);
}

#[test]
fn check_resource_estimate_for_iterative_and_eigen_specs() {
    let spec = edited_spec("resources-iterative", |v| {
        v["solver"] = serde_json::json!({ "mode": "iterative", "tol": 1e-8, "max_iters": 500 });
    });
    let v = json(&geode(&["check", spec.to_str().unwrap()]));
    let r = &v["resources"];
    assert_eq!(r["solver_mode"], "iterative");
    assert_eq!(r["n_factorizations"], 0);
    assert!(r["wall_time_s"].is_null() && r["wall_time_confidence"].is_null());
    let per = r["flops_per_iteration"].as_f64().unwrap();
    assert!(per > 0.0 && per.is_finite());
    assert_eq!(r["flops_max"].as_f64().unwrap(), per * 500.0 * 4.0);
    // Still a ratio against the direct anchor (a scale signal).
    assert_anchor_ratio(r);
    assert!(r["peak_memory_gb"].as_f64().unwrap() > 0.0);
    assert!(
        r["calibration_basis"]
            .as_str()
            .unwrap()
            .contains("no measured anchor")
    );

    // Eigen: the anchor's own kind — real pencil, one factorization.
    let v = json(&geode(&["check", sphere_spec().to_str().unwrap()]));
    let r = &v["resources"];
    assert_eq!(r["solver_mode"], "direct");
    assert_eq!(r["scalar"], "real");
    assert_eq!(r["n_factorizations"], 1);
    assert_eq!(r["n_rhs_per_frequency"], 0);
    assert!(r["wall_time_s"].as_f64().unwrap() > 0.0);
    assert_anchor_ratio(r);
    // Direct: wall time is the per-factorization unit × factorizations.
    assert_eq!(
        r["wall_time_s"].as_f64().unwrap(),
        r["wall_time_per_factorization_s"].as_f64().unwrap()
    );
}

/// The `"preconditioner_fallback"` warnings of a report.
fn preconditioner_fallbacks(v: &serde_json::Value) -> Vec<&serde_json::Value> {
    v["warnings"]
        .as_array()
        .map(|w| {
            w.iter()
                .filter(|w| w["kind"] == "preconditioner_fallback")
                .collect()
        })
        .unwrap_or_default()
}

/// Issue #930: with no `solver.preconditioner`, an iterative spec that
/// AMS supports (the spiral smoke: Leontovich conductors, one PEC
/// component, positive `Re ε`) resolves to `ams` in `check` and `driven`,
/// with no fallback warning, and the AMS solve converges.
#[test]
fn default_iterative_preconditioner_resolves_to_ams_without_a_warning() {
    let spec = edited_spec("pc-default", |v| {
        v["frequencies"]["values"] = serde_json::json!([5.0]);
        v["solver"] = serde_json::json!({ "mode": "iterative" });
    });
    for cmd in ["check", "driven"] {
        let out = geode(&[cmd, spec.to_str().unwrap()]);
        assert!(
            out.status.success(),
            "{cmd}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let v = json(&out);
        assert_eq!(v["solver"]["mode"], "iterative", "{cmd}");
        assert_eq!(v["solver"]["preconditioner"], "ams", "{cmd}");
        assert!(preconditioner_fallbacks(&v).is_empty(), "{cmd}: {v:#}");
        assert!(
            !String::from_utf8_lossy(&out.stderr).contains("fell back"),
            "{cmd}"
        );
    }
}

/// Issue #930: an explicit `solver.preconditioner = "jacobi"` is honoured
/// (reported as `jacobi`, and the resolved solver keeps it) with no
/// fallback warning.
#[test]
fn explicit_jacobi_preconditioner_is_honoured_without_a_warning() {
    let spec = edited_spec("pc-jacobi", |v| {
        v["solver"] = serde_json::json!({ "mode": "iterative", "preconditioner": "jacobi" });
    });
    let out = geode(&["check", spec.to_str().unwrap()]);
    assert!(out.status.success());
    let v = json(&out);
    assert_eq!(v["solver"]["preconditioner"], "jacobi");
    assert!(preconditioner_fallbacks(&v).is_empty(), "{v:#}");
    assert!(!String::from_utf8_lossy(&out.stderr).contains("fell back"));
    // A direct spec reports no preconditioner.
    let direct = json(&geode(&["check", &smoke_spec()]));
    assert!(direct["solver"].get("preconditioner").is_none());
}

#[test]
fn check_resource_estimate_counts_ilu0_factor_storage() {
    // Issue #708: `solver.preconditioner = "ilu0"` parses and threads into
    // the iterative vector-count model (factor storage + triangular sweeps).
    let estimate = |name: &str, pc: &str| {
        let spec = edited_spec(name, |v| {
            v["solver"] = serde_json::json!({ "mode": "iterative", "preconditioner": pc });
        });
        json(&geode(&["check", spec.to_str().unwrap()]))["resources"].clone()
    };
    let jac = estimate("resources-jacobi", "jacobi");
    let ilu = estimate("resources-ilu0", "ilu0");
    assert_eq!(ilu["solver_mode"], "iterative");
    let gb = |r: &serde_json::Value| r["peak_memory_gb"].as_f64().unwrap();
    let per = |r: &serde_json::Value| r["flops_per_iteration"].as_f64().unwrap();
    assert!(gb(&ilu) > gb(&jac), "{} vs {}", gb(&ilu), gb(&jac));
    assert!(per(&ilu) > per(&jac));

    // Issue #744: `"ams"` parses and adds the auxiliary-space storage (proxy,
    // G, Π, nodal LU, ΠᵀPΠ) and V-cycle work on top of Jacobi.
    let ams = estimate("resources-ams", "ams");
    assert_eq!(ams["solver_mode"], "iterative");
    assert!(gb(&ams) > gb(&jac), "{} vs {}", gb(&ams), gb(&jac));
    assert!(per(&ams) > per(&jac));

    // An unknown preconditioner is a spec error, not a silent default.
    let spec = edited_spec("resources-bad-pc", |v| {
        v["solver"] = serde_json::json!({ "mode": "iterative", "preconditioner": "amg" });
    });
    let out = geode(&["check", spec.to_str().unwrap()]);
    assert!(!out.status.success());
}

#[test]
fn touchstone_sorts_a_driven_sweep_and_round_trips() {
    // Spec order 5 GHz then 1 GHz: the report keeps spec order, the
    // `.s1p` is ascending.
    let spec = edited_spec("touchstone-sort", |v| {
        v["frequencies"] = serde_json::json!({ "unit": "ghz", "values": [5.0, 1.0] });
    });
    let ts_dir = scratch("touchstone-sort");
    let ts = ts_dir.join("nested-name.s1p");
    let out = geode(&[
        "driven",
        spec.to_str().unwrap(),
        "--touchstone",
        ts.to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v = json(&out);
    assert_eq!(
        v["results"][0]["frequency_hz"], 5e9,
        "report keeps spec order"
    );
    touchstone_support::assert_round_trip(&v, &ts);
    let text = std::fs::read_to_string(&ts).unwrap();
    assert!(text.contains("\n[Version] 2.0\n# HZ S RI R 5e1\n[Number of Ports] 1\n"));
    assert!(text.contains("\n[Reference] 5e1\n[Network Data]\n1e9 "));
    assert!(text.ends_with("[End]\n"));
    // Without the flag the field is omitted entirely.
    let v = json(&geode(&["driven", spec.to_str().unwrap()]));
    assert!(v.get("touchstone_file").is_none());
}

#[test]
fn touchstone_extract_round_trips() {
    let spec = fixtures().join("slcfet_extract_smoke.json");
    let ts_dir = scratch("touchstone-extract");
    let ts = ts_dir.join("l.s1p");
    let out = geode(&[
        "extract",
        spec.to_str().unwrap(),
        "--touchstone",
        ts.to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v = json(&out);
    assert_eq!(v["kind"], "extract");
    touchstone_support::assert_round_trip(&v, &ts);
}

/// Strip the run-dependent fields of a report so two runs compare equal.
fn stable(mut v: serde_json::Value) -> serde_json::Value {
    for k in [
        "provenance",
        "timestamp",
        "wall_time_s",
        "git_sha",
        "spec_path",
    ] {
        v.as_object_mut().unwrap().remove(k);
    }
    v
}

/// Issue #1265 (Epic #836 Phase 5a): `element_order` is a real spec field.
/// `1` is the default (byte-identical to omitting it); anything other than
/// the integers 1 and 2 is a `spec_parse` error naming the field.
#[test]
fn element_order_one_matches_omitting_it_and_bad_values_are_rejected() {
    let omitted = edited_spec("element-order-omitted", |_| {});
    let one = edited_spec("element-order-1", |v| {
        v["element_order"] = serde_json::json!(1);
    });
    let a = json(&geode(&["check", omitted.to_str().unwrap()]));
    let b = json(&geode(&["check", one.to_str().unwrap()]));
    assert_eq!(stable(a.clone()), stable(b), "order 1 == default");
    assert_eq!(a["mesh"]["element_order"], 1);
    assert_eq!(a["mesh"]["n_dofs"], a["mesh"]["n_edges"]);
    assert!(a["resources"].get("order_comparison").is_none());

    for (i, bad) in [
        serde_json::json!(3),
        serde_json::json!("2"),
        serde_json::json!(0),
    ]
    .into_iter()
    .enumerate()
    {
        let spec = edited_spec(&format!("element-order-bad-{i}"), |v| {
            v["element_order"] = bad.clone();
        });
        for command in ["check", "driven"] {
            let out = geode(&[command, spec.to_str().unwrap()]);
            let v = assert_error(&out, command, "spec_parse");
            let msg = v["error"]["message"].as_str().unwrap();
            assert!(msg.contains("element_order"), "{command}: {msg}");
        }
    }
}

/// `geode check` at order 2: exact DOF / nnz counts, the ratios against
/// order 1 on the same mesh, and `order_of_magnitude` confidence.
#[test]
fn check_at_order_two_reports_exact_counts_and_ratios() {
    let p1 = edited_spec("check-p1", |_| {});
    let p2 = edited_spec("check-p2", |v| {
        v["element_order"] = serde_json::json!(2);
    });
    let a = json(&geode(&["check", p1.to_str().unwrap()]));
    let out = geode(&["check", p2.to_str().unwrap()]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let b = json(&out);
    assert_eq!(b["mesh"]["element_order"], 2);
    assert_eq!(a["mesh"]["n_tets"], b["mesh"]["n_tets"]);
    let n_edges = b["mesh"]["n_edges"].as_u64().unwrap();
    let n_dofs = b["mesh"]["n_dofs"].as_u64().unwrap();
    // 2·n_edges + 2·n_faces, with n_faces strictly positive.
    assert!(
        n_dofs > 2 * n_edges && n_dofs.is_multiple_of(2),
        "{n_dofs} vs {n_edges}"
    );
    let c = &b["resources"]["order_comparison"];
    assert_eq!(c["n_dofs_p2"].as_u64().unwrap(), n_dofs);
    assert_eq!(c["n_interior_p2"], b["mesh"]["n_interior"]);
    assert_eq!(c["n_interior_p1"], a["mesh"]["n_interior"]);
    assert_eq!(c["nnz_p1"], a["resources"]["nnz_a"]);
    assert_eq!(c["nnz_p2"], b["resources"]["nnz_a"]);
    assert!(c["dof_ratio"].as_f64().unwrap() > 2.0);
    assert!(c["memory_ratio"].as_f64().unwrap() > 1.0);
    assert_eq!(
        b["resources"]["peak_memory_confidence"],
        "order_of_magnitude"
    );
    assert_eq!(c["fits_direct_lu"], true);
}

/// Every unsupported order-2 combination errors with `invalid_spec`
/// naming `element_order` and the feature (never solved at order 1).
#[test]
fn element_order_two_rejects_unsupported_combinations() {
    type Edit = fn(&mut serde_json::Value);
    let cases: [(&str, &str, Edit); 6] = [
        ("iterative", "iterative", |v| {
            v["solver"] = serde_json::json!({"mode": "iterative"});
        }),
        ("adaptive", "sweep.adaptive", |v| {
            v["sweep"] = serde_json::json!({"adaptive": {"tolerance": 1e-3}});
        }),
        ("upml", "absorbing_regions", |v| {
            v["absorbing_regions"] = serde_json::json!([
                {"physical_group": "dielectric", "thickness": 1.0, "sigma_0": 1.0}
            ]);
        }),
        ("wave-ports", "wave_ports", |v| {
            v["wave_ports"] = serde_json::json!([{"physical_group": "port"}]);
        }),
        ("capacitance", "capacitance", |v| {
            v["capacitance"] = serde_json::json!({
                "terminals": ["port"], "ground": ["outer_boundary"]
            });
        }),
        ("adaptive-mesh", "adaptive_mesh", |v| {
            v["adaptive_mesh"] = serde_json::json!({"target_error": 0.1});
        }),
    ];
    for (name, needle, edit) in cases {
        let spec = edited_spec(&format!("p2-reject-{name}"), |v| {
            v["element_order"] = serde_json::json!(2);
            edit(v);
        });
        let out = geode(&["check", spec.to_str().unwrap()]);
        // A spec the parser itself rejects is also fine, but it must not
        // be solved: the failure is a loud `invalid_spec`/`spec_parse`.
        assert!(!out.status.success(), "{name}: must fail");
        let v = json(&out);
        assert_eq!(v["kind"], "error", "{name}: {v:#}");
        let msg = v["error"]["message"].as_str().unwrap();
        assert!(
            msg.contains("element_order") && msg.contains(needle),
            "{name}: {msg}"
        );
    }
    // Hybrid ports / sensitivities name the unblocking phase.
    let spec = edited_spec("p2-reject-sensitivity", |v| {
        v["element_order"] = serde_json::json!(2);
        v["sensitivity"] = serde_json::json!({"parameters": []});
    });
    let out = geode(&["check", spec.to_str().unwrap()]);
    assert!(!out.status.success());
    let msg = json(&out)["error"]["message"].as_str().unwrap().to_string();
    assert!(
        msg.contains("sensitivity") && msg.contains("Phase 4"),
        "{msg}"
    );

    // Hybrid wave ports name Phase 3b.
    let spec = edited_spec("p2-reject-hybrid-ports", |v| {
        v["element_order"] = serde_json::json!(2);
        v["wave_ports"] = serde_json::json!([{"physical_group": "port"}]);
    });
    let out = geode(&["check", spec.to_str().unwrap()]);
    assert!(!out.status.success());
    let msg = json(&out)["error"]["message"].as_str().unwrap().to_string();
    assert!(
        msg.contains("wave_ports") && msg.contains("Phase 3b"),
        "{msg}"
    );
}

/// Library parity (issue #1265): the CLI's order-2 eigen result is exactly
/// what `solve_pec_cavity_modes_on_space` returns on the same mesh, PEC mask
/// and settings, so the CLI adds no numerics of its own at p=2.
#[test]
fn eigen_at_order_two_matches_the_library_on_space_solver() {
    use burn::tensor::backend::BackendTypes;
    use geode_core::assembly::hcurl_space::HcurlSpace;
    use geode_core::eigen::pec_cavity::{
        PecCavityMaterials, PecCavitySettings, solve_pec_cavity_modes_on_space,
    };
    use geode_core::elements::ElementOrder;
    use geode_core::mesh::read_tagged_tet_mesh;

    type B = burn::backend::NdArray<f64, i32>;
    let (n_modes, max_iters, tol) = (4usize, 160usize, 1e-9);

    let mesh_path = fixtures().join("../../../geode-core/tests/fixtures/sphere.msh");
    let mut spec: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(fixtures().join("sphere_pec_golden.json")).unwrap(),
    )
    .unwrap();
    spec["mesh"]["path"] = mesh_path
        .canonicalize()
        .unwrap()
        .display()
        .to_string()
        .into();
    spec["element_order"] = serde_json::json!(2);
    spec["eigen"]["n_modes"] = serde_json::json!(n_modes);
    spec["eigen"]["max_iters"] = serde_json::json!(max_iters);
    spec["eigen"]["tol"] = serde_json::json!(tol);
    let dir = scratch("p2-library-parity");
    let path = dir.join("spec.json");
    std::fs::write(&path, serde_json::to_string(&spec).unwrap()).unwrap();
    let out = geode(&["eigen", path.to_str().unwrap()]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let report = json(&out);

    let bytes = std::fs::read(&mesh_path).unwrap();
    let tagged = read_tagged_tet_mesh(&bytes).unwrap();
    let interior = tagged.physical_group_tag(3, "sphere_interior").unwrap();
    let outer = tagged.physical_group_tag(2, "outer_boundary").unwrap();
    let eps: Vec<f64> = tagged
        .tet_physical_tags
        .iter()
        .map(|&t| if t == interior { 2.25 } else { 1.0 })
        .collect();
    let outer_tris = tagged.triangles_with_tag(outer);
    let space = HcurlSpace::build(&tagged.mesh, ElementOrder::P2);
    let mask = space
        .pec_interior_mask(&tagged.mesh, &[outer_tris.as_slice()])
        .unwrap();
    let settings = PecCavitySettings {
        max_iters,
        tol,
        ..PecCavitySettings::new(1.0, n_modes)
    };
    let device = <B as BackendTypes>::Device::default();
    let lib = solve_pec_cavity_modes_on_space::<B>(
        &space,
        &tagged.mesh,
        &PecCavityMaterials::Isotropic(&eps),
        &mask,
        &[],
        &settings,
        &device,
    )
    .expect("library p=2 solve");

    let cli_modes = report["modes"].as_array().unwrap();
    assert_eq!(cli_modes.len(), lib.modes.modes.len());
    assert_eq!(
        report["mesh"]["n_interior"].as_u64().unwrap() as usize,
        lib.modes.n_interior
    );
    for (i, (c, l)) in cli_modes.iter().zip(&lib.modes.modes).enumerate() {
        let got = c["lambda"].as_f64().unwrap();
        let rel = (got - l.lambda).abs() / l.lambda;
        assert!(
            rel < 1e-9,
            "mode {i}: CLI {got} vs library {} (rel {rel:e})",
            l.lambda
        );
    }
}

/// An order-2 eigen run goes through the order-2 solver: same mesh and
/// spec, close to but not identical to the order-1 frequencies, and the
/// report records the order used.
#[test]
fn eigen_at_order_two_runs_the_order_two_solver() {
    let raw = std::fs::read_to_string(fixtures().join("sphere_pec_golden.json")).unwrap();
    let mut base: serde_json::Value = serde_json::from_str(&raw).unwrap();
    base["mesh"]["path"] = fixtures()
        .join(base["mesh"]["path"].as_str().unwrap())
        .display()
        .to_string()
        .into();
    let run = |order: u64| {
        let mut v = base.clone();
        v["element_order"] = serde_json::json!(order);
        let dir = scratch(&format!("eigen-order-{order}"));
        let path = dir.join("spec.json");
        std::fs::write(&path, serde_json::to_string(&v).unwrap()).unwrap();
        let out = geode(&["eigen", path.to_str().unwrap()]);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        json(&out)
    };
    let (a, b) = (run(1), run(2));
    assert_eq!(a["mesh"]["element_order"], 1);
    assert_eq!(b["mesh"]["element_order"], 2);
    assert!(b["mesh"]["n_dofs"].as_u64() > a["mesh"]["n_dofs"].as_u64());
    let f = |r: &serde_json::Value| r["modes"][0]["frequency_hz"].as_f64().unwrap();
    let rel = (f(&a) - f(&b)).abs() / f(&a);
    assert!(
        rel > 1e-9,
        "order 2 must not reproduce order 1 (rel {rel:e})"
    );
    assert!(rel < 0.2, "orders disagree wildly (rel {rel:e})");
}

/// `--outdir` field export is edge-based: rejected at order 2.
#[test]
fn element_order_two_rejects_outdir() {
    let spec = edited_spec("p2-outdir", |v| {
        v["element_order"] = serde_json::json!(2);
        v["frequencies"] = serde_json::json!({"unit": "ghz", "values": [1.0]});
    });
    let dir = scratch("p2-outdir-dir");
    let out = geode(&[
        "driven",
        spec.to_str().unwrap(),
        "--outdir",
        dir.to_str().unwrap(),
    ]);
    let v = assert_error(&out, "driven", "invalid_spec");
    let msg = v["error"]["message"].as_str().unwrap();
    assert!(
        msg.contains("element_order = 2") && msg.contains("--outdir"),
        "{msg}"
    );
}

#[test]
fn touchstone_is_rejected_for_eigen_and_duplicate_frequencies() {
    let dir = scratch("touchstone-reject");
    let ts = dir.join("x.s1p");
    // Eigen: rejected in the dispatch arm, before the spec is even read.
    let out = geode(&[
        "eigen",
        "/definitely/not/a/spec.json",
        "--touchstone",
        ts.to_str().unwrap(),
    ]);
    let v = assert_error(&out, "eigen", "invalid_spec");
    let msg = v["error"]["message"].as_str().unwrap();
    assert!(
        msg.contains("--touchstone") && msg.contains("eigen"),
        "{msg}"
    );
    // Duplicate driven frequencies cannot be two Touchstone rows.
    let spec = edited_spec("touchstone-dup", |v| {
        v["frequencies"] = serde_json::json!({ "unit": "ghz", "values": [1.0, 1.0] });
    });
    let out = geode(&[
        "driven",
        spec.to_str().unwrap(),
        "--touchstone",
        ts.to_str().unwrap(),
    ]);
    let v = assert_error(&out, "driven", "invalid_spec");
    assert!(
        v["error"]["message"]
            .as_str()
            .unwrap()
            .contains("more than once")
    );
    assert!(!ts.exists());
}

#[test]
fn touchstone_missing_parent_dir_fails_before_solving() {
    // Issue #713: the `.sNp` is written after the sweep, so its parent
    // directory is checked up front — an `io` error with no solve output.
    let dir = scratch("touchstone-noparent");
    let ts = dir.join("nonexistent_subdir").join("out.s1p");
    for (cmd, spec) in [
        ("driven", smoke_spec()),
        (
            "extract",
            fixtures()
                .join("slcfet_extract_smoke.json")
                .display()
                .to_string(),
        ),
    ] {
        let out = geode(&[cmd, &spec, "--touchstone", ts.to_str().unwrap()]);
        let v = assert_error(&out, cmd, "io");
        let msg = v["error"]["message"].as_str().unwrap();
        assert!(
            msg.contains("parent directory does not exist") && msg.contains("nonexistent_subdir"),
            "{msg}"
        );
        assert!(v.get("results").is_none(), "no solve output");
    }
    // A parent that is a regular file is rejected too.
    let file = dir.join("plain-file");
    std::fs::write(&file, "x").unwrap();
    let ts = file.join("out.s1p");
    let out = geode(&[
        "driven",
        &smoke_spec(),
        "--touchstone",
        ts.to_str().unwrap(),
    ]);
    let v = assert_error(&out, "driven", "io");
    assert!(
        v["error"]["message"]
            .as_str()
            .unwrap()
            .contains("not a directory")
    );
    assert!(!dir.join("nonexistent_subdir").exists());
}

/// Issue #736, item 4: a `--touchstone` parent directory that exists but
/// is permission-denied (not merely missing) must not be reported as
/// "does not exist". Unix-only (permission bits); skipped when running
/// as root, since root bypasses directory permission checks entirely.
#[test]
#[cfg(unix)]
fn touchstone_permission_denied_parent_is_not_reported_as_missing() {
    use std::os::unix::fs::PermissionsExt;

    let running_as_root = Command::new("id")
        .arg("-u")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "0")
        .unwrap_or(false);
    if running_as_root {
        eprintln!("SKIPPED: running as root, permission bits are not enforced");
        return;
    }

    let dir = scratch("touchstone-permdenied");
    let locked = dir.join("locked");
    std::fs::create_dir_all(&locked).unwrap();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();

    // The target's *parent* is one level inside `locked`: stat-ing it must
    // traverse `locked`, whose missing search (`x`) permission is what
    // actually triggers `EACCES` — denying permissions on the directory
    // being stat-ed directly is not enough on every platform (e.g. macOS
    // permits `stat` on a 0o000 directory itself as long as its own parent
    // is traversable).
    let ts = locked.join("sub").join("out.s1p");
    let out = geode(&[
        "driven",
        &smoke_spec(),
        "--touchstone",
        ts.to_str().unwrap(),
    ]);

    // Restore permissions before any assertion so a failure still leaves
    // the scratch dir removable when it drops.
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();

    let v = assert_error(&out, "driven", "io");
    let msg = v["error"]["message"].as_str().unwrap();
    assert!(
        !msg.contains("does not exist"),
        "permission-denied parent must not be reported as missing: {msg}"
    );
    assert!(
        msg.contains("permission denied") || msg.contains("not accessible"),
        "expected a permission-denied message, got: {msg}"
    );
    assert!(v.get("results").is_none(), "no solve output");
}

/// Issue #736, item 5: a `--touchstone` target that is itself an
/// existing directory is rejected fast, before any solve, with an `io`
/// error.
#[test]
fn touchstone_target_itself_a_directory_fails_before_solving() {
    let dir = scratch("touchstone-targetisdir");
    let ts = dir.join("out.s1p");
    std::fs::create_dir_all(&ts).unwrap();

    let out = geode(&[
        "driven",
        &smoke_spec(),
        "--touchstone",
        ts.to_str().unwrap(),
    ]);
    let v = assert_error(&out, "driven", "io");
    let msg = v["error"]["message"].as_str().unwrap();
    assert!(
        msg.contains("directory"),
        "expected a directory-target error, got: {msg}"
    );
    assert!(v.get("results").is_none(), "no solve output");
}

#[test]
fn check_writes_report_to_output_file() {
    let out_path_dir = scratch("check-output");
    let out_path = out_path_dir.join("report.json");
    let out = geode(&["check", &smoke_spec(), "-o", out_path.to_str().unwrap()]);
    assert!(out.status.success());
    assert!(out.stdout.is_empty(), "nothing on stdout with -o");
    let v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&out_path).unwrap()).unwrap();
    assert_eq!(v["kind"], "check");
}

#[test]
fn check_unresolved_group_lists_names() {
    let spec = edited_spec("unresolved", |v| {
        v["ports"][0]["physical_group"] = "no_such_port".into();
        // `air` exists, but as a volume (dim 3) group — not a surface.
        v["boundary_conditions"]["pec"] = serde_json::json!(["air"]);
    });
    let out = geode(&["check", spec.to_str().unwrap()]);
    let v = assert_error(&out, "check", "unresolved_physical_group");
    let msg = v["error"]["message"].as_str().unwrap();
    assert!(msg.contains("no_such_port"), "{msg}");
    assert!(msg.contains("`air` (dim 2, pec)"), "{msg}");
    assert!(msg.contains("`air` (dim 3)"), "lists what exists: {msg}");
}

#[test]
fn check_rejects_bad_inputs_with_codes() {
    // Missing spec file.
    let out = geode(&["check", "/definitely/not/here.json"]);
    assert_error(&out, "check", "io");

    // Malformed JSON.
    let bad_dir = scratch("malformed");
    let bad = bad_dir.join("spec.json");
    std::fs::write(&bad, "{ not json").unwrap();
    assert_error(
        &geode(&["check", bad.to_str().unwrap()]),
        "check",
        "spec_parse",
    );

    // Malformed TOML (routed by extension).
    let bad_toml_dir = scratch("malformed-toml");
    let bad_toml = bad_toml_dir.join("spec.toml");
    std::fs::write(&bad_toml, "schema_version = [").unwrap();
    assert_error(
        &geode(&["check", bad_toml.to_str().unwrap()]),
        "check",
        "spec_parse",
    );

    // Unknown field (typo) is rejected, not ignored.
    let typo = edited_spec("typo", |v| {
        v["frequencies"]["unit_typo"] = "ghz".into();
    });
    assert_error(
        &geode(&["check", typo.to_str().unwrap()]),
        "check",
        "spec_parse",
    );

    // Wrong schema version.
    let ver = edited_spec("version", |v| v["schema_version"] = 2.into());
    assert_error(
        &geode(&["check", ver.to_str().unwrap()]),
        "check",
        "schema_version",
    );

    // Gain medium (Im eps > 0) is rejected.
    let gain = edited_spec("gain", |v| {
        v["materials"][0]["eps_r"] = serde_json::json!([11.9, 0.1]);
    });
    assert_error(
        &geode(&["check", gain.to_str().unwrap()]),
        "check",
        "invalid_spec",
    );

    // The same physical group listed as two ports.
    let dup = edited_spec("dup-port", |v| {
        let port = v["ports"][0].clone();
        v["ports"].as_array_mut().unwrap().push(port);
    });
    let out = geode(&["check", dup.to_str().unwrap()]);
    let v = assert_error(&out, "check", "invalid_spec");
    let msg = v["error"]["message"].as_str().unwrap();
    assert!(msg.contains("more than one port"), "{msg}");

    // Missing mesh file.
    let nomesh = edited_spec("nomesh", |v| {
        v["mesh"]["path"] = "/definitely/not/here.msh".into();
    });
    assert_error(&geode(&["check", nomesh.to_str().unwrap()]), "check", "io");
}

#[test]
fn toml_spec_is_accepted() {
    let dir = scratch("toml");
    let spec = dir.join("spec.toml");
    let mesh = smoke_mesh();
    std::fs::write(
        &spec,
        format!(
            r#"
schema_version = 1

[mesh]
path = "{}"
length_unit_m = 1e-6

[[materials]]
physical_group = "substrate"
eps_r = [11.9, -0.0595]

[boundary_conditions]
pec = ["outer_boundary"]

[[ports]]
physical_group = "port"
e_hat = [0.0, 1.0, 0.0]
resistance_ohm = 50.0

[frequencies]
unit = "ghz"
start = 1.0
stop = 10.0
count = 3
spacing = "log"
"#,
            mesh.display()
        ),
    )
    .unwrap();
    let out = geode(&["check", spec.to_str().unwrap()]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v = json(&out);
    assert_eq!(v["frequencies"].as_array().unwrap().len(), 3);
    assert_eq!(v["leontovich"].as_array().unwrap().len(), 0);
}

#[test]
fn help_lists_every_subcommand() {
    let help = String::from_utf8_lossy(&geode(&["--help"]).stdout).to_string();
    for cmd in ["check", "driven", "eigen", "extract", "mesh"] {
        assert!(help.contains(cmd), "--help lists {cmd}: {help}");
    }
    let help = String::from_utf8_lossy(&geode(&["extract", "--help"]).stdout).to_string();
    assert!(help.contains("extract"), "{help}");
    assert!(help.contains("L0"), "{help}");
}

#[test]
fn backend_flag_only_confirms_the_compiled_backend() {
    let compiled = json(&geode(&["check", &smoke_spec()]))["backend"]
        .as_str()
        .unwrap()
        .to_string();
    let other = if compiled == "ndarray" {
        "wgpu"
    } else {
        "ndarray"
    };
    let out = geode(&["driven", &smoke_spec(), "--backend", other]);
    let v = assert_error(&out, "driven", "backend_mismatch");
    assert!(
        v["error"]["message"]
            .as_str()
            .unwrap()
            .contains("compiled with")
    );
    // The --help text documents the compile-time semantics.
    let help = String::from_utf8_lossy(&geode(&["driven", "--help"]).stdout).to_string();
    assert!(help.contains("BUILD time"), "{help}");
}

#[test]
fn error_report_goes_to_output_file_too() {
    let out_path_dir = scratch("err-output");
    let out_path = out_path_dir.join("report.json");
    // A driven spec under `geode extract` fails validation (no solve).
    let out = geode(&["extract", &smoke_spec(), "-o", out_path.to_str().unwrap()]);
    assert!(!out.status.success());
    let v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&out_path).unwrap()).unwrap();
    assert_error_value(&v, "extract", "invalid_spec");
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("error:"),
        "stderr carries the human-readable error"
    );
}

fn sphere_spec() -> PathBuf {
    fixtures().join("sphere_pec_golden.json")
}

/// The sphere eigen spec with `edit` applied, written to a scratch file
/// (mesh path made absolute).
fn edited_eigen_spec(name: &str, edit: impl FnOnce(&mut serde_json::Value)) -> ScratchFile {
    let raw = std::fs::read_to_string(sphere_spec()).unwrap();
    let mut v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let mesh = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../geode-core/tests/fixtures/sphere.msh")
        .canonicalize()
        .expect("sphere mesh exists");
    v["mesh"]["path"] = mesh.display().to_string().into();
    edit(&mut v);
    let text = serde_json::to_string_pretty(&v).unwrap();
    ScratchFile::write("geode-cli-test-", name, "spec.json", text)
}

#[test]
fn check_reports_the_analysis_kind() {
    // Driven spec: analysis "driven", eigen null.
    let v = json(&geode(&["check", &smoke_spec()]));
    assert_eq!(v["analysis"], "driven");
    assert!(v["eigen"].is_null());

    // Eigen spec: no ports / frequencies, resolved eigen settings echoed.
    let out = geode(&["check", sphere_spec().to_str().unwrap()]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v = json(&out);
    assert_eq!(v["analysis"], "eigen");
    assert_eq!(v["ports"].as_array().unwrap().len(), 0);
    assert_eq!(v["frequencies"].as_array().unwrap().len(), 0);
    let e = &v["eigen"];
    assert_eq!(e["n_modes"], 5);
    assert_eq!(e["shift_k0"], 1.0);
    assert_eq!(e["sigma"], 1.0);
    assert_eq!(e["max_iters"], 160);
    assert_eq!(e["residual_tol"], 1e-6);
    // k0 = 1 rad/cm → f = c / (2π · 0.01 m) ≈ 4.771 GHz.
    let f = e["shift_hz"].as_f64().unwrap();
    assert!((f - 4.771_345_159e9).abs() < 1e3, "{f}");
    assert_eq!(v["mesh"]["n_tets"], 3335);
}

#[test]
fn eigen_and_driven_reject_the_other_spec_kind() {
    // A driven spec (no `eigen` section) under `geode eigen`.
    let out = geode(&["eigen", &smoke_spec()]);
    let v = assert_error(&out, "eigen", "invalid_spec");
    let msg = v["error"]["message"].as_str().unwrap();
    assert!(msg.contains("`eigen` section"), "{msg}");

    // An eigen spec under `geode driven`.
    let out = geode(&["driven", sphere_spec().to_str().unwrap()]);
    let v = assert_error(&out, "driven", "invalid_spec");
    let msg = v["error"]["message"].as_str().unwrap();
    assert!(msg.contains("geode eigen"), "{msg}");
}

/// A boxed in-place edit of a spec's JSON.
type SpecEdit = Box<dyn FnOnce(&mut serde_json::Value)>;

#[test]
fn eigen_spec_validation_rejects_lossy_or_driven_inputs() {
    let cases: Vec<(&str, SpecEdit, &str)> = vec![
        (
            "eig-gain",
            Box::new(|v| v["materials"][0]["eps_r"] = serde_json::json!([2.25, 0.01])),
            "gain",
        ),
        (
            "eig-ports",
            Box::new(|v| {
                v["ports"] = serde_json::json!([{
                    "physical_group": "sphere_surface",
                    "e_hat": [0.0, 0.0, 1.0],
                    "resistance_ohm": 50.0
                }])
            }),
            "cannot have `ports`",
        ),
        (
            "eig-freqs",
            Box::new(|v| v["frequencies"] = serde_json::json!({"unit": "ghz", "values": [1.0]})),
            "cannot have `frequencies`",
        ),
        (
            "eig-leon",
            Box::new(|v| {
                v["boundary_conditions"]["leontovich"] = serde_json::json!([{
                    "physical_group": "sphere_surface",
                    "conductivity_s_m": 5.8e7
                }])
            }),
            "nonlinear eigenvalue problem",
        ),
        (
            "eig-sm",
            Box::new(|v| {
                v["boundary_conditions"]["silver_muller"] = serde_json::json!(["sphere_surface"])
            }),
            "quadratic (nonlinear) eigenvalue problem",
        ),
        (
            "eig-wave",
            Box::new(|v| {
                v["wave_ports"] = serde_json::json!([{ "physical_group": "sphere_surface" }])
            }),
            "wave_ports",
        ),
        (
            "eig-shift0",
            Box::new(|v| v["eigen"]["shift"] = 0.0.into()),
            "eigen.shift",
        ),
        (
            "eig-nmodes0",
            Box::new(|v| v["eigen"]["n_modes"] = 0.into()),
            "eigen.n_modes",
        ),
        (
            "eig-restol0",
            Box::new(|v| v["eigen"]["residual_tol"] = 0.0.into()),
            "eigen.residual_tol",
        ),
        (
            "eig-iterative",
            Box::new(|v| v["solver"] = serde_json::json!({"mode": "iterative"})),
            "direct",
        ),
    ];
    for (name, edit, needle) in cases {
        let spec = edited_eigen_spec(name, edit);
        for cmd in ["check", "eigen"] {
            let out = geode(&[cmd, spec.to_str().unwrap()]);
            let v = assert_error(&out, cmd, "invalid_spec");
            let msg = v["error"]["message"].as_str().unwrap();
            assert!(msg.contains(needle), "{name}/{cmd}: {msg}");
        }
    }

    // Typo in the eigen section is a parse error, not a silent default.
    let typo = edited_eigen_spec("eig-typo", |v| v["eigen"]["n_mode"] = 3.into());
    assert_error(
        &geode(&["eigen", typo.to_str().unwrap()]),
        "eigen",
        "spec_parse",
    );
}

/// Lossy materials and `absorbing_regions` are accepted in eigen specs
/// (issue #706): `geode check` classifies the pencil as complex.
#[test]
fn eigen_spec_accepts_lossy_materials_and_absorbing_regions() {
    let lossy = edited_eigen_spec("eig-lossy-ok", |v| {
        v["materials"][0]["eps_r"] = serde_json::json!([2.25, -0.01])
    });
    let out = geode(&["check", lossy.to_str().unwrap()]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v = json(&out);
    assert_eq!(v["analysis"], "eigen");
    assert_eq!(v["resources"]["scalar"], "complex");

    // The sphere mesh's outermost shell as a box UPML: accepted and
    // resolved (the solve itself is covered by `tests/eigen_upml_smoke.rs`).
    let upml = edited_eigen_spec("eig-upml-ok", |v| {
        v["absorbing_regions"] = serde_json::json!([{
            "physical_group": "pml_shell", "thickness": 0.5, "sigma_0": 2.0
        }])
    });
    let out = geode(&["check", upml.to_str().unwrap()]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v = json(&out);
    assert_eq!(v["absorbing_regions"][0]["physical_group"], "pml_shell");
    assert_eq!(v["resources"]["scalar"], "complex");

    // The lossless spec stays a real pencil.
    let v = json(&geode(&["check", sphere_spec().to_str().unwrap()]));
    assert_eq!(v["resources"]["scalar"], "real");
}

#[test]
fn driven_non_converging_iterative_solve_fails_with_solve_failed() {
    // One iteration cannot converge; non-convergence must be a hard,
    // structured error — never a silently wrong S-matrix.
    let spec = edited_spec("no-converge", |v| {
        v["frequencies"]["values"] = serde_json::json!([5.0]);
        v["solver"] = serde_json::json!({ "mode": "iterative", "tol": 1e-10, "max_iters": 1 });
    });
    let out = geode(&["driven", spec.to_str().unwrap()]);
    assert_error(&out, "driven", "solve_failed");
}

#[test]
fn eigen_unconverged_lanczos_fails_with_solve_failed() {
    // An 8-vector Lanczos basis cannot converge 5 sphere modes (residuals
    // up to ~17 before the gate existed). Unconverged Ritz values must be
    // a hard, structured error — never `status: ok` with wrong frequencies.
    let spec = edited_eigen_spec("eig-no-converge", |v| {
        v["eigen"]["max_iters"] = 8.into();
    });
    let out = geode(&["eigen", spec.to_str().unwrap()]);
    let v = assert_error(&out, "eigen", "solve_failed");
    let msg = v["error"]["message"].as_str().unwrap();
    assert!(msg.contains("did not converge"), "{msg}");
    assert!(msg.contains("max_iters"), "{msg}");
}

/// The spiral smoke driven spec turned into an extract spec (`extract`
/// section added), then `edit`ed.
fn edited_extract_spec(name: &str, edit: impl FnOnce(&mut serde_json::Value)) -> ScratchFile {
    edited_spec(name, |v| {
        v["extract"] = serde_json::json!({});
        edit(v);
    })
}

#[test]
fn check_resolves_extract_anchors() {
    // Default anchors: the spec's own (sorted) frequencies.
    let spec = edited_extract_spec("x-default", |v| {
        v["frequencies"]["values"] = serde_json::json!([5.0, 1.0, 20.0]);
    });
    let v = json(&geode(&["check", spec.to_str().unwrap()]));
    assert_eq!(v["analysis"], "extract");
    assert!(v["eigen"].is_null());
    let x = &v["extract"];
    assert_eq!(x["anchor_source"], "frequencies");
    assert!(x["l0_rel_tol"].is_null());
    let hz = |a: &serde_json::Value| -> Vec<f64> {
        a.as_array()
            .unwrap()
            .iter()
            .map(|f| f["frequency_hz"].as_f64().unwrap())
            .collect()
    };
    assert_eq!(hz(&x["anchor_frequencies"]), vec![1e9, 5e9, 20e9]);
    assert_eq!(hz(&v["frequencies"]), vec![1e9, 5e9, 20e9], "ascending");

    // Explicit anchor ladder, in another unit, overlapping the sweep:
    // solved = ascending union with the shared 1 GHz point collapsed.
    let spec = edited_extract_spec("x-anchors", |v| {
        v["frequencies"]["values"] = serde_json::json!([10.0, 1.0]);
        v["extract"] = serde_json::json!({
            "anchor_frequencies": { "unit": "hz", "values": [2e8, 1e8, 1e9] },
            "l0_rel_tol": 0.05
        });
    });
    let v = json(&geode(&["check", spec.to_str().unwrap()]));
    let x = &v["extract"];
    assert_eq!(x["anchor_source"], "anchor_frequencies");
    assert_eq!(x["l0_rel_tol"], 0.05);
    assert_eq!(hz(&x["anchor_frequencies"]), vec![1e8, 2e8, 1e9]);
    assert_eq!(hz(&v["frequencies"]), vec![1e8, 2e8, 1e9, 1e10]);

    // Driven / eigen specs report `extract: null`.
    let v = json(&geode(&["check", &smoke_spec()]));
    assert!(v["extract"].is_null());
}

#[test]
fn extract_and_other_subcommands_reject_each_others_specs() {
    // A driven spec under `geode extract`.
    let out = geode(&["extract", &smoke_spec()]);
    let v = assert_error(&out, "extract", "invalid_spec");
    let msg = v["error"]["message"].as_str().unwrap();
    assert!(msg.contains("`extract` section"), "{msg}");
    assert!(!msg.contains("  "), "stray whitespace: {msg:?}");

    // An extract spec under `geode driven` and `geode eigen`.
    let spec = edited_extract_spec("x-wrong-cmd", |_| {});
    for cmd in ["driven", "eigen"] {
        let out = geode(&[cmd, spec.to_str().unwrap()]);
        let v = assert_error(&out, cmd, "invalid_spec");
        let msg = v["error"]["message"].as_str().unwrap();
        assert!(msg.contains("geode extract"), "{cmd}: {msg}");
    }

    // An eigen spec under `geode extract`.
    let out = geode(&["extract", sphere_spec().to_str().unwrap()]);
    let v = assert_error(&out, "extract", "invalid_spec");
    assert!(
        v["error"]["message"]
            .as_str()
            .unwrap()
            .contains("geode eigen")
    );
}

#[test]
fn extract_spec_validation_rejects_underdetermined_inputs() {
    let cases: Vec<(&str, SpecEdit, &str)> = vec![
        (
            "x-one-freq",
            Box::new(|v| v["frequencies"]["values"] = serde_json::json!([5.0])),
            "at least 2 distinct L0 anchor",
        ),
        (
            "x-dup-freq",
            // 1 GHz given twice collapses to one distinct anchor.
            Box::new(|v| v["frequencies"]["values"] = serde_json::json!([1.0, 1.0])),
            "at least 2 distinct L0 anchor",
        ),
        (
            "x-one-anchor",
            Box::new(|v| {
                v["extract"] = serde_json::json!({
                    "anchor_frequencies": { "unit": "ghz", "values": [0.1] }
                })
            }),
            "`anchor_frequencies`",
        ),
        (
            "x-bad-anchor",
            Box::new(|v| {
                v["extract"] = serde_json::json!({
                    "anchor_frequencies": { "unit": "ghz", "values": [0.1, -0.2] }
                })
            }),
            "extract.anchor_frequencies",
        ),
        (
            "x-tol-two-anchors",
            Box::new(|v| {
                v["frequencies"]["values"] = serde_json::json!([1.0, 5.0]);
                v["extract"] = serde_json::json!({ "l0_rel_tol": 0.01 });
            }),
            "at least 3 distinct anchor",
        ),
        (
            "x-tol-zero",
            Box::new(|v| v["extract"] = serde_json::json!({ "l0_rel_tol": 0.0 })),
            "extract.l0_rel_tol",
        ),
        (
            "x-no-ports",
            Box::new(|v| v["ports"] = serde_json::json!([])),
            "lumped port",
        ),
        (
            "x-and-eigen",
            Box::new(|v| {
                v["eigen"] = serde_json::json!({ "n_modes": 1, "unit": "ghz", "shift": 1.0 })
            }),
            "found `eigen` and `extract`",
        ),
    ];
    for (name, edit, needle) in cases {
        let spec = edited_extract_spec(name, edit);
        for cmd in ["check", "extract"] {
            let out = geode(&[cmd, spec.to_str().unwrap()]);
            let v = assert_error(&out, cmd, "invalid_spec");
            let msg = v["error"]["message"].as_str().unwrap();
            assert!(msg.contains(needle), "{name}/{cmd}: {msg}");
        }
    }

    // Typo in the extract section is a parse error, not a silent default.
    let typo = edited_extract_spec("x-typo", |v| {
        v["extract"] = serde_json::json!({ "anchors": [0.1, 0.2] })
    });
    assert_error(
        &geode(&["extract", typo.to_str().unwrap()]),
        "extract",
        "spec_parse",
    );
}

#[test]
fn extract_l0_gate_fails_loudly_when_unconverged() {
    // Three anchors spanning 1-10 GHz are nowhere near the asymptotic
    // L(f) = L0 - a f^2 regime at a 1e-6 gate. An unconverged L0 must be a
    // hard, structured error — never `status: ok` with a wrong L0.
    let spec = edited_extract_spec("x-gate", |v| {
        v["frequencies"]["values"] = serde_json::json!([1.0, 5.0, 10.0]);
        v["extract"] = serde_json::json!({ "l0_rel_tol": 1e-6 });
    });
    let out = geode(&["extract", spec.to_str().unwrap()]);
    let v = assert_error(&out, "extract", "solve_failed");
    let msg = v["error"]["message"].as_str().unwrap();
    assert!(msg.contains("did not converge"), "{msg}");
    assert!(msg.contains("l0_rel_tol"), "{msg}");
}

#[test]
fn open_boundary_and_wave_port_spec_validation() {
    // Scalar rules (checked before the mesh is read) on the spiral smoke
    // driven spec: PEC `outer_boundary`, Leontovich `conductor_surface`,
    // lumped `port`.
    fn wave(v: &mut serde_json::Value) {
        v["ports"] = serde_json::json!([]);
        v["boundary_conditions"]["leontovich"] = serde_json::json!([]);
        v["wave_ports"] = serde_json::json!([{ "physical_group": "port" }]);
    }
    let upml = |t: f64, s0: f64| serde_json::json!({"physical_group": "air", "thickness": t, "sigma_0": s0});
    let cases: Vec<(&str, SpecEdit, &str)> = vec![
        // Wave ports with Leontovich walls are accepted (issue #776); a
        // Silver-Müller wall touching a port rim is a mesh-level rule,
        // covered in `tests/wave_port_driven.rs`.
        (
            "ob-wave-ainc",
            Box::new(|v| {
                wave(v);
                v["wave_ports"][0]["n_modes"] = 2.into();
                v["wave_ports"][0]["a_inc"] = serde_json::json!([[1.0, 0.0]]);
            }),
            "a_inc has 1 entries but n_modes = 2",
        ),
        (
            "ob-wave-ainc0",
            Box::new(|v| {
                wave(v);
                v["wave_ports"][0]["a_inc"] = serde_json::json!([[0.0, 0.0]]);
            }),
            "non-zero",
        ),
        (
            "ob-wave-nmodes0",
            Box::new(|v| {
                wave(v);
                v["wave_ports"][0]["n_modes"] = 0.into();
            }),
            "n_modes must be",
        ),
        (
            "ob-no-ports",
            Box::new(|v| v["ports"] = serde_json::json!([])),
            "lumped port (or wave port)",
        ),
        (
            "ob-sm-pec",
            Box::new(|v| {
                v["boundary_conditions"]["silver_muller"] = serde_json::json!(["outer_boundary"])
            }),
            "both a PEC surface and a Silver-Müller wall",
        ),
        (
            "ob-sm-leon",
            Box::new(|v| {
                v["boundary_conditions"]["silver_muller"] = serde_json::json!(["conductor_surface"])
            }),
            "both a Leontovich wall and a Silver-Müller wall",
        ),
        (
            "ob-sm-dup",
            Box::new(|v| {
                v["boundary_conditions"]["silver_muller"] = serde_json::json!(["abc", "abc"])
            }),
            "more than one Silver-Müller wall",
        ),
        (
            "ob-port-pec",
            Box::new(|v| {
                v["boundary_conditions"]["pec"] = serde_json::json!(["outer_boundary", "port"])
            }),
            "its edges would be eliminated",
        ),
        (
            "ob-upml-thick0",
            Box::new(move |v| v["absorbing_regions"] = serde_json::json!([upml(0.0, 25.0)])),
            "thickness must be finite and > 0",
        ),
        (
            "ob-upml-sigma-nan",
            Box::new(move |v| v["absorbing_regions"] = serde_json::json!([upml(1.0, -1.0)])),
            "sigma_0 must be finite and > 0",
        ),
        (
            "ob-upml-dup",
            Box::new(move |v| {
                v["absorbing_regions"] = serde_json::json!([upml(1.0, 25.0), upml(2.0, 25.0)])
            }),
            "listed more than once",
        ),
    ];
    for (name, edit, needle) in cases {
        let spec = edited_spec(name, edit);
        for cmd in ["check", "driven"] {
            let out = geode(&[cmd, spec.to_str().unwrap()]);
            let v = assert_error(&out, cmd, "invalid_spec");
            let msg = v["error"]["message"].as_str().unwrap();
            assert!(msg.contains(needle), "{name}/{cmd}: {msg}");
            assert!(!msg.contains("  "), "{name}: stray whitespace: {msg:?}");
        }
    }

    // `geode extract` needs port impedances: wave ports are rejected.
    let spec = edited_extract_spec("ob-extract-wave", wave);
    let v = assert_error(
        &geode(&["extract", spec.to_str().unwrap()]),
        "extract",
        "invalid_spec",
    );
    let msg = v["error"]["message"].as_str().unwrap();
    assert!(msg.contains("cannot have `wave_ports`"), "{msg}");

    // A Silver-Müller group that is not in the mesh is an unresolved
    // group, like any other reference.
    let spec = edited_spec("ob-sm-missing", |v| {
        v["boundary_conditions"]["silver_muller"] = serde_json::json!(["no_such_wall"])
    });
    let v = assert_error(
        &geode(&["check", spec.to_str().unwrap()]),
        "check",
        "unresolved_physical_group",
    );
    assert!(
        v["error"]["message"]
            .as_str()
            .unwrap()
            .contains("`no_such_wall` (dim 2, silver_muller)")
    );
}

#[test]
fn outdir_exports_lumped_port_fields_without_far_field_when_closed() {
    // One frequency of the (closed, PEC-bounded) spiral smoke spec: a
    // field file per row, but no NTFF (no `absorbing_regions` shell).
    let spec = edited_spec("outdir-closed", |v| {
        v["frequencies"] = serde_json::json!({ "unit": "ghz", "values": [5.0] });
    });
    let outdir_dir = scratch("outdir-closed");
    let outdir = outdir_dir.join("out");
    let out = geode(&[
        "driven",
        spec.to_str().unwrap(),
        "--outdir",
        outdir.to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v = json(&out);
    let r = &v["results"][0];
    assert_eq!(r["field_file"]["path"], "E_0000.vtu");
    assert_eq!(r["field_file"]["sha256"].as_str().unwrap().len(), 64);
    assert!(r.get("far_field").is_none(), "no UPML shell → no NTFF");
    let vtu = std::fs::read_to_string(outdir.join("E_0000.vtu")).unwrap();
    let n_nodes = v["mesh"]["n_nodes"].as_u64().unwrap();
    assert!(vtu.contains(&format!("NumberOfPoints=\"{n_nodes}\"")));
    assert!(vtu.contains("Name=\"E_real\"") && vtu.contains("Name=\"E_imag\""));
    let names: Vec<_> = std::fs::read_dir(&outdir)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(names, ["E_0000.vtu"], "exactly the referenced file");
}

#[test]
fn outdir_that_is_a_file_fails_with_io_before_solving() {
    let dir = scratch("outdir-file");
    let file = dir.join("not-a-dir");
    std::fs::write(&file, "x").unwrap();
    for cmd in ["driven", "extract", "eigen"] {
        let spec = match cmd {
            "driven" => smoke_spec(),
            "extract" => fixtures()
                .join("slcfet_extract_smoke.json")
                .display()
                .to_string(),
            _ => sphere_spec().display().to_string(),
        };
        let out = geode(&[cmd, &spec, "--outdir", file.to_str().unwrap()]);
        let v = assert_error(&out, cmd, "io");
        let msg = v["error"]["message"].as_str().unwrap();
        assert!(msg.contains("not-a-dir"), "{cmd}: {msg}");
    }
}
