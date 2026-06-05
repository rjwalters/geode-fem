//! Fixture loading + schema.
//!
//! A *fixture* is a single canonical (inputs, golden outputs) bundle
//! for one spine slice. The on-disk schema is documented in
//! `reference/README.md` and `reference/SCHEMA.md`; the corresponding
//! Rust types here are loose `Map<String, Field>` so the scaffolding
//! stays useful as the schema evolves (new fields can be added without
//! requiring a Rust-side type-update churn).
//!
//! ## Format support
//!
//! Phase A wires only the JSON format. The [`FixtureFormat`] enum is a
//! deliberate extension point — when the cube-cavity fixture (#92)
//! lands and starts carrying complex eigenvectors, a `FixtureFormat::Hdf5`
//! variant will be added behind a feature gate so contributors aren't
//! forced to install `libhdf5` to run the smoke tests.

use std::collections::BTreeMap;
use std::path::Path;

use num_complex::Complex64;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors raised by fixture I/O and validation.
#[derive(Debug, Error)]
pub enum FixtureError {
    /// Filesystem read failure.
    #[error("failed to read fixture file {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },

    /// JSON parse / structural validation failure.
    #[error("failed to parse fixture as JSON: {0}")]
    Json(#[from] serde_json::Error),

    /// On-disk schema version is not recognized by this build.
    #[error("unsupported fixture schema_version: {0} (this build supports: {1:?})")]
    UnsupportedSchemaVersion(String, &'static [&'static str]),

    /// The requested field was not declared in the fixture.
    #[error("fixture has no output field named '{0}'")]
    MissingField(String),

    /// HDF5 format requested but not compiled in.
    #[error("HDF5 fixture format is not enabled in this build")]
    HdfNotEnabled,

    /// A `c128` field had an odd-length interleaved payload (real-imag
    /// interleave requires `len == 2 * prod(shape)`).
    #[error(
        "fixture field '{name}' is dtype c128 but flattened payload has length {got}; \
         expected {expected} (= 2 × prod(shape) for real-imag interleave)"
    )]
    InvalidComplexLength {
        name: String,
        got: usize,
        expected: usize,
    },

    /// A field was requested under one dtype but declared as another.
    #[error("fixture field '{name}' has dtype '{declared}', not '{requested}'")]
    DtypeMismatch {
        name: String,
        declared: String,
        requested: &'static str,
    },
}

/// On-disk fixture format selector.
///
/// JSON is sufficient for small fixtures (Phase A smoke case);
/// HDF5 becomes the format-of-record once eigenvector-class outputs
/// land (#92 and later). The HDF5 variant is reserved here so callers
/// can pin format choice without waiting for the implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FixtureFormat {
    /// Human-readable JSON. Small fixtures, easy review in PRs.
    Json,
    /// Binary HDF5 (not yet wired up — placeholder for the
    /// eigenvector-class fixture work that will arrive with #92).
    Hdf5,
}

/// Versions of the on-disk schema this build accepts.
pub const SUPPORTED_SCHEMA_VERSIONS: &[&str] = &["1"];

/// A single fixture: inputs, golden outputs, and provenance.
///
/// Field naming follows `reference/SCHEMA.md`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fixture {
    /// Schema version (currently `"1"`).
    pub schema_version: String,

    /// Stable id of the fixture (e.g. `"p1_reference_tet/local_stiffness"`).
    pub fixture_id: String,

    /// Free-form one-liner describing what the fixture pins down.
    pub description: String,

    /// Units convention for the numeric values in this fixture.
    pub units: String,

    /// Input fields keyed by name (e.g. `"coords"`, `"mesh"`).
    #[serde(default)]
    pub inputs: BTreeMap<String, Field>,

    /// Golden output fields keyed by name (e.g. `"k_local"`,
    /// `"eigenvalues"`). Each carries its own tolerance.
    pub outputs: BTreeMap<String, OutputField>,

    /// Provenance metadata (where the golden values came from).
    pub provenance: Provenance,
}

/// An input field — shape + dtype + data — no tolerance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Field {
    /// Shape (row-major).
    pub shape: Vec<usize>,
    /// Element dtype (currently only `"f64"` and `"i64"` are exercised;
    /// stored as a string so the schema can grow without a Rust-side
    /// enum bump).
    pub dtype: String,
    /// Free-form per-field description.
    #[serde(default)]
    pub description: String,
    /// Flattened or nested numeric values.
    ///
    /// We accept both a flat `[a, b, c, ...]` array and a nested array
    /// matching `shape`, and normalize to row-major-flat at load time.
    pub data: serde_json::Value,
}

/// An output field — same as [`Field`] but with a tolerance attached.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputField {
    /// Shape (row-major).
    pub shape: Vec<usize>,
    /// Element dtype.
    pub dtype: String,
    /// Free-form per-field description.
    #[serde(default)]
    pub description: String,
    /// Absolute tolerance used when comparing actual against golden.
    /// Per-field is intentional — eigenvector residuals, eigenvalues,
    /// and matrix entries do not share a sensible tolerance.
    pub tolerance_abs: f64,
    /// Nested or flat numeric values. Normalized at load time.
    pub data: serde_json::Value,
}

