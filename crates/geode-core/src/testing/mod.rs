//! Testing utilities for geode-core.
//!
//! Backend and device selection are top-level dependency-injection
//! concerns, not library defaults. This module provides the *test-only*
//! seam for that: [`TestBackend`] (the backend tests run against, chosen
//! by the same feature flags as the rest of the build) and
//! [`device_tolerances`] (a generic selector that picks a per-test
//! tolerance set from the active device's backend name and dtype).
//!
//! The tolerance *cases* deliberately live in the individual validation /
//! regression tests — each suite has its own tolerance shape and its own
//! documented numbers. Core owns only the selection mechanic.

use burn::Tensor;
use burn::prelude::Backend;
use burn::tensor::DType;
use regex::Regex;

std::cfg_select! {
    feature = "cuda" => {
        pub type TestBackend = burn::backend::Cuda;
    }
    feature = "metal" => {
        pub type TestBackend = burn::backend::Metal<f64>;
    }
    feature = "wgpu" => {
        pub type TestBackend = burn::backend::Wgpu<f64>;
    }
    _ => {
        pub type TestBackend = burn::backend::NdArray<f64, i32>;
    }
}

/// Error returned by [`device_tolerances`] when a tolerance set cannot be
/// resolved for the active device.
#[derive(Debug, thiserror::Error)]
pub enum TolerancesError {
    /// A case pattern was not a valid regular expression.
    #[error("invalid backend-name pattern {pattern:?}: {source}")]
    BadPattern {
        pattern: String,
        #[source]
        source: regex::Error,
    },
    /// No case matched the active device's backend name and dtype.
    #[error(
        "no tolerance case matched backend name {name:?} with dtype {dtype:?} \
         (checked {checked} case(s))"
    )]
    NoMatch {
        name: String,
        dtype: DType,
        checked: usize,
    },
}

/// Select a tolerance value for the active device from a table of cases.
///
/// Each case is `(pattern, dtype, value)`. The first case is returned
/// whose `dtype` equals the device's float dtype **and** whose `pattern`
/// is found anywhere in `B::name(device)` (regex find, not anchored). An
/// empty `pattern` therefore matches any backend name, which is convenient
/// as a dtype-only catch-all (e.g. a generic f32 GPU fallback).
///
/// The tolerance type `T` is whatever a given test needs — a plain `f64`,
/// or a per-suite struct of named tolerances. Cases are owned by the
/// calling test; core only performs the selection.
///
/// # Errors
///
/// Returns [`TolerancesError::BadPattern`] if a pattern fails to compile,
/// or [`TolerancesError::NoMatch`] if no case matches the active device.
///
/// # Example
///
/// ```rust,no_run
/// use burn::tensor::DType;
/// use geode_core::testing::{device_tolerances, TestBackend};
///
/// #[derive(Clone, Copy)]
/// struct Tol { rel: f64, abs: f64 }
///
/// let device = Default::default();
/// let tol = device_tolerances::<TestBackend, Tol>(
///     &device,
///     &[
///         // f64 CPU path: near-bit-exact assembly.
///         ("ndarray", DType::F64, Tol { rel: 1e-8, abs: 5e-9 }),
///         // any other f32 backend: looser GPU envelope.
///         ("", DType::F32, Tol { rel: 5e-5, abs: 5e-5 }),
///     ],
/// ).expect("a tolerance case must match the active backend");
/// let _ = tol.rel;
/// ```
pub fn device_tolerances<B: Backend, T: Clone>(
    device: &B::Device,
    cases: &[(&str, DType, T)],
) -> Result<T, TolerancesError> {
    let name = B::name(device);
    let dtype = Tensor::<B, 1>::zeros([0], device).dtype();

    for (pattern, case_dtype, value) in cases {
        if *case_dtype != dtype {
            continue;
        }
        let re = Regex::new(pattern).map_err(|source| TolerancesError::BadPattern {
            pattern: (*pattern).to_string(),
            source,
        })?;
        if re.is_match(&name) {
            return Ok(value.clone());
        }
    }

    Err(TolerancesError::NoMatch {
        name,
        dtype,
        checked: cases.len(),
    })
}
