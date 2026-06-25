//! VTK `UnstructuredGrid` (`.vtu`) writer for tetrahedral meshes plus
//! per-node electromagnetic field data.
//!
//! This module serialises a [`TetMesh`] together with a complex `E`-field
//! sampled at the mesh nodes into the ASCII XML `UnstructuredGrid` (`.vtu`)
//! format understood by ParaView 5.x and the wider EM/VTK community. It is
//! the Phase 2A foundation of Epic #276: Phase 2B wires an `--export-field`
//! flag onto the benchmark examples and Phase 2C renders the result
//! headlessly with `pvbatch`.
//!
//! # Chosen XML schema
//!
//! We emit the "inline ASCII" flavour of the VTK XML format (no
//! appended/base64 binary blob). The document layout is:
//!
//! ```xml
//! <?xml version="1.0"?>
//! <VTKFile type="UnstructuredGrid" version="1.0" byte_order="LittleEndian">
//!   <UnstructuredGrid>
//!     <Piece NumberOfPoints="N" NumberOfCells="M">
//!       <Points>
//!         <DataArray type="Float64" NumberOfComponents="3" format="ascii"> ... </DataArray>
//!       </Points>
//!       <Cells>
//!         <DataArray type="Int64" Name="connectivity" format="ascii"> ... </DataArray>
//!         <DataArray type="Int64" Name="offsets"      format="ascii"> ... </DataArray>
//!         <DataArray type="UInt8" Name="types"        format="ascii"> ... </DataArray>
//!       </Cells>
//!       <PointData>
//!         <DataArray type="Float64" Name="E_real" NumberOfComponents="3" format="ascii"> ... </DataArray>
//!         <DataArray type="Float64" Name="|E|"    NumberOfComponents="1" format="ascii"> ... </DataArray>
//!         <!-- optional --> <DataArray Name="E_imag" .../>
//!         <!-- optional --> <DataArray Name="eps_r" .../>
//!       </PointData>
//!     </Piece>
//!   </UnstructuredGrid>
//! </VTKFile>
//! ```
//!
//! Cells use VTK cell type `10` (`VTK_TETRA`): each tet contributes four
//! connectivity entries (0-based node indices, matching `mesh.tets`
//! directly — VTK is 0-based) and a contiguous `offsets` entry equal to
//! `4 * (cell_index + 1)`.
//!
//! Point coordinates are written in `mesh.nodes` order so that point id `i`
//! is `mesh.nodes[i]`; the field arrays are indexed identically.
//!
//! The string is hand-rolled with [`std::fmt::Write`] and flushed in one
//! [`std::fs::write`] — no `vtkio` / serialisation dependency is pulled in,
//! matching the other text-format writers in this crate.
//!
//! # Reference
//!
//! Kitware VTK XML file format specification:
//! <https://docs.vtk.org/en/latest/design_documents/VTKFileFormats.html#xml-file-formats>

use std::fmt::Write as _;
use std::path::Path;

use crate::mesh::TetMesh;

