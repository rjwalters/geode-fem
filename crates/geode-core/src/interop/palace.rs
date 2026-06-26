//! Palace oracle ingestion (issue #239).
//!
//! Palace (<https://github.com/awslabs/palace>) is the eventual full-3D
//! gold-standard oracle for the geode-fem driven-port benchmarks. Palace
//! is not installed on the geode-fem dev machine — its run is
//! **operator-assisted** — but the *configuration* + *result-ingestion*
//! glue lives here so a Palace reference can be produced by anyone with
//! a working Palace install (or the sister-repo Docker recipe at
//! `~/GitHub/sphere/eda/mom/docker/palace`) and slotted into the
//! benchmark TOMLs without ad-hoc parsing per benchmark.
//!
//! # Two sides of the oracle
//!
//! 1. **Config generation** lives in the offline driver
//!    `reference/palace/geode_patch_baseline/` (cargo binary, outside
//!    the geode workspace — mirrors the `reference/mom/` offline
//!    pattern). It emits `reference/fixtures/patch_palace/palace_config.json`,
//!    which an operator can feed directly to Palace.
//!
//! 2. **Result ingestion** — this module — parses Palace's CSV output
//!    artifacts into a typed [`PalaceResults`] struct that can be:
//!      - serialized into the `[oracles.palace]` TOML block (via
//!        [`PalaceOracleSlot`]); or
//!      - consumed directly by a benchmark test in compare-with-band
//!        mode when the slot is `populated`, with a clean
//!        skip-with-note path when the slot is `pending_operator_run`.
//!
//! # Honesty about "pending_operator_run"
//!
//! The committed `[oracles.palace]` slot in the benchmark TOMLs stays
//! `pending_operator_run` until a real operator-supplied Palace run is
//! ingested. **No fabricated Palace numbers** go into a real
//! `[oracles.palace]` slot. The unit tests in
//! `crates/geode-core/tests/palace_ingestion.rs` exercise the parser
//! against a clearly-labeled example fixture under
//! `crates/geode-core/tests/fixtures/palace/` — that fixture is for
//! testing the *ingestion code*, not the *oracle*.

use std::fmt;
use std::path::Path;

/// Reference impedance Palace reports S-parameters against. The Palace
/// config emitted by `reference/palace/geode_patch_baseline/` uses a
/// 50 Ω lumped port, matching the geode-fem benchmark drive.
pub const PALACE_DEFAULT_PORT_OHM: f64 = 50.0;

/// One Palace driven-port sweep point as parsed from the standard
/// Palace `s-parameters.csv` artifact.
#[derive(Clone, Debug, PartialEq)]
pub struct PalaceSweepPoint {
    /// Drive frequency (GHz). Palace writes Hz; the ingester converts
    /// to GHz to match the geode-fem benchmark TOMLs.
    pub f_ghz: f64,
    /// Complex S11 at the lumped port.
    pub s11_re: f64,
    pub s11_im: f64,
}

impl PalaceSweepPoint {
    /// |S11|, the linear reflection magnitude (always in [0, 1] for a
    /// passive one-port).
    pub fn s11_mag(&self) -> f64 {
        (self.s11_re * self.s11_re + self.s11_im * self.s11_im).sqrt()
    }

    /// Return loss in dB: `20 log10 |S11|`.
    pub fn s11_db(&self) -> f64 {
        20.0 * self.s11_mag().max(1e-30).log10()
    }

    /// Re-derive port impedance from S11 and a reference resistance:
    /// `Z = R · (1 + S11) / (1 - S11)`.
    ///
    /// Returns `(re, im)` parts in ohms.
    pub fn z_from_s11(&self, r_ref_ohm: f64) -> (f64, f64) {
        // Z = R · (1 + s) / (1 - s) where s = s11_re + i s11_im.
        let num_re = 1.0 + self.s11_re;
        let num_im = self.s11_im;
        let den_re = 1.0 - self.s11_re;
        let den_im = -self.s11_im;
        let den_mag2 = den_re * den_re + den_im * den_im;
        let z_re = (num_re * den_re + num_im * den_im) / den_mag2;
        let z_im = (num_im * den_re - num_re * den_im) / den_mag2;
        (r_ref_ohm * z_re, r_ref_ohm * z_im)
    }
}

