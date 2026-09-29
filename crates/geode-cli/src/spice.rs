//! SPICE subcircuit export for `geode capacitance` (`--spice <PATH>`,
//! issue #715, Epic #702 Phase 3c).
//!
//! # Conversion
//!
//! A field solver produces the **Maxwell** capacitance matrix `C`
//! (diagonal `> 0`, off-diagonals `≤ 0`); a circuit simulator wants one
//! capacitor per physical branch, i.e. the **mutual (circuit) form**:
//!
//! * `C_ground(i) = Σ_j C_ij` (the report's `c_sigma_farad[i]`) — a
//!   capacitor from terminal *i* to SPICE ground node `0`;
//! * `C_mutual(i, j) = −(C_ij + C_ji) / 2` for `i < j` (the report's
//!   `c_farad`, symmetrized defensively) — a capacitor between terminals
//!   *i* and *j*.
//!
//! This network reproduces `C` exactly: the charge on terminal *i* is
//! `C_ground(i)·V_i + Σ_{j≠i} C_mutual(i, j)·(V_i − V_j) = Σ_j C_ij V_j`.
//!
//! `capacitance.ground` physical groups are not matrix rows / columns at
//! all; their potential reference **is** SPICE node `0`, which is global
//! and therefore never listed among the `.subckt` ports.
//!
//! # Noise threshold
//!
//! A branch whose `|value| < DROP_REL_TOL × max_i C_ii` is numerical
//! noise (e.g. the ground branch of a fully Faraday-shielded terminal,
//! mathematically `0`, computed at ~1e-15 relative) and is **dropped**,
//! leaving a `* dropped: …` comment line naming the branch and its value
//! — never silently. A branch at or above the threshold with the wrong
//! sign (negative ground or mutual capacitance) is a genuinely non-Maxwell
//! matrix, not noise: the export fails with
//! [`CliError::SpiceUnsupported`] (`invalid_spec`) and no file is
//! written — a negative capacitor is never emitted, nothing is clamped.
//! The threshold is the fixed [`DROP_REL_TOL`] in v1; it could later be
//! promoted to a spec field without changing the file format.
//!
//! # Dialect
//!
//! ```text
//! * <provenance comments: geode version + git sha, spec, mesh + sha256>
//! * <conversion formula, threshold, dropped branches, node renames>
//! .subckt CEXTRACT <node_1> <node_2> … <node_N>
//! C<i>_0 <node_i> 0 <C_ground(i)>
//! C<i>_<j> <node_i> <node_j> <C_mutual(i, j)>
//! .ends CEXTRACT
//! ```
//!
//! * **Subcircuit name**: always [`SUBCKT_NAME`] (`CEXTRACT`), so a
//!   testbench deck instantiating it does not depend on the spec's file
//!   name.
//! * **Ports**: the terminal nodes in `capacitance.terminals` (matrix)
//!   order; never node `0`.
//! * **Elements**: instance names are derived from **1-based terminal
//!   indices** (`C1_2` between terminals 1 and 2, `C1_0` from terminal 1
//!   to ground), never from terminal names, so they cannot collide.
//!   Order: for each terminal *i* in matrix order, its ground branch
//!   first, then its mutual branches to `j > i` ascending — a stable
//!   order, so re-runs of an unchanged matrix diff clean.
//! * **Values**: plain farads, no engineering suffix (`p`, `f`, …), in
//!   Rust's shortest round-trip `{:e}` formatting (`2.5e-13`), so parsing
//!   the file back yields the exported `f64` values bit for bit.
//! * **Comments**: whole-line `*` comments only (never `$`, which ngspice
//!   treats specially); embedded newlines in comment text are flattened.
//!
//! # Node names
//!
//! Terminal names are user-chosen physical-group names, not necessarily
//! valid SPICE identifiers. Each is sanitized deterministically:
//!
//! 1. every character outside `[A-Za-z0-9_]` becomes `_`;
//! 2. an empty result, or one starting with a digit, is prefixed `n_`;
//! 3. SPICE node names are case-insensitive, so a result equal
//!    (ignoring case) to an earlier terminal's node, or to the reserved
//!    ground names `0` / `gnd`, gets the first free suffix `_2`, `_3`, …
//!    (in terminal order).
//!
//! Every terminal whose node name differs from its terminal name gets a
//! `* node: "<terminal>" -> <node>` comment line.
//!
//! # Extension point (inductance, #714)
//!
//! The file is built as a generic [`Netlist`] (ports + [`Element`]s +
//! comments) and rendered by [`Netlist::render`]; only
//! [`capacitance_netlist`] is capacitance-specific. A future static
//! `L`-matrix export adds `Element` variants for self inductors and `K`
//! coupling statements plus an `inductance_netlist` builder, reusing the
//! header, node naming and rendering unchanged.

