//! Touchstone `.sNp` output for `geode driven` / `geode extract`
//! (`--touchstone <PATH>`, issue #703, Epic #702 Phase 1).
//!
//! # Dialect
//!
//! Always **Touchstone 2.0** (IBIS Open Forum, 2009), whatever the port
//! count, because each lumped port carries its own `resistance_ohm` and
//! only 2.0's `[Reference]` keyword can express unequal per-port
//! reference impedances. The file is:
//!
//! ```text
//! ! <provenance comments>
//! [Version] 2.0
//! # HZ S RI R <R₁>
//! [Number of Ports] N
//! [Two-Port Data Order] 21_12        (N = 2 only; required by 2.0)
//! [Number of Frequencies] F
//! [Reference] R₁ R₂ … R_N
//! [Network Data]
//! <f> <Re S…> <Im S…> …              (one block per frequency, ascending)
//! [End]
//! ```
//!
//! * **Parameter**: `S` only (the report's `results[].s`, referenced to
//!   each port's `resistance_ohm`). `Z` / `Y` export is a non-goal of
//!   this phase.
//! * **Format**: `RI` — the report's `[re, im]` pairs copied verbatim, no
//!   polar conversion.
//! * **Frequency unit**: `HZ` — the report's `frequency_hz`, no rescaling.
//! * **Numbers** use Rust's shortest round-trip `{:e}` formatting, so
//!   parsing the file back yields the report's `f64` values bit for bit.
//! * **Ordering**: rows are written in **ascending frequency** (a
//!   `driven` spec's frequencies are solved in spec order, which need not
//!   be ascending; the writer sorts). Duplicate frequencies cannot be
//!   expressed and are rejected up front ([`validate`]).
//! * **Matrix layout**: `[Matrix Format]` is the default `Full`. For
//!   `N = 2` the data order is the classic `S11 S21 S12 S22`, declared
//!   explicitly as `[Two-Port Data Order] 21_12` (so a 1.0-style reader
//!   that ignores the keyword still reads it correctly). For every other
//!   `N` the matrix is row-major (`S11 S12 … S1N`, then row 2, …), each
//!   matrix row on its own line(s) of at most four complex pairs.
//!
//! Only **lumped-port** specs are supported: a wave-port S-matrix is
//! power-normalized per channel with no real reference impedance, and a
//! placeholder `[Reference]` would silently mislabel it for every
//! downstream tool. `geode eigen` has no network parameters at all.
//! Both are rejected before any solve runs.

use std::fmt::Write as _;
use std::path::Path;

use crate::error::CliError;
use crate::export::file_ref_at;
use crate::problem::Problem;
use crate::report::{FileRef, FrequencyResult, Provenance};

/// Up-front `--touchstone` validation of a loaded problem and its target
/// `path` — called before the (expensive) sweep so an unsupported spec,
/// or a target whose parent directory is missing / not a directory /
/// read-only, fails fast (an `io` error, like an unwritable `--outdir`)
/// instead of after the sweep. The writability check is best-effort
/// (the read-only permission bit); nothing is created at `path`.
pub fn validate(p: &Problem, path: &Path) -> Result<(), CliError> {
    if !p.wave_ports.is_empty() {
        return Err(unsupported(
            "wave-port specs are not supported: their S-matrix is power-normalized per mode \
             channel with no real reference impedance, so a Touchstone [Reference] line would \
             mislabel it; use the JSON report's results[].s / wave_channels[] instead",
        ));
    }
    if p.ports.is_empty() {
        return Err(unsupported(
            "the spec has no lumped ports, so there is no network to write",
        ));
    }
    let mut hz: Vec<f64> = p.frequencies.iter().map(|f| f.hz).collect();
    hz.sort_by(f64::total_cmp);
    if let Some(w) = hz.windows(2).find(|w| w[0] == w[1]) {
        return Err(unsupported(&format!(
            "the spec lists frequency {:e} Hz more than once, and a Touchstone file cannot \
             hold two rows at the same frequency; remove the duplicate",
            w[0]
        )));
    }
    check_parent_dir(path)
}

