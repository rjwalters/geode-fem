//! MSH 4.1 ASCII writer for [`TaggedTetMesh`] (issue #1267, Epic #835
//! Phase 6).
//!
//! The exact inverse of [`read_tagged_tet_mesh`](super::read_tagged_tet_mesh):
//! `read_tagged_tet_mesh(write_tagged_tet_mesh(m)?) == m` in nodes, tets,
//! boundary triangles, every physical tag and the physical-group names.
//! The adaptive loop uses it to export the refined mesh so it can be
//! re-solved without `adaptive_mesh`.
//!
//! Layout: one node block (contiguous 1-based tags, coordinates printed in
//! Rust's shortest round-trip form), then one element block per **run** of
//! consecutive tets (or triangles) sharing a physical tag, so the element
//! order — and with it the tet and triangle indices — is preserved.

use std::fmt::Write as _;

use super::{MeshError, TaggedTetMesh};

/// Write `mesh` as MSH 4.1 ASCII.
///
/// # Errors
///
/// [`MeshError::Parse`] if the tag arrays are not parallel to the element
/// arrays, or a tet has physical tag `0` (the reader requires every tet to
/// belong to a physical volume).
pub fn write_tagged_tet_mesh(mesh: &TaggedTetMesh) -> Result<String, MeshError> {
    let tm = &mesh.mesh;
    if mesh.tet_physical_tags.len() != tm.tets.len() {
        return Err(MeshError::Parse(format!(
            "tet_physical_tags has {} entries for {} tets",
            mesh.tet_physical_tags.len(),
            tm.tets.len()
        )));
    }
    if mesh.triangle_physical_tags.len() != mesh.boundary_triangles.len() {
        return Err(MeshError::Parse(format!(
            "triangle_physical_tags has {} entries for {} triangles",
            mesh.triangle_physical_tags.len(),
            mesh.boundary_triangles.len()
        )));
    }
    if let Some(i) = mesh.tet_physical_tags.iter().position(|&t| t == 0) {
        return Err(MeshError::Parse(format!(
            "tet {i} has no physical tag; the MSH reader requires one per tet"
        )));
    }

    let vol_runs = runs(&mesh.tet_physical_tags);
    let tri_runs = runs(&mesh.triangle_physical_tags);
    let bbox = |pts: &mut dyn Iterator<Item = u32>| -> [f64; 6] {
        let mut b = [
            f64::INFINITY,
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ];
        for n in pts {
            let p = tm.nodes[n as usize];
            for k in 0..3 {
                b[k] = b[k].min(p[k]);
                b[3 + k] = b[3 + k].max(p[k]);
            }
        }
        if b[0].is_infinite() {
            b = [0.0; 6];
        }
        b
    };

    let mut s = String::new();
    s.push_str("$MeshFormat\n4.1 0 8\n$EndMeshFormat\n");

    if !tm.physical_groups.is_empty() {
        let _ = writeln!(s, "$PhysicalNames\n{}", tm.physical_groups.len());
        for ((dim, tag), name) in &tm.physical_groups {
            let _ = writeln!(s, "{dim} {tag} \"{name}\"");
        }
        s.push_str("$EndPhysicalNames\n");
    }

    // Entities: no points or curves; one surface per triangle run, one
    // volume per tet run.
    let _ = writeln!(s, "$Entities\n0 0 {} {}", tri_runs.len(), vol_runs.len());
    let entity_row = |s: &mut String, tag: usize, b: [f64; 6], phys: i32| {
        let _ = write!(
            s,
            "{tag} {} {} {} {} {} {} ",
            b[0], b[1], b[2], b[3], b[4], b[5]
        );
        if phys != 0 {
            let _ = writeln!(s, "1 {phys} 0");
        } else {
            let _ = writeln!(s, "0 0");
        }
    };
    for (i, r) in tri_runs.iter().enumerate() {
        let b = bbox(
            &mut mesh.boundary_triangles[r.start..r.end]
                .iter()
                .flatten()
                .copied(),
        );
        entity_row(&mut s, i + 1, b, r.tag);
    }
    for (i, r) in vol_runs.iter().enumerate() {
        let b = bbox(&mut tm.tets[r.start..r.end].iter().flatten().copied());
        entity_row(&mut s, i + 1, b, r.tag);
    }
    s.push_str("$EndEntities\n");

    // Nodes: a single block with contiguous tags 1..=n.
    let n = tm.nodes.len();
    if n == 0 {
        s.push_str("$Nodes\n0 0 0 0\n$EndNodes\n");
    } else {
        let _ = writeln!(s, "$Nodes\n1 {n} 1 {n}\n3 1 0 {n}");
        for i in 1..=n {
            let _ = writeln!(s, "{i}");
        }
        for p in &tm.nodes {
            let _ = writeln!(s, "{} {} {}", p[0], p[1], p[2]);
        }
        s.push_str("$EndNodes\n");
    }

    // Elements.
    let n_el = tm.tets.len() + mesh.boundary_triangles.len();
    let n_blocks = vol_runs.len() + tri_runs.len();
    let _ = writeln!(s, "$Elements\n{n_blocks} {n_el} 1 {n_el}");
    let mut next = 1usize;
    for (i, r) in vol_runs.iter().enumerate() {
        let _ = writeln!(s, "3 {} 4 {}", i + 1, r.end - r.start);
        for t in &tm.tets[r.start..r.end] {
            let _ = writeln!(
                s,
                "{next} {} {} {} {}",
                t[0] + 1,
                t[1] + 1,
                t[2] + 1,
                t[3] + 1
            );
            next += 1;
        }
    }
    for (i, r) in tri_runs.iter().enumerate() {
        let _ = writeln!(s, "2 {} 2 {}", i + 1, r.end - r.start);
        for t in &mesh.boundary_triangles[r.start..r.end] {
            let _ = writeln!(s, "{next} {} {} {}", t[0] + 1, t[1] + 1, t[2] + 1);
            next += 1;
        }
    }
    s.push_str("$EndElements\n");
    Ok(s)
}

