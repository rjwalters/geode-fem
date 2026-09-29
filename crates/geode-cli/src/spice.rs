//! SPICE subcircuit export for `geode capacitance` (`--spice <PATH>`,
//! issue #715, Epic #702 Phase 3c) and `geode inductance` (issue #719; see
//! [Inductance](#inductance-issue-719) below).
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
//! # Inductance (issue #719)
//!
//! Both builders produce a generic [`Netlist`] (subcircuit name + ports +
//! [`Element`]s + comments) rendered by [`Netlist::render`], with the same
//! provenance header, node naming and value formatting;
//! [`capacitance_netlist`] and [`inductance_netlist`] are the only
//! quantity-specific parts. The subcircuit name is a field of the netlist:
//! [`SUBCKT_NAME`] (`CEXTRACT`) for capacitance, [`SUBCKT_NAME_INDUCTANCE`]
//! (`LEXTRACT`) for inductance.
//!
//! **Ports.** A `geode inductance` path's source face and sink face are
//! both PEC contacts on the *same* connected PEC return wall, so the wall
//! is the common return of every path's loop: a path has one terminal
//! pair, (path node, return). Each path therefore becomes **one** SPICE
//! node (named from the path name by the rules of [Node
//! names](#node-names)) and the return wall is SPICE ground `0` — exactly
//! the shape of the capacitance ground branch.
//!
//! **Elements.** The Maxwell inductance matrix `L` (H, symmetric positive
//! definite) becomes
//!
//! * a self inductor `L<i>_0 <node_i> 0 <L_ii>` per path, and
//! * a coupling statement `K<i>_<j> L<i>_0 L<j>_0 <k_ij>` per pair `i < j`
//!   with `k_ij = L_ij / √(L_ii L_jj)` (`L_ij` symmetrized defensively as
//!   `(L_ij + L_ji) / 2`; the library already writes both from one value).
//!
//! SPICE has no mutual-inductor branch: the `K` coefficient is the only
//! way to express `L_ij`, and a coupled pair `L<i>_0`, `L<j>_0` with
//! coefficient `k_ij` has `V_i = jω (L_ii I_i + k_ij √(L_ii L_jj) I_j)`,
//! i.e. exactly `jω Σ_j L_ij I_j`.
//!
//! **Sign.** Every self inductor is declared with the same node order
//! (path node first, ground second), so each path's reference current
//! (into the path node, i.e. source → sink through the conductor, back
//! through the return) enters the dotted end of its inductor. `k_ij`
//! therefore carries the sign of `L_ij` directly — a negative `k` (legal
//! ngspice syntax) is emitted as is, no node order is ever flipped.
//!
//! **Noise threshold.** `k_ij` is dimensionless and bounded by `|k| < 1`
//! for an SPD matrix, so the threshold is absolute: a coupling with
//! `|k_ij| < K_DROP_TOL` is dropped with a `* dropped: K(<path_i>,
//! <path_j>) = …` comment, never silently. A coupling with `|k_ij| ≥ 1`
//! (or a non-positive / non-finite self inductance) is not a physical
//! network: the export fails with [`CliError::SpiceUnsupported`]
//! (`invalid_spec`) and no file is written. `geode inductance` already
//! gates the whole matrix on SPD before any report exists; this check is
//! independent defense in depth.
//!
//! ```text
//! * <provenance comments: geode version + git sha, spec, mesh + sha256>
//! * <conversion formula, port rule, threshold, dropped couplings, node renames>
//! .subckt LEXTRACT <node_1> … <node_N>
//! L<i>_0 <node_i> 0 <L_ii>
//! K<i>_<j> L<i>_0 L<j>_0 <k_ij>
//! .ends LEXTRACT
//! ```
//!
//! Element order: all self inductors in path order, then the couplings in
//! `(i, j)` lexicographic order (a `K` line references inductors declared
//! above it). Values are henries / plain `k` in the same shortest
//! round-trip `{:e}` formatting as the capacitance export.

use std::fmt::Write as _;
use std::path::Path;

use crate::error::CliError;
use crate::export::file_ref_at;
use crate::report::{CapacitanceReport, FileRef, InductanceReport, MeshSummary, Provenance};

/// Relative noise threshold: a branch with `|C| < DROP_REL_TOL × max_i
/// C_ii` is dropped. Matches the `1e-9` slack of the report's
/// `maxwell_sign_structure` check. On the golden triax fixture the
/// shielded `inner` ground branch sits at ~1e-15 relative and the real
/// `shield` ground branch at ~0.34, so any value in `1e-12..1e-3` would
/// separate them.
pub const DROP_REL_TOL: f64 = 1e-9;

