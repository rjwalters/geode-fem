//! Opt-in gradient-labelled dataset export under `--dataset <DIR>` (issue
//! #1257, Epic #1253 Phase 1).
//!
//! One run of `geode driven` or `geode eigen` with a `sensitivity` section
//! writes **one sample**: the mesh, the parameter vector, the solution
//! field, the observables and the gradient of every observable w.r.t. every
//! parameter, plus provenance. Nothing here runs without the flag.
//!
//! # Format
//!
//! A **Zarr v2 directory store** with no compressor: every array is a single
//! chunk of raw little-endian bytes (`<f8`, `<u4`, `<i4`, C order) described
//! by a `.zarray` file, groups carry `.zgroup`, and the root `.zattrs` holds
//! the JSON metadata. It needs no dependency beyond `serde_json` (HDF5 would
//! be a C dependency, which the FFI policy gates on operator sign-off) and
//! `zarr.open(dir)` reads it directly in Python. The layout is specified in
//! `docs/dataset-schema.md`; [`ARRAYS`] is the machine-readable twin that
//! `tests/dataset_roundtrip.rs` checks against the document.
//!
//! # Gradients
//!
//! The gradients are the in-memory [`SensitivityReport`] entries, serialised
//! unchanged, so they equal the same run's `-o` report bit for bit.

// The binary only writes samples: the reader and the schema's dtype / kind
// metadata are the module's read-side API, exercised by the unit tests
// (this is a bin-only crate, so they would otherwise warn as dead code).
#![cfg_attr(not(test), allow(dead_code))]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use faer::c64;
use geode_core::mesh::TetMesh;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::error::CliError;
use crate::problem::Problem;
use crate::report::{Provenance, SensitivityReport};

/// Dataset schema version (`.zattrs` `schema_version`); bumped on any
/// breaking layout change.
pub const DATASET_SCHEMA_VERSION: u32 = 1;

/// `.zattrs` `format` value.
pub const FORMAT: &str = "geode-dataset";

/// Element type of a stored array.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dtype {
    F64,
    U32,
    I32,
}

impl Dtype {
    /// Zarr v2 dtype string (little endian).
    pub fn zarr(self) -> &'static str {
        match self {
            Dtype::F64 => "<f8",
            Dtype::U32 => "<u4",
            Dtype::I32 => "<i4",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s {
            "<f8" => Some(Dtype::F64),
            "<u4" => Some(Dtype::U32),
            "<i4" => Some(Dtype::I32),
            _ => None,
        }
    }

    fn size(self) -> usize {
        match self {
            Dtype::F64 => 8,
            Dtype::U32 | Dtype::I32 => 4,
        }
    }
}

/// Which `kind`s of sample carry an array.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kinds {
    Both,
    DrivenOnly,
    EigenOnly,
}

/// Schema entry of one array: path, dtype, named dimensions, kinds.
pub struct ArraySpec {
    pub path: &'static str,
    pub dtype: Dtype,
    pub dims: &'static [&'static str],
    pub kinds: Kinds,
}

const fn a(
    path: &'static str,
    dtype: Dtype,
    dims: &'static [&'static str],
    kinds: Kinds,
) -> ArraySpec {
    ArraySpec {
        path,
        dtype,
        dims,
        kinds,
    }
}

/// The array layout of schema version [`DATASET_SCHEMA_VERSION`]. Mirrored
/// by the table in `docs/dataset-schema.md` (drift-tested).
pub const ARRAYS: &[ArraySpec] = &[
    a("mesh/nodes", Dtype::F64, &["node", "xyz"], Kinds::Both),
    a("mesh/tets", Dtype::U32, &["tet", "vertex"], Kinds::Both),
    a("mesh/tet_physical_tag", Dtype::I32, &["tet"], Kinds::Both),
    a("params/values", Dtype::F64, &["param"], Kinds::Both),
    a(
        "params/bounds",
        Dtype::F64,
        &["param", "bound"],
        Kinds::Both,
    ),
    a(
        "field/e_edges",
        Dtype::F64,
        &["sample", "edge", "reim"],
        Kinds::Both,
    ),
    a(
        "field/e_cell_centre",
        Dtype::F64,
        &["sample", "tet", "xyz", "reim"],
        Kinds::Both,
    ),
    a("obs/frequency_hz", Dtype::F64, &["sample"], Kinds::Both),
    a(
        "obs/s",
        Dtype::F64,
        &["sample", "port", "port", "reim"],
        Kinds::DrivenOnly,
    ),
    a("obs/q", Dtype::F64, &["sample"], Kinds::EigenOnly),
    a("grad/entry_parameter", Dtype::U32, &["entry"], Kinds::Both),
    a("grad/entry_sample", Dtype::U32, &["entry"], Kinds::Both),
    a("grad/entry_observable", Dtype::I32, &["entry"], Kinds::Both),
    a("grad/value", Dtype::F64, &["entry"], Kinds::Both),
    a("grad/gradient", Dtype::F64, &["entry"], Kinds::Both),
];

