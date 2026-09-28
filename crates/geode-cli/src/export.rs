//! Opt-in field / far-field export under `--outdir` (issue #684, Epic
//! #680 Phase 4).
//!
//! Nothing in this module runs unless `--outdir <DIR>` was given: the
//! solve paths hold an `Option<OutDir>` and skip every export (and every
//! filesystem touch) on `None`.
//!
//! # Directory and overwrite semantics
//!
//! [`OutDir::create`] creates `<DIR>` (and missing parents) up front,
//! before any solve, so an unwritable path fails fast with an `io` error
//! instead of after an expensive sweep. Files are written under fixed,
//! index-derived names ([`field_file_name`], [`mode_file_name`],
//! [`pattern_file_name`]); an existing file of the same name is
//! **overwritten** (the repo's `--export-field` / `OutputDir` precedent),
//! and nothing else in `<DIR>` is touched or removed.
//!
//! # What is written
//!
//! * **Fields** — one ASCII `.vtu` (`UnstructuredGrid`, readable by
//!   ParaView) per report row / eigenmode via
//!   [`geode_core::postproc::viz::write_vtu`]: `E_real`, `E_imag`
//!   (driven only), `|E|` and a per-node `eps_r` as `PointData`. The
//!   per-node `E` is [`geode_util::viz::edge_field_to_nodes`]'s
//!   **Whitney-average** reconstruction — each incident tet's Whitney
//!   interpolant evaluated at the vertex, averaged onto the shared node.
//!   It is a visualization aid for ParaView, not a quadrature-accurate
//!   field sample (normal `E` is discontinuous across material
//!   interfaces, and the average smears it).
//! * **Far field** — the principal-plane pattern cuts of the NTFF
//!   ([`geode_core::postproc::ntff::principal_plane_cuts`]) as a small
//!   JSON file.
//!
//! The report references each file as a [`FileRef`]: its path
//! **relative to `<DIR>`** plus the hex SHA-256 of the bytes actually
//! written (read back after the write).

use std::path::{Path, PathBuf};

use faer::c64;
use geode_core::mesh::TetMesh;
use geode_core::postproc::ntff::PatternCut;
use geode_core::postproc::viz::write_vtu;
use geode_util::viz::edge_field_to_nodes;
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::error::CliError;
use crate::report::FileRef;

/// An existing (created on demand) `--outdir` export directory.
#[derive(Debug, Clone)]
pub struct OutDir {
    root: PathBuf,
}

impl OutDir {
    /// Create `root` (and missing parents) if needed.
    pub fn create(root: &Path) -> Result<Self, CliError> {
        std::fs::create_dir_all(root).map_err(|err| CliError::Io {
            path: root.to_path_buf(),
            err,
        })?;
        Ok(Self {
            root: root.to_path_buf(),
        })
    }

    /// `Some(OutDir)` for `Some(path)`, `None` (no filesystem access)
    /// otherwise.
    pub fn create_opt(root: Option<&Path>) -> Result<Option<Self>, CliError> {
        root.map(Self::create).transpose()
    }

    /// Write a complex edge-DOF field (`mesh.edges()` order) as a `.vtu`
    /// named `name`. `imag = false` drops the imaginary part (real
    /// eigenvectors).
    pub fn write_field(
        &self,
        name: &str,
        mesh: &TetMesh,
        e_edges: &[c64],
        imag: bool,
        eps_nodes: &[f64],
    ) -> Result<FileRef, CliError> {
        let (e_re, e_im) = edge_field_to_nodes(mesh, e_edges);
        let path = self.root.join(name);
        write_vtu(
            &path,
            mesh,
            &e_re,
            imag.then_some(e_im.as_slice()),
            Some(eps_nodes),
        )
        .map_err(|err| CliError::Io {
            path: path.clone(),
            err,
        })?;
        self.file_ref(name)
    }

    /// Serialize `value` as pretty JSON to `name`.
    pub fn write_json<T: Serialize>(&self, name: &str, value: &T) -> Result<FileRef, CliError> {
        let mut json = serde_json::to_string_pretty(value)?;
        json.push('\n');
        let path = self.root.join(name);
        std::fs::write(&path, json).map_err(|err| CliError::Io { path, err })?;
        self.file_ref(name)
    }