/// The capacitance `.subckt` name (fixed; see the module docs).
pub const SUBCKT_NAME: &str = "CEXTRACT";

/// The inductance `.subckt` name (fixed; see the module docs).
pub const SUBCKT_NAME_INDUCTANCE: &str = "LEXTRACT";

/// Absolute threshold on the (dimensionless) coupling coefficient: a `K`
/// statement with `|k_ij| < K_DROP_TOL` is dropped with a comment. Neither
/// golden inductance fixture comes near it (the triax core–tube coupling
/// is `k ≈ 0.7`); it separates a genuinely decoupled pair computed at
/// round-off from a real coupling.
pub const K_DROP_TOL: f64 = 1e-9;

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
    /// A two-terminal inductor `L<name> <a> <b> <henry>`.
    Inductor {
        /// Instance name, including the leading `L`.
        name: String,
        /// First node (the dotted end for `K` couplings).
        a: String,
        /// Second node.
        b: String,
        /// Inductance (H).
        henry: f64,
    },
    /// A mutual coupling `K<name> <l1> <l2> <k>` between two inductors
    /// declared earlier in the netlist.
    Coupling {
        /// Instance name, including the leading `K`.
        name: String,
        /// First inductor's instance name.
        l1: String,
        /// Second inductor's instance name.
        l2: String,
        /// Coupling coefficient, `-1 < k < 1`.
        k: f64,
    },
}

/// A `.subckt` body: header comments, port nodes, elements.
#[derive(Debug, Clone, PartialEq)]
pub struct Netlist {
    /// The `.subckt` name ([`SUBCKT_NAME`] / [`SUBCKT_NAME_INDUCTANCE`]).
    pub name: &'static str,
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
        let subckt = self.name;
        let _ = writeln!(t, ".subckt {subckt} {}", self.ports.join(" "));
        for e in &self.elements {
            match e {
                Element::Capacitor { name, a, b, farad } => {
                    let _ = writeln!(t, "{name} {a} {b} {farad:e}");
                }
                Element::Inductor { name, a, b, henry } => {
                    let _ = writeln!(t, "{name} {a} {b} {henry:e}");
                }
                Element::Coupling { name, l1, l2, k } => {
                    let _ = writeln!(t, "{name} {l1} {l2} {k:e}");
                }
            }
        }
        let _ = writeln!(t, ".ends {subckt}");
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
        name: SUBCKT_NAME,
        comments,
        ports: nodes,
        elements,
    })
}

/// Build the self-inductor + `K`-coupling network of the Maxwell
/// inductance matrix `l` (H, row-major, `paths` order; see the module
/// docs). `header` becomes the leading comment lines.
pub fn inductance_netlist(
    header: &[String],
    paths: &[String],
    l: &[Vec<f64>],
) -> Result<Netlist, CliError> {
    let n = paths.len();
    if n == 0 {
        return Err(unsupported("no paths, so there is no network to write"));
    }
    if l.len() != n || l.iter().any(|row| row.len() != n) {
        return Err(unsupported(&format!(
            "the inductance matrix is not {n} x {n} (one row / column per path)"
        )));
    }
    if l.iter().flatten().any(|v| !v.is_finite()) {
        return Err(unsupported("the inductance matrix has a non-finite entry"));
    }
    for (i, p) in paths.iter().enumerate() {
        if l[i][i] <= 0.0 {
            return Err(unsupported(&format!(
                "L({p}, {p}) = {:e} H is not a positive self inductance, so the matrix is not \
                 a physical inductance matrix",
                l[i][i]
            )));
        }
    }
    let nodes = node_names(paths);

    let mut comments: Vec<String> = header.to_vec();
    comments.push(
        "Maxwell inductance matrix as self inductors plus K couplings, henries: \
         L<i>_0 = L_ii from path node i to 0; K<i>_<j> = L_ij / sqrt(L_ii * L_jj)"
            .to_string(),
    );
    comments.push(
        "port: a path's source and sink faces both contact the PEC return wall, which is \
         ground 0, so each path is one node; every inductor is declared node -> 0, so k \
         carries the sign of L_ij"
            .to_string(),
    );
    comments.push(format!("noise threshold: |k| < {K_DROP_TOL:e} is dropped"));
    for (p, node) in paths.iter().zip(&nodes) {
        if p != node {
            comments.push(format!("node: {p:?} -> {node}"));
        }
    }

    let ind = |i: usize| format!("L{}_0", i + 1);
    let mut elements: Vec<Element> = (0..n)
        .map(|i| Element::Inductor {
            name: ind(i),
            a: nodes[i].clone(),
            b: GROUND.to_string(),
            henry: l[i][i],
        })
        .collect();
    for i in 0..n {
        for j in i + 1..n {
            let lij = 0.5 * (l[i][j] + l[j][i]);
            let k = lij / (l[i][i] * l[j][j]).sqrt();
            let label = format!("K({}, {})", paths[i], paths[j]);
            if k.abs() >= 1.0 {
                return Err(unsupported(&format!(
                    "{label} = {k:e} (L_ij = {lij:e} H): |k| >= 1 is not a physical coupling \
                     (the matrix is not positive definite); refine / repair the mesh"
                )));
            } else if k.abs() < K_DROP_TOL {
                comments.push(format!(
                    "dropped: {label} = {k:e}, |k| < {K_DROP_TOL:e} -- below noise threshold"
                ));
            } else {
                elements.push(Element::Coupling {
                    name: format!("K{}_{}", i + 1, j + 1),
                    l1: ind(i),
                    l2: ind(j),
                    k,
                });
            }
        }
    }
    Ok(Netlist {
        name: SUBCKT_NAME_INDUCTANCE,
        comments,
        ports: nodes,
        elements,
    })
}