/// All ingested Palace artifacts for one benchmark fixture.
#[derive(Clone, Debug, PartialEq)]
pub struct PalaceResults {
    /// Free-form Palace version string (e.g. `"0.13.0-git"`). Recorded
    /// in provenance and the `[oracles.palace]` TOML block.
    pub palace_version: String,
    /// Hex-encoded SHA256 of the Palace JSON config the run consumed.
    /// Lets the comparison test confirm Palace ran on the same config
    /// the geode-fem generator emitted (the "are we comparing the same
    /// problem?" check).
    pub config_sha256: String,
    /// Port reference resistance (Ω) the Palace run used.
    pub port_resistance_ohm: f64,
    /// Per-frequency sweep points.
    pub points: Vec<PalaceSweepPoint>,
}

impl PalaceResults {
    /// Parse a Palace `s-parameters.csv` artifact into [`PalaceResults`].
    ///
    /// Palace's CSV format (as of 0.13) is a comma-separated table with
    /// a header line of column names and one row per sweep point. The
    /// columns relevant to a one-port driven sweep are:
    ///
    /// ```text
    /// f (GHz),               Re(S[1][1]),         Im(S[1][1]),       ...
    /// ```
    ///
    /// The ingester is **lenient**: column names are matched
    /// case-insensitively with whitespace trimmed, and extra columns
    /// are ignored. Lines beginning with `#` are treated as comments.
    /// Frequency is detected as either `f (GHz)` or `f (Hz)`; Hz values
    /// are converted to GHz on read.
    pub fn from_palace_csv(
        csv_text: &str,
        palace_version: impl Into<String>,
        config_sha256: impl Into<String>,
        port_resistance_ohm: f64,
    ) -> Result<Self, PalaceIngestError> {
        let mut lines = csv_text
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty() && !l.starts_with('#'));

        let header = lines.next().ok_or(PalaceIngestError::EmptyCsv)?;
        let headers: Vec<String> = header
            .split(',')
            .map(|c| c.trim().to_ascii_lowercase())
            .collect();

        // Locate the frequency column (Palace varies between `f (GHz)`
        // and `f (Hz)` across versions; we read either and normalize).
        let mut f_idx: Option<usize> = None;
        let mut f_in_hz = false;
        for (i, h) in headers.iter().enumerate() {
            // Accept "f (ghz)", "freq (ghz)", "frequency (ghz)", etc.
            if h.contains("ghz") && (h.starts_with('f') || h.contains("freq")) {
                f_idx = Some(i);
                f_in_hz = false;
                break;
            }
            if h.contains("hz") && (h.starts_with('f') || h.contains("freq")) {
                f_idx = Some(i);
                f_in_hz = true;
            }
        }
        let f_idx = f_idx.ok_or(PalaceIngestError::MissingColumn("frequency".to_string()))?;

        // Real and imaginary S11 columns. Palace writes them as
        // `Re(S[1][1])` / `Im(S[1][1])`. After lowercasing + trimming
        // the patterns we accept are quite loose so different Palace
        // versions land in the same parser.
        let re_idx = find_col(&headers, &["re(s[1][1])", "re(s11)", "real(s[1][1])"])
            .ok_or(PalaceIngestError::MissingColumn("Re(S[1][1])".to_string()))?;
        let im_idx = find_col(&headers, &["im(s[1][1])", "im(s11)", "imag(s[1][1])"])
            .ok_or(PalaceIngestError::MissingColumn("Im(S[1][1])".to_string()))?;

        let mut points = Vec::new();
        for (lineno, row) in lines.enumerate() {
            let cols: Vec<&str> = row.split(',').map(|c| c.trim()).collect();
            if cols.len() <= f_idx.max(re_idx).max(im_idx) {
                return Err(PalaceIngestError::ShortRow {
                    line: lineno + 2, // header was line 1, rows start at 2
                    expected: f_idx.max(re_idx).max(im_idx) + 1,
                    got: cols.len(),
                });
            }
            let parse = |i: usize| -> Result<f64, PalaceIngestError> {
                cols[i]
                    .parse::<f64>()
                    .map_err(|e| PalaceIngestError::ParseFloat {
                        line: lineno + 2,
                        column: i,
                        value: cols[i].to_string(),
                        err: e.to_string(),
                    })
            };
            let f_raw = parse(f_idx)?;
            let s11_re = parse(re_idx)?;
            let s11_im = parse(im_idx)?;
            let f_ghz = if f_in_hz { f_raw / 1.0e9 } else { f_raw };
            points.push(PalaceSweepPoint {
                f_ghz,
                s11_re,
                s11_im,
            });
        }

        if points.is_empty() {
            return Err(PalaceIngestError::NoPoints);
        }