/// Where the golden values came from.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provenance {
    /// Human description of the source (e.g. "hand-computed exact rationals",
    /// "NumPy reference impl at reference/numpy/p1_local_matrices.py").
    pub source: String,
    /// Optional cross-reference to a Rust-side check.
    #[serde(default)]
    pub verified_against: String,
    /// Issue/PR number tying this fixture to a tracker.
    #[serde(default)]
    pub issue: String,
}

impl Fixture {
    /// Load a fixture from disk in the requested format.
    pub fn load_from(path: &Path, format: FixtureFormat) -> Result<Self, FixtureError> {
        match format {
            FixtureFormat::Json => {
                let bytes = std::fs::read(path).map_err(|e| FixtureError::Io {
                    path: path.display().to_string(),
                    source: e,
                })?;
                let fixture: Fixture = serde_json::from_slice(&bytes)?;
                fixture.check_schema_version()?;
                Ok(fixture)
            }
            FixtureFormat::Hdf5 => Err(FixtureError::HdfNotEnabled),
        }
    }

    /// Auto-detect format from file extension.
    pub fn load(path: &Path) -> Result<Self, FixtureError> {
        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let format = match ext.as_str() {
            "json" => FixtureFormat::Json,
            "h5" | "hdf5" => FixtureFormat::Hdf5,
            _ => FixtureFormat::Json, // default for now
        };
        Self::load_from(path, format)
    }

    /// Verify schema version is supported by this build.
    fn check_schema_version(&self) -> Result<(), FixtureError> {
        if SUPPORTED_SCHEMA_VERSIONS.contains(&self.schema_version.as_str()) {
            Ok(())
        } else {
            Err(FixtureError::UnsupportedSchemaVersion(
                self.schema_version.clone(),
                SUPPORTED_SCHEMA_VERSIONS,
            ))
        }
    }

    /// Get a golden output field by name, returning the values as a
    /// flat row-major `Vec<f64>` together with the declared shape and
    /// tolerance.
    ///
    /// The returned [`GoldenF64`] borrows `name` and `shape` from
    /// `self` (not from the caller-supplied `name` argument), so the
    /// lifetime is tied to the fixture.
    pub fn output_f64<'a>(&'a self, name: &str) -> Result<GoldenF64<'a>, FixtureError> {
        let (key, f) = self
            .outputs
            .get_key_value(name)
            .ok_or_else(|| FixtureError::MissingField(name.to_string()))?;
        let data = flatten_to_f64(&f.data);
        Ok(GoldenF64 {
            name: key.as_str(),
            shape: &f.shape,
            tolerance_abs: f.tolerance_abs,
            data,
        })
    }

    /// Iterate over all output fields in deterministic (sorted) order.
    pub fn iter_outputs(&self) -> impl Iterator<Item = (&str, &OutputField)> {
        self.outputs.iter().map(|(k, v)| (k.as_str(), v))
    }

    /// Get a golden output field by name, returning the values as a
    /// flat row-major `Vec<Complex64>`.
    ///
    /// The on-disk encoding is the real-imag interleaved layout
    /// documented in `reference/SCHEMA.md` ("Complex encoding
    /// (`c128`)"). The declared `shape` is taken from the fixture
    /// without modification — the interleave is invisible to callers
    /// (length `prod(shape)`, not `2 * prod(shape)`).
    ///
    /// # Errors
    ///
    /// - [`FixtureError::MissingField`] if the field is not declared.
    /// - [`FixtureError::DtypeMismatch`] if the field is declared with
    ///   a non-`c128` dtype.
    /// - [`FixtureError::InvalidComplexLength`] if the flattened
    ///   payload length is not `2 * prod(shape)`.
    pub fn output_c128<'a>(&'a self, name: &str) -> Result<GoldenC128<'a>, FixtureError> {
        let (key, f) = self
            .outputs
            .get_key_value(name)
            .ok_or_else(|| FixtureError::MissingField(name.to_string()))?;
        if f.dtype != "c128" {
            return Err(FixtureError::DtypeMismatch {
                name: name.to_string(),
                declared: f.dtype.clone(),
                requested: "c128",
            });
        }
        let flat = flatten_to_f64(&f.data);
        let expected_re_im = 2 * f.shape.iter().product::<usize>();
        if flat.len() != expected_re_im {
            return Err(FixtureError::InvalidComplexLength {
                name: name.to_string(),
                got: flat.len(),
                expected: expected_re_im,
            });
        }
        let data = flat
            .chunks_exact(2)
            .map(|c| Complex64::new(c[0], c[1]))
            .collect();
        Ok(GoldenC128 {
            name: key.as_str(),
            shape: &f.shape,
            tolerance_abs: f.tolerance_abs,
            data,
        })
    }

    /// Like [`output_c128`](Self::output_c128) but reads from `inputs`
    /// instead of `outputs`. Used by reference-impl drivers that need
    /// to consume a complex input (e.g. per-tet `epsilon_r_complex`)
    /// from the fixture.
    pub fn input_c128(&self, name: &str) -> Result<Vec<Complex64>, FixtureError> {
        let f = self
            .inputs
            .get(name)
            .ok_or_else(|| FixtureError::MissingField(name.to_string()))?;
        if f.dtype != "c128" {
            return Err(FixtureError::DtypeMismatch {
                name: name.to_string(),
                declared: f.dtype.clone(),
                requested: "c128",
            });
        }
        let flat = flatten_to_f64(&f.data);
        let expected_re_im = 2 * f.shape.iter().product::<usize>();
        if flat.len() != expected_re_im {
            return Err(FixtureError::InvalidComplexLength {
                name: name.to_string(),
                got: flat.len(),
                expected: expected_re_im,
            });
        }
        Ok(flat
            .chunks_exact(2)
            .map(|c| Complex64::new(c[0], c[1]))
            .collect())
    }
}