/// The provenance header shared by every export: geode version + git
/// sha, spec path, mesh path + sha256.
fn provenance_header(p: &Provenance, mesh: &MeshSummary) -> Vec<String> {
    vec![
        format!(
            "SPICE subcircuit written by geode {} ({})",
            p.geode_version, p.git_sha
        ),
        format!("spec: {}", p.spec_path),
        format!("mesh: {} sha256={}", mesh.path, mesh.sha256),
    ]
}

/// Render the SPICE subcircuit of a finished capacitance report (pure;
/// no I/O).
pub fn render_capacitance(report: &CapacitanceReport) -> Result<String, CliError> {
    let header = provenance_header(&report.provenance, &report.mesh);
    Ok(capacitance_netlist(
        &header,
        &report.terminals,
        &report.c_farad,
        &report.c_sigma_farad,
    )?
    .render())
}

/// Render the SPICE subcircuit of a finished inductance report (pure; no
/// I/O).
pub fn render_inductance(report: &InductanceReport) -> Result<String, CliError> {
    let header = provenance_header(&report.provenance, &report.mesh);
    Ok(inductance_netlist(&header, &report.paths, &report.l_henry)?.render())
}

/// Write `text` to `path` and return its `{path, sha256}` reference.
fn write_text(path: &Path, text: String) -> Result<FileRef, CliError> {
    std::fs::write(path, text).map_err(|err| CliError::Io {
        path: path.to_path_buf(),
        err,
    })?;
    file_ref_at(path, path.display().to_string())
}

/// Write the SPICE subcircuit of `report` to `path` and return its
/// `{path, sha256}` reference: `path` is the `--spice` argument **as
/// given on the command line** (like `spec_path`), `sha256` the hash of
/// the bytes on disk. Nothing is written if the matrix is rejected.
pub fn write(path: &Path, report: &CapacitanceReport) -> Result<FileRef, CliError> {
    write_text(path, render_capacitance(report)?)
}

/// [`write()`] for an inductance report ([`render_inductance`]).
pub fn write_inductance(path: &Path, report: &InductanceReport) -> Result<FileRef, CliError> {
    write_text(path, render_inductance(report)?)
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
    /// `(instance, node_a, node_b, henry)` in file order.
    pub inds: Vec<(String, String, String, f64)>,
    /// `(instance, inductor_1, inductor_2, k)` in file order.
    pub couplings: Vec<(String, String, String, f64)>,
    /// Comment lines (without `* `).
    pub comments: Vec<String>,
}

/// Parse the SPICE subset [`Netlist::render`] writes (test-only round
/// trip).
#[cfg(test)]
pub fn parse(text: &str) -> Result<Parsed, String> {
    let (mut name, mut ports, mut caps, mut comments) = (None, Vec::new(), Vec::new(), Vec::new());
    let (mut inds, mut couplings) = (Vec::new(), Vec::new());
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
            Some(k) if k.starts_with(['c', 'l', 'k']) && name.is_some() && !ended => {
                if toks.len() != 4 {
                    return Err(format!("element line {line:?}"));
                }
                let v = toks[3]
                    .parse::<f64>()
                    .map_err(|e| format!("{line:?}: {e}"))?;
                let e = (toks[0].into(), toks[1].into(), toks[2].into(), v);
                match k.as_bytes()[0] {
                    b'c' => caps.push(e),
                    b'l' => inds.push(e),
                    _ => {
                        let declared = |l: &str| inds.iter().any(|(n, ..)| n == l);
                        if !declared(toks[1]) || !declared(toks[2]) {
                            return Err(format!("{line:?} couples an undeclared inductor"));
                        }
                        couplings.push(e)
                    }
                }
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
        inds,
        couplings,
        comments,
    })
}

