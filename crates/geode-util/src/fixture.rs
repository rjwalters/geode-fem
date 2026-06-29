//! Fixture adaptors + JSON fixture loader/schema.
//!
//! Staging home (Epic #414, Phase 2) for the fixture-output glue that the
//! standalone example crates would otherwise copy-paste: the TOML
//! write-to-disk tail, the indexed per-row TOML table seam, the ParaView
//! `.pvd` collection writer, and the frequency-sweep point helper used to
//! generate and record regression fixtures.
//!
//! Each example assembles its own bespoke `[meta]` / oracle / geometry
//! prose (those sections are unique per benchmark and not shared); the
//! reusable pieces extracted here are the parts that were genuinely
//! identical across crates.
//!
//! This module is also the canonical home (Epic #429, Phase 2) of the JSON
//! `Fixture` loader, its schema types ([`Fixture`], [`Field`],
//! [`OutputField`], [`Provenance`], [`FixtureError`], [`FixtureFormat`]),
//! and the golden-value accessors ([`Fixture::output_f64`],
//! [`Fixture::output_c128`], [`Fixture::input_c128`],
//! [`Fixture::iter_outputs`]). `geode-validation` re-exports these so its
//! reference-test suite keeps its existing call sites. The validation
//! *diff artifact* (`ComparisonReport` + the comparison entry points)
//! stays in `geode-validation`.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;

use num_complex::Complex64;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Write an assembled TOML document to `path`, creating parent directories
/// and logging `wrote <path>` to stderr.
///
/// Centralises the `create_dir_all(parent)` + `fs::write` + `eprintln!`
/// tail that every example's fixture writer repeated verbatim.
pub fn write_toml(path: &Path, contents: &str) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, contents)?;
    eprintln!("wrote {}", path.display());
    Ok(())
}

/// Append a single `[<header>]` TOML table to `out`, serializing `row`'s
/// `key = value` body through the [`toml`] crate (followed by a blank
/// line).
///
/// `row` is any `#[derive(Serialize)]` value whose fields are TOML
/// scalars (or inline arrays / `Option`s) — the serializer emits the body
/// in struct-field declaration order. This is the serde-derive
/// replacement (Epic #429, Phase 3) for the hand-rolled `{:.15e}` /
/// `{:.3e}` per-field `write_fields` impls the example crates previously
/// copy-pasted: the numeric values are identical (the `toml` crate emits
/// the exact, shortest-round-tripping decimal for each `f64`), only the
/// float *spelling* changes (decimal / shortest instead of fixed-width
/// exponential).
///
/// `header` is interpolated verbatim into `[<header>]`, so callers that
/// need a quoted/dotted table name (e.g. `"\"group.input\""`) pass the
/// already-quoted string.
///
/// # Panics
///
/// Panics if `row` fails to serialize as a TOML table (e.g. a non-string
/// map key, or a non-finite float — neither of which the example rows
/// produce).
pub fn push_table<T: Serialize>(out: &mut String, header: &str, row: &T) {
    out.push_str(&format!("[{header}]\n"));
    let body =
        toml::to_string(row).unwrap_or_else(|e| panic!("serialize TOML row under [{header}]: {e}"));
    out.push_str(&body);
    out.push('\n');
}

/// Append `rows` to `out` as a sequence of indexed `[<prefix>_<i>]` TOML
/// tables, each serialized via [`push_table`] and followed by a blank
/// line.
///
/// Replaces the `for (i, r) in rows.iter().enumerate() { ... }` table
/// loop (and the hand-rolled `TomlRow::write_fields` formatting) that the
/// example fixture writers duplicated. Each `row` is a
/// `#[derive(Serialize)]` value; the row's column order is its
/// struct-field declaration order.
pub fn push_rows<T: Serialize>(out: &mut String, prefix: &str, rows: &[T]) {
    for (i, row) in rows.iter().enumerate() {
        push_table(out, &format!("{prefix}_{i}"), row);
    }
}