/// Serialise `mesh` plus the node-sampled field data to an ASCII XML
/// `UnstructuredGrid` (`.vtu`) file at `path`.
///
/// * `e_field` — real part of the per-node `E` field, length
///   `mesh.n_nodes()`. Written as the `E_real` Vec3 `PointData` array.
/// * `e_imag` — optional imaginary part (same length); when present it is
///   written as the `E_imag` Vec3 array and folded into the `|E|` magnitude
///   (`sqrt(re² + im²)` per component, summed). When absent, `|E|` is just
///   the magnitude of `e_field`.
/// * `eps_r` — optional per-node relative permittivity (length
///   `mesh.n_nodes()`); written as the scalar `eps_r` array when present.
///
/// The writer does not decide where `e_field` comes from — evaluating the
/// Nédélec edge DOFs at nodes is the caller's responsibility (Phase 2B).
///
/// # Panics
///
/// Panics if any provided slice length does not equal `mesh.n_nodes()`.
/// This is a programmer error (mismatched field/mesh), not an I/O failure.
pub fn write_vtu(
    path: &Path,
    mesh: &TetMesh,
    e_field: &[[f64; 3]],
    e_imag: Option<&[[f64; 3]]>,
    eps_r: Option<&[f64]>,
) -> std::io::Result<()> {
    let n_nodes = mesh.n_nodes();
    let n_tets = mesh.n_tets();

    assert_eq!(
        e_field.len(),
        n_nodes,
        "e_field length ({}) must equal mesh.n_nodes() ({})",
        e_field.len(),
        n_nodes
    );
    if let Some(im) = e_imag {
        assert_eq!(
            im.len(),
            n_nodes,
            "e_imag length ({}) must equal mesh.n_nodes() ({})",
            im.len(),
            n_nodes
        );
    }
    if let Some(eps) = eps_r {
        assert_eq!(
            eps.len(),
            n_nodes,
            "eps_r length ({}) must equal mesh.n_nodes() ({})",
            eps.len(),
            n_nodes
        );
    }

    // Pre-size generously: header + per-node and per-tet lines. Growth is
    // cheap; this just avoids a handful of early reallocations.
    let mut s = String::with_capacity(512 + n_nodes * 96 + n_tets * 32);

    s.push_str("<?xml version=\"1.0\"?>\n");
    s.push_str("<VTKFile type=\"UnstructuredGrid\" version=\"1.0\" byte_order=\"LittleEndian\">\n");
    s.push_str("  <UnstructuredGrid>\n");
    let _ = writeln!(
        s,
        "    <Piece NumberOfPoints=\"{n_nodes}\" NumberOfCells=\"{n_tets}\">"
    );

    // --- Points -----------------------------------------------------------
    s.push_str("      <Points>\n");
    s.push_str("        <DataArray type=\"Float64\" NumberOfComponents=\"3\" format=\"ascii\">\n");
    for [x, y, z] in &mesh.nodes {
        let _ = writeln!(
            s,
            "          {} {} {}",
            fmt_f64(*x),
            fmt_f64(*y),
            fmt_f64(*z)
        );
    }
    s.push_str("        </DataArray>\n");
    s.push_str("      </Points>\n");

    // --- Cells ------------------------------------------------------------
    s.push_str("      <Cells>\n");
    s.push_str("        <DataArray type=\"Int64\" Name=\"connectivity\" format=\"ascii\">\n");
    for [a, b, c, d] in &mesh.tets {
        let _ = writeln!(s, "          {a} {b} {c} {d}");
    }
    s.push_str("        </DataArray>\n");

    s.push_str("        <DataArray type=\"Int64\" Name=\"offsets\" format=\"ascii\">\n");
    for cell in 0..n_tets {
        let _ = writeln!(s, "          {}", 4 * (cell + 1));
    }
    s.push_str("        </DataArray>\n");

    s.push_str("        <DataArray type=\"UInt8\" Name=\"types\" format=\"ascii\">\n");
    for _ in 0..n_tets {
        // 10 == VTK_TETRA
        s.push_str("          10\n");
    }
    s.push_str("        </DataArray>\n");
    s.push_str("      </Cells>\n");

    // --- PointData --------------------------------------------------------
    s.push_str("      <PointData>\n");

    // E_real (Vec3)
    s.push_str(
        "        <DataArray type=\"Float64\" Name=\"E_real\" NumberOfComponents=\"3\" format=\"ascii\">\n",
    );
    for [x, y, z] in e_field {
        let _ = writeln!(
            s,
            "          {} {} {}",
            fmt_f64(*x),
            fmt_f64(*y),
            fmt_f64(*z)
        );
    }
    s.push_str("        </DataArray>\n");

    // |E| (scalar): sqrt(re² + im²) folded over components when imag present.
    s.push_str(
        "        <DataArray type=\"Float64\" Name=\"|E|\" NumberOfComponents=\"1\" format=\"ascii\">\n",
    );
    for (node, re) in e_field.iter().enumerate() {
        let mut sumsq = re[0] * re[0] + re[1] * re[1] + re[2] * re[2];
        if let Some(im) = e_imag {
            let imv = im[node];
            sumsq += imv[0] * imv[0] + imv[1] * imv[1] + imv[2] * imv[2];
        }
        let _ = writeln!(s, "          {}", fmt_f64(sumsq.sqrt()));
    }
    s.push_str("        </DataArray>\n");

    // E_imag (Vec3) — optional.
    if let Some(im) = e_imag {
        s.push_str(
            "        <DataArray type=\"Float64\" Name=\"E_imag\" NumberOfComponents=\"3\" format=\"ascii\">\n",
        );
        for [x, y, z] in im {
            let _ = writeln!(
                s,
                "          {} {} {}",
                fmt_f64(*x),
                fmt_f64(*y),
                fmt_f64(*z)
            );
        }
        s.push_str("        </DataArray>\n");
    }

    // eps_r (scalar) — optional.
    if let Some(eps) = eps_r {
        s.push_str(
            "        <DataArray type=\"Float64\" Name=\"eps_r\" NumberOfComponents=\"1\" format=\"ascii\">\n",
        );
        for v in eps {
            let _ = writeln!(s, "          {}", fmt_f64(*v));
        }
        s.push_str("        </DataArray>\n");
    }

    s.push_str("      </PointData>\n");

    // --- Close ------------------------------------------------------------
    s.push_str("    </Piece>\n");
    s.push_str("  </UnstructuredGrid>\n");
    s.push_str("</VTKFile>\n");

    std::fs::write(path, s)
}

