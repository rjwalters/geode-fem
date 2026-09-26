//! Compile-time backend selection (Phase 1 semantics).
//!
//! The Burn backend is a generic type parameter on every geode-core
//! solve, so it is chosen when the binary is **built**, via this crate's
//! `wgpu` / `cuda` / `metal` Cargo features (all off by default → the
//! `ndarray` f64 CPU backend, zero GPU dependencies). The runtime
//! `--backend` flag only *confirms* the compiled-in backend and fails
//! with [`CliError::BackendMismatch`] otherwise; it never switches.

use clap::ValueEnum;

use crate::error::CliError;

std::cfg_select! {
    feature = "cuda" => {
        /// The Burn backend this binary was compiled with.
        pub type CompiledBackend = burn::backend::Cuda;
        /// Name of [`CompiledBackend`] as reported in `backend`.
        pub const COMPILED_BACKEND: &str = "cuda";
    }
    feature = "metal" => {
        /// The Burn backend this binary was compiled with.
        pub type CompiledBackend = burn::backend::Metal<f64>;
        /// Name of [`CompiledBackend`] as reported in `backend`.
        pub const COMPILED_BACKEND: &str = "metal";
    }
    feature = "wgpu" => {
        /// The Burn backend this binary was compiled with.
        pub type CompiledBackend = burn::backend::Wgpu<f64>;
        /// Name of [`CompiledBackend`] as reported in `backend`.
        pub const COMPILED_BACKEND: &str = "wgpu";
    }
    _ => {
        /// The Burn backend this binary was compiled with.
        pub type CompiledBackend = burn::backend::NdArray<f64, i32>;
        /// Name of [`CompiledBackend`] as reported in `backend`.
        pub const COMPILED_BACKEND: &str = "ndarray";
    }
}

/// Values accepted by `--backend`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum BackendChoice {
    /// ndarray f64 CPU backend (the default build).
    Ndarray,
    /// wgpu (requires a `--features wgpu` build).
    Wgpu,
    /// CUDA (requires a `--features cuda` build).
    Cuda,
    /// Apple Metal (requires a `--features metal` build).
    Metal,
}

impl BackendChoice {
    fn name(self) -> &'static str {
        match self {
            BackendChoice::Ndarray => "ndarray",
            BackendChoice::Wgpu => "wgpu",
            BackendChoice::Cuda => "cuda",
            BackendChoice::Metal => "metal",
        }
    }
}

/// Confirm a requested backend against the compiled-in one.
pub fn confirm(requested: Option<BackendChoice>) -> Result<(), CliError> {
    match requested {
        Some(b) if b.name() != COMPILED_BACKEND => Err(CliError::BackendMismatch {
            requested: b.name().to_string(),
            compiled: COMPILED_BACKEND,
        }),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confirm_accepts_compiled_and_rejects_others() {
        assert!(confirm(None).is_ok());
        let all = [
            BackendChoice::Ndarray,
            BackendChoice::Wgpu,
            BackendChoice::Cuda,
            BackendChoice::Metal,
        ];
        let ok: Vec<_> = all.iter().filter(|b| confirm(Some(**b)).is_ok()).collect();
        assert_eq!(ok.len(), 1);
        assert_eq!(ok[0].name(), COMPILED_BACKEND);
    }
}
