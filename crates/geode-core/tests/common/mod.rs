//! Shared test utilities for `geode-core` integration tests.
//!
//! This module lives under `tests/common/` so Cargo does not compile it
//! as its own test binary — only files at the top level of `tests/`
//! become test binaries. Consumers pull it in with `mod common;` at
//! the top of each test file.

pub mod box_fixtures;
pub mod wave_port;

use burn::tensor::backend::Backend;
use burn::tensor::{ElementConversion, Tensor};
use faer::c64;
use geode_core::driven::solve::CurrentSource;
use geode_core::mesh::TetMesh;

/// Read a `D`-dimensional float Burn tensor back to host as `Vec<f64>`,
/// regardless of whether the active backend's `B::FloatElem` is `f32`
/// or `f64`.
///
/// `TensorData::to_vec::<E>` requires the storage element type to match
/// exactly, so we read at `B::FloatElem` and upcast each entry to `f64`
/// for the test logic. This is the canonical readback pattern across
/// the `geode-core` test surface — the previous `to_vec::<f32>()`
/// anti-pattern panics on the f64 `ndarray` backend with
/// `TypeMismatch (expected F64, got F32)`.
#[allow(dead_code)] // some test binaries only use a subset of helpers
pub fn readback_f64<BB: Backend, const D: usize>(t: Tensor<BB, D>) -> Vec<f64> {
    t.into_data()
        .to_vec::<BB::FloatElem>()
        .expect("readback at B::FloatElem")
        .into_iter()
        .map(|x| x.elem::<f64>())
        .collect()
}

/// Faces of `mesh` (interior **and** boundary) whose three nodes all lie in
/// the plane `coord[axis] == value`.
///
/// Use this when interior faces matter (e.g. an internal port plane); use
/// [`boundary_plane_faces`] when only exterior-boundary faces are wanted.
#[allow(dead_code)] // some test binaries only use a subset of helpers
pub fn all_plane_faces(mesh: &TetMesh, axis: usize, value: f64) -> Vec<[u32; 3]> {
    mesh.faces()
        .into_iter()
        .filter(|f| {
            f.iter()
                .all(|&n| (mesh.nodes[n as usize][axis] - value).abs() < 1e-12)
        })
        .collect()
}

/// Exterior-boundary faces of `mesh` whose three nodes all lie in the plane
/// `coord[axis] == value`. Interior faces in the plane are excluded; see
/// [`all_plane_faces`] for the variant that includes them.
#[allow(dead_code)] // some test binaries only use a subset of helpers
pub fn boundary_plane_faces(mesh: &TetMesh, axis: usize, value: f64) -> Vec<[u32; 3]> {
    mesh.boundary_faces()
        .into_iter()
        .filter(|f| {
            f.iter()
                .all(|&n| (mesh.nodes[n as usize][axis] - value).abs() < 1e-12)
        })
        .collect()
}

/// Split `mesh.tet_edges()` into the per-tet `(idx, sign)` tables the
/// assemblers take.
#[allow(dead_code)] // some test binaries only use a subset of helpers
pub fn edge_tables(mesh: &TetMesh) -> (Vec<[u32; 6]>, Vec<[i8; 6]>) {
    let te = mesh.tet_edges();
    (
        te.iter().map(|r| std::array::from_fn(|i| r[i].0)).collect(),
        te.iter().map(|r| std::array::from_fn(|i| r[i].1)).collect(),
    )
}

/// PEC interior-edge mask eliminating every edge whose **both** endpoints lie
/// on the same listed plane `(axis, value)`.
#[allow(dead_code)] // some test binaries only use a subset of helpers
pub fn pec_mask_for_planes(
    mesh: &TetMesh,
    edges: &[[u32; 2]],
    planes: &[(usize, f64)],
) -> Vec<bool> {
    edges
        .iter()
        .map(|e| {
            let a = mesh.nodes[e[0] as usize];
            let b = mesh.nodes[e[1] as usize];
            !planes.iter().any(|&(axis, value)| {
                (a[axis] - value).abs() < 1e-12 && (b[axis] - value).abs() < 1e-12
            })
        })
        .collect()
}

#[allow(unused_imports)] // some test binaries only use a subset of helpers
pub use geode_util::repo::repo_root;
#[allow(unused_imports)] // some test binaries only use a subset of helpers
pub use geode_util::rng::Lcg;

/// Sparse CSC matrix–vector product `K x` (real).
#[allow(dead_code)] // some test binaries only use a subset of helpers
pub fn spmv(k: &faer::sparse::SparseColMat<usize, f64>, x: &[f64]) -> Vec<f64> {
    let kr = k.as_ref();
    let mut out = vec![0.0; kr.nrows()];
    for (j, &xj) in x.iter().enumerate().take(kr.ncols()) {
        for idx in kr.col_ptr()[j]..kr.col_ptr()[j + 1] {
            out[kr.row_idx()[idx]] += kr.val()[idx] * xj;
        }
    }
    out
}

/// Euclidean norm of a real slice.
#[allow(dead_code)] // some test binaries only use a subset of helpers
pub fn norm(v: &[f64]) -> f64 {
    v.iter().map(|x| x * x).sum::<f64>().sqrt()
}

/// Vacuum relative permittivity (`1 + 0i`) on every tet of `mesh`.
#[allow(dead_code)] // some test binaries only use a subset of helpers
pub fn vacuum_eps(mesh: &TetMesh) -> Vec<faer::c64> {
    vec![faer::c64::new(1.0, 0.0); mesh.n_tets()]
}

/// Read a TOML number (float or integer) as `f64`, panicking otherwise.
#[allow(dead_code)] // some test binaries only use a subset of helpers
pub fn toml_num(v: &toml::Value) -> f64 {
    v.as_float()
        .or_else(|| v.as_integer().map(|i| i as f64))
        .unwrap_or_else(|| panic!("expected a number, got {v:?}"))
}

/// An all-zero (no-source) current distribution on `mesh`.
#[allow(dead_code)] // some test binaries only use a subset of helpers
pub fn zero_source(mesh: &TetMesh) -> CurrentSource {
    CurrentSource {
        j_tet: vec![[c64::new(0.0, 0.0); 3]; mesh.n_tets()],
    }
}
