//! The port-face solves of one `geode driven` run, made once and shared
//! (issue #952).
//!
//! A geometric wave port's modes are geometric (`ε`-independent), so one
//! face mode solve per port serves the whole run. Before #952 every consumer
//! solved the face again: the degeneracy notes (#923), `--touchstone`
//! classification, the N-port sensitivity's port specs and the sweep itself
//! (geometric-only or alongside hybrid ports), so a `--touchstone` run with
//! sensitivities solved each geometric face four times. [`PortSolves`]
//! solves each port on first use
//! ([`PortFaceProjection::wave_port_with_candidates`], which also yields
//! the degeneracy records of the same solve) and hands every later caller
//! the same result.
//!
//! The face-only sweep of a **hybrid** port (no 3-D solve, accuracy estimate
//! off) is shared the same way between `--touchstone` classification and
//! the `z0` line-impedance precheck of the sensitivity (#953). The 3-D
//! hybrid sweep still solves its faces itself, per frequency.
//!
//! Lazy, so a run that fails before a consumer asks never solves, and the
//! first consumer pays where the old one did. A failed solve is not kept:
//! each consumer that asks again solves again and gets its own error, as it
//! did before #952 (only a failing run pays that).
//!
//! [`PortFaceProjection::wave_port_with_candidates`]: geode_core::driven::ports::PortFaceProjection::wave_port_with_candidates

use std::sync::OnceLock;

use geode_core::driven::ports::{DegenerateCandidate, HybridFaceSweep, PortFaceError, WavePort};

use crate::error::CliError;
use crate::problem::Problem;

/// The shared port-face solves of one run over `p` (module docs).
pub struct PortSolves<'p> {
    p: &'p Problem,
    /// Per wave port: the geometric port (vacuum medium) and the candidate
    /// records of its solve, once solved.
    geometric: Vec<OnceLock<(WavePort, Vec<DegenerateCandidate>)>>,
    /// Per wave port: the hybrid face sweep without the accuracy estimate,
    /// once swept.
    hybrid: Vec<OnceLock<HybridFaceSweep>>,
}

impl<'p> PortSolves<'p> {
    /// No port solved yet.
    pub fn new(p: &'p Problem) -> Self {
        Self {
            p,
            geometric: p.wave_ports.iter().map(|_| OnceLock::new()).collect(),
            hybrid: p.wave_ports.iter().map(|_| OnceLock::new()).collect(),
        }
    }

    /// Geometric wave port `k`'s port and records, solving it on first use.
    fn solved(&self, k: usize) -> Result<&(WavePort, Vec<DegenerateCandidate>), PortFaceError> {
        if let Some(s) = self.geometric[k].get() {
            return Ok(s);
        }
        let w = &self.p.wave_ports[k];
        let s = w
            .projection
            .wave_port_with_candidates(&self.p.edges, &w.a_inc)?;
        Ok(self.geometric[k].get_or_init(|| s))
    }

    /// Geometric wave port `k` as
    /// [`PortFaceProjection::wave_port`](geode_core::driven::ports::PortFaceProjection::wave_port)
    /// builds it (vacuum medium: the caller sets the port's fill).
    ///
    /// # Errors
    ///
    /// [`CliError::WavePort`] naming the port if its face solve fails.
    pub fn wave_port(&self, k: usize) -> Result<WavePort, CliError> {
        self.solved(k)
            .map(|s| s.0.clone())
            .map_err(|err| CliError::WavePort {
                name: self.p.wave_ports[k].surface.name.clone(),
                err,
            })
    }

    /// The candidate degenerate pairs of geometric wave port `k`'s face
    /// solve, as
    /// [`PortFaceProjection::degenerate_candidates`](geode_core::driven::ports::PortFaceProjection::degenerate_candidates)`(n_modes, P1)`
    /// returns them.
    ///
    /// # Errors
    ///
    /// The port's solve error; the records may still be available from
    /// `degenerate_candidates`, which does not gauge the modes.
    pub fn candidates(&self, k: usize) -> Result<&[DegenerateCandidate], PortFaceError> {
        self.solved(k).map(|s| s.1.as_slice())
    }

    /// Hybrid wave port `k`'s face sweep without the accuracy estimate
    /// ([`crate::hybrid::face_sweep`]`(p, k, false)`), swept on first use.
    ///
    /// # Errors
    ///
    /// As [`crate::hybrid::face_sweep`].
    ///
    /// # Panics
    ///
    /// If wave port `k` is not hybrid.
    pub fn hybrid_face_sweep(&self, k: usize) -> Result<&HybridFaceSweep, CliError> {
        if let Some(s) = self.hybrid[k].get() {
            return Ok(s);
        }
        let s = crate::hybrid::face_sweep(self.p, k, false)?;
        Ok(self.hybrid[k].get_or_init(|| s))
    }
}
