//! `--dataset` (issue #1257, Epic #1253 Phase 1): the gradient-labelled
//! sample written by `geode driven` / `geode eigen`.
//!
//! The binary crate is not importable, so this test carries its own small
//! Zarr v2 reader — an independent check of the on-disk layout — and
//! asserts, for one driven S11 sensitivity sample (the cookbook spiral)
//! and one eigen f sensitivity sample (the cookbook box cavity):
//!
//! * every array equals the same run's `-o` report **exactly** (stored
//!   gradients, values, S, frequencies, Q — the gradients are the report's
//!   own entries, so the bar is bit equality, tighter than the 1e-12 of
//!   the issue);
//! * the written store matches the table in `docs/dataset-schema.md`
//!   (names, dtypes, dimension names, shapes) — the schema drift test;
//! * validation: a spec without `sensitivity` fails `invalid_spec` before
//!   writing anything, and `geode extract` rejects the flag.
//!
//! Write → read exactness is the unit test `dataset::tests`.

#[path = "support/scratch.rs"]
mod scratch_support;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use scratch_support::Scratch;
use serde_json::Value;

fn manifest() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn example(name: &str) -> PathBuf {
    manifest().join("examples").join(name)
}

fn geode(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_geode"))
        .args(args)
        .output()
        .expect("spawn geode")
}

fn run_ok(sub: &str, spec: &Path, dataset: &Path, report: &Path) -> Value {
    let out = geode(&[
        sub,
        spec.to_str().unwrap(),
        "--dataset",
        dataset.to_str().unwrap(),
        "-o",
        report.to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "geode {sub} failed ({}):\nstderr: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&std::fs::read(report).unwrap()).expect("report is JSON")
}

// ---------------------------------------------------------------------
// A minimal, independent Zarr v2 reader (single-chunk, uncompressed).
// ---------------------------------------------------------------------

struct Arr {
    dtype: String,
    shape: Vec<usize>,
    dims: Vec<String>,
    bytes: Vec<u8>,
}

impl Arr {
    fn f64s(&self) -> Vec<f64> {
        assert_eq!(self.dtype, "<f8");
        self.bytes
            .chunks_exact(8)
            .map(|c| f64::from_le_bytes(c.try_into().unwrap()))
            .collect()
    }
    fn u32s(&self) -> Vec<u32> {
        assert_eq!(self.dtype, "<u4");
        self.bytes
            .chunks_exact(4)
            .map(|c| u32::from_le_bytes(c.try_into().unwrap()))
            .collect()
    }
    fn i32s(&self) -> Vec<i32> {
        assert_eq!(self.dtype, "<i4");
        self.bytes
            .chunks_exact(4)
            .map(|c| i32::from_le_bytes(c.try_into().unwrap()))
            .collect()
    }
}

fn json_file(p: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(p).unwrap_or_else(|e| panic!("{}: {e}", p.display())))
        .unwrap()
}

/// Every array under `root` (recursively found by `.zarray`), by relative path.
fn read_store(root: &Path) -> (Value, BTreeMap<String, Arr>) {
    assert_eq!(json_file(&root.join(".zgroup"))["zarr_format"], 2);
    let attrs = json_file(&root.join(".zattrs"));
    let mut out = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for e in std::fs::read_dir(&dir).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                stack.push(p);
            }
        }
        if dir.join(".zarray").exists() {
            let meta = json_file(&dir.join(".zarray"));
            assert_eq!(meta["zarr_format"], 2);
            assert!(meta["compressor"].is_null() && meta["filters"].is_null());
            assert_eq!(meta["order"], "C");
            let shape: Vec<usize> = meta["shape"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_u64().unwrap() as usize)
                .collect();
            assert_eq!(meta["chunks"], meta["shape"], "one chunk per array");
            let key = vec!["0"; shape.len()].join(".");
            let bytes = std::fs::read(dir.join(&key)).unwrap();
            let dims = json_file(&dir.join(".zattrs"))["_ARRAY_DIMENSIONS"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_string())
                .collect();
            let dtype = meta["dtype"].as_str().unwrap().to_string();
            let size = if dtype == "<f8" { 8 } else { 4 };
            assert_eq!(bytes.len(), shape.iter().product::<usize>() * size);
            let rel = dir
                .strip_prefix(root)
                .unwrap()
                .to_str()
                .unwrap()
                .to_string();
            out.insert(
                rel,
                Arr {
                    dtype,
                    shape,
                    dims,
                    bytes,
                },
            );
        }
    }
    (attrs, out)
}

