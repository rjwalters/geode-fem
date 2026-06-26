//! Round-trip integration test for the VTK `.vtu` writer
//! ([`geode_core::postproc::viz::write_vtu`]).
//!
//! Builds a `cube_tet_mesh(2, 1.0)`, assigns a synthetic linear field
//! `E(r) = [x, y, z]` at each node, writes it to a tempfile, then re-parses
//! the ASCII XML with a tiny in-test reader and asserts the structural and
//! numerical contract:
//!
//! * node count and tet count match the mesh,
//! * `Points`, `connectivity`, `offsets`, `types` arrays are present,
//! * `E_real` round-trips bit-for-bit,
//! * `|E|` matches `sqrt(x² + y² + z²)` within `1e-12`,
//! * optional `E_imag` and `eps_r` arrays appear only when supplied.
//!
//! This test is the de-facto correctness contract for Phase 2A and runs in
//! CI (no ParaView dependency).

use std::path::PathBuf;

use geode_core::mesh::cube_tet_mesh;
use geode_core::postproc::viz::{write_vtu, write_vtu_surface};

/// Unique tempfile path under the OS temp dir (no `tempfile` dev-dep).
fn temp_vtu(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    p.push(format!(
        "geode_vtu_roundtrip_{tag}_{}_{nanos}.vtu",
        std::process::id()
    ));
    p
}

/// Extract the whitespace-separated tokens between the opening and closing
/// tags of the `DataArray` whose `Name="{name}"`. For the unnamed `Points`
/// array pass `name = ""` and we key off the `<Points>` block instead.
fn extract_array(xml: &str, name: &str) -> Vec<String> {
    // Find the DataArray open tag containing `Name="{name}"`.
    let needle = format!("Name=\"{name}\"");
    let tag_start = xml
        .find(&needle)
        .unwrap_or_else(|| panic!("array Name=\"{name}\" not found"));
    // From there, the array body starts after the next '>'.
    let body_start = tag_start + xml[tag_start..].find('>').unwrap() + 1;
    let body_end = body_start + xml[body_start..].find("</DataArray>").unwrap();
    xml[body_start..body_end]
        .split_whitespace()
        .map(str::to_owned)
        .collect()
}

/// The Points block has no `Name`; pull its single DataArray body directly.
fn extract_points(xml: &str) -> Vec<String> {
    let p_start = xml.find("<Points>").expect("<Points> not found");
    let da = p_start + xml[p_start..].find("<DataArray").unwrap();
    let body_start = da + xml[da..].find('>').unwrap() + 1;
    let body_end = body_start + xml[body_start..].find("</DataArray>").unwrap();
    xml[body_start..body_end]
        .split_whitespace()
        .map(str::to_owned)
        .collect()
}

fn linear_field(mesh: &geode_core::mesh::TetMesh) -> Vec<[f64; 3]> {
    mesh.nodes.iter().map(|&[x, y, z]| [x, y, z]).collect()
}

#[test]
fn real_only_roundtrip() {
    let mesh = cube_tet_mesh(2, 1.0);
    let e = linear_field(&mesh);

    let path = temp_vtu("real");
    write_vtu(&path, &mesh, &e, None, None).expect("write_vtu");
    let xml = std::fs::read_to_string(&path).expect("read back");
    let _ = std::fs::remove_file(&path);

    // Header counts.
    assert!(xml.contains(&format!("NumberOfPoints=\"{}\"", mesh.n_nodes())));
    assert!(xml.contains(&format!("NumberOfCells=\"{}\"", mesh.n_tets())));

    // Required structural arrays present.
    assert!(xml.contains("<Points>"));
    assert!(xml.contains("Name=\"connectivity\""));
    assert!(xml.contains("Name=\"offsets\""));
    assert!(xml.contains("Name=\"types\""));
    assert!(xml.contains("Name=\"E_real\""));
    assert!(xml.contains("Name=\"|E|\""));
    // Optional arrays must be absent.
    assert!(!xml.contains("Name=\"E_imag\""));
    assert!(!xml.contains("Name=\"eps_r\""));

    // Points round-trip: 3 components per node.
    let pts = extract_points(&xml);
    assert_eq!(pts.len(), mesh.n_nodes() * 3);
    for (i, node) in mesh.nodes.iter().enumerate() {
        for c in 0..3 {
            let got: f64 = pts[i * 3 + c].parse().unwrap();
            assert_eq!(got, node[c], "point {i} comp {c}");
        }
    }

    // Cells: connectivity (4/tet), offsets contiguous, all types == 10.
    let conn = extract_array(&xml, "connectivity");
    assert_eq!(conn.len(), mesh.n_tets() * 4);
    for (i, tet) in mesh.tets.iter().enumerate() {
        for c in 0..4 {
            let got: u32 = conn[i * 4 + c].parse().unwrap();
            assert_eq!(got, tet[c], "tet {i} vertex {c}");
        }
    }
    let offsets = extract_array(&xml, "offsets");
    assert_eq!(offsets.len(), mesh.n_tets());
    for (i, off) in offsets.iter().enumerate() {
        let got: usize = off.parse().unwrap();
        assert_eq!(got, 4 * (i + 1), "offset {i}");
    }
    let types = extract_array(&xml, "types");
    assert_eq!(types.len(), mesh.n_tets());
    assert!(types.iter().all(|t| t == "10"), "all cells VTK_TETRA");

    // E_real round-trips bit-for-bit.
    let ereal = extract_array(&xml, "E_real");
    assert_eq!(ereal.len(), mesh.n_nodes() * 3);
    for (i, v) in e.iter().enumerate() {
        for c in 0..3 {
            let got: f64 = ereal[i * 3 + c].parse().unwrap();
            assert_eq!(got.to_bits(), v[c].to_bits(), "E_real {i} comp {c}");
        }
    }

    // |E| == sqrt(x² + y² + z²) within 1e-12.
    let mag = extract_array(&xml, "|E|");
    assert_eq!(mag.len(), mesh.n_nodes());
    for (i, v) in e.iter().enumerate() {
        let got: f64 = mag[i].parse().unwrap();
        let want = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        assert!((got - want).abs() < 1e-12, "|E| node {i}: {got} vs {want}");
    }
}

