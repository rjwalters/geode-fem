//! Physical / electromagnetic constants shared across the crate and its
//! consumers (reference tests, example binaries, `geode-util` conversions).
//!
//! Centralizes the speed-of-light and free-space-impedance literals that
//! were otherwise copy-pasted as per-file `const`s. Length-unit variants
//! of `c` are provided because the FEM meshes carry coordinates in
//! millimeters or micrometers, and the natural-unit `ω = 2π f / c` then
//! lands in the matching inverse-length unit (see
//! `geode_util::units::ghz_to_omega`, which takes the appropriate `c`).

/// Free-space (vacuum) wave impedance `η₀ = √(μ₀/ε₀)`, in ohms.
pub const ETA_0_OHM: f64 = 376.730_313_668;

/// Speed of light in vacuum, meters per second.
pub const C_M_PER_S: f64 = 2.997_924_58e8;

/// Speed of light in vacuum, millimeters per second.
pub const C_MM_PER_S: f64 = 2.997_924_58e11;

/// Speed of light in vacuum, micrometers per second.
pub const C_UM_PER_S: f64 = 2.997_924_58e14;
