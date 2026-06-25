//! Port models for the driven solver: lumped ports and waveguide ports.
//!
//! The implementation lives in two private leaf modules — a lumped-port
//! path and a waveguide-port path — re-exported here so callers see a
//! single `driven::ports` namespace:
//!
//! - [`LumpedPort`] and the `assemble_port_*` / `port_*` helpers — lumped
//!   (gap) port excitation plus current / voltage / impedance extraction.
//! - [`WavePort`] / [`PortMode`] and the waveguide mesh + mode-reduction
//!   helpers — modal waveguide ports and their parameter sweeps.

mod lumped;
mod wave;

pub use lumped::{
    LumpedPort, assemble_port_flux, assemble_port_surface_mass, port_current, port_input_impedance,
    port_voltage,
};
pub use wave::{
    ExtrudedHeightStepMesh, ExtrudedWaveguideMesh, PortMode, WavePort, WavePortSweepPoint,
    extruded_height_step_waveguide_mesh, extruded_rect_waveguide_mesh,
    map_mode_profile_to_full_mesh, solve_wave_port_sweep, solve_wave_port_sweep_with_mode,
    waveguide_mode_reduce,
};
