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
//!   tracked per frequency (Epic #778 Phase 2, issue #804); with interior
//!   PEC conductors (strips, sheets) through the volume mask, degenerate
//!   clusters tracked as subspaces, and per-channel line impedances (Phase
//!   3b, issue #817).
//! - [`strip_line_section`] — 3-D shielded microstrip / stripline sections
//!   extruded from a 2-D strip face, with their hybrid ports (#817).
//! - [`HybridPortFace::from_volume_lossy`] and the dispersive sweeps
//!   [`solve_wave_port_spec_sweep_dispersive_with_mode`] /
//!   [`solve_mixed_port_spec_sweep_dispersive_with_mode`] — hybrid ports on
//!   **lossy and dispersive** substrates (complex-symmetric port pencil,
//!   per-ω `ε`), Epic #778 Phase 4, issue #806.
//! - [`solve_hybrid_port_face_sweep`] — the port-face half of a hybrid sweep
//!   (modes, tracking, line impedances, warnings) without the 3-D solve, for
//!   front ends that preview a port before solving (Phase 5, issue #807).
//! - [`ImpedanceAccuracy`] — the per-channel line-impedance accuracy
//!   estimate (observed-rate Richardson on the `h/2` face; #807 review).
//! - [`PortFaceProjection::wave_port_on_space`] and the `*_on_space` sweeps
//!   ([`solve_wave_port_sweep_on_space`], [`solve_mixed_port_sweep_on_space`],
//!   [`solve_wave_port_spec_sweep_on_space`],
//!   [`solve_mixed_port_spec_sweep_on_space`]) — geometric wave ports on an
//!   order-pluggable H(curl) space, including p=2 (Epic #836 Phase 3a, issue
//!   #884); the order-aware TE-only TM guard [`tm_guard_margin_at_order`],
//!   with its P2 face estimate
//!   [`PortFaceProjection::tm_cutoff_estimate_at_order`].

mod hybrid;
mod hybrid_lossy;
mod hybrid_z;
mod lumped;
pub(crate) mod mixed;
mod mode_gauge;
mod strip_line;
mod wave;
mod wave_face;
pub(crate) mod wave_p2;

pub(crate) use hybrid::{ChanAt, ChanOrigin, FaceModeSet, hybrid_port_channel_sweep};
pub use hybrid::{
    DEFAULT_MIN_TRACK_OVERLAP, DispersiveEps, FaceConductor, HybridChannelReport,
    HybridComplexLineReport, HybridFaceSweep, HybridLineReport, HybridModalFlux, HybridPortFace,
    HybridPortPointReport, HybridPortReport, HybridWavePort, HybridWavePortOpts,
    MixedPortSpecSweep, PortAccuracyOpts, PortWarning, PortWarningKind, WavePortSpec,
    WavePortSpecSweep, solve_hybrid_port_face_sweep,
    solve_mixed_port_spec_sweep_dispersive_with_mode, solve_mixed_port_spec_sweep_with_mode,
    solve_wave_port_spec_sweep_dispersive_with_mode, solve_wave_port_spec_sweep_with_mode,
};
pub(crate) use hybrid_z::line_complex;
pub use hybrid_z::{
    DEFAULT_IMPEDANCE_ACCURACY_THRESHOLD, ImpedanceAccuracy, ImpedanceEstimate, LineImpedance,
    MAX_IMPEDANCE_RATE, MIN_IMPEDANCE_RATE, SINGULAR_IMPEDANCE_RATE,
};
pub use lumped::{
    LumpedPort, assemble_port_flux, assemble_port_surface_mass, port_current, port_input_impedance,
    port_voltage,
};
pub use mixed::{MixedPortSweepPoint, solve_mixed_port_sweep_with_mode};
pub use mode_gauge::{
    DEGENERATE_CANDIDATE_REL_TOL, DEGENERATE_CONVERGENCE_RATIO, DEGENERATE_EXACT_REL_TOL,
    GAUGE_FLOOR, GAUGE_LEAD_RATIO, N_REFERENCE_FIELDS, REF_MAX_INDEX, reference_field,
    relative_gap,
};
pub use strip_line::{StripLineSection, strip_line_section};
pub(crate) use wave::assemble_modal_flux;
pub use wave::{
    ExtrudedHeightStepMesh, ExtrudedWaveguideMesh, PortMedium, PortMode, WavePort,
    WavePortSweepPoint, extruded_height_step_waveguide_mesh, extruded_rect_waveguide_mesh,
    map_mode_profile_to_full_mesh, solve_wave_port_sweep, solve_wave_port_sweep_with_mode,
    waveguide_mode_reduce,
};
pub use wave_face::{
    GuideAxialMesh, GuideScan, PLANARITY_REL_TOL, PortFaceError, PortFaceProjection,
    TM_GUARD_AXIAL_COEFF, TM_GUARD_AXIAL_COEFF_P2, TM_GUARD_MARGIN, TM_GUARD_MARGIN_P2_COARSE_FACE,
    TM_GUARD_MEASURED_KH, TM_GUARD_MEASURED_KH_P2, TM_GUARD_MIN_ELEMENTS_ACROSS_P2,
    TM_GUARD_REACH_CUTOFF_WAVELENGTHS, TmCutoffEstimate, project_port_face, tm_evanescent_leak,
    tm_guard_axial_reach, tm_guard_margin, tm_guard_margin_at_order, wave_port_from_faces,
};
pub use wave_p2::{
    PortFaceModeP2, solve_mixed_port_spec_sweep_on_space, solve_mixed_port_sweep_on_space,
    solve_wave_port_spec_sweep_on_space, solve_wave_port_sweep_on_space,
    wave_port_from_faces_on_space, waveguide_mode_reduce_on_space,
};
