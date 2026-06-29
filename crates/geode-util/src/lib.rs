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

#[cfg(test)]
mod tests {
    //! Crate-root public-surface smoke test.
    //!
    //! `lib.rs` is purely `pub mod` declarations, so the crate root's
    //! observable "behavior" is the re-export surface itself. This test
    //! reaches one public item per declared module *through its
    //! crate-root path* and exercises the pure ones. An accidental
    //! `pub mod` → `mod` downgrade — or removal/rename of one of these
    //! re-exported helpers — fails to compile here rather than silently
    //! shrinking the crate's public API.
    //!
    //! Deep per-module behavior is covered in each module's own `tests`
    //! submodule; this guard only asserts reachability + a trivial sanity
    //! result so the crate root is no longer at zero coverage.

    #[test]
    fn declared_modules_are_publicly_reachable() {
        // repo: an absolute workspace root resolved from the crate root path.
        assert!(crate::repo::repo_root().is_absolute());

        // convert: empty input round-trips to an empty Vec.
        assert!(crate::convert::complex_slice_to_vec(&[]).is_empty());

        // interop: empty interleaved payload decodes to no complex values.
        assert!(crate::interop::decode_real_imag_interleave(&[]).is_empty());

        // fixture (deep tests owned by Phase 4.2): the type and a free
        // helper must remain reachable from the crate root.
        let _fixture_ty: Option<crate::fixture::Fixture> = None;
        assert_eq!(crate::fixture::sweep_freqs(1.0, 2.0, 3).len(), 3);

        // viz: bind the public entry point as a fn item to prove the path
        // resolves (its numerics are exercised in viz::tests).
        let _ = crate::viz::edge_field_to_nodes;
    }
}