/// Write a ParaView `.pvd` collection mapping each frame's `.vtu` file to
/// a `timestep`, so ParaView (and `sweep_animate.py`) treats a frequency
/// sweep as a time-series.
///
/// `frames` is `(timestep, file_name)` pairs where `file_name` is the
/// frame's path **relative to the `.pvd`** (e.g. `E_0000.vtu`). The `.pvd`
/// is a tiny hand-rolled XML (no XML dependency).
pub fn write_pvd(path: &Path, frames: &[(f64, String)]) -> io::Result<()> {
    let mut s = String::new();
    s.push_str("<?xml version=\"1.0\"?>\n");
    s.push_str("<VTKFile type=\"Collection\" version=\"0.1\" byte_order=\"LittleEndian\">\n");
    s.push_str("  <Collection>\n");
    for (timestep, file) in frames {
        s.push_str(&format!(
            "    <DataSet timestep=\"{timestep}\" group=\"\" part=\"0\" file=\"{file}\"/>\n"
        ));
    }
    s.push_str("  </Collection>\n");
    s.push_str("</VTKFile>\n");
    fs::write(path, s)
}

/// Evenly-spaced sweep frequencies (GHz) over `[f_start_ghz, f_stop_ghz]`
/// inclusive.
///
/// A single-point sweep (`n <= 1`) returns just `f_start_ghz`.
pub fn sweep_freqs(f_start_ghz: f64, f_stop_ghz: f64, n: usize) -> Vec<f64> {
    let n = n.max(1);
    if n == 1 {
        return vec![f_start_ghz];
    }
    let step = (f_stop_ghz - f_start_ghz) / (n - 1) as f64;
    (0..n).map(|i| f_start_ghz + step * i as f64).collect()
}

// ---------------------------------------------------------------------------
// JSON (`serde_json::Value`) numeric primitives
//
// Shared home for the recursive numeric-flatten + scalar-extract helpers the
// validation reference tests previously copy-pasted. These operate purely on
// `serde_json::Value`; the `&Fixture`-typed loader helpers are a separate
// (later) staging concern.
// ---------------------------------------------------------------------------

/// Recursively flatten a (possibly nested) JSON array of numbers into a
/// flat, row-major [`Vec<f64>`].
///
/// - [`Number`](serde_json::Value::Number) nodes are appended as `f64`,
///   preferring [`as_f64`](serde_json::Number::as_f64) and falling back to
///   `as_i64` / `as_u64` for integers outside the lossless-float range.
/// - [`Array`](serde_json::Value::Array) nodes recurse, depth-first, so a
///   nested array matching some shape and an already-flat array both yield
///   the same row-major sequence.
/// - All other node kinds (null, bool, string, object) are silently
///   skipped.
///
/// A scalar number flattens to a single-element vector; an empty array (or
/// a non-numeric node) flattens to an empty vector.
pub fn flatten_numeric(v: &serde_json::Value) -> Vec<f64> {
    let mut out = Vec::new();
    push_numeric(v, &mut out);
    out
}

fn push_numeric(v: &serde_json::Value, out: &mut Vec<f64>) {
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
                push_numeric(item, out);
            }
        }
        _ => {}
    }
}

/// Extract a scalar [`f64`] from a JSON [`Number`](serde_json::Value::Number)
/// node, returning [`None`] for any other node kind.
///
/// Integer numbers convert via `as_i64` / `as_u64` when they fall outside
/// the lossless-float range, mirroring [`flatten_numeric`].
pub fn value_f64(v: &serde_json::Value) -> Option<f64> {
    match v {
        serde_json::Value::Number(n) => n
            .as_f64()
            .or_else(|| n.as_i64().map(|x| x as f64))
            .or_else(|| n.as_u64().map(|x| x as f64)),
        _ => None,
    }
}

