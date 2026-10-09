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
//! * **Parameter**: `S` only. For a lumped-port spec, the report's
//!   `results[].s`, referenced to each port's `resistance_ohm`; for a
//!   wave-port or mixed spec, its renormalization (below). `Z` / `Y`
//!   export is a non-goal of this phase.
//! * **Format**: `RI` — for a lumped-port spec the report's `[re, im]`
//!   pairs copied verbatim, no polar conversion.
//! * **Frequency unit**: `HZ` — the report's `frequency_hz`, no rescaling.
//! * **Numbers** use Rust's shortest round-trip `{:e}` formatting, so
//!   parsing a lumped-port file back yields the report's `f64` values bit
//!   for bit.
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
//! # Wave-port and mixed specs (issue #775)
//!
//! A wave channel's S in the JSON report (`results[].s`) is the **modal**
//! S: each channel is a transmission line whose voltage is the amplitude
//! of its unit-norm transverse mode and whose characteristic impedance is
//! the mode's wave impedance `Z_c = Z_TE(ω) = η₀·k₀·μ_t/β` (ohms; the
//! traveling-wave convention `a = V⁺/√Z_c`, principal root, exactly the
//! solver's `√(y/ω)` weight). `Z_TE` varies with frequency, so the file
//! renormalizes every **written** wave channel to its port's constant real
//! `wave_ports[].reference_ohm` (required with `--touchstone`, no
//! default); lumped ports keep `resistance_ohm` (for them `Z_c = R`, so
//! the transform is the identity). [`renormalize`] is the exact Γ-form
//! `S' = A⁻¹(S − Γ)(I − ΓS)⁻¹A` with `Γ = diag((R − Z_c)/(R + Z_c))`,
//! `A = diag(2√R·√Z_c/(R + Z_c))`, which never inverts `I − S`.
//!
//! A waveguide mode has no unique characteristic impedance: `Z_TE` (this
//! file, and openEMS's `RefImpedance` renormalization) differs from
//! HFSS's `Zpi` / `Zpv` / `Zvi` by an ideal transformer per port, so a
//! renormalized `|S|` is **convention-dependent**. The modal JSON S is the
//! cross-tool comparable quantity.
//!
//! * **Port set.** One Touchstone port per *kept* channel, in JSON channel
//!   order (lumped ports first, then wave ports port-major, mode-minor),
//!   named by `! Port[k] = …` comment lines. [`validate`] classifies each
//!   wave channel from `β(k₀)` at every sweep frequency with the report's
//!   own predicate `Re β > |Im β|`: a channel evanescent at **every**
//!   frequency is excluded (the modal sub-block is taken *before*
//!   renormalizing, so the excluded mode stays terminated in its own
//!   modal impedance — the semi-infinite guide the solver models); a
//!   channel that crosses its cutoff inside the sweep is rejected (a
//!   Touchstone file cannot change its port count). `β` is recomputed
//!   analytically from the port's `k_c` and fill at each row, bit for bit
//!   the solver's.
//! * **Comments.** No comment line may be read as data by scikit-rf, which
//!   lets HFSS-style `! Port Impedance` / `! Gamma` comments override
//!   `[Reference]`; see [`RESERVED_COMMENT_PREFIXES`]. The per-frequency
//!   `Z_TE` is therefore not written (it is recoverable from the JSON
//!   report's `wave_channels[].beta`, `k0` and `wave_ports[].medium`).
//! * **Pure-lumped** files are byte-identical to the pre-#775 writer.
//!
//! `geode eigen` has no network parameters at all and is rejected.

use std::fmt::Write as _;
use std::path::Path;

use faer::c64;
use geode_core::constants::ETA_0_OHM;
use geode_core::driven::ports::PortMedium;

use crate::error::CliError;
use crate::export::file_ref_at;
use crate::problem::Problem;
use crate::report::{Complex, FileRef, FrequencyResult, Provenance};

/// Comment-line prefixes (lower-cased, after `!` and leading blanks) that
/// scikit-rf's Touchstone reader treats as data rather than commentary —
/// `! Port Impedance` / `! Gamma` take precedence over `[Reference]`, and
/// the `Modal` / `Terminal` export banners switch its `s_def`. No comment
/// a wave or mixed `--touchstone` file writes may start with one.
pub const RESERVED_COMMENT_PREFIXES: [&str; 4] = [
    "port impedance",
    "gamma",
    "modal data exported",
    "terminal data exported",
];

/// Substrings no written comment may contain: scikit-rf reads
/// `S-parameter uses the` as an `s_def` declaration and `::` as a
/// Sigrity-style port name.
pub const RESERVED_COMMENT_SUBSTRINGS: [&str; 2] = ["S-parameter uses the", "::"];

/// The reserved prefix or substring `comment` (the text after `! `)
/// carries, if any ([`RESERVED_COMMENT_PREFIXES`],
/// [`RESERVED_COMMENT_SUBSTRINGS`]).
pub fn reserved_in_comment(comment: &str) -> Option<&'static str> {
    let lead = comment.trim_start().to_ascii_lowercase();
    RESERVED_COMMENT_PREFIXES
        .into_iter()
        .find(|k| lead.starts_with(k))
        .or_else(|| {
            RESERVED_COMMENT_SUBSTRINGS
                .into_iter()
                .find(|k| comment.contains(k))
        })
}