#[test]
fn real_plus_imag_roundtrip() {
    let mesh = cube_tet_mesh(2, 1.0);
    let e = linear_field(&mesh);
    // Imag part: a distinct linear field so magnitude folds both in.
    let e_imag: Vec<[f64; 3]> = mesh
        .nodes
        .iter()
        .map(|&[x, y, z]| [0.5 * x, -y, 2.0 * z])
        .collect();

    let path = temp_vtu("imag");
    write_vtu(&path, &mesh, &e, Some(&e_imag), None).expect("write_vtu");
    let xml = std::fs::read_to_string(&path).expect("read back");
    let _ = std::fs::remove_file(&path);

    assert!(xml.contains("Name=\"E_real\""));
    assert!(xml.contains("Name=\"E_imag\""));
    assert!(xml.contains("Name=\"|E|\""));
    assert!(!xml.contains("Name=\"eps_r\""));

    // E_imag round-trips bit-for-bit.
    let eimag = extract_array(&xml, "E_imag");
    assert_eq!(eimag.len(), mesh.n_nodes() * 3);
    for (i, v) in e_imag.iter().enumerate() {
        for c in 0..3 {
            let got: f64 = eimag[i * 3 + c].parse().unwrap();
            assert_eq!(got.to_bits(), v[c].to_bits(), "E_imag {i} comp {c}");
        }
    }

    // |E| folds in the imaginary part: sqrt(|re|² + |im|²).
    let mag = extract_array(&xml, "|E|");
    assert_eq!(mag.len(), mesh.n_nodes());
    for i in 0..mesh.n_nodes() {
        let re = e[i];
        let im = e_imag[i];
        let want = (re[0] * re[0]
            + re[1] * re[1]
            + re[2] * re[2]
            + im[0] * im[0]
            + im[1] * im[1]
            + im[2] * im[2])
            .sqrt();
        let got: f64 = mag[i].parse().unwrap();
        assert!((got - want).abs() < 1e-12, "|E| node {i}: {got} vs {want}");
    }
}

#[test]
fn with_eps_r_overlay() {
    let mesh = cube_tet_mesh(2, 1.0);
    let e = linear_field(&mesh);
    // eps_r: 1.0 in lower half-space, 4.0 in upper (z >= 0.5).
    let eps_r: Vec<f64> = mesh
        .nodes
        .iter()
        .map(|&[_, _, z]| if z >= 0.5 { 4.0 } else { 1.0 })
        .collect();

    let path = temp_vtu("eps");
    write_vtu(&path, &mesh, &e, None, Some(&eps_r)).expect("write_vtu");
    let xml = std::fs::read_to_string(&path).expect("read back");
    let _ = std::fs::remove_file(&path);

    assert!(xml.contains("Name=\"E_real\""));
    assert!(xml.contains("Name=\"eps_r\""));
    assert!(!xml.contains("Name=\"E_imag\""));

    let eps = extract_array(&xml, "eps_r");
    assert_eq!(eps.len(), mesh.n_nodes());
    for (i, want) in eps_r.iter().enumerate() {
        let got: f64 = eps[i].parse().unwrap();
        assert_eq!(got.to_bits(), want.to_bits(), "eps_r node {i}");
    }
}