        Ok(PalaceResults {
            palace_version: palace_version.into(),
            config_sha256: config_sha256.into(),
            port_resistance_ohm,
            points,
        })
    }

    /// Convenience wrapper: read CSV from disk and parse via
    /// [`Self::from_palace_csv`].
    pub fn from_palace_csv_file(
        path: &Path,
        palace_version: impl Into<String>,
        config_sha256: impl Into<String>,
        port_resistance_ohm: f64,
    ) -> Result<Self, PalaceIngestError> {
        let text = std::fs::read_to_string(path).map_err(|e| PalaceIngestError::Io {
            path: path.to_string_lossy().into_owned(),
            err: e.to_string(),
        })?;
        Self::from_palace_csv(&text, palace_version, config_sha256, port_resistance_ohm)
    }

    /// First-resonance estimate: the lowest-frequency point at which
    /// `Im Z` crosses through zero from positive to negative (or
    /// negative to positive). Returns `None` if no crossing is found
    /// in the sweep.
    pub fn estimate_resonance_ghz(&self) -> Option<f64> {
        let zs: Vec<(f64, f64)> = self
            .points
            .iter()
            .map(|p| {
                let (_, z_im) = p.z_from_s11(self.port_resistance_ohm);
                (p.f_ghz, z_im)
            })
            .collect();
        for w in zs.windows(2) {
            let (f0, im0) = w[0];
            let (f1, im1) = w[1];
            if im0 == 0.0 {
                return Some(f0);
            }
            if (im0 > 0.0) != (im1 > 0.0) {
                // Linear interpolation to the zero crossing.
                let t = im0 / (im0 - im1);
                return Some(f0 + t * (f1 - f0));
            }
        }
        None
    }

    /// S11 dip (frequency and value in dB). Returns the most-negative
    /// `s11_db` in the sweep — the strongest match. None for an empty
    /// sweep.
    pub fn s11_dip_db(&self) -> Option<(f64, f64)> {
        self.points
            .iter()
            .map(|p| (p.f_ghz, p.s11_db()))
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
    }
}

fn find_col(headers: &[String], needles: &[&str]) -> Option<usize> {
    for (i, h) in headers.iter().enumerate() {
        let normalized: String = h.chars().filter(|c| !c.is_whitespace()).collect();
        for n in needles {
            let n_norm: String = n.chars().filter(|c| !c.is_whitespace()).collect();
            if normalized == n_norm {
                return Some(i);
            }
        }
    }
    None
}

/// Errors emitted by the Palace CSV ingester.
#[derive(Clone, Debug, PartialEq)]
pub enum PalaceIngestError {
    EmptyCsv,
    MissingColumn(String),
    ShortRow {
        line: usize,
        expected: usize,
        got: usize,
    },
    ParseFloat {
        line: usize,
        column: usize,
        value: String,
        err: String,
    },
    Io {
        path: String,
        err: String,
    },
    NoPoints,
}

impl fmt::Display for PalaceIngestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyCsv => write!(f, "Palace CSV is empty"),
            Self::MissingColumn(c) => write!(f, "missing required column: {c}"),
            Self::ShortRow {
                line,
                expected,
                got,
            } => write!(
                f,
                "Palace CSV row {line}: expected at least {expected} columns, got {got}"
            ),
            Self::ParseFloat {
                line,
                column,
                value,
                err,
            } => write!(
                f,
                "Palace CSV row {line}, column {column}: cannot parse {value:?} as f64: {err}"
            ),
            Self::Io { path, err } => write!(f, "I/O error reading {path}: {err}"),
            Self::NoPoints => write!(f, "Palace CSV has no data points"),
        }
    }
}

impl std::error::Error for PalaceIngestError {}

/// Lightweight typed view of the `[oracles.palace]` TOML block in a
/// benchmark `results.toml`.
///
/// The block has two valid shapes, matching the two phases of the
/// operator-assisted lifecycle:
///
/// - **`pending_operator_run`**: Palace has not been run for this
///   benchmark on this fixture. The slot still carries its
///   provenance-style `note`. Benchmark tests **skip with a note** in
///   this state — they never silently pass.
/// - **`populated`**: A real operator-run Palace reference has been
///   ingested. The block carries `palace_version`, `config_sha256`,
///   `port_resistance_ohm`, and the parsed `points`. Benchmark tests
///   compare FEM results against these values within a documented band.
///
/// The [`from_toml_table`](Self::from_toml_table) loader is robust
/// against extra fields (so future schema extensions don't trip the
/// parser).
#[derive(Clone, Debug, PartialEq)]
pub enum PalaceOracleSlot {
    PendingOperatorRun { note: Option<String> },
    Populated(Box<PalaceResults>),
}

impl PalaceOracleSlot {
    /// True if the slot has been populated with an operator-run
    /// reference (vs. still `pending_operator_run`).
    pub fn is_populated(&self) -> bool {
        matches!(self, Self::Populated(_))
    }