/// Group directories (each holds a `.zgroup`).
pub const GROUPS: &[&str] = &["mesh", "params", "field", "obs", "grad"];

/// Typed array payload.
#[derive(Clone, Debug, PartialEq)]
pub enum Data {
    F64(Vec<f64>),
    U32(Vec<u32>),
    I32(Vec<i32>),
}

impl Data {
    fn dtype(&self) -> Dtype {
        match self {
            Data::F64(_) => Dtype::F64,
            Data::U32(_) => Dtype::U32,
            Data::I32(_) => Dtype::I32,
        }
    }

    fn len(&self) -> usize {
        match self {
            Data::F64(v) => v.len(),
            Data::U32(v) => v.len(),
            Data::I32(v) => v.len(),
        }
    }

    fn to_le_bytes(&self) -> Vec<u8> {
        match self {
            Data::F64(v) => v.iter().flat_map(|x| x.to_le_bytes()).collect(),
            Data::U32(v) => v.iter().flat_map(|x| x.to_le_bytes()).collect(),
            Data::I32(v) => v.iter().flat_map(|x| x.to_le_bytes()).collect(),
        }
    }

    fn from_le_bytes(dtype: Dtype, b: &[u8]) -> Self {
        match dtype {
            Dtype::F64 => Data::F64(
                b.chunks_exact(8)
                    .map(|c| f64::from_le_bytes(c.try_into().unwrap()))
                    .collect(),
            ),
            Dtype::U32 => Data::U32(
                b.chunks_exact(4)
                    .map(|c| u32::from_le_bytes(c.try_into().unwrap()))
                    .collect(),
            ),
            Dtype::I32 => Data::I32(
                b.chunks_exact(4)
                    .map(|c| i32::from_le_bytes(c.try_into().unwrap()))
                    .collect(),
            ),
        }
    }
}

/// One stored array: C-order `data` of `shape`.
#[derive(Clone, Debug, PartialEq)]
pub struct Array {
    pub shape: Vec<usize>,
    pub data: Data,
}

impl Array {
    fn new(shape: Vec<usize>, data: Data) -> Self {
        debug_assert_eq!(shape.iter().product::<usize>(), data.len());
        Self { shape, data }
    }

    /// The `f64` payload (panics on another dtype).
    pub fn f64s(&self) -> &[f64] {
        match &self.data {
            Data::F64(v) => v,
            d => panic!("array is {:?}, not f64", d.dtype()),
        }
    }
}

/// A whole sample: the arrays by schema path plus the root metadata.
#[derive(Clone, Debug, PartialEq)]
pub struct Sample {
    /// Root `.zattrs`.
    pub attrs: Value,
    /// Arrays keyed by [`ArraySpec::path`].
    pub arrays: BTreeMap<String, Array>,
}

fn io_err(path: &Path, err: std::io::Error) -> CliError {
    CliError::Io {
        path: path.to_path_buf(),
        err,
    }
}

fn bad(path: &Path, msg: impl std::fmt::Display) -> CliError {
    CliError::Io {
        path: path.to_path_buf(),
        err: std::io::Error::new(std::io::ErrorKind::InvalidData, msg.to_string()),
    }
}

fn write_json(path: &Path, v: &Value) -> Result<(), CliError> {
    let mut s = serde_json::to_string_pretty(v)?;
    s.push('\n');
    std::fs::write(path, s).map_err(|e| io_err(path, e))
}

fn read_json(path: &Path) -> Result<Value, CliError> {
    let bytes = std::fs::read(path).map_err(|e| io_err(path, e))?;
    serde_json::from_slice(&bytes).map_err(|e| bad(path, e))
}

/// Zarr chunk key of a single-chunk array of rank `rank`: `0.0…0`.
fn chunk_key(rank: usize) -> String {
    vec!["0"; rank].join(".")
}