/// Rebuild the inductance matrix from a parsed network (test-only):
/// `L_ii` from the inductor at port node *i* to ground, `L_ij = k_ij
/// √(L_ii L_jj)` from the `K` line.
#[cfg(test)]
pub fn inductance_from(parsed: &Parsed) -> Vec<Vec<f64>> {
    let n = parsed.ports.len();
    let mut l = vec![vec![0.0; n]; n];
    let mut port_of = std::collections::HashMap::new();
    for (name, a, b, v) in &parsed.inds {
        assert_eq!(b, GROUND, "{name}: inductor not to ground");
        let i = parsed.ports.iter().position(|p| p == a).expect("port node");
        l[i][i] = *v;
        port_of.insert(name.as_str(), i);
    }
    for (_, l1, l2, k) in &parsed.couplings {
        let (i, j) = (port_of[l1.as_str()], port_of[l2.as_str()]);
        let m = k * (l[i][i] * l[j][j]).sqrt();
        l[i][j] = m;
        l[j][i] = m;
    }
    l
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

    /// The inductance comment preamble (no header, no renames).
    fn l_preamble() -> String {
        "* Maxwell inductance matrix as self inductors plus K couplings, henries: L<i>_0 = \
         L_ii from path node i to 0; K<i>_<j> = L_ij / sqrt(L_ii * L_jj)\n\
         * port: a path's source and sink faces both contact the PEC return wall, which is \
         ground 0, so each path is one node; every inductor is declared node -> 0, so k \
         carries the sign of L_ij\n\
         * noise threshold: |k| < 1e-9 is dropped\n"
            .to_string()
    }

    #[test]
    fn one_path_inductance_exact_bytes() {
        let t = inductance_netlist(&s(&["hdr"]), &s(&["core"]), &[vec![2.5e-10]])
            .unwrap()
            .render();
        assert_eq!(
            t,
            format!(
                "* hdr\n{}.subckt LEXTRACT core\nL1_0 core 0 2.5e-10\n.ends LEXTRACT\n",
                l_preamble()
            )
        );
        let p = parse(&t).unwrap();
        assert_eq!(p.name, SUBCKT_NAME_INDUCTANCE);
        assert!(p.caps.is_empty() && p.couplings.is_empty());
        assert_eq!(inductance_from(&p), vec![vec![2.5e-10]]);
    }

    #[test]
    fn two_path_inductance_exact_bytes_and_round_trip() {
        // k = 1e-10 / sqrt(4e-10 * 1e-10) = 0.5 (to round-off).
        let l = vec![vec![4e-10, 1e-10], vec![1e-10, 1e-10]];
        let net = inductance_netlist(&[], &s(&["core", "tube"]), &l).unwrap();
        let k = 1e-10 / (4e-10f64 * 1e-10).sqrt();
        assert!((k - 0.5).abs() < 1e-15, "{k}");
        let t = net.render();
        assert_eq!(
            t,
            format!(
                "{}.subckt LEXTRACT core tube\n\
                 L1_0 core 0 4e-10\n\
                 L2_0 tube 0 1e-10\n\
                 K1_2 L1_0 L2_0 {k:e}\n\
                 .ends LEXTRACT\n",
                l_preamble()
            )
        );
        let p = parse(&t).unwrap();
        assert_eq!(p.ports, s(&["core", "tube"]));
        assert_eq!(p.couplings.len(), 1);
        let back = inductance_from(&p);
        for i in 0..2 {
            for j in 0..2 {
                assert!(
                    (back[i][j] - l[i][j]).abs() <= 1e-15 * l[i][i],
                    "L[{i}][{j}]: {} vs {}",
                    back[i][j],
                    l[i][j]
                );
            }
        }
    }

    #[test]
    fn negative_mutual_is_a_negative_k_without_reordering() {
        let l = vec![vec![2e-9, -0.6e-9], vec![-0.6e-9, 1e-9]];
        let t = inductance_netlist(&[], &s(&["a", "b"]), &l)
            .unwrap()
            .render();
        let p = parse(&t).unwrap();
        // Node order is fixed (node -> 0) for every inductor.
        assert_eq!(
            p.inds,
            vec![
                ("L1_0".into(), "a".into(), "0".into(), 2e-9),
                ("L2_0".into(), "b".into(), "0".into(), 1e-9),
            ]
        );
        let (_, l1, l2, k) = &p.couplings[0];
        assert_eq!((l1.as_str(), l2.as_str()), ("L1_0", "L2_0"));
        assert!(*k < 0.0);
        let back = inductance_from(&p);
        assert!((back[0][1] - l[0][1]).abs() <= 1e-15 * l[0][0]);
    }

    #[test]
    fn three_path_inductance_drops_near_zero_coupling() {
        // a–b and b–c coupled, a–c decoupled at round-off.
        let noise = 1e-22;
        let l = vec![
            vec![3e-9, 1e-9, noise],
            vec![1e-9, 2e-9, -0.5e-9],
            vec![noise, -0.5e-9, 1e-9],
        ];
        let t = inductance_netlist(&[], &s(&["a", "b", "c"]), &l)
            .unwrap()
            .render();
        let k = |i: usize, j: usize| l[i][j] / (l[i][i] * l[j][j]).sqrt();
        let body: Vec<&str> = t.lines().filter(|x| !x.starts_with('*')).collect();
        assert_eq!(
            body,
            [
                ".subckt LEXTRACT a b c",
                "L1_0 a 0 3e-9",
                "L2_0 b 0 2e-9",
                "L3_0 c 0 1e-9",
                &format!("K1_2 L1_0 L2_0 {:e}", k(0, 1)),
                &format!("K2_3 L2_0 L3_0 {:e}", k(1, 2)),
                ".ends LEXTRACT",
            ]
        );
        let dropped: Vec<&str> = t.lines().filter(|x| x.contains("dropped:")).collect();
        assert_eq!(
            dropped,
            [format!(
                "* dropped: K(a, c) = {:e}, |k| < 1e-9 -- below noise threshold",
                k(0, 2)
            )]
        );
        let back = inductance_from(&parse(&t).unwrap());
        for i in 0..3 {
            for j in 0..3 {
                assert!((back[i][j] - l[i][j]).abs() <= 1e-9 * 3e-9, "L[{i}][{j}]");
            }
        }
    }

    #[test]
    fn non_physical_inductance_is_rejected() {
        let rej = |paths: &[&str], l: &[Vec<f64>]| {
            let e = inductance_netlist(&[], &s(paths), l);
            let Err(err) = e else {
                panic!("accepted {l:?}")
            };
            assert!(matches!(err, CliError::SpiceUnsupported { .. }), "{err:?}");
            assert_eq!(err.code(), "invalid_spec");
            err.to_string()
        };
        // |k| > 1: L_12^2 > L_11 L_22.
        let msg = rej(&["a", "b"], &[vec![1e-9, 2e-9], vec![2e-9, 1e-9]]);
        assert!(msg.contains("K(a, b)") && msg.contains("|k| >= 1"), "{msg}");
        // |k| = 1 exactly (singular), and negative.
        let msg = rej(&["a", "b"], &[vec![1e-9, -1e-9], vec![-1e-9, 1e-9]]);
        assert!(msg.contains("|k| >= 1"), "{msg}");
        // Non-positive self inductance.
        let msg = rej(&["a", "b"], &[vec![1e-9, 0.0], vec![0.0, -1e-9]]);
        assert!(msg.contains("L(b, b)"), "{msg}");
        // Malformed.
        rej(&[], &[]);
        rej(&["a", "b"], &[vec![1e-9]]);
        rej(&["a"], &[vec![f64::INFINITY]]);
        rej(&["a"], &[vec![f64::NAN]]);
    }

    #[test]
    fn inductance_nodes_are_sanitized() {
        let l = vec![vec![1e-9, 0.1e-9], vec![0.1e-9, 1e-9]];
        let t = inductance_netlist(&[], &s(&["path 1", "0"]), &l)
            .unwrap()
            .render();
        assert!(t.contains("* node: \"path 1\" -> path_1\n"), "{t}");
        assert!(t.contains("* node: \"0\" -> n_0\n"), "{t}");
        assert!(t.contains(".subckt LEXTRACT path_1 n_0\n"), "{t}");
        assert!(t.contains("L2_0 n_0 0 1e-9\n"), "{t}");
        parse(&t).unwrap();
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
