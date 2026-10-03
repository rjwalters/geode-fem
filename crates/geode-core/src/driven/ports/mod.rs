//! Port models for the driven solver: lumped ports and waveguide ports.
//!
//! The implementation lives in private leaf modules — a lumped-port
//! path, a waveguide-port path, and the tagged-face → wave-port
//! projection — re-exported here so callers see a single
//! `driven::ports` namespace:
//!
//! - [`LumpedPort`] and the `assemble_port_*` / `port_*` helpers — lumped
//!   (gap) port excitation plus current / voltage / impedance extraction.
//! - [`WavePort`] / [`PortMode`] / [`PortMedium`] and the waveguide mesh +
//!   mode-reduction helpers — modal waveguide ports (vacuum or
//!   homogeneously filled, issue #777) and their parameter sweeps.
//! - [`project_port_face`] / [`wave_port_from_faces`] — build a
//!   [`WavePort`] from a tagged planar port face of an arbitrary tet mesh
//!   (e.g. a Gmsh physical group), issue #683.
//! - [`solve_mixed_port_sweep_with_mode`] / [`MixedPortSweepPoint`] —
//!   lumped and wave ports in one operator with a power-wave S-matrix,
//!   issue #759.
//! - [`HybridWavePort`] / [`WavePortSpec`] and the spec sweeps
//!   [`solve_wave_port_spec_sweep_with_mode`] /
//!   [`solve_mixed_port_spec_sweep_with_mode`] — wave ports on
//!   **inhomogeneous** cross-sections, whose hybrid modes are re-solved and
//!   tracked per frequency (Epic #778 Phase 2, issue #804).

mod hybrid;
mod lumped;
pub(crate) mod mixed;
mod wave;
mod wave_face;

pub use hybrid::{
    DEFAULT_MIN_TRACK_OVERLAP, HybridChannelReport, HybridModalFlux, HybridPortFace,
    HybridPortPointReport, HybridPortReport, HybridWavePort, HybridWavePortOpts,
    MixedPortSpecSweep, PortAccuracyOpts, PortWarning, PortWarningKind, WavePortSpec,
    WavePortSpecSweep, solve_mixed_port_spec_sweep_with_mode, solve_wave_port_spec_sweep_with_mode,
};
pub use lumped::{
    LumpedPort, assemble_port_flux, assemble_port_surface_mass, port_current, port_input_impedance,
    port_voltage,
};
pub use mixed::{MixedPortSweepPoint, solve_mixed_port_sweep_with_mode};
pub use wave::{
    ExtrudedHeightStepMesh, ExtrudedWaveguideMesh, PortMedium, PortMode, WavePort,
    WavePortSweepPoint, extruded_height_step_waveguide_mesh, extruded_rect_waveguide_mesh,
    map_mode_profile_to_full_mesh, solve_wave_port_sweep, solve_wave_port_sweep_with_mode,
    waveguide_mode_reduce,
};
pub use wave_face::{
    PLANARITY_REL_TOL, PortFaceError, PortFaceProjection, project_port_face, wave_port_from_faces,
};