/// Write `sample` as a Zarr v2 directory store at `dir` (created if
/// missing; same-named files are overwritten, nothing else is touched).
pub fn write(dir: &Path, sample: &Sample) -> Result<(), CliError> {
    std::fs::create_dir_all(dir).map_err(|e| io_err(dir, e))?;
    write_json(&dir.join(".zgroup"), &json!({"zarr_format": 2}))?;
    write_json(&dir.join(".zattrs"), &sample.attrs)?;
    for g in GROUPS {
        let gd = dir.join(g);
        std::fs::create_dir_all(&gd).map_err(|e| io_err(&gd, e))?;
        write_json(&gd.join(".zgroup"), &json!({"zarr_format": 2}))?;
    }
    for (path, arr) in &sample.arrays {
        let spec = ARRAYS
            .iter()
            .find(|s| s.path == path)
            .ok_or_else(|| bad(dir, format!("array `{path}` is not in the schema")))?;
        let ad = dir.join(path);
        std::fs::create_dir_all(&ad).map_err(|e| io_err(&ad, e))?;
        let zarray = json!({
            "zarr_format": 2,
            "shape": arr.shape,
            "chunks": arr.shape,
            "dtype": arr.data.dtype().zarr(),
            "compressor": null,
            "fill_value": null,
            "order": "C",
            "filters": null,
        });
        write_json(&ad.join(".zarray"), &zarray)?;
        write_json(
            &ad.join(".zattrs"),
            &json!({"_ARRAY_DIMENSIONS": spec.dims}),
        )?;
        let chunk = ad.join(chunk_key(arr.shape.len()));
        std::fs::write(&chunk, arr.data.to_le_bytes()).map_err(|e| io_err(&chunk, e))?;
    }
    Ok(())
}

/// Read a sample back from `dir` (every schema array that is present).
pub fn read(dir: &Path) -> Result<Sample, CliError> {
    let attrs = read_json(&dir.join(".zattrs"))?;
    let mut arrays = BTreeMap::new();
    for spec in ARRAYS {
        let ad = dir.join(spec.path);
        if !ad.join(".zarray").exists() {
            continue;
        }
        let meta = read_json(&ad.join(".zarray"))?;
        let dtype = meta["dtype"]
            .as_str()
            .and_then(Dtype::parse)
            .ok_or_else(|| bad(&ad, "unsupported dtype"))?;
        let shape: Vec<usize> = meta["shape"]
            .as_array()
            .ok_or_else(|| bad(&ad, "missing shape"))?
            .iter()
            .map(|v| v.as_u64().map(|n| n as usize))
            .collect::<Option<_>>()
            .ok_or_else(|| bad(&ad, "bad shape"))?;
        let chunk = ad.join(chunk_key(shape.len()));
        let bytes = std::fs::read(&chunk).map_err(|e| io_err(&chunk, e))?;
        let n: usize = shape.iter().product();
        if bytes.len() != n * dtype.size() {
            return Err(bad(&chunk, "chunk size does not match shape"));
        }
        arrays.insert(
            spec.path.to_string(),
            Array::new(shape, Data::from_le_bytes(dtype, &bytes)),
        );
    }
    Ok(Sample { attrs, arrays })
}

/// Evaluate the lowest-order Nédélec (Whitney) interpolant of `e_edges`
/// (`mesh.edges()` order, global orientation low → high node) at every tet
/// centroid: `E = Σ_e d_e (λ_a∇λ_b − λ_b∇λ_a)` with `λ = ¼` at the
/// centroid. Returns `[Ex, Ey, Ez]` per tet.
///
/// Unlike [`geode_util::viz::edge_field_to_nodes`] (a vertex average for
/// plotting) this is the interpolant itself, sampled inside the element.
pub fn cell_centre_field(mesh: &TetMesh, e_edges: &[c64]) -> Vec<[c64; 3]> {
    const LOCAL_EDGES: [(usize, usize); 6] = [(0, 1), (0, 2), (0, 3), (1, 2), (1, 3), (2, 3)];
    assert_eq!(e_edges.len(), mesh.edges().len(), "p=1 edge DOF vector");
    let tet_edges = mesh.tet_edges();
    mesh.tets
        .iter()
        .enumerate()
        .map(|(t, tet)| {
            let grad = barycentric_gradients(mesh, tet);
            let mut e = [c64::new(0.0, 0.0); 3];
            for (slot, &(la, lb)) in LOCAL_EDGES.iter().enumerate() {
                let (idx, sign) = tet_edges[t][slot];
                let d = e_edges[idx as usize] * c64::new(f64::from(sign), 0.0);
                for (k, e_k) in e.iter_mut().enumerate() {
                    *e_k += d * c64::new(0.25 * (grad[lb][k] - grad[la][k]), 0.0);
                }
            }
            e
        })
        .collect()
}