/// Rows of the schema table in `docs/dataset-schema.md`:
/// path → (dtype, dims, kind).
fn doc_table() -> BTreeMap<String, (String, Vec<String>, String)> {
    let doc = std::fs::read_to_string(manifest().join("../../docs/dataset-schema.md")).unwrap();
    doc.lines()
        .filter(|l| l.starts_with("| `"))
        .map(|l| {
            let c: Vec<String> = l
                .trim_matches('|')
                .split('|')
                .map(|s| s.trim().trim_matches('`').to_string())
                .collect();
            assert_eq!(c.len(), 4, "{l}");
            let dims = c[2].split(',').map(|d| d.trim().to_string()).collect();
            (c[0].clone(), (c[1].clone(), dims, c[3].clone()))
        })
        .collect()
}

/// Schema drift: the written arrays are exactly the table's rows for
/// `kind`, with the documented dtype and dimension names.
fn assert_matches_doc(kind: &str, store: &BTreeMap<String, Arr>) {
    let table = doc_table();
    let want: Vec<&String> = table
        .iter()
        .filter(|(_, (_, _, k))| k == "both" || k == kind)
        .map(|(p, _)| p)
        .collect();
    let got: Vec<&String> = store.keys().collect();
    assert_eq!(got, want, "arrays written vs docs/dataset-schema.md");
    for (path, arr) in store {
        let (dtype, dims, _) = &table[path];
        assert_eq!(&arr.dtype, dtype, "{path} dtype");
        assert_eq!(&arr.dims, dims, "{path} dims");
        assert_eq!(arr.shape.len(), dims.len(), "{path} rank");
    }
}

fn f(v: &Value) -> f64 {
    v.as_f64().unwrap()
}

/// Checks shared by both kinds; returns `(n_samples, n_entries)`.
fn assert_common(kind: &str, report: &Value, attrs: &Value, st: &BTreeMap<String, Arr>) {
    assert_matches_doc(kind, st);
    assert_eq!(attrs["format"], "geode-dataset");
    assert_eq!(attrs["schema_version"], 1);
    assert_eq!(attrs["kind"], kind);
    let prov = &attrs["provenance"];
    assert_eq!(prov["geode_version"], report["geode_version"]);
    assert_eq!(prov["git_sha"], report["git_sha"]);
    assert_eq!(prov["mesh_sha256"], report["mesh"]["sha256"]);
    assert_eq!(prov["spec_sha256"].as_str().unwrap().len(), 64);

    // Mesh.
    let (n_nodes, n_tets, n_edges) = (
        report["mesh"]["n_nodes"].as_u64().unwrap() as usize,
        report["mesh"]["n_tets"].as_u64().unwrap() as usize,
        report["mesh"]["n_edges"].as_u64().unwrap() as usize,
    );
    assert_eq!(st["mesh/nodes"].shape, [n_nodes, 3]);
    assert_eq!(st["mesh/tets"].shape, [n_tets, 4]);
    assert_eq!(st["mesh/tet_physical_tag"].shape, [n_tets]);
    assert!(
        st["mesh/tets"]
            .u32s()
            .iter()
            .all(|&v| (v as usize) < n_nodes)
    );

    // Parameters.
    let sens = &report["sensitivities"];
    let params = sens["parameters"].as_array().unwrap();
    assert_eq!(attrs["parameters"], sens["parameters"]);
    let values: Vec<f64> = params.iter().map(|q| f(&q["value"])).collect();
    assert_eq!(st["params/values"].f64s(), values);
    assert_eq!(st["params/bounds"].shape, [params.len(), 2]);

    // Fields: finite, right shapes, one sample per report row / mode.
    let n_samples = st["obs/frequency_hz"].shape[0];
    assert_eq!(st["field/e_edges"].shape, [n_samples, n_edges, 2]);
    assert_eq!(st["field/e_cell_centre"].shape, [n_samples, n_tets, 3, 2]);
    for p in ["field/e_edges", "field/e_cell_centre"] {
        let v = st[p].f64s();
        assert!(v.iter().all(|x| x.is_finite()), "{p}");
        assert!(v.iter().any(|&x| x != 0.0), "{p} is all zero");
    }

    // Gradients: the report's entries, exactly (≤ 1e-12 required).
    let entries = sens["entries"].as_array().unwrap();
    let (par, smp, obs) = (
        st["grad/entry_parameter"].u32s(),
        st["grad/entry_sample"].u32s(),
        st["grad/entry_observable"].i32s(),
    );
    let (val, grad) = (st["grad/value"].f64s(), st["grad/gradient"].f64s());
    assert_eq!(grad.len(), entries.len());
    for (i, e) in entries.iter().enumerate() {
        assert_eq!(par[i] as u64, e["parameter"].as_u64().unwrap(), "entry {i}");
        assert_eq!(smp[i] as u64, e["index"][0].as_u64().unwrap(), "entry {i}");
        let want_obs = e["observable"].as_i64().unwrap_or(-1);
        assert_eq!(i64::from(obs[i]), want_obs, "entry {i}");
        assert_eq!(val[i], f(&e["value"]), "entry {i} value");
        assert_eq!(grad[i], f(&e["gradient"]), "entry {i} gradient");
        let rel = (grad[i] - f(&e["gradient"])).abs() / f(&e["gradient"]).abs().max(1e-300);
        assert!(rel <= 1e-12, "entry {i}: {rel:e}");
        assert!(grad[i].is_finite() && grad[i] != 0.0, "entry {i}");
    }
}