/// Build a small known UV-sphere (lat/long grid) surface: `n_theta` polar
/// rings × `n_phi` azimuth columns, unit radius. Returns
/// `(points, tris, theta_scalar)` where `theta_scalar` is the polar angle
/// at each vertex (a deterministic per-vertex scalar to round-trip).
fn unit_sphere(n_theta: usize, n_phi: usize) -> (Vec<[f64; 3]>, Vec<[usize; 3]>, Vec<f64>) {
    use std::f64::consts::PI;
    let mut points = Vec::new();
    let mut theta_scalar = Vec::new();
    for it in 0..n_theta {
        let th = PI * it as f64 / (n_theta - 1) as f64;
        let (st, ct) = th.sin_cos();
        for jp in 0..n_phi {
            let ph = 2.0 * PI * jp as f64 / n_phi as f64;
            let (sp, cp) = ph.sin_cos();
            points.push([st * cp, st * sp, ct]);
            theta_scalar.push(th);
        }
    }
    let mut tris = Vec::new();
    let vid = |it: usize, jp: usize| it * n_phi + (jp % n_phi);
    for it in 0..n_theta - 1 {
        for jp in 0..n_phi {
            let a = vid(it, jp);
            let b = vid(it, jp + 1);
            let c = vid(it + 1, jp + 1);
            let d = vid(it + 1, jp);
            tris.push([a, b, d]);
            tris.push([b, c, d]);
        }
    }
    (points, tris, theta_scalar)
}

#[test]
fn surface_writer_roundtrip() {
    let (points, tris, theta_scalar) = unit_sphere(7, 8);
    // A second scalar to confirm multiple PointData arrays round-trip.
    let radius_scalar: Vec<f64> = points
        .iter()
        .map(|[x, y, z]| (x * x + y * y + z * z).sqrt())
        .collect();

    let path = temp_vtu("surface");
    write_vtu_surface(
        &path,
        &points,
        &tris,
        &[("theta", &theta_scalar), ("radius", &radius_scalar)],
    )
    .expect("write_vtu_surface");
    let xml = std::fs::read_to_string(&path).expect("read back");
    let _ = std::fs::remove_file(&path);

    // Header counts.
    assert!(xml.contains(&format!("NumberOfPoints=\"{}\"", points.len())));
    assert!(xml.contains(&format!("NumberOfCells=\"{}\"", tris.len())));

    // Structural arrays present; named scalars present.
    assert!(xml.contains("<Points>"));
    assert!(xml.contains("Name=\"connectivity\""));
    assert!(xml.contains("Name=\"offsets\""));
    assert!(xml.contains("Name=\"types\""));
    assert!(xml.contains("Name=\"theta\""));
    assert!(xml.contains("Name=\"radius\""));

    // Points round-trip bit-for-bit, 3 components per vertex.
    let pts = extract_points(&xml);
    assert_eq!(pts.len(), points.len() * 3);
    for (i, node) in points.iter().enumerate() {
        for c in 0..3 {
            let got: f64 = pts[i * 3 + c].parse().unwrap();
            assert_eq!(got.to_bits(), node[c].to_bits(), "point {i} comp {c}");
        }
    }

    // Connectivity: 3 indices per triangle, all in range.
    let conn = extract_array(&xml, "connectivity");
    assert_eq!(conn.len(), tris.len() * 3);
    for (i, t) in tris.iter().enumerate() {
        for c in 0..3 {
            let got: usize = conn[i * 3 + c].parse().unwrap();
            assert_eq!(got, t[c], "tri {i} vertex {c}");
            assert!(got < points.len());
        }
    }

    // Offsets contiguous (3 per cell); all cell types == 5 (VTK_TRIANGLE).
    let offsets = extract_array(&xml, "offsets");
    assert_eq!(offsets.len(), tris.len());
    for (i, off) in offsets.iter().enumerate() {
        let got: usize = off.parse().unwrap();
        assert_eq!(got, 3 * (i + 1), "offset {i}");
    }
    let types = extract_array(&xml, "types");
    assert_eq!(types.len(), tris.len());
    assert!(types.iter().all(|t| t == "5"), "all cells VTK_TRIANGLE");

    // Scalars round-trip bit-for-bit.
    let theta = extract_array(&xml, "theta");
    assert_eq!(theta.len(), points.len());
    for (i, want) in theta_scalar.iter().enumerate() {
        let got: f64 = theta[i].parse().unwrap();
        assert_eq!(got.to_bits(), want.to_bits(), "theta vertex {i}");
    }
    let radius = extract_array(&xml, "radius");
    assert_eq!(radius.len(), points.len());
    for (i, want) in radius_scalar.iter().enumerate() {
        let got: f64 = radius[i].parse().unwrap();
        assert_eq!(got.to_bits(), want.to_bits(), "radius vertex {i}");
    }
}