use std::fmt::Write as _;
use std::path::Path;

use crate::error::CliError;
use crate::export::file_ref_at;
use crate::report::{CapacitanceReport, FileRef};

/// Relative noise threshold: a branch with `|C| < DROP_REL_TOL × max_i
/// C_ii` is dropped. Matches the `1e-9` slack of the report's
/// `maxwell_sign_structure` check. On the golden triax fixture the
/// shielded `inner` ground branch sits at ~1e-15 relative and the real
/// `shield` ground branch at ~0.34, so any value in `1e-12..1e-3` would
/// separate them.
pub const DROP_REL_TOL: f64 = 1e-9;

/// The `.subckt` name (fixed; see the module docs).
pub const SUBCKT_NAME: &str = "CEXTRACT";

/// SPICE ground node.
const GROUND: &str = "0";

/// Reserved node names (compared case-insensitively).
const RESERVED: [&str; 2] = ["0", "gnd"];

/// One netlist element.
#[derive(Debug, Clone, PartialEq)]
pub enum Element {
    /// A two-terminal capacitor `C<name> <a> <b> <farad>`.
    Capacitor {
        /// Instance name, including the leading `C`.
        name: String,
        /// First node.
        a: String,
        /// Second node.
        b: String,
        /// Capacitance (F).
        farad: f64,
    },
}

/// A `.subckt` body: header comments, port nodes, elements.
#[derive(Debug, Clone, PartialEq)]
pub struct Netlist {
    /// Leading `*` comment lines (without the `* ` prefix).
    pub comments: Vec<String>,
    /// Port nodes, in order.
    pub ports: Vec<String>,
    /// Elements, in order.
    pub elements: Vec<Element>,
}

impl Netlist {
    /// The SPICE text of this subcircuit.
    pub fn render(&self) -> String {
        let mut t = String::new();
        for c in &self.comments {
            let flat: String = c
                .chars()
                .map(|ch| if ch == '\n' || ch == '\r' { ' ' } else { ch })
                .collect();
            // `writeln!` into a `String` cannot fail.
            let _ = writeln!(t, "* {flat}");
        }
        let _ = writeln!(t, ".subckt {SUBCKT_NAME} {}", self.ports.join(" "));
        for e in &self.elements {
            match e {
                Element::Capacitor { name, a, b, farad } => {
                    let _ = writeln!(t, "{name} {a} {b} {farad:e}");
                }
            }
        }
        let _ = writeln!(t, ".ends {SUBCKT_NAME}");
        t
    }
}

/// The error for a matrix / command that cannot produce a SPICE file.
pub fn unsupported(reason: &str) -> CliError {
    CliError::SpiceUnsupported {
        reason: reason.to_string(),
    }
}

/// Sanitized, collision-free SPICE node names for `terminals` (see the
/// module docs for the rule).
pub fn node_names(terminals: &[String]) -> Vec<String> {
    let mut taken: Vec<String> = RESERVED.iter().map(|s| s.to_string()).collect();
    let mut out = Vec::with_capacity(terminals.len());
    for name in terminals {
        let mut base: String = name
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        if base.is_empty() || base.starts_with(|c: char| c.is_ascii_digit()) {
            base.insert_str(0, "n_");
        }
        let free = |cand: &str| !taken.iter().any(|t| t.eq_ignore_ascii_case(cand));
        let mut cand = base.clone();
        let mut k = 2;
        while !free(&cand) {
            cand = format!("{base}_{k}");
            k += 1;
        }
        taken.push(cand.clone());
        out.push(cand);
    }
    out
}