/// `∇λ_i` of the four barycentric coordinates of `tet`.
fn barycentric_gradients(mesh: &TetMesh, tet: &[u32; 4]) -> [[f64; 3]; 4] {
    let x: [[f64; 3]; 4] = std::array::from_fn(|i| mesh.nodes[tet[i] as usize]);
    let col = |i: usize| -> [f64; 3] { std::array::from_fn(|k| x[i][k] - x[0][k]) };
    let (a, b, c) = (col(1), col(2), col(3));
    let cross = |u: [f64; 3], v: [f64; 3]| -> [f64; 3] {
        [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ]
    };
    let det = {
        let bc = cross(b, c);
        a[0] * bc[0] + a[1] * bc[1] + a[2] * bc[2]
    };
    // Rows of J⁻¹ (J = [a b c] by columns): ∇λ_1 = (b×c)/det, etc.
    let scale = |v: [f64; 3]| -> [f64; 3] { std::array::from_fn(|k| v[k] / det) };
    let g1 = scale(cross(b, c));
    let g2 = scale(cross(c, a));
    let g3 = scale(cross(a, b));
    let g0: [f64; 3] = std::array::from_fn(|k| -(g1[k] + g2[k] + g3[k]));
    [g0, g1, g2, g3]
}

/// Physical-domain bounds of a parameter kind: `[lower, upper]`, `±∞`
/// meaning unbounded on that side (the spec carries no bounds of its own).
fn param_bounds(kind: &str) -> [f64; 2] {
    match kind {
        "eps_r_imag" | "tan_delta" => [0.0, f64::INFINITY],
        "eps_r" | "nu_r" | "mu_r" => [f64::MIN_POSITIVE, f64::INFINITY],
        _ => [f64::NEG_INFINITY, f64::INFINITY],
    }
}

/// What the solver hands the dataset builder besides the problem and the
/// gradient report.
pub struct SampleInputs<'a> {
    /// `"driven"` or `"eigen"`.
    pub kind: &'static str,
    pub provenance: &'a Provenance,
    pub sensitivities: &'a SensitivityReport,
    /// Edge-DOF vector per sample (frequency row or mode), `mesh.edges()` order.
    pub e_edges: &'a [Vec<c64>],
    /// Observable frequency per sample (Hz).
    pub frequency_hz: Vec<f64>,
    /// Driven: `S[sample][i][j]` as `[re, im]`.
    pub s: Option<Vec<Vec<Vec<[f64; 2]>>>>,
    /// Eigen: `Q` per sample, NaN where the report has `null`.
    pub q: Option<Vec<f64>>,
}

fn flat_complex(v: impl IntoIterator<Item = c64>) -> Vec<f64> {
    v.into_iter().flat_map(|z| [z.re, z.im]).collect()
}