/// One wave channel of a `--touchstone` run ([`Plan`]).
#[derive(Debug, Clone, PartialEq)]
pub struct WaveChannelPlan {
    /// Wave-port index (spec order).
    pub port: usize,
    /// Mode index within the port.
    pub mode: usize,
    /// JSON channel index (lumped ports first).
    pub channel: usize,
    /// Geometric cutoff wavenumber of the mode (mesh units); `0` (unused)
    /// for a hybrid channel.
    pub k_c: f64,
    /// A hybrid port's channel (issue #807): its `Z_c` is the line
    /// impedance `wave_channels[].hybrid.z_line_ohm` of each report row, not
    /// `Z_TE`.
    pub hybrid: bool,
}

/// What a `--touchstone` file will hold, decided by [`validate`] before the
/// sweep: empty for a lumped-only spec.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Plan {
    /// Wave channels written as Touchstone ports, in JSON channel order.
    pub kept: Vec<WaveChannelPlan>,
    /// Wave channels evanescent at every sweep frequency (not written).
    pub excluded: Vec<WaveChannelPlan>,
}

/// Up-front `--touchstone` validation of a loaded problem and its target
/// `path` — called before the (expensive) sweep so an unsupported spec,
/// or a target whose parent directory is missing / not a directory /
/// read-only, fails fast (an `io` error, like an unwritable `--outdir`)
/// instead of after the sweep. The writability check is best-effort
/// (the read-only permission bit); nothing is created at `path`.
///
/// For a spec with wave ports this runs each port's (cheap, 2-D) modal
/// solve to classify its channels ([`Plan`]): a channel that crosses its
/// cutoff inside the sweep, a wave port without `reference_ohm`, or a
/// spec left with no port to write is `invalid_spec`. Excluded
/// (always-evanescent) channels are noted on stderr.
pub fn validate(p: &Problem, path: &Path) -> Result<Plan, CliError> {
    let plan = if p.wave_ports.is_empty() {
        if p.ports.is_empty() {
            return Err(unsupported(
                "the spec has no lumped ports, so there is no network to write",
            ));
        }
        Plan::default()
    } else {
        classify(p)?
    };
    let mut hz: Vec<f64> = p.frequencies.iter().map(|f| f.hz).collect();
    hz.sort_by(f64::total_cmp);
    if let Some(w) = hz.windows(2).find(|w| w[0] == w[1]) {
        return Err(unsupported(&format!(
            "the spec lists frequency {:e} Hz more than once, and a Touchstone file cannot \
             hold two rows at the same frequency; remove the duplicate",
            w[0]
        )));
    }
    check_not_dir(path)?;
    check_parent_dir(path)?;
    for c in &plan.excluded {
        eprintln!(
            "note: --touchstone: {} is evanescent at every sweep frequency and is not written \
             (it stays terminated in its own modal impedance; see the JSON report's \
             wave_channels[])",
            channel_label(p, c)
        );
    }
    Ok(plan)
}

/// Whether a channel of propagation constant `beta` propagates — the
/// report's `wave_channels[].propagating` predicate, so the file and the
/// JSON can never disagree.
fn propagating(beta: c64) -> bool {
    beta.re > beta.im.abs()
}

/// `(Z_c, √Z_c)` in ohms of a mode of cutoff `k_c` in `medium` at `k0`:
/// `Z_c = η₀·k₀/y`, `y = β/μ_t`, with `√Z_c = √η₀·√k₀/√y` built from the
/// solver's own principal `√y` (its S weight is `√y/√ω`).
fn modal_impedance(medium: &PortMedium, k0: f64, k_c: f64) -> (c64, c64) {
    let y = medium.admittance(medium.beta(k0, k_c));
    let z = c64::new(ETA_0_OHM * k0, 0.0) / y;
    let sqrt_z = c64::new((ETA_0_OHM * k0).sqrt(), 0.0) / y.sqrt();
    (z, sqrt_z)
}

/// `"wave port <group> mode <m> (JSON channel <c>)"`.
fn channel_label(p: &Problem, c: &WaveChannelPlan) -> String {
    format!(
        "wave port {} mode {} (JSON channel {})",
        p.wave_ports[c.port].surface.name, c.mode, c.channel
    )
}