/// Serialise a triangulated surface (points + `VTK_TRIANGLE` cells) plus
/// one or more named scalar `PointData` arrays to an ASCII XML
/// `UnstructuredGrid` (`.vtu`) file at `path`.
///
/// This is the sibling of [`write_vtu`] for *surface* geometry: instead of
/// `VTK_TETRA` (type 10) volume cells it emits `VTK_TRIANGLE` (type 5)
/// cells, each contributing three 0-based connectivity entries and a
/// contiguous `offsets` entry equal to `3 * (cell_index + 1)`. The XML
/// scaffolding (header, `Points`, `Cells`, `PointData`, `{:?}` f64
/// formatting for bit-exact round-tripping) is identical to [`write_vtu`]
/// so all `.vtu` writing stays in this one module — no `vtkio`, no new
/// dependency.
///
/// It is the Phase 3A foundation of Epic #276: the patch-antenna
/// `pattern-3d` directive uses it to write a 3D radiation-lobe surface
/// whose vertex radius encodes normalised directivity, with `D` / `D_dB`
/// carried as `PointData` so ParaView can colour the lobe.
///
/// * `points` — surface vertex coordinates, one `[x, y, z]` per point.
/// * `tris` — triangle connectivity as 0-based indices into `points`.
/// * `named_scalar_data` — `(name, values)` scalar `PointData` arrays; each
///   `values` slice must have length `points.len()`. Written in order.
///
/// # Panics
///
/// Panics if any triangle index is out of range for `points`, or if any
/// scalar array length does not equal `points.len()`. These are programmer
/// errors (malformed surface / mismatched scalar), not I/O failures.
pub fn write_vtu_surface(
    path: &Path,
    points: &[[f64; 3]],
    tris: &[[usize; 3]],
    named_scalar_data: &[(&str, &[f64])],
) -> std::io::Result<()> {
    let n_points = points.len();
    let n_tris = tris.len();

    for (ti, t) in tris.iter().enumerate() {
        for &idx in t {
            assert!(
                idx < n_points,
                "triangle {ti} references point {idx} but only {n_points} points exist"
            );
        }
    }
    for (name, values) in named_scalar_data {
        assert_eq!(
            values.len(),
            n_points,
            "scalar array {name:?} length ({}) must equal points.len() ({})",
            values.len(),
            n_points
        );
    }

    let mut s = String::with_capacity(512 + n_points * 96 + n_tris * 24);

    s.push_str("<?xml version=\"1.0\"?>\n");
    s.push_str("<VTKFile type=\"UnstructuredGrid\" version=\"1.0\" byte_order=\"LittleEndian\">\n");
    s.push_str("  <UnstructuredGrid>\n");
    let _ = writeln!(
        s,
        "    <Piece NumberOfPoints=\"{n_points}\" NumberOfCells=\"{n_tris}\">"
    );

    // --- Points -----------------------------------------------------------
    s.push_str("      <Points>\n");
    s.push_str("        <DataArray type=\"Float64\" NumberOfComponents=\"3\" format=\"ascii\">\n");
    for [x, y, z] in points {
        let _ = writeln!(
            s,
            "          {} {} {}",
            fmt_f64(*x),
            fmt_f64(*y),
            fmt_f64(*z)
        );
    }
    s.push_str("        </DataArray>\n");
    s.push_str("      </Points>\n");

    // --- Cells ------------------------------------------------------------
    s.push_str("      <Cells>\n");
    s.push_str("        <DataArray type=\"Int64\" Name=\"connectivity\" format=\"ascii\">\n");
    for [a, b, c] in tris {
        let _ = writeln!(s, "          {a} {b} {c}");
    }
    s.push_str("        </DataArray>\n");

    s.push_str("        <DataArray type=\"Int64\" Name=\"offsets\" format=\"ascii\">\n");
    for cell in 0..n_tris {
        let _ = writeln!(s, "          {}", 3 * (cell + 1));
    }
    s.push_str("        </DataArray>\n");

    s.push_str("        <DataArray type=\"UInt8\" Name=\"types\" format=\"ascii\">\n");
    for _ in 0..n_tris {
        // 5 == VTK_TRIANGLE
        s.push_str("          5\n");
    }
    s.push_str("        </DataArray>\n");
    s.push_str("      </Cells>\n");

    // --- PointData --------------------------------------------------------
    s.push_str("      <PointData>\n");
    for (name, values) in named_scalar_data {
        let _ = writeln!(
            s,
            "        <DataArray type=\"Float64\" Name=\"{name}\" NumberOfComponents=\"1\" format=\"ascii\">"
        );
        for v in *values {
            let _ = writeln!(s, "          {}", fmt_f64(*v));
        }
        s.push_str("        </DataArray>\n");
    }
    s.push_str("      </PointData>\n");

    // --- Close ------------------------------------------------------------
    s.push_str("    </Piece>\n");
    s.push_str("  </UnstructuredGrid>\n");
    s.push_str("</VTKFile>\n");

    std::fs::write(path, s)
}

/// Format an `f64` for the ASCII data arrays with enough precision to
/// round-trip the IEEE-754 value bit-for-bit (`{:?}` on `f64` emits the
/// shortest decimal that parses back to the exact same bits).
fn fmt_f64(v: f64) -> String {
    format!("{v:?}")
}