/// A golden output field flattened into f64 row-major.
///
/// The `name` and `shape` borrow from the parent [`Fixture`] so callers
/// can iterate cheaply without cloning per-field metadata.
#[derive(Debug, Clone)]
pub struct GoldenF64<'fix> {
    pub name: &'fix str,
    pub shape: &'fix [usize],
    pub tolerance_abs: f64,
    pub data: Vec<f64>,
}

impl GoldenF64<'_> {
    /// Total element count implied by `shape`.
    pub fn numel(&self) -> usize {
        self.shape.iter().product()
    }
}

/// A golden output field flattened into `Complex64` row-major.
///
/// On-disk this corresponds to a `c128`-dtype field with real-imag
/// interleaved storage (length `2 * prod(shape)` on disk, length
/// `prod(shape)` in memory). See `reference/SCHEMA.md` for the
/// encoding convention.
#[derive(Debug, Clone)]
pub struct GoldenC128<'fix> {
    pub name: &'fix str,
    pub shape: &'fix [usize],
    /// Absolute tolerance applied to the **complex modulus** of the
    /// per-element residual (`|actual − golden|`). See
    /// `reference/SCHEMA.md` → "Complex encoding (c128)".
    pub tolerance_abs: f64,
    pub data: Vec<Complex64>,
}

impl GoldenC128<'_> {
    /// Total element count implied by `shape`.
    pub fn numel(&self) -> usize {
        self.shape.iter().product()
    }
}

/// Recursively flatten a nested `serde_json::Value` of numbers into a
/// row-major `Vec<f64>`. Accepts both nested arrays matching `shape`
/// and an already-flat array.
pub(crate) fn flatten_to_f64(v: &serde_json::Value) -> Vec<f64> {
    let mut out = Vec::new();
    push_numbers(v, &mut out);
    out
}

fn push_numbers(v: &serde_json::Value, out: &mut Vec<f64>) {
    match v {
        serde_json::Value::Number(n) => {
            if let Some(x) = n.as_f64() {
                out.push(x);
            } else if let Some(x) = n.as_i64() {
                out.push(x as f64);
            } else if let Some(x) = n.as_u64() {
                out.push(x as f64);
            }
        }
        serde_json::Value::Array(arr) => {
            for item in arr {
                push_numbers(item, out);
            }
        }
        _ => {
            // Non-numeric values are silently skipped — the schema
            // validator would have caught this at load time if we had
            // one (deliberately deferred to keep Phase A small).
        }
    }
}

impl Fixture {
    /// Compare a set of named actual outputs against the golden values
    /// in this fixture. Returns a [`ComparisonReport`] describing each
    /// field's pass/fail status. Missing fields, shape mismatches, and
    /// tolerance violations all surface as distinct failure modes.
    ///
    /// Only `f64`-dtype output fields are checked by this entry point;
    /// `c128` fields go through [`compare_complex_against`](Self::compare_complex_against).
    pub fn compare_against(&self, actual: &BTreeMap<String, Vec<f64>>) -> crate::ComparisonReport {
        crate::diff::compare(self, actual)
    }

    /// Compare a set of named complex actual outputs against the
    /// `c128`-dtype golden fields in this fixture. Per-field tolerance
    /// is applied to the **complex modulus** of the residual
    /// `|actual − golden|`.
    ///
    /// Fields whose declared dtype is not `c128` are skipped (the
    /// caller is expected to compare them separately via
    /// [`compare_against`](Self::compare_against) — the two diff
    /// reports can be merged or kept independent depending on the
    /// downstream tool).
    pub fn compare_complex_against(
        &self,
        actual: &BTreeMap<String, Vec<Complex64>>,
    ) -> crate::ComparisonReport {
        crate::diff::compare_complex(self, actual)
    }
}