    /// Return the populated results if present.
    pub fn as_results(&self) -> Option<&PalaceResults> {
        match self {
            Self::Populated(r) => Some(r),
            _ => None,
        }
    }

    /// Parse the `[oracles.palace]` table out of a `toml::Value` (any
    /// flavor: a `toml::Value::Table` matching the schema below).
    ///
    /// Pending shape:
    /// ```toml
    /// [oracles.palace]
    /// status = "pending_operator_run"
    /// note = "..."
    /// ```
    ///
    /// Populated shape:
    /// ```toml
    /// [oracles.palace]
    /// status = "populated"
    /// palace_version = "0.13.0-git"
    /// config_sha256 = "..."
    /// port_resistance_ohm = 50
    ///
    /// [[oracles.palace.points]]
    /// f_ghz = 2.30
    /// s11_re = -0.43
    /// s11_im = -0.26
    /// ```
    pub fn from_toml_table(table: &toml::Value) -> Result<Self, PalaceSlotError> {
        let t = table
            .as_table()
            .ok_or_else(|| PalaceSlotError::Schema("[oracles.palace] is not a table".into()))?;
        let status = t
            .get("status")
            .and_then(|v| v.as_str())
            .ok_or_else(|| PalaceSlotError::Schema("missing or non-string `status`".into()))?;

        match status {
            "pending_operator_run" | "deferred" => {
                let note = t.get("note").and_then(|v| v.as_str()).map(String::from);
                Ok(Self::PendingOperatorRun { note })
            }
            "populated" => {
                let palace_version = t
                    .get("palace_version")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        PalaceSlotError::Schema("populated slot missing `palace_version`".into())
                    })?
                    .to_string();
                let config_sha256 = t
                    .get("config_sha256")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        PalaceSlotError::Schema("populated slot missing `config_sha256`".into())
                    })?
                    .to_string();
                let port_resistance_ohm = t
                    .get("port_resistance_ohm")
                    .and_then(|v| v.as_float().or_else(|| v.as_integer().map(|i| i as f64)))
                    .ok_or_else(|| {
                        PalaceSlotError::Schema(
                            "populated slot missing `port_resistance_ohm`".into(),
                        )
                    })?;
                let points_val = t.get("points").ok_or_else(|| {
                    PalaceSlotError::Schema("populated slot missing `points` array".into())
                })?;
                let points_arr = points_val.as_array().ok_or_else(|| {
                    PalaceSlotError::Schema("`points` is not an array of tables".into())
                })?;
                let mut points = Vec::with_capacity(points_arr.len());
                for (i, p) in points_arr.iter().enumerate() {
                    let getf = |k: &str| {
                        p.get(k).and_then(|v| v.as_float()).ok_or_else(|| {
                            PalaceSlotError::Schema(format!("points[{i}].{k} missing or not float"))
                        })
                    };
                    points.push(PalaceSweepPoint {
                        f_ghz: getf("f_ghz")?,
                        s11_re: getf("s11_re")?,
                        s11_im: getf("s11_im")?,
                    });
                }
                if points.is_empty() {
                    return Err(PalaceSlotError::Schema(
                        "populated slot has empty `points`".into(),
                    ));
                }
                Ok(Self::Populated(Box::new(PalaceResults {
                    palace_version,
                    config_sha256,
                    port_resistance_ohm,
                    points,
                })))
            }
            other => Err(PalaceSlotError::UnknownStatus(other.to_string())),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum PalaceSlotError {
    Schema(String),
    UnknownStatus(String),
}

impl fmt::Display for PalaceSlotError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Schema(s) => write!(f, "[oracles.palace] schema error: {s}"),
            Self::UnknownStatus(s) => write!(
                f,
                "[oracles.palace] unknown `status` {s:?} (expected \
                 \"pending_operator_run\" or \"populated\")"
            ),
        }
    }
}