#[test]
fn eigen_sample_matches_report() {
    let tmp = Scratch::new("geode-dataset-", "eigen");
    let ds = tmp.join("sample");
    let report = run_ok(
        "eigen",
        &example("sensitivity/eigen_box_cavity.json"),
        &ds,
        &tmp.join("r.json"),
    );
    assert_eq!(report["sensitivities"]["observable"], "frequency_hz");
    let (attrs, st) = read_store(&ds);
    assert_common("eigen", &report, &attrs, &st);

    let modes = report["modes"].as_array().unwrap();
    assert_eq!(st["obs/frequency_hz"].shape, [modes.len()]);
    let freq: Vec<f64> = modes.iter().map(|m| f(&m["frequency_hz"])).collect();
    assert_eq!(st["obs/frequency_hz"].f64s(), freq);
    // Lossless pencil: Q is null in the report, NaN in the sample.
    assert!(modes.iter().all(|m| m["q"].is_null()));
    assert!(st["obs/q"].f64s().iter().all(|q| q.is_nan()));
    assert!(!st.contains_key("obs/s"));
    // The sensitivity's mode index addresses the same sample row.
    assert_eq!(
        report["sensitivities"]["entries"].as_array().unwrap().len(),
        2
    );
}

#[test]
fn driven_sample_matches_report() {
    let tmp = Scratch::new("geode-dataset-", "driven");
    // The cookbook spiral without its FD self-check (four extra sweeps
    // are not what this test is about) and with an absolute mesh path.
    let mut spec: Value = json_file(&example("sensitivity/driven_spiral.json"));
    spec["sensitivity"]
        .as_object_mut()
        .unwrap()
        .remove("fd_check");
    let mesh = example("sensitivity").join(spec["mesh"]["path"].as_str().unwrap());
    spec["mesh"]["path"] = mesh.canonicalize().unwrap().to_str().unwrap().into();
    let spec_path = tmp.join("spec.json");
    std::fs::write(&spec_path, serde_json::to_string(&spec).unwrap()).unwrap();

    let ds = tmp.join("sample");
    let report = run_ok("driven", &spec_path, &ds, &tmp.join("r.json"));
    assert_eq!(report["sensitivities"]["observable"], "s11_mag_sq");
    let (attrs, st) = read_store(&ds);
    assert_common("driven", &report, &attrs, &st);

    let rows = report["results"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    let freq: Vec<f64> = rows.iter().map(|r| f(&r["frequency_hz"])).collect();
    assert_eq!(st["obs/frequency_hz"].f64s(), freq);
    // S matrix, exactly the report's `results[].s`.
    assert_eq!(st["obs/s"].shape, [2, 1, 1, 2]);
    let s: Vec<f64> = rows
        .iter()
        .flat_map(|r| [f(&r["s"][0][0][0]), f(&r["s"][0][0][1])])
        .collect();
    assert_eq!(st["obs/s"].f64s(), s);
    // The sensitivity's `value` is |S11|² of the same forward.
    let value = st["grad/value"].f64s();
    for (i, v) in value.iter().enumerate() {
        let row = st["grad/entry_sample"].u32s()[i] as usize;
        let want = s[2 * row].powi(2) + s[2 * row + 1].powi(2);
        assert!((v - want).abs() <= 1e-12 * want.max(1e-300), "entry {i}");
    }
}

#[test]
fn dataset_needs_a_sensitivity_section_and_writes_nothing() {
    let tmp = Scratch::new("geode-dataset-", "reject");
    let ds = tmp.join("sample");
    let spec = manifest().join("examples/eigen/sphere_cavity.json");
    let out = geode(&[
        "eigen",
        spec.to_str().unwrap(),
        "--dataset",
        ds.to_str().unwrap(),
    ]);
    assert!(!out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["error"]["code"], "invalid_spec", "{v:#}");
    assert!(
        v["error"]["message"]
            .as_str()
            .unwrap()
            .contains("sensitivity"),
        "{v:#}"
    );
    assert!(!ds.exists(), "nothing may be written on rejection");
}

#[test]
fn extract_rejects_the_flag() {
    let tmp = Scratch::new("geode-dataset-", "extract");
    let spec = manifest().join("examples/extract/slcfet_spiral.json");
    let out = geode(&[
        "extract",
        spec.to_str().unwrap(),
        "--dataset",
        tmp.join("s").to_str().unwrap(),
    ]);
    assert!(!out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["error"]["code"], "invalid_spec", "{v:#}");
    assert!(!tmp.join("s").exists());
}