/// Classify every wave channel over the sweep and require the references.
fn classify(p: &Problem) -> Result<Plan, CliError> {
    let hz_per_k0 =
        crate::problem::to_frequency(1.0, crate::spec::FrequencyUnit::K0, p.length_unit_m()).hz;
    let mut plan = Plan::default();
    // Per wave port: the Z_TE (geometric) or Z_line (hybrid) range of its
    // kept channels (for the missing-reference message).
    let mut z_ranges: Vec<Vec<(usize, f64, f64)>> = vec![Vec::new(); p.wave_ports.len()];
    let mut channel = p.ports.len();
    for (port, w) in p.wave_ports.iter().enumerate() {
        if let Some(h) = &w.hybrid {
            channel = classify_hybrid(p, port, h, channel, &mut plan, &mut z_ranges[port])?;
            continue;
        }
        let wp = w
            .projection
            .wave_port(&p.edges, &w.a_inc)
            .map_err(|err| CliError::WavePort {
                name: w.surface.name.clone(),
                err,
            })?;
        for (mode, m) in wp.modes.iter().enumerate() {
            let c = WaveChannelPlan {
                port,
                mode,
                channel,
                k_c: m.k_c,
                hybrid: false,
            };
            channel += 1;
            let props: Vec<bool> = p
                .frequencies
                .iter()
                .map(|f| propagating(p.port_medium_at(w, f.hz).beta(f.k0, m.k_c)))
                .collect();
            if props.iter().all(|&b| b) {
                let (lo, hi) = p
                    .frequencies
                    .iter()
                    .map(|f| {
                        modal_impedance(&p.port_medium_at(w, f.hz), f.k0, m.k_c)
                            .0
                            .re
                    })
                    .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), z| {
                        (lo.min(z), hi.max(z))
                    });
                z_ranges[port].push((mode, lo, hi));
                plan.kept.push(c);
            } else if props.iter().any(|&b| b) {
                let first_hz = p.frequencies.first().map_or(0.0, |f| f.hz);
                let cutoff_hz = p.port_medium_at(w, first_hz).cutoff_k0(m.k_c) * hz_per_k0;
                return Err(unsupported(&format!(
                    "{} crosses its cutoff (≈ {cutoff_hz:.6e} Hz in its fill) inside the sweep: \
                     it propagates at some frequencies and is evanescent at others, and a \
                     Touchstone file cannot change its port count; split the sweep at the \
                     cutoff or lower wave_ports[{}].n_modes",
                    channel_label(p, &c),
                    w.surface.name
                )));
            } else {
                plan.excluded.push(c);
            }
        }
    }
    let missing: Vec<String> = p
        .wave_ports
        .iter()
        .zip(&z_ranges)
        .filter(|(w, _)| w.reference_ohm.is_none())
        .map(|(w, zs)| {
            let name = &w.surface.name;
            let z = if w.hybrid.is_some() { "Z_line" } else { "Z_TE" };
            if zs.is_empty() {
                format!("wave_ports[{name}] (no propagating mode in this sweep)")
            } else {
                let modes: Vec<String> = zs
                    .iter()
                    .map(|(m, lo, hi)| format!("mode {m} Re {z} {lo:.4e}..{hi:.4e} Ω"))
                    .collect();
                format!("wave_ports[{name}] ({})", modes.join(", "))
            }
        })
        .collect();
    if !missing.is_empty() {
        return Err(unsupported(&format!(
            "wave_ports[].reference_ohm is required with --touchstone and has no default: a \
             wave channel's S is referenced to its own modal wave impedance Z_TE(ω) = \
             η₀·k₀·μ_t/β (the modal Z_c convention), which varies with frequency, so the file \
             renormalizes each written mode to a constant real reference you choose; missing \
             on {} (Z_TE over this sweep shown; a reference far from Z_TE makes a matched guide \
             look mismatched, which is the physically correct renormalized result{})",
            missing.join("; "),
            if p.wave_ports.iter().any(|w| w.hybrid.is_some()) {
                "; a hybrid port's channels are referenced to their line impedance Z_line under \
                 wave_ports[].impedance_definition instead (default power_current, Z_PI), shown \
                 as Z_line"
            } else {
                ""
            }
        )));
    }
    if p.ports.is_empty() && plan.kept.is_empty() {
        return Err(unsupported(
            "every wave channel is evanescent at every sweep frequency and the spec has no \
             lumped ports, so there is no network to write",
        ));
    }
    Ok(plan)
}
/// Classify hybrid port `port`'s channels (issue #807) from its face sweep
/// over the spec's frequencies (no 3-D solve; [`crate::hybrid::face_sweep`]),
/// with the geometric rule: kept when propagating at every frequency,
/// excluded when evanescent at every one, `invalid` when it crosses. A
/// hybrid port needs a line impedance (a floating conductor). Returns the
/// next channel index.
fn classify_hybrid(
    p: &Problem,
    port: usize,
    h: &crate::problem::HybridPortDef,
    mut channel: usize,
    plan: &mut Plan,
    z_range: &mut Vec<(usize, f64, f64)>,
) -> Result<usize, CliError> {
    let w = &p.wave_ports[port];
    let Some(def) = h.impedance_definition else {
        return Err(unsupported(&format!(
            "hybrid wave port `{}` has no floating conductor on its face (an inhomogeneously \
             filled waveguide), so its modes have no line impedance to renormalize to, and the \
             TE wave impedance is not offered for hybrid ports; use the JSON report's modal \
             results[].s for this port",
            w.surface.name
        )));
    };
    let sweep = crate::hybrid::face_sweep(p, port, false)?;
    for mode in 0..w.a_inc.len() {
        let c = WaveChannelPlan {
            port,
            mode,
            channel,
            k_c: 0.0,
            hybrid: true,
        };
        channel += 1;
        let chans: Vec<&geode_core::driven::ports::HybridChannelReport> = sweep
            .report
            .points
            .iter()
            .map(|pt| &pt.channels[mode])
            .collect();
        let props: Vec<bool> = chans.iter().map(|ch| propagating(ch.beta)).collect();
        if props.iter().all(|&b| b) {
            // A waveguide (TE / TM) mode with no net conductor current has
            // no line impedance to renormalize to (#953).
            if chans.iter().any(|ch| {
                crate::hybrid::channel_result(h, ch)
                    .line
                    .is_some_and(|l| l.no_net_current)
            }) {
                return Err(unsupported(&format!(
                    "{} carries no net conductor current (its conductor currents cancel to \
                     round-off: a TE/TM waveguide mode of the port face, not a line mode), so it \
                     has no line impedance to renormalize to; a propagating mode must be a \
                     channel, so keep the sweep below its cutoff for a Touchstone file, or use \
                     the JSON report's modal results[].s",
                    channel_label(p, &c)
                )));
            }
            let zs: Vec<f64> = chans
                .iter()
                .filter_map(|ch| crate::hybrid::channel_result(h, ch).z_line_ohm)
                .map(|z| z[0])
                .collect();
            if zs.len() != chans.len() {
                return Err(unsupported(&format!(
                    "{} has no {} line impedance at every sweep frequency (a voltage path is \
                     missing); use impedance_definition = \"power_current\"",
                    channel_label(p, &c),
                    def.name()
                )));
            }
            let (lo, hi) = zs
                .iter()
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), &z| {
                    (lo.min(z), hi.max(z))
                });
            z_range.push((mode, lo, hi));
            plan.kept.push(c);
        } else if props.iter().any(|&b| b) {
            return Err(unsupported(&format!(
                "{} crosses its cutoff inside the sweep: it propagates at some frequencies and \
                 is evanescent at others, and a Touchstone file cannot change its port count; \
                 split the sweep at the cutoff or lower wave_ports[{}].n_modes",
                channel_label(p, &c),
                w.surface.name
            )));
        } else {
            plan.excluded.push(c);
        }
    }
    Ok(channel)
}
/// The `--touchstone` target `path` itself is not an existing directory
/// (rejecting it fast, before any solve, instead of only at the final
/// `std::fs::write` after the frequency sweep).
fn check_not_dir(path: &Path) -> Result<(), CliError> {
    if path.is_dir() {
        return Err(CliError::Io {
            path: path.to_path_buf(),
            err: std::io::Error::new(
                std::io::ErrorKind::IsADirectory,
                format!(
                    "Touchstone output path is an existing directory, not a file: `{}`",
                    path.display()
                ),
            ),
        });
    }
    Ok(())
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
    let meta = std::fs::metadata(parent).map_err(|e| {
        if e.kind() == std::io::ErrorKind::PermissionDenied {
            io(
                std::io::ErrorKind::PermissionDenied,
                "Touchstone output's parent directory is not accessible (permission denied):",
            )
        } else {
            io(
                std::io::ErrorKind::NotFound,
                "Touchstone output's parent directory does not exist:",
            )
        }
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

/// One `(frequency_hz, S[row][col])` row of a network to write.
pub type Row = (f64, Vec<Vec<Complex>>);

/// Render the Touchstone 2.0 text of `results` (any order; written
/// ascending) against the per-port reference resistances
/// `reference_ohm`. `comments` become leading `!` lines.
pub fn render(
    comments: &[String],
    reference_ohm: &[f64],
    results: &[FrequencyResult],
) -> Result<String, CliError> {
    let rows: Vec<Row> = results
        .iter()
        .map(|r| (r.frequency_hz, r.s.clone()))
        .collect();
    render_rows(comments, reference_ohm, rows)
}

/// [`render`] over prepared `(frequency_hz, S)` rows (any order; written
/// ascending).
pub fn render_rows(
    comments: &[String],
    reference_ohm: &[f64],
    mut rows: Vec<Row>,
) -> Result<String, CliError> {
    let n = reference_ohm.len();
    rows.sort_by(|a, b| a.0.total_cmp(&b.0));
    if let Some(w) = rows.windows(2).find(|w| w[0].0 == w[1].0) {
        return Err(unsupported(&format!(
            "duplicate frequency {:e} Hz in the solved list",
            w[0].0
        )));
    }
    for (f, s) in &rows {
        if s.len() != n || s.iter().any(|row| row.len() != n) {
            return Err(unsupported(&format!(
                "the S-matrix at {f:e} Hz is not {n} x {n} (one row / column per port)"
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
    for (freq, s) in &rows {
        let f = format!("{freq:e}");
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

/// The two provenance comment lines every file starts with.
fn provenance_comments(provenance: &Provenance) -> Vec<String> {
    vec![
        format!(
            "Touchstone 2.0 written by geode {} ({})",
            provenance.geode_version, provenance.git_sha
        ),
        format!("spec: {}", provenance.spec_path),
    ]
}

/// Write `text` to `path` and return its `{path, sha256}` reference.
fn write_text(path: &Path, text: String) -> Result<FileRef, CliError> {
    std::fs::write(path, text).map_err(|err| CliError::Io {
        path: path.to_path_buf(),
        err,
    })?;
    file_ref_at(path, path.display().to_string())
}

/// Write a **lumped-port** spec's `results` as Touchstone 2.0 to `path`
/// and return its `{path, sha256}` reference: `path` is the
/// `--touchstone` argument **as given on the command line** (like
/// [`Provenance::spec_path`]), `sha256` the hash of the bytes on disk
/// (read back after the write).
pub fn write(
    path: &Path,
    provenance: &Provenance,
    reference_ohm: &[f64],
    results: &[FrequencyResult],
) -> Result<FileRef, CliError> {
    let mut comments = provenance_comments(provenance);
    comments.push(
        "S-parameters vs per-port lumped resistance_ohm ([Reference]); RI format; Hz".to_string(),
    );
    write_text(path, render(&comments, reference_ohm, results)?)
}

/// [`write()`] for a spec with wave ports: the kept channels of `plan`
/// (with every lumped port) renormalized by [`wave_network`].
pub fn write_wave(
    path: &Path,
    provenance: &Provenance,
    p: &Problem,
    plan: &Plan,
    results: &[FrequencyResult],
) -> Result<FileRef, CliError> {
    let net = wave_network(p, plan, provenance_comments(provenance), results)?;
    write_text(
        path,
        render_rows(&net.comments, &net.reference_ohm, net.rows)?,
    )
}

/// Make a comment line safe for scikit-rf: collapse every `::` (a
/// Sigrity-style port-name marker to scikit-rf), e.g. from a physical
/// group or spec path.
fn sanitize(c: String) -> String {
    let mut c = c;
    while c.contains("::") {
        c = c.replace("::", ":");
    }
    c
}

/// A wave-port / mixed network ready to [`render_rows`].
#[derive(Debug, Clone)]
pub struct WaveNetwork {
    /// Leading `!` comment lines.
    pub comments: Vec<String>,
    /// `[Reference]` values, one per written port.
    pub reference_ohm: Vec<f64>,
    /// Renormalized `(frequency_hz, S)` rows.
    pub rows: Vec<Row>,
}

/// The comment lines, `[Reference]` values and renormalized rows of a
/// wave-port or mixed spec's Touchstone file: every lumped port (its
/// `resistance_ohm`), then each of `plan`'s kept channels (its port's
/// `reference_ohm`). Per row, the modal S sub-block of the kept ports is
/// taken first, then [`renormalize`]d with `Z_c` recomputed analytically
/// from the channel's `k_c` and the port fill at the row's frequency.
pub fn wave_network(
    p: &Problem,
    plan: &Plan,
    head: Vec<String>,
    results: &[FrequencyResult],
) -> Result<WaveNetwork, CliError> {
    let n_lumped = p.ports.len();
    let wave_ref = |c: &WaveChannelPlan| {
        p.wave_ports[c.port].reference_ohm.ok_or_else(|| {
            unsupported(&format!(
                "wave_ports[{}].reference_ohm is required",
                p.wave_ports[c.port].surface.name
            ))
        })
    };
    let mut refs: Vec<f64> = p.ports.iter().map(|q| q.resistance_ohm).collect();
    for c in &plan.kept {
        refs.push(wave_ref(c)?);
    }
    let keep: Vec<usize> = (0..n_lumped)
        .chain(plan.kept.iter().map(|c| c.channel))
        .collect();

    let lumped: Vec<&str> = p.ports.iter().map(|q| q.surface.name.as_str()).collect();
    let kept: Vec<String> = plan.kept.iter().map(|c| channel_label(p, c)).collect();
    let excluded: Vec<String> = plan.excluded.iter().map(|c| channel_label(p, c)).collect();
    let any_hybrid = plan.kept.iter().any(|c| c.hybrid);
    let any_geometric = plan.kept.iter().any(|c| !c.hybrid);
    let comments = wave_comments(head, &lumped, &kept, &excluded, any_hybrid, any_geometric)?;

    let mut rows = Vec::with_capacity(results.len());
    for r in results {
        let n_all = r.s.len();
        if let Some(&bad) = keep.iter().find(|&&c| c >= n_all) {
            return Err(unsupported(&format!(
                "channel {bad} is outside the {n_all} x {n_all} S-matrix at {:e} Hz",
                r.frequency_hz
            )));
        }
        let n = keep.len();
        let s: Vec<c64> = keep
            .iter()
            .flat_map(|&i| keep.iter().map(move |&j| (i, j)))
            .map(|(i, j)| {
                let [re, im] = r.s[i][j];
                c64::new(re, im)
            })
            .collect();
        let mut z = vec![None; n];
        for (k, c) in plan.kept.iter().enumerate() {
            if c.hybrid {
                z[n_lumped + k] = Some(hybrid_line_impedance(p, c, r)?);
                continue;
            }
            let medium = p.port_medium_at(&p.wave_ports[c.port], r.frequency_hz);
            if !propagating(medium.beta(r.k0, c.k_c)) {
                return Err(unsupported(&format!(
                    "{} is not propagating at {:e} Hz, but it was classified as propagating \
                     over the sweep",
                    channel_label(p, c),
                    r.frequency_hz
                )));
            }
            z[n_lumped + k] = Some(modal_impedance(&medium, r.k0, c.k_c));
        }
        let renormalized = renormalize(&s, &z, &refs).ok_or_else(|| {
            unsupported(&format!(
                "the renormalization of the S-matrix at {:e} Hz is singular",
                r.frequency_hz
            ))
        })?;
        let matrix = renormalized
            .chunks(n)
            .map(|row| row.iter().map(|z| [z.re, z.im]).collect())
            .collect();
        rows.push((r.frequency_hz, matrix));
    }
    Ok(WaveNetwork {
        comments,
        reference_ohm: refs,
        rows,
    })
}

/// `(Z_c, √Z_c)` of a kept **hybrid** channel at report row `r` (issue
/// #807): its line impedance under the port's `impedance_definition`
/// (`wave_channels[].hybrid.z_line_ohm`; complex on a lossy face), with the
/// principal root (`Re Z_c > 0`).
fn hybrid_line_impedance(
    p: &Problem,
    c: &WaveChannelPlan,
    r: &FrequencyResult,
) -> Result<(c64, c64), CliError> {
    let ch = r
        .wave_channels
        .iter()
        .find(|ch| ch.channel == c.channel)
        .filter(|ch| ch.propagating)
        .ok_or_else(|| {
            unsupported(&format!(
                "{} is not propagating at {:e} Hz, but it was classified as propagating over the \
                 sweep",
                channel_label(p, c),
                r.frequency_hz
            ))
        })?;
    let [re, im] = ch
        .hybrid
        .as_ref()
        .and_then(|h| h.z_line_ohm)
        .ok_or_else(|| {
            unsupported(&format!(
                "{} has no line impedance at {:e} Hz",
                channel_label(p, c),
                r.frequency_hz
            ))
        })?;
    let z = c64::new(re, im);
    if !(z.re > 0.0 && z.re.is_finite() && z.im.is_finite()) {
        return Err(unsupported(&format!(
            "{} has the line impedance {z} Ω at {:e} Hz (Re Z must be finite and > 0 to \
             renormalize)",
            channel_label(p, c),
            r.frequency_hz
        )));
    }
    Ok((z, z.sqrt()))
}

/// The comment lines of a wave-port / mixed file after the provenance
/// `head`: the renormalization and convention notes, one `Excluded:` line
/// per always-evanescent channel, then one `Port[k] = <label>` line per
/// written port (`lumped` group names, then the `kept` channel labels).
/// Every line is [`sanitize`]d and must be free of the scikit-rf keywords
/// ([`reserved_in_comment`]); one that is not (only possible through a
/// physical group or spec path) is `invalid_spec`.
fn wave_comments(
    head: Vec<String>,
    lumped: &[&str],
    kept: &[String],
    excluded: &[String],
    hybrid: bool,
    geometric: bool,
) -> Result<Vec<String>, CliError> {
    let mut comments = head;
    if !hybrid {
        comments.push(
            "S renormalized: lumped vs resistance_ohm, wave channels vs wave_ports[].reference_ohm \
             (modal V = unit-norm modal amplitude, Z_c = Z_TE = eta0*k0*mu_t/beta); RI; Hz"
                .to_string(),
        );
        comments.push(
            "Convention: Z_c = Z_TE (as openEMS RefImpedance); HFSS Zpi/Zpv/Zvi differ by an \
             ideal transformer, so renormalized |S| is convention-dependent; the JSON report's \
             modal results[].s is the cross-tool comparable quantity"
                .to_string(),
        );
    } else {
        comments.push(
            "S renormalized: lumped vs resistance_ohm, wave channels vs wave_ports[].reference_ohm; \
             RI; Hz"
                .to_string(),
        );
        comments.push(
            "Hybrid wave channels (microstrip / stripline / inhomogeneous): Z_c = the channel's \
             line impedance under wave_ports[].impedance_definition (default power_current = \
             Z_PI = 2P/|I|^2, contour-independent; never Z_TE), per frequency, complex on a lossy \
             face: the JSON report's wave_channels[].hybrid.z_line_ohm"
                .to_string(),
        );
        if geometric {
            comments.push(
                "Geometric wave channels: Z_c = Z_TE = eta0*k0*mu_t/beta (as openEMS \
                 RefImpedance)"
                    .to_string(),
            );
        }
    }
    for c in excluded {
        comments.push(format!(
            "Excluded: {c} is evanescent at every frequency (terminated in its own modal \
             impedance, not written)"
        ));
    }
    for (k, name) in lumped.iter().enumerate() {
        comments.push(format!(
            "Port[{}] = lumped {name} (JSON channel {k})",
            k + 1
        ));
    }
    for (k, c) in kept.iter().enumerate() {
        comments.push(format!("Port[{}] = {c}", lumped.len() + k + 1));
    }
    let comments: Vec<String> = comments.into_iter().map(sanitize).collect();
    if let Some((c, k)) = comments
        .iter()
        .find_map(|c| reserved_in_comment(c).map(|k| (c, k)))
    {
        return Err(unsupported(&format!(
            "the Touchstone comment {c:?} would carry the scikit-rf keyword {k:?}, which a \
             reader could take as data; rename the physical group or spec path"
        )));
    }
    Ok(comments)
}

/// Exact renormalization of the row-major `n × n` S-matrix `s` from each
/// port's own `Z_c` to the real `reference_ohm`: port `k` with
/// `z[k] = Some((Z_c, √Z_c))` (a modal channel, traveling-wave
/// `a = V⁺/√Z_c`) is renormalized, a port with `None` is already
/// referenced to `reference_ohm[k]` (a lumped port; `Γ = 0`, `A = 1`).
///
/// The Γ-form `S' = A⁻¹(S − Γ)(I − ΓS)⁻¹A`, with
/// `Γ_k = (R_k − Z_k)/(R_k + Z_k)` and `A_k = 2√R_k·√Z_k/(R_k + Z_k)`,
/// equals the impedance route `Z = √Z_c(I − S)⁻¹(I + S)√Z_c`,
/// `S' = √R⁻¹(Z − R)(Z + R)⁻¹√R` but never inverts `I − S` (singular at a
/// lossless resonance); `I − ΓS` is invertible for a passive `S` and
/// `Re Z_c > 0`. `None` if it is numerically singular.
pub fn renormalize(s: &[c64], z: &[Option<(c64, c64)>], reference_ohm: &[f64]) -> Option<Vec<c64>> {
    let n = reference_ohm.len();
    let one = c64::new(1.0, 0.0);
    let (gamma, a): (Vec<c64>, Vec<c64>) = z
        .iter()
        .zip(reference_ohm)
        .map(|(z, &r)| match z {
            None => (c64::new(0.0, 0.0), one),
            Some((zc, sqrt_zc)) => {
                let r_c = c64::new(r, 0.0);
                let den = r_c + zc;
                (
                    (r_c - zc) / den,
                    c64::new(2.0 * r.sqrt(), 0.0) * sqrt_zc / den,
                )
            }
        })
        .unzip();
    // I − ΓS (row i scaled by Γ_i).
    let mut m: Vec<c64> = (0..n * n)
        .map(|k| {
            let (i, j) = (k / n, k % n);
            let d = if i == j { one } else { c64::new(0.0, 0.0) };
            d - gamma[i] * s[k]
        })
        .collect();
    m = crate::driven::invert(&m, n)?;
    let mut out = vec![c64::new(0.0, 0.0); n * n];
    for i in 0..n {
        for j in 0..n {
            let mut acc = c64::new(0.0, 0.0);
            for k in 0..n {
                let sg = if i == k {
                    s[i * n + k] - gamma[i]
                } else {
                    s[i * n + k]
                };
                acc += sg * m[k * n + j];
            }
            out[i * n + j] = acc * a[j] / a[i];
        }
    }
    Some(out)
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
#[path = "../tests/support/touchstone.rs"]
mod touchstone_support;

#[cfg(test)]
mod tests {
    use super::*;

    fn row(f: f64, s: Vec<Vec<[f64; 2]>>) -> FrequencyResult {
        FrequencyResult {
            frequency_hz: f,
            k0: 0.0,
            omega_rad_s: 0.0,
            residual_rel: 0.0,
            solved: None,
            iterations: Vec::new(),
            z_ohm: Vec::new(),
            y_s: None,
            s,
            ports: Vec::new(),
            sigma_max: None,
            wave_channels: Vec::new(),
            roughness_k: Vec::new(),
            materials: Vec::new(),
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
        // Removed on drop, also if the scikit-rf check panics (issue #766).
        let tmp = tempfile::Builder::new()
            .prefix("geode-skrf-unit-")
            .tempdir()
            .unwrap();
        let dir = tmp.path();
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
    }

    /// The reserved scikit-rf comment keywords (issue #775), as an
    /// independent copy of the curation's list.
    fn assert_skrf_safe(text: &str) {
        for line in text.lines().filter(|l| l.starts_with('!')) {
            let body = line[1..].trim_start().to_ascii_lowercase();
            for k in [
                "port impedance",
                "gamma",
                "modal data exported",
                "terminal data exported",
            ] {
                assert!(!body.starts_with(k), "reserved prefix in {line:?}");
            }
            assert!(!line.contains("S-parameter uses the"), "{line:?}");
            assert!(!line.contains("::"), "{line:?}");
            if body.starts_with("port") {
                let rest = line.strip_prefix("! Port[").expect("only `! Port[k] = …`");
                let (k, label) = rest.split_once("] = ").expect("`] = `");
                assert!(k.parse::<usize>().is_ok() && !label.is_empty(), "{line:?}");
            }
        }
    }

    #[test]
    fn wave_comments_carry_no_scikit_rf_keywords() {
        let head = vec![
            "Touchstone 2.0 written by geode 0.0.0 (abc)".to_string(),
            "spec: dir/spec.json".to_string(),
        ];
        // Adversarial group names: a Sigrity `::` and keyword look-alikes.
        let comments = wave_comments(
            head.clone(),
            &["gamma::sheet"],
            &[
                "wave port Port Impedance mode 0 (JSON channel 1)".to_string(),
                "wave port out mode 0 (JSON channel 3)".to_string(),
            ],
            &["wave port in mode 1 (JSON channel 2)".to_string()],
            false,
            true,
        )
        .unwrap();
        assert_eq!(comments[..2], head[..]);
        // The hybrid-channel convention lines (issue #807) are safe too.
        for geometric in [false, true] {
            let hybrid = wave_comments(head.clone(), &[], &[], &[], true, geometric).unwrap();
            let text = render_rows(&hybrid, &[50.0], vec![(1e9, vec![vec![[0.1, 0.0]]])]).unwrap();
            assert_skrf_safe(&text);
            assert!(text.contains("never Z_TE"));
            assert_eq!(text.contains("Geometric wave channels"), geometric);
        }
        let refs = [50.0, 75.0, 100.0];
        let row = vec![vec![[0.1, 0.0]; 3]; 3];
        let text = render_rows(&comments, &refs, vec![(1e9, row)]).unwrap();
        assert_skrf_safe(&text);
        assert!(text.contains("\n! Port[1] = lumped gamma:sheet (JSON channel 0)\n"));
        assert!(text.contains("\n! Port[3] = wave port out mode 0 (JSON channel 3)\n"));
        assert!(text.contains("\n! Excluded: wave port in mode 1 (JSON channel 2) is"));
        // Every comment precedes `[Version]`.
        let version = text.find("[Version]").unwrap();
        assert!(!text[version..].contains('!'));

        // The guard itself, and the error a reserved substring yields.
        for bad in [
            "Port Impedance 50 0",
            "  gamma 0.1",
            "Modal data exported",
            "TERMINAL DATA EXPORTED",
            "x S-parameter uses the power-wave definition",
            "a::b",
        ] {
            assert!(reserved_in_comment(bad).is_some(), "{bad:?}");
        }
        for ok in ["Port[1] = x", "spec: a:b", "Excluded: gammas later"] {
            assert_eq!(reserved_in_comment(ok), None, "{ok:?}");
        }
        let e = wave_comments(
            head,
            &[],
            &["S-parameter uses the".to_string()],
            &[],
            false,
            true,
        );
        assert!(matches!(e, Err(CliError::TouchstoneUnsupported { .. })));
    }

    /// The Γ-form equals the test-side impedance route (a different
    /// algebraic path) on a synthetic 3-port with a lumped port and two
    /// complex-`Z_c` channels, and is the identity for lumped-only rows.
    #[test]
    fn renormalize_matches_the_z_route_and_is_identity_for_lumped_ports() {
        let c = c64::new;
        let s = vec![
            c(0.1, -0.2),
            c(0.3, 0.4),
            c(-0.05, 0.1),
            c(0.3, 0.4),
            c(-0.2, 0.1),
            c(0.25, -0.3),
            c(-0.05, 0.1),
            c(0.25, -0.3),
            c(0.4, 0.2),
        ];
        let refs = [50.0, 75.0, 600.0];
        let z_c = [c(50.0, 0.0), c(480.0, -35.0), c(120.0, 260.0)];
        let z = [
            None,
            Some((z_c[1], z_c[1].sqrt())),
            Some((z_c[2], z_c[2].sqrt())),
        ];
        let got = renormalize(&s, &z, &refs).unwrap();
        let want = touchstone_support::z_route(&s, 3, &z_c, &refs);
        let err = got
            .iter()
            .zip(&want)
            .map(|(a, b)| (a - b).norm())
            .fold(0.0, f64::max);
        assert!(err < 1e-13, "Γ-form vs Z-route: {err:e}");
        // Reciprocity is preserved.
        for i in 0..3 {
            for j in 0..3 {
                assert!((got[i * 3 + j] - got[j * 3 + i]).norm() < 1e-14);
            }
        }
        // Lumped-only (Γ = 0, A = 1): exactly the input.
        assert_eq!(renormalize(&s, &[None; 3], &refs).unwrap(), s);
        // R = Z_c (real) on a channel: Γ = 0, A = 1 up to round-off.
        let z = [None, Some((c(75.0, 0.0), c(75.0f64.sqrt(), 0.0))), None];
        let id = renormalize(&s, &z, &refs).unwrap();
        assert!(id.iter().zip(&s).all(|(a, b)| (a - b).norm() < 1e-15));
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

    /// A `--touchstone` target whose parent directory exists but is
    /// permission-denied (not merely missing) must not be reported as
    /// "does not exist" (issue #736, item 4). Unix-only (permission bits);
    /// skipped when running as root, since root bypasses the directory
    /// permission check entirely and the test would otherwise spuriously
    /// fail.
    #[test]
    #[cfg(unix)]
    fn parent_permission_denied_is_not_reported_as_missing() {
        use std::os::unix::fs::PermissionsExt;

        // Root ignores directory permission bits, so the `chmod 0o000`
        // below would not actually block metadata access there.
        if running_as_root() {
            eprintln!("SKIPPED: running as root, permission bits are not enforced");
            return;
        }

        let dir = std::env::temp_dir().join(format!(
            "geode-touchstone-perm-denied-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let locked = dir.join("locked");
        std::fs::create_dir_all(&locked).unwrap();
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();

        // The target's *parent* is one level inside `locked`: stat-ing it
        // must traverse `locked`, whose missing search (`x`) permission is
        // what actually triggers `EACCES` — denying permissions on the
        // directory being stat-ed directly is not enough on every platform
        // (e.g. macOS permits `stat` on a 0o000 directory itself as long as
        // its own parent is traversable).
        let target = locked.join("sub").join("out.s1p");
        let result = check_parent_dir(&target);

        // Always restore permissions before asserting, so a failed
        // assertion still leaves the temp dir removable.
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();

        match result {
            Err(CliError::Io { err, .. }) => {
                let msg = err.to_string();
                assert!(
                    !msg.contains("does not exist"),
                    "permission-denied parent must not be reported as missing: {msg}"
                );
                assert!(
                    msg.contains("permission denied") || msg.contains("not accessible"),
                    "expected a permission-denied message, got: {msg}"
                );
            }
            other => panic!("expected a permission-denied io error, got: {other:?}"),
        }
    }

    #[cfg(unix)]
    fn running_as_root() -> bool {
        // Avoid an extra `libc` dependency: shell out to `id -u`.
        std::process::Command::new("id")
            .arg("-u")
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "0")
            .unwrap_or(false)
    }

    /// A `--touchstone` target that is itself an existing directory is
    /// rejected fast by `validate`'s directory check, with a clear `io`
    /// error (issue #736, item 5).
    #[test]
    fn target_path_itself_a_directory_is_rejected() {
        let dir = std::env::temp_dir().join(format!(
            "geode-touchstone-isdir-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();

        let result = check_not_dir(&dir);
        std::fs::remove_dir_all(&dir).unwrap();

        match result {
            Err(CliError::Io { path, err }) => {
                assert_eq!(path, dir);
                assert_eq!(err.kind(), std::io::ErrorKind::IsADirectory);
            }
            other => panic!("expected an IsADirectory io error, got: {other:?}"),
        }
    }
}