/// Build the mutual-form capacitor network of the Maxwell matrix `c`
/// (F, row-major, `terminals` order) with row sums `c_sigma` (F).
/// `header` becomes the leading comment lines.
pub fn capacitance_netlist(
    header: &[String],
    terminals: &[String],
    c: &[Vec<f64>],
    c_sigma: &[f64],
) -> Result<Netlist, CliError> {
    let n = terminals.len();
    if n == 0 {
        return Err(unsupported("no terminals, so there is no network to write"));
    }
    if c.len() != n || c.iter().any(|row| row.len() != n) || c_sigma.len() != n {
        return Err(unsupported(&format!(
            "the capacitance matrix is not {n} x {n} with {n} row sums (one per terminal)"
        )));
    }
    if c.iter().flatten().chain(c_sigma).any(|v| !v.is_finite()) {
        return Err(unsupported("the capacitance matrix has a non-finite entry"));
    }
    let max_diag = (0..n).map(|i| c[i][i]).fold(f64::NEG_INFINITY, f64::max);
    if max_diag <= 0.0 {
        return Err(unsupported(&format!(
            "the capacitance matrix has no positive diagonal entry (max {max_diag:e} F), \
             so it is not a Maxwell matrix"
        )));
    }
    let thr = DROP_REL_TOL * max_diag;
    let nodes = node_names(terminals);

    let mut comments: Vec<String> = header.to_vec();
    comments.push(
        "Mutual (circuit) capacitance from the Maxwell matrix, farads: \
         C_ground(i) = sum_j C_ij; C_mutual(i,j) = -C_ij (i != j)"
            .to_string(),
    );
    comments.push(format!(
        "noise threshold: |C| < {DROP_REL_TOL:e} * max(diag) = {DROP_REL_TOL:e} * {max_diag:e} F \
         = {thr:e} F is dropped"
    ));
    for (t, node) in terminals.iter().zip(&nodes) {
        if t != node {
            comments.push(format!("node: {t:?} -> {node}"));
        }
    }

    let mut elements = Vec::new();
    for i in 0..n {
        let mut branches = vec![(format!("C{}_0", i + 1), GROUND, "ground", c_sigma[i])];
        for j in i + 1..n {
            branches.push((
                format!("C{}_{}", i + 1, j + 1),
                nodes[j].as_str(),
                terminals[j].as_str(),
                -0.5 * (c[i][j] + c[j][i]),
            ));
        }
        for (name, b, b_label, v) in branches {
            let label = format!("C({}, {b_label})", terminals[i]);
            if v.abs() < thr {
                comments.push(format!(
                    "dropped: {label} = {v:e} F, |C| < {thr:e} F -- below noise threshold"
                ));
            } else if v < 0.0 {
                return Err(unsupported(&format!(
                    "{label} = {v:e} F is negative beyond the noise threshold ({thr:e} F = \
                     {DROP_REL_TOL:e} * max(diag)): the matrix violates the Maxwell sign \
                     structure (see the report's maxwell_sign_structure), and a negative \
                     capacitor would not be a physical network; refine / repair the mesh"
                )));
            } else {
                elements.push(Element::Capacitor {
                    name,
                    a: nodes[i].clone(),
                    b: b.to_string(),
                    farad: v,
                });
            }
        }
    }
    Ok(Netlist {
        comments,
        ports: nodes,
        elements,
    })
}

/// Render the SPICE subcircuit of a finished capacitance report (pure;
/// no I/O).
pub fn render_capacitance(report: &CapacitanceReport) -> Result<String, CliError> {
    let p = &report.provenance;
    let header = vec![
        format!(
            "SPICE subcircuit written by geode {} ({})",
            p.geode_version, p.git_sha
        ),
        format!("spec: {}", p.spec_path),
        format!("mesh: {} sha256={}", report.mesh.path, report.mesh.sha256),
    ];
    Ok(capacitance_netlist(
        &header,
        &report.terminals,
        &report.c_farad,
        &report.c_sigma_farad,
    )?
    .render())
}

