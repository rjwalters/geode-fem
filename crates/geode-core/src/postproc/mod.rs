//! Post-processing of solved fields into derived outputs.
//!
//! This directory-backed module groups the crate's post-processing stages
//! — the transforms that turn a solved near-field solution into reportable
//! quantities and on-disk visualisations:
//!
//! - [`electromigration`] — Black, Blech and 1-D Korhonen EM lifetime
//!   models as pure consumers of `j` and `T`.
//! - [`mode_character`] — field-character identification of
//!   dielectric-sphere eigenmodes (energy split, multipole projection,
//!   multiplet clustering).
//! - [`ntff`] — near-to-far-field (NTFF) transform via Love's surface
//!   equivalence: angular far field, directivity, and gain.
//! - [`viz`] — VTK `UnstructuredGrid` (`.vtu`) writers for tetrahedral
//!   meshes plus per-node electromagnetic field data.

pub mod electromigration;
pub mod mode_character;
pub mod ntff;
pub mod viz;