/// Assemble the in-memory [`Sample`] of a finished run.
pub fn build(p: &Problem, inp: SampleInputs<'_>) -> Result<Sample, CliError> {
    let mesh = &p.tagged.mesh;
    let sens = inp.sensitivities;
    let n_samples = inp.e_edges.len();
    let n_edges = p.edges.len();
    let n_tets = mesh.n_tets();
    let mut arrays = BTreeMap::new();
    let mut put = |path: &str, shape: Vec<usize>, data: Data| {
        arrays.insert(path.to_string(), Array::new(shape, data));
    };

    put(
        "mesh/nodes",
        vec![mesh.n_nodes(), 3],
        Data::F64(mesh.nodes.iter().flatten().copied().collect()),
    );
    put(
        "mesh/tets",
        vec![n_tets, 4],
        Data::U32(mesh.tets.iter().flatten().copied().collect()),
    );
    put(
        "mesh/tet_physical_tag",
        vec![n_tets],
        Data::I32(p.tagged.tet_physical_tags.clone()),
    );

    let n_params = sens.parameters.len();
    put(
        "params/values",
        vec![n_params],
        Data::F64(sens.parameters.iter().map(|q| q.value).collect()),
    );
    put(
        "params/bounds",
        vec![n_params, 2],
        Data::F64(
            sens.parameters
                .iter()
                .flat_map(|q| param_bounds(q.kind))
                .collect(),
        ),
    );

    for (i, e) in inp.e_edges.iter().enumerate() {
        if e.len() != n_edges || e.iter().any(|z| !(z.re.is_finite() && z.im.is_finite())) {
            return Err(CliError::NonFinite {
                index: i,
                what: "--dataset field: wrong length or non-finite edge DOF".into(),
            });
        }
    }
    put(
        "field/e_edges",
        vec![n_samples, n_edges, 2],
        Data::F64(flat_complex(inp.e_edges.iter().flatten().copied())),
    );
    let centres: Vec<f64> = inp
        .e_edges
        .iter()
        .flat_map(|e| flat_complex(cell_centre_field(mesh, e).into_iter().flatten()))
        .collect();
    put(
        "field/e_cell_centre",
        vec![n_samples, n_tets, 3, 2],
        Data::F64(centres),
    );

    put(
        "obs/frequency_hz",
        vec![n_samples],
        Data::F64(inp.frequency_hz),
    );
    if let Some(s) = inp.s {
        let n = s.first().map_or(0, Vec::len);
        put(
            "obs/s",
            vec![n_samples, n, n, 2],
            Data::F64(s.into_iter().flatten().flatten().flatten().collect()),
        );
    }
    if let Some(q) = inp.q {
        put("obs/q", vec![n_samples], Data::F64(q));
    }

    let entries = &sens.entries;
    let sample_of = |e: &crate::report::SensitivityEntry| -> Result<u32, CliError> {
        e.index
            .first()
            .and_then(|&i| u32::try_from(i).ok())
            .filter(|&i| (i as usize) < n_samples)
            .ok_or_else(|| {
                CliError::InvalidSpec(
                    "--dataset: a sensitivity entry has no sample index into the report".into(),
                )
            })
    };
    let n_entries = entries.len();
    put(
        "grad/entry_parameter",
        vec![n_entries],
        Data::U32(entries.iter().map(|e| e.parameter as u32).collect()),
    );
    put(
        "grad/entry_sample",
        vec![n_entries],
        Data::U32(entries.iter().map(sample_of).collect::<Result<_, _>>()?),
    );
    put(
        "grad/entry_observable",
        vec![n_entries],
        Data::I32(
            entries
                .iter()
                .map(|e| e.observable.map_or(-1, |o| o as i32))
                .collect(),
        ),
    );
    put(
        "grad/value",
        vec![n_entries],
        Data::F64(entries.iter().map(|e| e.value).collect()),
    );
    put(
        "grad/gradient",
        vec![n_entries],
        Data::F64(entries.iter().map(|e| e.gradient).collect()),
    );

    let spec_sha256 = std::fs::read(&inp.provenance.spec_path)
        .map(|b| format!("{:x}", Sha256::digest(&b)))
        .map_err(|err| CliError::Io {
            path: PathBuf::from(&inp.provenance.spec_path),
            err,
        })?;
    let physical_groups: Vec<Value> = mesh
        .physical_groups
        .iter()
        .map(|(&(dim, tag), name)| json!({"dim": dim, "tag": tag, "name": name}))
        .collect();
    let attrs = json!({
        "format": FORMAT,
        "schema_version": DATASET_SCHEMA_VERSION,
        "kind": inp.kind,
        "provenance": {
            "geode_version": inp.provenance.geode_version,
            "git_sha": inp.provenance.git_sha,
            "backend": inp.provenance.backend,
            "spec_sha256": spec_sha256,
            "mesh_sha256": p.mesh_sha256,
        },
        "length_unit_m": p.length_unit_m(),
        "physical_groups": physical_groups,
        "parameters": serde_json::to_value(&sens.parameters)?,
        "observable": sens.observable,
        "observable_unit": sens.observable_unit,
        "gradient_method": sens.method,
        "observables": serde_json::to_value(&sens.observables)?,
    });
    Ok(Sample { attrs, arrays })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tet() -> TetMesh {
        TetMesh {
            nodes: vec![
                [0.1, 0.0, 0.2],
                [1.3, 0.2, 0.0],
                [0.0, 0.9, 0.3],
                [0.2, 0.1, 1.1],
            ],
            tets: vec![[0, 1, 2, 3]],
            ..Default::default()
        }
    }

    /// Edge DOFs of the exact tangential line integrals of `field`
    /// (linear in `x`, so the midpoint rule is exact).
    fn dofs(mesh: &TetMesh, field: impl Fn([f64; 3]) -> [f64; 3]) -> Vec<c64> {
        mesh.edges()
            .iter()
            .map(|&[a, b]| {
                let (xa, xb) = (mesh.nodes[a as usize], mesh.nodes[b as usize]);
                let mid: [f64; 3] = std::array::from_fn(|k| 0.5 * (xa[k] + xb[k]));
                let e = field(mid);
                c64::new((0..3).map(|k| e[k] * (xb[k] - xa[k])).sum(), 0.0)
            })
            .collect()
    }

    /// The Whitney space contains constants and `b × x`: both are
    /// reproduced exactly at the centroid.
    #[test]
    fn cell_centre_field_reproduces_constant_and_rotational_fields() {
        let mesh = tet();
        let c = [0.3, -0.7, 0.45];
        let b = [0.5, 0.25, -1.5];
        let centroid: [f64; 3] =
            std::array::from_fn(|k| mesh.nodes.iter().map(|n| n[k]).sum::<f64>() / 4.0);
        let rot = |x: [f64; 3]| -> [f64; 3] {
            [
                b[1] * x[2] - b[2] * x[1],
                b[2] * x[0] - b[0] * x[2],
                b[0] * x[1] - b[1] * x[0],
            ]
        };
        let check = |name: &str, got: Vec<[c64; 3]>, want: [f64; 3]| {
            for k in 0..3 {
                assert!(
                    (got[0][k].re - want[k]).abs() < 1e-12 && got[0][k].im == 0.0,
                    "{name}[{k}]: {} vs {}",
                    got[0][k].re,
                    want[k]
                );
            }
        };
        check("constant", cell_centre_field(&mesh, &dofs(&mesh, |_| c)), c);
        check(
            "b x r",
            cell_centre_field(&mesh, &dofs(&mesh, rot)),
            rot(centroid),
        );
    }

    #[test]
    fn write_read_round_trips_every_dtype_bitwise() {
        let mut arrays = BTreeMap::new();
        arrays.insert(
            "mesh/nodes".to_string(),
            Array::new(
                vec![2, 3],
                Data::F64(vec![0.1, -0.0, f64::NAN, 1e-300, 2.0, f64::INFINITY]),
            ),
        );
        arrays.insert(
            "mesh/tets".to_string(),
            Array::new(vec![1, 4], Data::U32(vec![0, 1, 2, u32::MAX])),
        );
        arrays.insert(
            "grad/entry_observable".to_string(),
            Array::new(vec![2], Data::I32(vec![-1, 7])),
        );
        let sample = Sample {
            attrs: json!({"format": FORMAT}),
            arrays,
        };
        let dir = tempfile_dir("unit-roundtrip");
        write(&dir, &sample).unwrap();
        let back = read(&dir).unwrap();
        assert_eq!(back.attrs, sample.attrs);
        let bits = |s: &Sample| -> Vec<u64> {
            s.arrays["mesh/nodes"]
                .f64s()
                .iter()
                .map(|x| x.to_bits())
                .collect()
        };
        assert_eq!(bits(&back), bits(&sample));
        assert_eq!(back.arrays["mesh/tets"], sample.arrays["mesh/tets"]);
        assert_eq!(
            back.arrays["grad/entry_observable"],
            sample.arrays["grad/entry_observable"]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `ARRAYS` is the machine-readable twin of the table in
    /// `docs/dataset-schema.md`: same rows, dtypes, dimensions and kinds.
    #[test]
    fn arrays_table_matches_the_schema_document() {
        let doc = include_str!("../../../docs/dataset-schema.md");
        let rows: Vec<Vec<String>> = doc
            .lines()
            .filter(|l| l.starts_with("| `"))
            .map(|l| {
                l.trim_matches('|')
                    .split('|')
                    .map(|c| c.trim().trim_matches('`').to_string())
                    .collect()
            })
            .collect();
        let want: Vec<Vec<String>> = ARRAYS
            .iter()
            .map(|a| {
                vec![
                    a.path.to_string(),
                    a.dtype.zarr().to_string(),
                    a.dims.join(", "),
                    match a.kinds {
                        Kinds::Both => "both",
                        Kinds::DrivenOnly => "driven",
                        Kinds::EigenOnly => "eigen",
                    }
                    .to_string(),
                ]
            })
            .collect();
        assert_eq!(rows, want);
        assert!(doc.contains(&format!("(version {DATASET_SCHEMA_VERSION})")));
    }

    fn tempfile_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("geode-dataset-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }
}