/// Write the SPICE subcircuit of `report` to `path` and return its
/// `{path, sha256}` reference: `path` is the `--spice` argument **as
/// given on the command line** (like `spec_path`), `sha256` the hash of
/// the bytes on disk. Nothing is written if the matrix is rejected.
pub fn write(path: &Path, report: &CapacitanceReport) -> Result<FileRef, CliError> {
    let text = render_capacitance(report)?;
    std::fs::write(path, text).map_err(|err| CliError::Io {
        path: path.to_path_buf(),
        err,
    })?;
    file_ref_at(path, path.display().to_string())
}

/// A parsed subcircuit (only the subset [`Netlist::render`] emits).
#[cfg(test)]
#[derive(Debug, Clone, PartialEq)]
pub struct Parsed {
    /// `.subckt` name.
    pub name: String,
    /// Port nodes.
    pub ports: Vec<String>,
    /// `(instance, node_a, node_b, farad)` in file order.
    pub caps: Vec<(String, String, String, f64)>,
    /// Comment lines (without `* `).
    pub comments: Vec<String>,
}

/// Parse the SPICE subset [`Netlist::render`] writes (test-only round
/// trip).
#[cfg(test)]
pub fn parse(text: &str) -> Result<Parsed, String> {
    let (mut name, mut ports, mut caps, mut comments) = (None, Vec::new(), Vec::new(), Vec::new());
    let mut ended = false;
    for line in text.lines() {
        if let Some(c) = line.strip_prefix('*') {
            comments.push(c.trim_start().to_string());
            continue;
        }
        let toks: Vec<&str> = line.split_whitespace().collect();
        match toks.first().map(|t| t.to_ascii_lowercase()) {
            None => {}
            Some(k) if k == ".subckt" => {
                if name.is_some() {
                    return Err("second .subckt".into());
                }
                name = Some(toks.get(1).ok_or("unnamed .subckt")?.to_string());
                ports = toks[2..].iter().map(|s| s.to_string()).collect();
            }
            Some(k) if k == ".ends" => {
                if toks.get(1).map(|s| s.to_string()) != name {
                    return Err(format!(".ends name mismatch: {line:?}"));
                }
                ended = true;
            }
            Some(k) if k.starts_with('c') && name.is_some() && !ended => {
                if toks.len() != 4 {
                    return Err(format!("capacitor line {line:?}"));
                }
                let v = toks[3]
                    .parse::<f64>()
                    .map_err(|e| format!("{line:?}: {e}"))?;
                caps.push((toks[0].into(), toks[1].into(), toks[2].into(), v));
            }
            _ => return Err(format!("stray line {line:?}")),
        }
    }
    if !ended {
        return Err("missing .ends".into());
    }
    Ok(Parsed {
        name: name.ok_or("missing .subckt")?,
        ports,
        caps,
        comments,
    })
}

