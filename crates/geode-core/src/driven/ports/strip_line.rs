//! **3-D shielded strip-line sections** for hybrid wave ports (Epic #778
//! Phase 3b, issue #817).
//!
//! [`strip_line_section`] extrudes a 2-D strip-line port face
//! ([`crate::analytic::microstrip::ShieldedStripFace`], or any
//! [`StripFaceMesh`]) along `z` with [`extrude_tri_mesh`] and carries its
//! fill and its PEC set into the volume:
//!
//! - every tet takes the permittivity of its source triangle;
//! - the 3-D PEC mask is the `z`-invariant extrusion of the face's PEC edges
//!   ([`ExtrudedTriMesh::pec_interior_mask`]): the shield walls, every
//!   zero-thickness strip (a PEC **sheet** inside the volume) and the walls of
//!   carved-out thick strips (a tunnel through the volume) are eliminated,
//!   and the two end faces stay free except for their PEC edges: they are the
//!   ports.
//!
//! [`StripLineSection::hybrid_ports`] then builds the two hybrid wave ports
//! with [`HybridPortFace::from_volume_with_pec`], so each port face sees the
//! strip through the volume mask, exactly as a face read from a mesh file
//! would.

use faer::c64;

use super::hybrid::{HybridPortFace, HybridWavePort, HybridWavePortOpts, WavePortSpec};
use super::wave_face::PortFaceError;
use crate::analytic::microstrip::StripFaceMesh;
use crate::mesh::{ExtrudedTriMesh, extrude_tri_mesh};

/// A straight shielded strip-line section ([`strip_line_section`]).
#[derive(Debug, Clone)]
pub struct StripLineSection {
    /// The extruded volume mesh and its 2-D ↔ 3-D bookkeeping.
    pub extruded: ExtrudedTriMesh,
    /// Real relative permittivity per tet.
    pub eps_tet: Vec<f64>,
    /// [`crate::driven::solve::DrivenBcs::pec_interior_mask`] of the section.
    pub pec_interior_mask: Vec<bool>,
}

/// Extrude `face` through `nz` slabs of total length `length` (module docs).
///
/// # Panics
///
/// As [`extrude_tri_mesh`].
pub fn strip_line_section(face: &StripFaceMesh, nz: usize, length: f64) -> StripLineSection {
    let extruded = extrude_tri_mesh(&face.mesh, nz, length);
    let eps_tet = extruded.per_tet(&face.eps_r);
    let pec_interior_mask = extruded.pec_interior_mask(&face.masks.pec_edges);
    StripLineSection {
        extruded,
        eps_tet,
        pec_interior_mask,
    }
}

impl StripLineSection {
    /// Section length.
    pub fn length(&self) -> f64 {
        self.extruded.length()
    }

    /// The two port faces (`z = 0`, `z = L`) with the volume PEC mask.
    ///
    /// # Errors
    ///
    /// Any [`HybridPortFace::from_volume_with_pec`] error.
    pub fn port_faces(&self) -> Result<[HybridPortFace; 2], PortFaceError> {
        let mesh = &self.extruded.mesh;
        let edges = mesh.edges();
        let mk = |faces: &[[u32; 3]]| {
            HybridPortFace::from_volume(mesh, faces, &self.eps_tet)?
                .with_interior_pec(&edges, &self.pec_interior_mask)
        };
        Ok([
            mk(&self.extruded.port1_faces)?,
            mk(&self.extruded.port2_faces)?,
        ])
    }

    /// Two hybrid wave ports reporting `k` channels each (unit incident
    /// amplitudes) with `opts`.
    ///
    /// # Errors
    ///
    /// As [`Self::port_faces`].
    pub fn hybrid_ports(
        &self,
        k: usize,
        opts: HybridWavePortOpts,
    ) -> Result<[WavePortSpec; 2], PortFaceError> {
        let [f1, f2] = self.port_faces()?;
        let one = vec![c64::new(1.0, 0.0); k];
        Ok([
            WavePortSpec::from(HybridWavePort::new(f1, one.clone()).with_opts(opts)),
            WavePortSpec::from(HybridWavePort::new(f2, one).with_opts(opts)),
        ])
    }
}