    /// `{path, sha256}` of a file just written under the root (hash of
    /// the bytes on disk).
    fn file_ref(&self, name: &str) -> Result<FileRef, CliError> {
        let path = self.root.join(name);
        let bytes = std::fs::read(&path).map_err(|err| CliError::Io { path, err })?;
        Ok(FileRef {
            path: name.to_string(),
            sha256: format!("{:x}", Sha256::digest(&bytes)),
        })
    }
}

/// Field file of driven / extract report row `index`.
pub fn field_file_name(index: usize) -> String {
    format!("E_{index:04}.vtu")
}

/// Field file of eigenmode `index`.
pub fn mode_file_name(index: usize) -> String {
    format!("E_mode_{index:04}.vtu")
}

/// Far-field pattern file of driven / extract report row `index`.
pub fn pattern_file_name(index: usize) -> String {
    format!("pattern_{index:04}.json")
}

/// Per-node real relative permittivity for the `.vtu` `eps_r` array: the
/// mean of `Re ε_r` over the tets incident to each node (1 for an
/// orphan node).
pub fn eps_per_node(mesh: &TetMesh, eps: &[c64]) -> Vec<f64> {
    let mut sum = vec![0.0_f64; mesh.n_nodes()];
    let mut cnt = vec![0_u32; mesh.n_nodes()];
    for (tet, e) in mesh.tets.iter().zip(eps) {
        for &v in tet {
            sum[v as usize] += e.re;
            cnt[v as usize] += 1;
        }
    }
    sum.iter()
        .zip(&cnt)
        .map(|(&s, &c)| if c > 0 { s / f64::from(c) } else { 1.0 })
        .collect()
}

/// The pattern file body: principal-plane cuts, `|E|` normalized to each
/// cut's own maximum.
#[derive(Debug, Serialize)]
pub struct PatternFile {
    /// Frequency (Hz).
    pub frequency_hz: f64,
    /// Polar angles `θ ∈ [0, π]` (radians), shared by both cuts.
    pub theta_rad: Vec<f64>,
    /// E-plane cut (`φ = 0`, x-z plane): normalized `|E(θ)|`.
    pub e_plane_e_norm: Vec<f64>,
    /// H-plane cut (`φ = π/2`, y-z plane): normalized `|E(θ)|`.
    pub h_plane_e_norm: Vec<f64>,
}

impl PatternFile {
    /// From the two [`principal_plane_cuts`](geode_core::postproc::ntff::principal_plane_cuts).
    pub fn new(frequency_hz: f64, e_plane: PatternCut, h_plane: PatternCut) -> Self {
        Self {
            frequency_hz,
            theta_rad: e_plane.theta,
            e_plane_e_norm: e_plane.e_norm,
            h_plane_e_norm: h_plane.e_norm,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_are_zero_padded_and_distinct() {
        assert_eq!(field_file_name(3), "E_0003.vtu");
        assert_eq!(mode_file_name(12), "E_mode_0012.vtu");
        assert_eq!(pattern_file_name(0), "pattern_0000.json");
    }

    /// A scratch directory removed (recursively) on drop — even when an
    /// assertion panics first (mirrors `tests/patch_extract_golden.rs`).
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "geode-cli-export-unit-{name}-{}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn json_file_ref_hashes_the_written_bytes() {
        let tmp = TempDir::new("json-ref");
        let dir = &tmp.0;
        let out = OutDir::create(&dir.join("nested")).unwrap();
        let r = out.write_json("x.json", &[1, 2]).unwrap();
        assert_eq!(r.path, "x.json");
        let bytes = std::fs::read(dir.join("nested/x.json")).unwrap();
        assert_eq!(r.sha256, format!("{:x}", Sha256::digest(&bytes)));
        assert_eq!(r.sha256.len(), 64);
        // Overwrite semantics: same name, new content, new hash.
        let r2 = out.write_json("x.json", &[3]).unwrap();
        assert_ne!(r.sha256, r2.sha256);
    }
}