/// Rebuild the Maxwell matrix from a parsed network (test-only):
/// `C_ii = Σ` of every capacitor at node *i*, `C_ij = −C_mutual(i, j)`.
#[cfg(test)]
pub fn maxwell_from(parsed: &Parsed) -> Vec<Vec<f64>> {
    let n = parsed.ports.len();
    let idx = |node: &str| parsed.ports.iter().position(|p| p == node);
    let mut c = vec![vec![0.0; n]; n];
    for (_, a, b, v) in &parsed.caps {
        match (idx(a), idx(b)) {
            (Some(i), None) | (None, Some(i)) => c[i][i] += v,
            (Some(i), Some(j)) => {
                c[i][i] += v;
                c[j][j] += v;
                c[i][j] -= v;
                c[j][i] -= v;
            }
            (None, None) => panic!("capacitor between two non-port nodes"),
        }
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    fn row_sums(c: &[Vec<f64>]) -> Vec<f64> {
        c.iter().map(|r| r.iter().sum()).collect()
    }

    /// The measured triax smoke matrix (`capacitance_triax_smoke.toml`,
    /// 2026-09-28): `inner` is fully shielded, so its row sum is noise.
    const C1: f64 = 2.376772660410382e-13;
    const C_SIGMA_INNER: f64 = -8.271806125530277e-28;
    const C_SIGMA_SHIELD: f64 = 1.249659259928909e-13;

    #[test]
    fn two_terminal_triax_exact_bytes() {
        let c = vec![vec![C1, -C1], vec![-C1, C1 + C_SIGMA_SHIELD]];
        let net = capacitance_netlist(
            &s(&["hdr"]),
            &s(&["inner", "shield"]),
            &c,
            &[C_SIGMA_INNER, C_SIGMA_SHIELD],
        )
        .unwrap();
        let max_diag = C1 + C_SIGMA_SHIELD;
        let thr = DROP_REL_TOL * max_diag;
        let t = net.render();
        assert_eq!(
            t,
            format!(
                "* hdr\n\
                 * Mutual (circuit) capacitance from the Maxwell matrix, farads: C_ground(i) = \
                 sum_j C_ij; C_mutual(i,j) = -C_ij (i != j)\n\
                 * noise threshold: |C| < 1e-9 * max(diag) = 1e-9 * {max_diag:e} F = {thr:e} F \
                 is dropped\n\
                 * dropped: C(inner, ground) = -8.271806125530277e-28 F, |C| < {thr:e} F -- below \
                 noise threshold\n\
                 .subckt CEXTRACT inner shield\n\
                 C1_2 inner shield 2.376772660410382e-13\n\
                 C2_0 shield 0 1.249659259928909e-13\n\
                 .ends CEXTRACT\n"
            )
        );
        let p = parse(&t).unwrap();
        assert_eq!(p.name, SUBCKT_NAME);
        assert_eq!(p.ports, s(&["inner", "shield"]));
        assert_eq!(
            p.caps,
            vec![
                ("C1_2".into(), "inner".into(), "shield".into(), C1),
                ("C2_0".into(), "shield".into(), "0".into(), C_SIGMA_SHIELD),
            ]
        );
        // The network reproduces the matrix up to the dropped noise.
        let back = maxwell_from(&p);
        for i in 0..2 {
            for j in 0..2 {
                assert!((back[i][j] - c[i][j]).abs() <= thr, "C[{i}][{j}]");
            }
        }
    }

    #[test]
    fn three_terminal_exact_bytes_and_both_drop_paths() {
        // a–b coupled, c shielded from a (mutual ~noise), b's ground ~noise.
        let (cab, cbc, ca0, cc0) = (3e-12, 2e-12, 1e-12, 4e-12);
        let (noise_ac, noise_b0) = (1e-26, -2e-25);
        let c = vec![
            vec![ca0 + cab + noise_ac, -cab, -noise_ac],
            vec![-cab, cab + cbc + noise_b0, -cbc],
            vec![-noise_ac, -cbc, cbc + cc0 + noise_ac],
        ];
        let sig = row_sums(&c);
        let net = capacitance_netlist(&[], &s(&["a", "b", "c"]), &c, &sig).unwrap();
        let t = net.render();
        let body: Vec<&str> = t.lines().filter(|l| !l.starts_with('*')).collect();
        assert_eq!(
            body,
            [
                ".subckt CEXTRACT a b c",
                &format!("C1_0 a 0 {:e}", sig[0]),
                "C1_2 a b 3e-12",
                &format!("C2_3 b c {:e}", cbc),
                &format!("C3_0 c 0 {:e}", sig[2]),
                ".ends CEXTRACT",
            ]
        );
        let dropped: Vec<&str> = t.lines().filter(|l| l.contains("dropped:")).collect();
        assert_eq!(dropped.len(), 2, "{t}");
        assert!(dropped[0].starts_with("* dropped: C(a, c) = 1e-26 F"));
        assert!(dropped[1].starts_with(&format!("* dropped: C(b, ground) = {:e} F", sig[1])));
        let p = parse(&t).unwrap();
        let caps: Vec<f64> = p.caps.iter().map(|c| c.3).collect();
        assert_eq!(caps, [sig[0], cab, cbc, sig[2]]);
        let back = maxwell_from(&p);
        let thr = DROP_REL_TOL * c[2][2];
        for i in 0..3 {
            for j in 0..3 {
                assert!((back[i][j] - c[i][j]).abs() <= 2.0 * thr, "C[{i}][{j}]");
            }
        }
    }

    #[test]
    fn round_trip_recovers_matrix_exactly_when_nothing_dropped() {
        let c = vec![
            vec![5e-12, -1e-12, -2e-12],
            vec![-1e-12, 4e-12, -0.5e-12],
            vec![-2e-12, -0.5e-12, 7e-12],
        ];
        let sig = row_sums(&c);
        let t = capacitance_netlist(&[], &s(&["x", "y", "z"]), &c, &sig)
            .unwrap()
            .render();
        assert!(!t.contains("dropped:"));
        let p = parse(&t).unwrap();
        assert_eq!(p.caps.len(), 6);
        let back = maxwell_from(&p);
        for i in 0..3 {
            for j in 0..3 {
                assert!(
                    (back[i][j] - c[i][j]).abs() <= 1e-15 * c[i][i].abs(),
                    "C[{i}][{j}]: {} vs {}",
                    back[i][j],
                    c[i][j]
                );
            }
        }
    }

    #[test]
    fn node_names_are_sanitized_and_disambiguated() {
        let names = node_names(&s(&[
            "my term", "my-term", "2nd", "", "GND", "Inner", "inner", "0", "n_0",
        ]));
        assert_eq!(
            names,
            s(&[
                "my_term",
                "my_term_2",
                "n_2nd",
                "n_",
                "GND_2",
                "Inner",
                "inner_2",
                "n_0",
                "n_0_2",
            ])
        );
        for n in &names {
            assert!(n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'));
            assert!(!n.starts_with(|c: char| c.is_ascii_digit()));
        }
        let c = vec![vec![2e-12, -1e-12], vec![-1e-12, 2e-12]];
        let t = capacitance_netlist(&[], &s(&["sig+", "sig-"]), &c, &row_sums(&c))
            .unwrap()
            .render();
        assert!(t.contains("* node: \"sig+\" -> sig_\n"));
        assert!(t.contains("* node: \"sig-\" -> sig__2\n"));
        assert!(t.contains(".subckt CEXTRACT sig_ sig__2\n"));
        parse(&t).unwrap();
    }

    #[test]
    fn sign_violations_beyond_threshold_are_rejected() {
        // Positive off-diagonal → negative mutual capacitance.
        let c = vec![vec![2e-12, 1e-12], vec![1e-12, 2e-12]];
        let e = capacitance_netlist(&[], &s(&["a", "b"]), &c, &row_sums(&c));
        assert!(matches!(e, Err(CliError::SpiceUnsupported { .. })), "{e:?}");
        // Negative row sum → negative ground capacitance.
        let c = vec![vec![1e-12, -2e-12], vec![-2e-12, 3e-12]];
        let e = capacitance_netlist(&[], &s(&["a", "b"]), &c, &row_sums(&c));
        let Err(err) = e else { panic!("accepted") };
        assert_eq!(err.code(), "invalid_spec");
        assert!(err.to_string().contains("C(a, ground)"), "{err}");
        // But a tiny positive off-diagonal is noise, dropped not rejected.
        let c = vec![vec![2e-12, 1e-24], vec![1e-24, 2e-12]];
        capacitance_netlist(&[], &s(&["a", "b"]), &c, &row_sums(&c)).unwrap();
    }

    #[test]
    fn malformed_matrices_are_rejected() {
        let e = capacitance_netlist(&[], &[], &[], &[]);
        assert!(matches!(e, Err(CliError::SpiceUnsupported { .. })));
        let e = capacitance_netlist(&[], &s(&["a", "b"]), &[vec![1.0]], &[1.0]);
        assert!(matches!(e, Err(CliError::SpiceUnsupported { .. })));
        let e = capacitance_netlist(&[], &s(&["a"]), &[vec![f64::NAN]], &[1.0]);
        assert!(matches!(e, Err(CliError::SpiceUnsupported { .. })));
        let e = capacitance_netlist(&[], &s(&["a"]), &[vec![0.0]], &[0.0]);
        assert!(matches!(e, Err(CliError::SpiceUnsupported { .. })));
    }

    #[test]
    fn comment_newlines_are_flattened() {
        let c = vec![vec![1e-12]];
        let t = capacitance_netlist(&s(&["spec: a\nb"]), &s(&["x"]), &c, &[1e-12])
            .unwrap()
            .render();
        assert!(t.starts_with("* spec: a b\n"));
        parse(&t).unwrap();
    }
}