impl std::error::Error for PalaceSlotError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_palace_csv() {
        let csv = "\
# Palace s-parameters (example)
f (GHz), Re(S[1][1]), Im(S[1][1])
2.30, -0.43, -0.26
2.40, -0.88, 0.16
";
        let r = PalaceResults::from_palace_csv(csv, "0.13.0-test", "abcd", 50.0).unwrap();
        assert_eq!(r.points.len(), 2);
        assert!((r.points[0].f_ghz - 2.30).abs() < 1e-12);
        assert!((r.points[0].s11_re + 0.43).abs() < 1e-12);
        assert!((r.points[1].s11_im - 0.16).abs() < 1e-12);
        assert!((r.points[0].s11_mag() - (0.43_f64 * 0.43 + 0.26 * 0.26).sqrt()).abs() < 1e-12);
        assert_eq!(r.port_resistance_ohm, 50.0);
        assert_eq!(r.palace_version, "0.13.0-test");
    }

    #[test]
    fn parse_hz_frequency_column() {
        // Same content but f is in Hz: the ingester normalizes.
        let csv = "\
f (Hz), Re(S[1][1]), Im(S[1][1])
2.30e9, -0.5, 0.0
";
        let r = PalaceResults::from_palace_csv(csv, "v", "h", 50.0).unwrap();
        assert!((r.points[0].f_ghz - 2.30).abs() < 1e-9);
    }

    #[test]
    fn missing_column_is_reported() {
        let csv = "f (GHz), Re(S[1][1])\n2.30, -0.5\n";
        match PalaceResults::from_palace_csv(csv, "v", "h", 50.0) {
            Err(PalaceIngestError::MissingColumn(c)) => assert!(c.to_lowercase().contains("im")),
            other => panic!("expected MissingColumn, got {other:?}"),
        }
    }

    #[test]
    fn z_round_trip_matched_load() {
        // S11 = 0 → Z = R = 50 ohm + 0i.
        let p = PalaceSweepPoint {
            f_ghz: 2.4,
            s11_re: 0.0,
            s11_im: 0.0,
        };
        let (re, im) = p.z_from_s11(50.0);
        assert!((re - 50.0).abs() < 1e-12);
        assert!(im.abs() < 1e-12);
    }

    #[test]
    fn z_round_trip_short_and_open() {
        // S11 = -1 → Z = 0; S11 = +1 → Z = infinity (denominator zero is
        // a numerical limit we just check the magnitude blows up for).
        let short = PalaceSweepPoint {
            f_ghz: 2.4,
            s11_re: -1.0,
            s11_im: 0.0,
        };
        let (re, im) = short.z_from_s11(50.0);
        assert!(re.abs() < 1e-12);
        assert!(im.abs() < 1e-12);

        let nearly_open = PalaceSweepPoint {
            f_ghz: 2.4,
            s11_re: 0.999,
            s11_im: 0.0,
        };
        let (re, _) = nearly_open.z_from_s11(50.0);
        assert!(
            re > 1.0e4,
            "open termination should drive Z to large positive"
        );
    }

    #[test]
    fn pending_slot_round_trip() {
        let toml_text = r#"
[oracles.palace]
status = "pending_operator_run"
note = "Palace is operator-assisted; ingest via geode_core::interop::palace."
"#;
        let doc: toml::Value = toml::from_str(toml_text).unwrap();
        let block = &doc["oracles"]["palace"];
        let slot = PalaceOracleSlot::from_toml_table(block).unwrap();
        assert!(!slot.is_populated());
        assert!(slot.as_results().is_none());
    }

    #[test]
    fn populated_slot_round_trip() {
        let toml_text = r#"
[oracles.palace]
status = "populated"
palace_version = "0.13.0-git"
config_sha256 = "deadbeef"
port_resistance_ohm = 50

[[oracles.palace.points]]
f_ghz = 2.30
s11_re = -0.43
s11_im = -0.26

[[oracles.palace.points]]
f_ghz = 2.40
s11_re = -0.88
s11_im = 0.16
"#;
        let doc: toml::Value = toml::from_str(toml_text).unwrap();
        let slot = PalaceOracleSlot::from_toml_table(&doc["oracles"]["palace"]).unwrap();
        assert!(slot.is_populated());
        let r = slot.as_results().unwrap();
        assert_eq!(r.palace_version, "0.13.0-git");
        assert_eq!(r.points.len(), 2);
        let dip = r.s11_dip_db().unwrap();
        // The deeper dip is at 2.40 GHz (s11_mag ~ 0.894 → ~-0.97 dB);
        // 2.30 GHz has s11_mag ~ 0.502 → ~ -5.98 dB. The dip is deeper
        // at 2.30 GHz.
        assert!((dip.0 - 2.30).abs() < 1e-9);
        assert!(dip.1 < -5.0 && dip.1 > -7.0);
    }

    #[test]
    fn unknown_status_is_reported() {
        let toml_text = r#"
[oracles.palace]
status = "running"
"#;
        let doc: toml::Value = toml::from_str(toml_text).unwrap();
        match PalaceOracleSlot::from_toml_table(&doc["oracles"]["palace"]) {
            Err(PalaceSlotError::UnknownStatus(s)) => assert_eq!(s, "running"),
            other => panic!("expected UnknownStatus, got {other:?}"),
        }
    }
}