/// The `--touchstone` target's parent directory exists, is a directory
/// and is not read-only.
fn check_parent_dir(path: &Path) -> Result<(), CliError> {
    let parent = path
        .parent()
        .filter(|d| !d.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let io = |kind: std::io::ErrorKind, msg: &str| CliError::Io {
        path: path.to_path_buf(),
        err: std::io::Error::new(kind, format!("{msg} `{}`", parent.display())),
    };
    let meta = std::fs::metadata(parent).map_err(|_| {
        io(
            std::io::ErrorKind::NotFound,
            "Touchstone output's parent directory does not exist:",
        )
    })?;
    if !meta.is_dir() {
        return Err(io(
            std::io::ErrorKind::NotADirectory,
            "Touchstone output's parent is not a directory:",
        ));
    }
    if meta.permissions().readonly() {
        return Err(io(
            std::io::ErrorKind::PermissionDenied,
            "Touchstone output's parent directory is read-only:",
        ));
    }
    Ok(())
}

/// The error for a subcommand / spec that cannot produce a Touchstone file.
pub fn unsupported(reason: &str) -> CliError {
    CliError::TouchstoneUnsupported {
        reason: reason.to_string(),
    }
}

/// Render the Touchstone 2.0 text of `results` (any order; written
/// ascending) against the per-port reference resistances
/// `reference_ohm`. `comments` become leading `!` lines.
pub fn render(
    comments: &[String],
    reference_ohm: &[f64],
    results: &[FrequencyResult],
) -> Result<String, CliError> {
    let n = reference_ohm.len();
    let mut rows: Vec<&FrequencyResult> = results.iter().collect();
    rows.sort_by(|a, b| a.frequency_hz.total_cmp(&b.frequency_hz));
    if let Some(w) = rows
        .windows(2)
        .find(|w| w[0].frequency_hz == w[1].frequency_hz)
    {
        return Err(unsupported(&format!(
            "duplicate frequency {:e} Hz in the solved list",
            w[0].frequency_hz
        )));
    }
    for r in &rows {
        if r.s.len() != n || r.s.iter().any(|row| row.len() != n) {
            return Err(unsupported(&format!(
                "the S-matrix at {:e} Hz is not {n} x {n} (one row / column per lumped port)",
                r.frequency_hz
            )));
        }
    }

    let mut t = String::new();
    for c in comments {
        // `writeln!` into a `String` cannot fail.
        let _ = writeln!(t, "! {c}");
    }
    let _ = writeln!(t, "[Version] 2.0");
    let _ = writeln!(
        t,
        "# HZ S RI R {:e}",
        reference_ohm.first().copied().unwrap_or(50.0)
    );
    let _ = writeln!(t, "[Number of Ports] {n}");
    if n == 2 {
        let _ = writeln!(t, "[Two-Port Data Order] 21_12");
    }
    let _ = writeln!(t, "[Number of Frequencies] {}", rows.len());
    let refs: Vec<String> = reference_ohm.iter().map(|r| format!("{r:e}")).collect();
    let _ = writeln!(t, "[Reference] {}", refs.join(" "));
    let _ = writeln!(t, "[Network Data]");
    let pair = |c: &[f64; 2]| format!("{:e} {:e}", c[0], c[1]);
    for r in rows {
        let s = &r.s;
        let f = format!("{:e}", r.frequency_hz);
        match n {
            1 => {
                let _ = writeln!(t, "{f} {}", pair(&s[0][0]));
            }
            2 => {
                let _ = writeln!(
                    t,
                    "{f} {} {} {} {}",
                    pair(&s[0][0]),
                    pair(&s[1][0]),
                    pair(&s[0][1]),
                    pair(&s[1][1])
                );
            }
            _ => {
                for (i, row) in s.iter().enumerate() {
                    for (k, chunk) in row.chunks(4).enumerate() {
                        let body: Vec<String> = chunk.iter().map(pair).collect();
                        let lead = if i == 0 && k == 0 { f.as_str() } else { "" };
                        let sep = if lead.is_empty() { "" } else { " " };
                        let _ = writeln!(t, "{lead}{sep}{}", body.join(" "));
                    }
                }
            }
        }
    }
    let _ = writeln!(t, "[End]");
    Ok(t)
}

/// Write `results` as Touchstone 2.0 to `path` and return its
/// `{path, sha256}` reference: `path` is the `--touchstone` argument **as
/// given on the command line** (like [`Provenance::spec_path`]), `sha256`
/// the hash of the bytes on disk (read back after the write).
pub fn write(
    path: &Path,
    provenance: &Provenance,
    reference_ohm: &[f64],
    results: &[FrequencyResult],
) -> Result<FileRef, CliError> {
    let comments = vec![
        format!(
            "Touchstone 2.0 written by geode {} ({})",
            provenance.geode_version, provenance.git_sha
        ),
        format!("spec: {}", provenance.spec_path),
        "S-parameters vs per-port lumped resistance_ohm ([Reference]); RI format; Hz".to_string(),
    ];
    let text = render(&comments, reference_ohm, results)?;
    std::fs::write(path, text).map_err(|err| CliError::Io {
        path: path.to_path_buf(),
        err,
    })?;
    file_ref_at(path, path.display().to_string())
}

/// A parsed Touchstone 2.0 file (only the subset [`render`] emits).
#[cfg(test)]
#[derive(Debug, Clone, PartialEq)]
pub struct Parsed {
    /// `[Number of Ports]`.
    pub n_ports: usize,
    /// `[Reference]` values.
    pub reference_ohm: Vec<f64>,
    /// `(frequency_hz, S[row][col])` rows in file order.
    pub rows: Vec<(f64, Vec<Vec<[f64; 2]>>)>,
}

/// Parse the Touchstone subset [`render`] writes (test-only round trip).
#[cfg(test)]
pub fn parse(text: &str) -> Result<Parsed, String> {
    let mut n_ports = None;
    let mut n_freq = None;
    let mut reference_ohm = Vec::new();
    let mut order_21_12 = false;
    let mut in_data = false;
    let mut nums: Vec<f64> = Vec::new();
    let mut option_seen = false;
    for raw in text.lines() {
        let line = raw.split('!').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let num = |s: &str| s.parse::<f64>().map_err(|e| format!("{s:?}: {e}"));
        if let Some(rest) = line.strip_prefix('[') {
            let (kw, val) = rest.split_once(']').ok_or("unterminated keyword")?;
            let val = val.trim();
            match kw.to_ascii_lowercase().as_str() {
                "version" if val != "2.0" => return Err(format!("version {val}")),
                "version" => {}
                "number of ports" => {
                    n_ports = Some(val.parse::<usize>().map_err(|e| e.to_string())?)
                }
                "two-port data order" => order_21_12 = val == "21_12",
                "number of frequencies" => {
                    n_freq = Some(val.parse::<usize>().map_err(|e| e.to_string())?);
                }
                "reference" => {
                    reference_ohm = val.split_whitespace().map(num).collect::<Result<_, _>>()?;
                }
                "network data" => in_data = true,
                "end" => in_data = false,
                other => return Err(format!("unexpected keyword [{other}]")),
            }
        } else if let Some(opt) = line.strip_prefix('#') {
            let opts: Vec<String> = opt
                .split_whitespace()
                .map(str::to_ascii_uppercase)
                .collect();
            if opts.get(..3) != Some(&["HZ".to_string(), "S".into(), "RI".into()][..]) {
                return Err(format!("option line {line:?}"));
            }
            option_seen = true;
        } else if in_data {
            for tok in line.split_whitespace() {
                nums.push(num(tok)?);
            }
        } else {
            return Err(format!("stray line {line:?}"));
        }
    }
    let n = n_ports.ok_or("missing [Number of Ports]")?;
    if !option_seen || reference_ohm.len() != n {
        return Err("missing option line or wrong [Reference] count".into());
    }
    let per = 1 + 2 * n * n;
    if !nums.len().is_multiple_of(per) {
        return Err(format!("{} numbers is not a multiple of {per}", nums.len()));
    }
    let rows: Vec<(f64, Vec<Vec<[f64; 2]>>)> = nums
        .chunks(per)
        .map(|c| {
            let mut s = vec![vec![[0.0; 2]; n]; n];
            for k in 0..n * n {
                let (mut i, mut j) = (k / n, k % n);
                if n == 2 && order_21_12 {
                    // Column-major for the classic two-port order.
                    (i, j) = (j, i);
                }
                s[i][j] = [c[1 + 2 * k], c[2 + 2 * k]];
            }
            (c[0], s)
        })
        .collect();
    if n_freq != Some(rows.len()) {
        return Err(format!(
            "[Number of Frequencies] {n_freq:?} vs {} rows",
            rows.len()
        ));
    }
    Ok(Parsed {
        n_ports: n,
        reference_ohm,
        rows,
    })
}

#[cfg(test)]
#[path = "../tests/support/skrf.rs"]
mod skrf_support;

#[cfg(test)]
mod tests {
    use super::*;

    fn row(f: f64, s: Vec<Vec<[f64; 2]>>) -> FrequencyResult {
        FrequencyResult {
            frequency_hz: f,
            k0: 0.0,
            omega_rad_s: 0.0,
            residual_rel: 0.0,
            iterations: Vec::new(),
            z_ohm: Vec::new(),
            y_s: None,
            s,
            ports: Vec::new(),
            wave_channels: Vec::new(),
            field_file: None,
            far_field: None,
        }
    }

    #[test]
    fn one_port_exact_bytes() {
        let rows = [
            row(2.5e9, vec![vec![[0.5, -0.25]]]),
            row(1e9, vec![vec![[-0.1, 1.0 / 3.0]]]),
        ];
        let t = render(&["hi".into()], &[50.0], &rows).unwrap();
        assert_eq!(
            t,
            "! hi\n[Version] 2.0\n# HZ S RI R 5e1\n[Number of Ports] 1\n\
             [Number of Frequencies] 2\n[Reference] 5e1\n[Network Data]\n\
             1e9 -1e-1 3.333333333333333e-1\n2.5e9 5e-1 -2.5e-1\n[End]\n"
        );
        let p = parse(&t).unwrap();
        assert_eq!(p.n_ports, 1);
        assert_eq!(p.reference_ohm, [50.0]);
        // Sorted ascending and bit-exact.
        assert_eq!(p.rows[0], (1e9, vec![vec![[-0.1, 1.0 / 3.0]]]));
        assert_eq!(p.rows[1], (2.5e9, vec![vec![[0.5, -0.25]]]));
    }

    #[test]
    fn two_port_unequal_references_classic_order() {
        let s = vec![
            vec![[0.11, 0.12], [0.13, 0.14]],
            vec![[0.21, 0.22], [0.23, 0.24]],
        ];
        let t = render(&[], &[50.0, 75.0], &[row(1e9, s.clone())]).unwrap();
        assert!(t.contains("[Two-Port Data Order] 21_12\n"));
        assert!(t.contains("[Reference] 5e1 7.5e1\n"));
        // S11 S21 S12 S22.
        assert!(t.contains("\n1e9 1.1e-1 1.2e-1 2.1e-1 2.2e-1 1.3e-1 1.4e-1 2.3e-1 2.4e-1\n"));
        let p = parse(&t).unwrap();
        assert_eq!(p.reference_ohm, [50.0, 75.0]);
        assert_eq!(p.rows, vec![(1e9, s)]);
    }

    #[test]
    fn five_port_row_major_wraps_at_four_pairs() {
        let n = 5;
        let s: Vec<Vec<[f64; 2]>> = (0..n)
            .map(|i| (0..n).map(|j| [i as f64, j as f64 + 0.5]).collect())
            .collect();
        let rs = vec![row(3e9, s.clone()), row(1e9, s.clone())];
        let t = render(&[], &[10.0, 20.0, 30.0, 40.0, 50.0], &rs).unwrap();
        assert!(!t.contains("Two-Port"));
        let data: Vec<&str> = t
            .lines()
            .skip_while(|l| *l != "[Network Data]")
            .skip(1)
            .take_while(|l| *l != "[End]")
            .collect();
        // Two lines per matrix row (4 + 1 pairs), 5 rows, 2 frequencies.
        assert_eq!(data.len(), 20);
        assert!(data[0].starts_with("1e9 0e0 5e-1 0e0 1.5e0"));
        assert!(data.iter().all(|l| l.split_whitespace().count() <= 9));
        let p = parse(&t).unwrap();
        assert_eq!(p.rows, vec![(1e9, s.clone()), (3e9, s)]);
    }

    /// Optional scikit-rf read of `render`'s 2-port (classic `21_12`
    /// order, unequal references) and 5-port (row-major, wrapped) output
    /// (issue #713); skipped loudly without Python + scikit-rf.
    #[test]
    fn scikit_rf_reads_two_and_five_port_files() {
        let dir = std::env::temp_dir().join(format!("geode-skrf-unit-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let s2 = |f: f64| {
            vec![
                vec![[0.11 * f, -0.12], [0.13, 1.0 / 3.0]],
                vec![[-0.21, 0.22 * f], [0.23, -0.24]],
            ]
        };
        let refs2 = [50.0, 75.0];
        let rows2 = [(1e9, s2(1.0)), (2.5e9, s2(2.0))];
        let rs: Vec<_> = rows2
            .iter()
            .rev()
            .map(|(f, s)| row(*f, s.clone()))
            .collect();
        let p2 = dir.join("two.s2p");
        std::fs::write(&p2, render(&["2-port".into()], &refs2, &rs).unwrap()).unwrap();
        let n = 5;
        let s5 = |f: f64| -> Vec<Vec<[f64; 2]>> {
            (0..n)
                .map(|i| {
                    (0..n)
                        .map(|j| [0.01 * (i * n + j) as f64 * f, -(j as f64) / 7.0])
                        .collect()
                })
                .collect()
        };
        let refs5 = [10.0, 20.0, 30.0, 40.0, 50.0];
        let rows5 = [(1e9, s5(1.0)), (3e9, s5(3.0))];
        let rs: Vec<_> = rows5.iter().map(|(f, s)| row(*f, s.clone())).collect();
        let p5 = dir.join("five.s5p");
        std::fs::write(&p5, render(&[], &refs5, &rs).unwrap()).unwrap();
        skrf_support::check("render 2-port", &p2, &refs2, &rows2);
        skrf_support::check("render 5-port", &p5, &refs5, &rows5);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn duplicate_or_misshapen_rows_are_rejected() {
        let one = vec![vec![[0.0, 0.0]]];
        let e = render(
            &[],
            &[50.0],
            &[row(1e9, one.clone()), row(1e9, one.clone())],
        );
        assert!(matches!(e, Err(CliError::TouchstoneUnsupported { .. })));
        let e = render(&[], &[50.0, 50.0], &[row(1e9, one)]);
        assert!(matches!(e, Err(CliError::TouchstoneUnsupported { .. })));
    }
}