struct Run {
    start: usize,
    end: usize,
    tag: i32,
}

/// Maximal runs of equal consecutive tags.
fn runs(tags: &[i32]) -> Vec<Run> {
    let mut out: Vec<Run> = Vec::new();
    for (i, &t) in tags.iter().enumerate() {
        match out.last_mut() {
            Some(r) if r.tag == t => r.end = i + 1,
            _ => out.push(Run {
                start: i,
                end: i + 1,
                tag: t,
            }),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::{TetMesh, read_tagged_tet_mesh};

    fn sample() -> TaggedTetMesh {
        let mut physical_groups = std::collections::BTreeMap::new();
        physical_groups.insert((3, 1), "air".to_string());
        physical_groups.insert((3, 2), "sub strate".to_string());
        physical_groups.insert((2, 5), "port".to_string());
        TaggedTetMesh {
            mesh: TetMesh {
                nodes: vec![
                    [0.0, 0.0, 0.0],
                    [1.0 / 3.0, 0.0, 0.0],
                    [0.0, 0.1, 0.0],
                    [0.0, 0.0, 1e-9],
                    [1.0, 1.0, -1.0],
                ],
                tets: vec![[0, 1, 2, 3], [1, 2, 3, 4], [0, 1, 3, 4]],
                physical_groups,
            },
            tet_physical_tags: vec![1, 2, 1],
            boundary_triangles: vec![[0, 1, 2], [1, 2, 4], [0, 3, 4]],
            triangle_physical_tags: vec![5, 5, 0],
        }
    }

    #[test]
    fn round_trips_exactly() {
        let m = sample();
        let text = write_tagged_tet_mesh(&m).unwrap();
        assert_eq!(read_tagged_tet_mesh(text.as_bytes()).unwrap(), m);
    }

    #[test]
    fn rejects_untagged_tet_and_length_mismatch() {
        let mut m = sample();
        m.tet_physical_tags[1] = 0;
        assert!(write_tagged_tet_mesh(&m).is_err());
        let mut m = sample();
        m.triangle_physical_tags.pop();
        assert!(write_tagged_tet_mesh(&m).is_err());
    }
}
