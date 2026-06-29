//! GEODE-FEM pre-core staging layer.
//!
//! `geode-util` is a *staging layer that sits above [`geode_core`]*: a home
//! for shared math / format-conversion / interop / fixture / visualization
//! helpers that several consumers (`geode-validation`, the standalone example
//! crates, future tools) would otherwise re-implement, but which are not (yet)
//! core FEM kernels and so do not belong in `geode-core`.
//!
//! # Layering rule
//!
//! - `geode-util` depends on [`geode_core`] + [`burn`] + [`faer`].
//! - [`geode_core`] stays **completely unaware** of `geode-util` — there is no
//!   dependency edge back into core. Core remains the bottom of the stack; this
//!   crate is glue layered on top. Keeping the arrow one-directional is what
//!   lets `geode-core` stay a focused kernel crate while shared conveniences
//!   accrete here.
//!
//! # Epic #414
//!
//! This crate is introduced by Epic #414 ("Introduce `geode-util` crate"). It
//! starts life (Phase 1) as an empty grouped-module skeleton; the Phase 2/3
//! migrations move existing helpers out of `geode-validation` / the example
//! crates into the modules declared below as pure code-moves. Each module's
//! doc header records what lands there.

pub mod convert;
pub mod fixture;
pub mod interop;
pub mod repo;
pub mod viz;