/// Extract a scalar [`i64`] from a JSON [`Number`](serde_json::Value::Number)
/// node, returning [`None`] for any other node kind or for numbers that are
/// not representable as an `i64` (e.g. fractional or out-of-range values).
pub fn value_i64(v: &serde_json::Value) -> Option<i64> {
    match v {
        serde_json::Value::Number(n) => n.as_i64(),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// JSON fixture loader + schema + golden accessors
//
// Canonical home (Epic #429, Phase 2) of the JSON `Fixture` machinery
// migrated from `geode-validation/src/fixture.rs`. `geode-validation`
// re-exports these types so its reference-test suite is unchanged.
//
// A *fixture* is a single canonical (inputs, golden outputs) bundle for one
// spine slice. The on-disk schema is documented in `reference/README.md` and
// `reference/SCHEMA.md`; the corresponding Rust types here are loose
// `Map<String, Field>` so the scaffolding stays useful as the schema evolves
// (new fields can be added without requiring a Rust-side type-update churn).
//
// ## Format support
//
// Phase A wires only the JSON format. The [`FixtureFormat`] enum is a
// deliberate extension point — when the cube-cavity fixture (#92) lands and
// starts carrying complex eigenvectors, a `FixtureFormat::Hdf5` variant will
// be added behind a feature gate so contributors aren't forced to install
// `libhdf5` to run the smoke tests.
// ---------------------------------------------------------------------------

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

    /// Load a JSON fixture by its path relative to `reference/fixtures/`
    /// (e.g. `"cube_cavity/baseline.json"`).
    ///
    /// Convenience wrapper over [`Fixture::load_from`] +
    /// [`crate::repo::fixture_path`] — the one-call form of the
    /// `load_from(&fixture_path(...), FixtureFormat::Json)` pattern the
    /// reference tests repeat.
    pub fn load_json(relative: impl AsRef<Path>) -> Result<Self, FixtureError> {
        Self::load_from(&crate::repo::fixture_path(relative), FixtureFormat::Json)
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
        let expected = f.shape.iter().product::<usize>();
        // Real-imag interleave reassembly lives in `crate::interop`
        // (Epic #414, Phase 2); the schema-level length error stays here.
        let data = crate::interop::decode_real_imag_interleave_exact(&flat, expected).ok_or_else(
            || FixtureError::InvalidComplexLength {
                name: name.to_string(),
                got: flat.len(),
                expected: 2 * expected,
            },
        )?;
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
        let expected = f.shape.iter().product::<usize>();
        // Same interleave decode as `output_c128`, reading from `inputs`.
        crate::interop::decode_real_imag_interleave_exact(&flat, expected).ok_or_else(|| {
            FixtureError::InvalidComplexLength {
                name: name.to_string(),
                got: flat.len(),
                expected: 2 * expected,
            }
        })
    }

    // -----------------------------------------------------------------------
    // Scalar / vector convenience accessors for reference-test drivers
    //
    // These panic on missing or malformed fields — they are deliberately
    // ergonomic helpers for the reference-test suite (call sites read like
    // `fixture.input_f64("side")`), not fallible library entry points. Use
    // the field maps (`inputs`/`outputs`) or the `*_f64`/`*_c128` accessors
    // directly when a non-panicking path is needed.
    // -----------------------------------------------------------------------

    /// Pull a scalar `f64` from an **input** field.
    ///
    /// The field's `data` is flattened through [`flatten_numeric`] (so a
    /// bare number, a `[x]` array, or a nested singleton all work) and
    /// asserted to hold exactly one value.
    ///
    /// # Panics
    ///
    /// Panics if the input field is missing or does not flatten to exactly
    /// one numeric element.
    pub fn input_f64(&self, name: &str) -> f64 {
        let field = self
            .inputs
            .get(name)
            .unwrap_or_else(|| panic!("fixture missing input `{name}`"));
        let v = flatten_numeric(&field.data);
        assert_eq!(
            v.len(),
            1,
            "expected scalar input `{name}`, got len {}",
            v.len()
        );
        v[0]
    }

    /// Pull a scalar `i64` from an **input** field.
    ///
    /// Identical to [`input_f64`](Self::input_f64) but truncates the
    /// (integer-valued) scalar to `i64`. The v1 schema has no typed integer
    /// accessor, so this round-trips through `f64`.
    ///
    /// # Panics
    ///
    /// Panics if the input field is missing or is not a single value.
    pub fn input_i64(&self, name: &str) -> i64 {
        self.input_f64(name) as i64
    }

    /// Pull a full **input** field as a flat row-major `Vec<f64>`,
    /// flattening nested arrays through [`flatten_numeric`].
    ///
    /// # Panics
    ///
    /// Panics if the input field is missing.
    pub fn input_vec(&self, name: &str) -> Vec<f64> {
        let field = self
            .inputs
            .get(name)
            .unwrap_or_else(|| panic!("fixture missing input `{name}`"));
        flatten_numeric(&field.data)
    }

    /// Pull a scalar `f64` from an **output** (golden) field.
    ///
    /// Mirrors [`input_f64`](Self::input_f64) but reads from `outputs` via
    /// [`output_f64`](Self::output_f64), preserving the input-vs-output
    /// distinction.
    ///
    /// # Panics
    ///
    /// Panics if the output field is missing or is not a single value.
    pub fn output_scalar(&self, name: &str) -> f64 {
        let golden = self
            .output_f64(name)
            .unwrap_or_else(|e| panic!("fixture missing scalar output `{name}`: {e}"));
        assert_eq!(
            golden.data.len(),
            1,
            "scalar output `{name}` should be length 1"
        );
        golden.data[0]
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// Unique scratch path under the system temp dir (no `tempfile` dep).
    fn scratch(name: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "geode-util-fixture-{}-{nanos}-{name}",
            std::process::id()
        ))
    }

    #[derive(Serialize)]
    struct Pt {
        f_ghz: f64,
        q: f64,
    }

    #[test]
    fn push_rows_emits_indexed_tables() {
        let rows = vec![
            Pt {
                f_ghz: 1.0,
                q: 10.0,
            },
            Pt {
                f_ghz: 2.0,
                q: 20.0,
            },
        ];
        let mut s = String::new();
        push_rows(&mut s, "point", &rows);
        // Float spelling is the `toml` crate's shortest-round-trip decimal
        // (`1.0`, not the old `1.000e0`); the indexed-table framing and the
        // struct-field column order are unchanged.
        let expected = "\
[point_0]
f_ghz = 1.0
q = 10.0

[point_1]
f_ghz = 2.0
q = 20.0

";
        assert_eq!(s, expected);
    }

    #[test]
    fn push_rows_values_round_trip_numerically() {
        // The serde+toml seam must preserve every f64 exactly (shortest
        // round-trip), so a re-parse of the emitted TOML recovers the
        // original values bit-for-bit — the property the old `{:.15e}`
        // formatting only approximated (16 significant figures).
        let rows = vec![
            Pt {
                f_ghz: 2.4000000001,
                q: 1.234_567_890_123_456_7e3,
            },
            Pt {
                f_ghz: -3.0e-12,
                q: std::f64::consts::PI,
            },
        ];
        let mut s = String::new();
        push_rows(&mut s, "point", &rows);
        let doc: toml::Value = toml::from_str(&s).expect("emitted rows are valid TOML");
        for (i, r) in rows.iter().enumerate() {
            let t = &doc[format!("point_{i}")];
            assert_eq!(t["f_ghz"].as_float().unwrap(), r.f_ghz);
            assert_eq!(t["q"].as_float().unwrap(), r.q);
        }
    }

    #[test]
    fn push_table_uses_verbatim_header_and_serde_body() {
        // `push_table` interpolates the header verbatim (so a quoted/dotted
        // table name is the caller's responsibility) and serializes the row
        // body via serde+toml.
        #[derive(Serialize)]
        struct Baseline<'a> {
            group: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            input: Option<&'a str>,
            median_ns: f64,
        }
        let mut s = String::new();
        push_table(
            &mut s,
            "\"assemble.10\"",
            &Baseline {
                group: "assemble",
                input: Some("10"),
                median_ns: 1234.5,
            },
        );
        // No `input` key when it is `None`.
        push_table(
            &mut s,
            "\"eigensolve\"",
            &Baseline {
                group: "eigensolve",
                input: None,
                median_ns: 6.0,
            },
        );
        let expected = "\
[\"assemble.10\"]
group = \"assemble\"
input = \"10\"
median_ns = 1234.5

[\"eigensolve\"]
group = \"eigensolve\"
median_ns = 6.0

";
        assert_eq!(s, expected);
    }

    #[test]
    fn write_toml_creates_parents_and_writes() {
        let dir = scratch("toml");
        let path = dir.join("nested").join("results.toml");
        write_toml(&path, "[meta]\nx = 1\n").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "[meta]\nx = 1\n");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn sweep_freqs_inclusive_endpoints() {
        let f = sweep_freqs(1.0, 3.0, 3);
        assert_eq!(f, vec![1.0, 2.0, 3.0]);
        // Single-point sweep collapses to the start frequency.
        assert_eq!(sweep_freqs(2.5, 9.0, 1), vec![2.5]);
        assert_eq!(sweep_freqs(2.5, 9.0, 0), vec![2.5]);
    }

    #[test]
    fn write_pvd_emits_collection_xml() {
        let path = scratch("sweep.pvd");
        let frames = vec![
            (1.0_f64, "E_0000.vtu".to_string()),
            (2.0, "E_0001.vtu".to_string()),
        ];
        write_pvd(&path, &frames).unwrap();
        let xml = fs::read_to_string(&path).unwrap();
        assert!(xml.starts_with("<?xml version=\"1.0\"?>\n"));
        assert!(
            xml.contains("<DataSet timestep=\"1\" group=\"\" part=\"0\" file=\"E_0000.vtu\"/>")
        );
        assert!(
            xml.contains("<DataSet timestep=\"2\" group=\"\" part=\"0\" file=\"E_0001.vtu\"/>")
        );
        assert!(xml.trim_end().ends_with("</VTKFile>"));
        let _ = fs::remove_file(&path);
    }

    use serde_json::json;

    #[test]
    fn flatten_numeric_empty_array() {
        assert_eq!(flatten_numeric(&json!([])), Vec::<f64>::new());
    }

    #[test]
    fn flatten_numeric_flat_array() {
        assert_eq!(
            flatten_numeric(&json!([1.0, 2.5, -3.0])),
            vec![1.0, 2.5, -3.0]
        );
    }

    #[test]
    fn flatten_numeric_nested_arrays_row_major() {
        // 2x3 nested array flattens row-major.
        let v = json!([[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]]);
        assert_eq!(flatten_numeric(&v), vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
        // Deeper / ragged nesting flattens depth-first as well.
        let v = json!([[[1.0], [2.0, 3.0]], [4.0]]);
        assert_eq!(flatten_numeric(&v), vec![1.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn flatten_numeric_mixed_int_and_float() {
        let v = json!([1, 2.5, 3, -4]);
        assert_eq!(flatten_numeric(&v), vec![1.0, 2.5, 3.0, -4.0]);
    }

    #[test]
    fn flatten_numeric_scalar() {
        assert_eq!(flatten_numeric(&json!(42)), vec![42.0]);
        assert_eq!(flatten_numeric(&json!(2.5)), vec![2.5]);
    }

    #[test]
    fn flatten_numeric_skips_non_numeric_nodes() {
        // null / bool / string / object nodes are silently skipped, while
        // numeric siblings are still collected in order.
        let v = json!([1.0, null, "x", true, [2.0, {"k": 9.0}], 3.0]);
        assert_eq!(flatten_numeric(&v), vec![1.0, 2.0, 3.0]);
        // A bare non-numeric node flattens to empty.
        assert_eq!(flatten_numeric(&json!("nope")), Vec::<f64>::new());
        assert_eq!(flatten_numeric(&json!(null)), Vec::<f64>::new());
    }

    #[test]
    fn flatten_numeric_large_u64() {
        // Integer beyond the lossless-f64 range still flattens via the
        // u64 fallback (lossy, but non-panicking).
        let big = u64::MAX;
        let v = json!([big]);
        assert_eq!(flatten_numeric(&v), vec![big as f64]);
    }

    #[test]
    fn value_f64_extracts_numbers_only() {
        assert_eq!(value_f64(&json!(2.5)), Some(2.5));
        assert_eq!(value_f64(&json!(7)), Some(7.0));
        assert_eq!(value_f64(&json!(-3)), Some(-3.0));
        assert_eq!(value_f64(&json!("2.5")), None);
        assert_eq!(value_f64(&json!(true)), None);
        assert_eq!(value_f64(&json!([1.0])), None);
        assert_eq!(value_f64(&json!(null)), None);
    }

    #[test]
    fn value_i64_extracts_integers_only() {
        assert_eq!(value_i64(&json!(7)), Some(7));
        assert_eq!(value_i64(&json!(-3)), Some(-3));
        // Fractional values are not i64-representable.
        assert_eq!(value_i64(&json!(2.5)), None);
        // u64 beyond i64::MAX is not representable as i64.
        assert_eq!(value_i64(&json!(u64::MAX)), None);
        assert_eq!(value_i64(&json!("7")), None);
        assert_eq!(value_i64(&json!(null)), None);
    }

    // -----------------------------------------------------------------------
    // Scalar / vector convenience accessors
    // -----------------------------------------------------------------------

    /// Build a minimal in-memory [`Fixture`] from the given input and output
    /// `data` JSON nodes (each declared `f64`), bypassing disk I/O.
    fn scalar_fixture(
        inputs: &[(&str, serde_json::Value)],
        outputs: &[(&str, serde_json::Value)],
    ) -> Fixture {
        let to_input = |(name, data): &(&str, serde_json::Value)| {
            (
                name.to_string(),
                json!({"shape": [1], "dtype": "f64", "data": data}),
            )
        };
        let to_output = |(name, data): &(&str, serde_json::Value)| {
            (
                name.to_string(),
                json!({"shape": [1], "dtype": "f64", "tolerance_abs": 0.0, "data": data}),
            )
        };
        let value = json!({
            "schema_version": "1",
            "fixture_id": "unit/scalar_accessors",
            "description": "",
            "units": "",
            "inputs": inputs.iter().map(to_input).collect::<serde_json::Map<_, _>>(),
            "outputs": outputs.iter().map(to_output).collect::<serde_json::Map<_, _>>(),
            "provenance": {"source": "unit test"},
        });
        serde_json::from_value(value).expect("scalar_fixture should deserialize")
    }

    #[test]
    fn input_f64_reads_scalar_from_inputs() {
        let fx = scalar_fixture(&[("side", json!([0.25])), ("bare", json!(1.5))], &[]);
        // Both array-wrapped and bare scalars flatten to a single value.
        assert_eq!(fx.input_f64("side"), 0.25);
        assert_eq!(fx.input_f64("bare"), 1.5);
    }

    #[test]
    fn input_i64_truncates_scalar_from_inputs() {
        let fx = scalar_fixture(&[("n", json!([4]))], &[]);
        assert_eq!(fx.input_i64("n"), 4_i64);
    }

    #[test]
    fn input_vec_reads_full_input_field() {
        let fx = scalar_fixture(&[("ka_curve", json!([1.0, 2.0, 3.5]))], &[]);
        assert_eq!(fx.input_vec("ka_curve"), vec![1.0, 2.0, 3.5]);
        // Nested arrays flatten row-major.
        let fx = scalar_fixture(&[("grid", json!([[1.0, 2.0], [3.0, 4.0]]))], &[]);
        assert_eq!(fx.input_vec("grid"), vec![1.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn output_scalar_reads_scalar_from_outputs() {
        let fx = scalar_fixture(&[], &[("analytic_tm11_k", json!([2.74]))]);
        assert_eq!(fx.output_scalar("analytic_tm11_k"), 2.74);
    }

    #[test]
    fn input_and_output_accessors_respect_the_inputs_vs_outputs_split() {
        // A name present only as an input is not visible to `output_scalar`,
        // and vice-versa — the two accessors read disjoint maps.
        let fx = scalar_fixture(&[("k", json!([1.0]))], &[("k", json!([9.0]))]);
        assert_eq!(fx.input_f64("k"), 1.0);
        assert_eq!(fx.output_scalar("k"), 9.0);
    }

    #[test]
    #[should_panic(expected = "fixture missing input `nope`")]
    fn input_f64_panics_on_missing_input() {
        let fx = scalar_fixture(&[("side", json!([0.25]))], &[]);
        fx.input_f64("nope");
    }

    #[test]
    #[should_panic(expected = "fixture missing scalar output `nope`")]
    fn output_scalar_panics_on_missing_output() {
        let fx = scalar_fixture(&[], &[("k", json!([1.0]))]);
        fx.output_scalar("nope");
    }

    #[test]
    #[should_panic(expected = "expected scalar input `vec`")]
    fn input_f64_panics_on_non_scalar_input() {
        let fx = scalar_fixture(&[("vec", json!([1.0, 2.0]))], &[]);
        fx.input_f64("vec");
    }
}
