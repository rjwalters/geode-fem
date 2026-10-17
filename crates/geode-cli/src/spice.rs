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
//! Physically, a path's SPICE port is the **break in that path's current
//! loop at its source face**: the exposed node is the conductor side of
//! the break, `0` is the common PEC return wall on the other side, and a
//! current driven *into* the node is the path's reference current (source
//! face → conductor → sink face → return wall → back to the source face).
//! E.g. `I1 0 core DC 0 AC 1` drives 1 A of reference current around the
//! `core` loop, and `V(core)` is that loop's voltage `jω Σ_j L_core,j I_j`.
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
//!
//! # Options (issue #723)
//!
//! [`SpiceOptions`] carries two opt-in variants; with both off (the
//! default) the output is byte-identical to the plain export above.
//!
//! **`ret_pin`** (`--spice-ret-pin`, both exports): the ground / return
//! reference is exposed as an extra, **last** `.subckt` port named
//! [`RET_PIN`] (`ret`) instead of being tied to the simulator's global
//! node `0` inside the subcircuit, so the instantiating deck decides how
//! to wire it. `ret` must still be given a DC path to node `0` (e.g.
//! through a resistor, as in the README's `RDC r 0 1` guidance) — left
//! floating with no DC path, ngspice fails with `singular matrix: check
//! node r`. `ret` is reserved case-insensitively for that run, so a
//! terminal / path literally named `ret` / `Ret` gets the usual `_2`
//! suffix instead of colliding with the pin.
//!
//! **`positive_k`** (`--spice-positive-k`, inductance only): some
//! simulators (PSpice / OrCAD historically) accept only `0 < k ≤ 1`. A
//! path *i* may be emitted **flipped** — its inductor declared
//! `L<i>_0 <ret> <node_i>` instead of `<node_i> <ret>`, moving its dot to
//! the return side — and every emitted `k_ij` becomes `s_i s_j k_ij` with
//! `s_i = −1` for flipped paths. The port-level network is unchanged:
//! with `S = diag(s)`, the inductor branch voltages / currents are `S V`,
//! `S I` of the port quantities, so `V = jω S (S L S) S I = jω L I` — the
//! *port* reference current of every path (into its node, source → sink)
//! and the report's `l_henry` (signs included) are exactly preserved.
//! Only the flipped inductor's *own* SPICE branch quantities are reversed:
//! ngspice's `i(L<i>_0)` of a flipped path reads the negative of the
//! path's reference current, and its dotted end is the return side.
//!
//! Choosing the flips is signed-graph switching: the emitted couplings
//! are edges signed by `sign(k_ij)`, and a positive edge needs `s_i = s_j`,
//! a negative one `s_i ≠ s_j`. A breadth-first 2-colouring per connected
//! component (lowest-index path of each component unflipped, so the
//! choice is deterministic) finds `s` exactly when every cycle has an even
//! number of negative edges — always for two paths (no cycle), not in
//! general for three or more. When no assignment exists the export fails
//! with [`CliError::SpiceUnsupported`] (`invalid_spec`) naming a
//! frustrated cycle of paths; it never falls back to mixed signs.

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
/// is `k ≈ 0.646`, exactly `6.457431227858365e-1`); it separates a
/// genuinely decoupled pair computed at round-off from a real coupling.
pub const K_DROP_TOL: f64 = 1e-9;

/// SPICE ground node.
const GROUND: &str = "0";

/// Reserved node names (compared case-insensitively).
const RESERVED: [&str; 2] = ["0", "gnd"];

/// The return-reference port name under [`SpiceOptions::ret_pin`].
pub const RET_PIN: &str = "ret";

/// Opt-in export variants (issue #723; see the module docs). The default
/// (both off) is the plain export, byte for byte.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SpiceOptions {
    /// Expose the return as a last `.subckt` port [`RET_PIN`] instead of
    /// the global node `0` (`--spice-ret-pin`).
    pub ret_pin: bool,
    /// Flip path orientations so every emitted `k` is positive, or fail
    /// (`--spice-positive-k`; inductance only, ignored by capacitance).
    pub positive_k: bool,
}

impl SpiceOptions {
    /// The node every ground / return branch connects to.
    fn return_node(self) -> &'static str {
        if self.ret_pin { RET_PIN } else { GROUND }
    }

    /// Extra reserved node names for this run.
    fn reserved(self) -> &'static [&'static str] {
        if self.ret_pin { &[RET_PIN] } else { &[] }
    }
}

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
#[cfg(test)]
pub fn node_names(terminals: &[String]) -> Vec<String> {
    node_names_reserving(terminals, &[])
}

/// Sanitized, collision-free SPICE node names for `terminals`, with
/// `extra` names (e.g. [`RET_PIN`]) reserved alongside `0` / `gnd`.
fn node_names_reserving(terminals: &[String], extra: &[&str]) -> Vec<String> {
    let mut taken: Vec<String> = RESERVED
        .iter()
        .chain(extra)
        .map(|s| s.to_string())
        .collect();
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
/// `header` becomes the leading comment lines; `opts.ret_pin` exposes the
/// ground reference as a `ret` port (`opts.positive_k` does not apply).
pub fn capacitance_netlist(
    header: &[String],
    terminals: &[String],
    c: &[Vec<f64>],
    c_sigma: &[f64],
    opts: SpiceOptions,
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
    let nodes = node_names_reserving(terminals, opts.reserved());
    let ret = opts.return_node();

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
    if opts.ret_pin {
        comments.push(format!(
            "return: the ground reference is the last port {RET_PIN}, not global node 0; \
             wire it at the instance"
        ));
    }
    for (t, node) in terminals.iter().zip(&nodes) {
        if t != node {
            comments.push(format!("node: {t:?} -> {node}"));
        }
    }

    let mut elements = Vec::new();
    for i in 0..n {
        let mut branches = vec![(format!("C{}_0", i + 1), ret, "ground", c_sigma[i])];
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
    let mut ports = nodes;
    if opts.ret_pin {
        ports.push(RET_PIN.to_string());
    }
    Ok(Netlist {
        name: SUBCKT_NAME,
        comments,
        ports,
        elements,
    })
}

/// Build the self-inductor + `K`-coupling network of the Maxwell
/// inductance matrix `l` (H, row-major, `paths` order; see the module
/// docs). `header` becomes the leading comment lines; `opts` selects the
/// `ret`-pin and positive-`k` variants ([`SpiceOptions`]).
pub fn inductance_netlist(
    header: &[String],
    paths: &[String],
    l: &[Vec<f64>],
    opts: SpiceOptions,
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
    let nodes = node_names_reserving(paths, opts.reserved());
    let ret = opts.return_node();

    // Couplings first (validated / thresholded), so the orientation choice
    // sees exactly the emitted set.
    let mut emitted: Vec<(usize, usize, f64)> = Vec::new();
    let mut dropped: Vec<String> = Vec::new();
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
                dropped.push(format!(
                    "dropped: {label} = {k:e}, |k| < {K_DROP_TOL:e} -- below noise threshold"
                ));
            } else {
                emitted.push((i, j, k));
            }
        }
    }
    let flip: Vec<bool> = if opts.positive_k {
        positive_k_orientation(paths, &emitted)?
    } else {
        vec![false; n]
    };

    let mut comments: Vec<String> = header.to_vec();
    comments.push(format!(
        "Maxwell inductance matrix as self inductors plus K couplings, henries: \
         L<i>_0 = L_ii from path node i to {ret}; K<i>_<j> = L_ij / sqrt(L_ii * L_jj)"
    ));
    let wall = if opts.ret_pin {
        format!("the last port {RET_PIN} (not global node 0)")
    } else {
        format!("ground {GROUND}")
    };
    let orientation = if opts.positive_k {
        format!(
            "positive-k: every inductor is declared node -> {ret} except the flipped paths \
             below (declared {ret} -> node), and k_ij = s_i s_j L_ij / sqrt(L_ii * L_jj) with \
             s = -1 on flipped paths, so every k is positive; the port currents and the L \
             matrix seen at the ports are unchanged"
        )
    } else {
        format!("every inductor is declared node -> {ret}, so k carries the sign of L_ij")
    };
    comments.push(format!(
        "port: a path's source and sink faces both contact the PEC return wall, which is \
         {wall}, so each path is one node; {orientation}"
    ));
    comments.push(format!("noise threshold: |k| < {K_DROP_TOL:e} is dropped"));
    for (p, node) in paths.iter().zip(&nodes) {
        if p != node {
            comments.push(format!("node: {p:?} -> {node}"));
        }
    }
    if opts.positive_k {
        let flipped: Vec<usize> = (0..n).filter(|&i| flip[i]).collect();
        if flipped.is_empty() {
            comments.push("flipped: none (every emitted k is already positive)".to_string());
        }
        for i in flipped {
            comments.push(format!(
                "flipped: {:?} (L{}_0 declared {ret} -> {}; its i(L{}_0) is minus the path's \
                 reference current)",
                paths[i],
                i + 1,
                nodes[i],
                i + 1
            ));
        }
    }
    comments.extend(dropped);

    let ind = |i: usize| format!("L{}_0", i + 1);
    let mut elements: Vec<Element> = (0..n)
        .map(|i| {
            let (a, b) = if flip[i] {
                (ret.to_string(), nodes[i].clone())
            } else {
                (nodes[i].clone(), ret.to_string())
            };
            Element::Inductor {
                name: ind(i),
                a,
                b,
                henry: l[i][i],
            }
        })
        .collect();
    for (i, j, k) in emitted {
        // Negation is exact, so a flipped k still round-trips bit for bit.
        let k = if flip[i] != flip[j] { -k } else { k };
        elements.push(Element::Coupling {
            name: format!("K{}_{}", i + 1, j + 1),
            l1: ind(i),
            l2: ind(j),
            k,
        });
    }
    let mut ports = nodes;
    if opts.ret_pin {
        ports.push(RET_PIN.to_string());
    }
    Ok(Netlist {
        name: SUBCKT_NAME_INDUCTANCE,
        comments,
        ports,
        elements,
    })
}

/// Signed-graph switching for `--spice-positive-k`: per-path flips
/// (`true` = flipped) making `s_i s_j sign(k_ij) > 0` on every emitted
/// coupling `(i, j, k_ij)`, found by a breadth-first 2-colouring (the
/// lowest-index path of each connected component unflipped). Fails with
/// `invalid_spec` naming a cycle with an odd number of negative
/// couplings when no such assignment exists.
fn positive_k_orientation(
    paths: &[String],
    emitted: &[(usize, usize, f64)],
) -> Result<Vec<bool>, CliError> {
    let n = paths.len();
    let mut adj: Vec<Vec<(usize, bool)>> = vec![Vec::new(); n];
    for &(i, j, k) in emitted {
        adj[i].push((j, k < 0.0));
        adj[j].push((i, k < 0.0));
    }
    let mut flip: Vec<Option<bool>> = vec![None; n];
    let mut parent: Vec<Option<usize>> = vec![None; n];
    for root in 0..n {
        if flip[root].is_some() {
            continue;
        }
        flip[root] = Some(false);
        let mut queue = std::collections::VecDeque::from([root]);
        while let Some(u) = queue.pop_front() {
            let fu = flip[u].expect("queued paths are coloured");
            for &(v, negative) in &adj[u] {
                let want = fu != negative;
                match flip[v] {
                    None => {
                        flip[v] = Some(want);
                        parent[v] = Some(u);
                        queue.push_back(v);
                    }
                    Some(fv) if fv == want => {}
                    Some(_) => return Err(frustrated(paths, emitted, &parent, u, v)),
                }
            }
        }
    }
    Ok(flip.into_iter().map(|f| f == Some(true)).collect())
}

/// The `--spice-positive-k` infeasibility error: the BFS-tree cycle
/// closed by the conflicting edge `(u, v)`, with its couplings.
fn frustrated(
    paths: &[String],
    emitted: &[(usize, usize, f64)],
    parent: &[Option<usize>],
    u: usize,
    v: usize,
) -> CliError {
    let to_root = |mut x: usize| {
        let mut chain = vec![x];
        while let Some(p) = parent[x] {
            chain.push(p);
            x = p;
        }
        chain
    };
    let (cu, cv) = (to_root(u), to_root(v));
    // Lowest common ancestor: the first node of u's chain on v's chain.
    let (iu, lca) = cu
        .iter()
        .enumerate()
        .find(|(_, x)| cv.contains(x))
        .map(|(i, &x)| (i, x))
        .expect("same BFS tree");
    let iv = cv.iter().position(|&x| x == lca).expect("lca on v's chain");
    // u -> … -> lca -> … -> v -> u.
    let mut cycle: Vec<usize> = cu[..=iu].to_vec();
    cycle.extend(cv[..iv].iter().rev());
    cycle.push(u);
    let k_of = |a: usize, b: usize| {
        emitted
            .iter()
            .find(|&&(i, j, _)| (i, j) == (a.min(b), a.max(b)))
            .map(|&(_, _, k)| k)
            .expect("cycle edges are emitted couplings")
    };
    let names: Vec<&str> = cycle.iter().map(|&i| paths[i].as_str()).collect();
    let ks: Vec<String> = cycle
        .windows(2)
        .map(|w| {
            format!(
                "K({}, {}) = {:e}",
                paths[w[0]],
                paths[w[1]],
                k_of(w[0], w[1])
            )
        })
        .collect();
    unsupported(&format!(
        "--spice-positive-k: no choice of path orientations makes every k positive: the cycle \
         {} has an odd number of negative couplings ({}), and flipping a path negates all of its \
         couplings, so the product of k signs around a cycle cannot change; drop \
         --spice-positive-k for this spec (the signed-k netlist is exact for simulators that \
         accept -1 < k < 1, e.g. ngspice)",
        names.join(" -> "),
        ks.join(", ")
    ))
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
pub fn render_capacitance(
    report: &CapacitanceReport,
    opts: SpiceOptions,
) -> Result<String, CliError> {
    let header = provenance_header(&report.provenance, &report.mesh);
    Ok(capacitance_netlist(
        &header,
        &report.terminals,
        &report.c_farad,
        &report.c_sigma_farad,
        opts,
    )?
    .render())
}

/// Render the SPICE subcircuit of a finished inductance report (pure; no
/// I/O).
pub fn render_inductance(
    report: &InductanceReport,
    opts: SpiceOptions,
) -> Result<String, CliError> {
    let header = provenance_header(&report.provenance, &report.mesh);
    Ok(inductance_netlist(&header, &report.paths, &report.l_henry, opts)?.render())
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
pub fn write(
    path: &Path,
    report: &CapacitanceReport,
    opts: SpiceOptions,
) -> Result<FileRef, CliError> {
    write_text(path, render_capacitance(report, opts)?)
}

/// [`write()`] for an inductance report ([`render_inductance`]).
pub fn write_inductance(
    path: &Path,
    report: &InductanceReport,
    opts: SpiceOptions,
) -> Result<FileRef, CliError> {
    write_text(path, render_inductance(report, opts)?)
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

/// Rebuild the inductance matrix from a parsed network (test-only),
/// as seen at the ports: `L_ii` from the inductor between port node *i*
/// and the return (ground `0`, or a last `ret` port), `L_ij = s_i s_j k_ij
/// √(L_ii L_jj)` from the `K` line, with `s_i = −1` for an inductor
/// declared return → node (its dot on the return side).
#[cfg(test)]
pub fn inductance_from(parsed: &Parsed) -> Vec<Vec<f64>> {
    let ret = return_of(parsed);
    let paths = &parsed.ports[..path_count(parsed)];
    let n = paths.len();
    let mut l = vec![vec![0.0; n]; n];
    let mut port_of = std::collections::HashMap::new();
    for (name, a, b, v) in &parsed.inds {
        let (node, sign) = if b == ret {
            (a, 1.0)
        } else if a == ret {
            (b, -1.0)
        } else {
            panic!("{name}: inductor not to the return {ret}")
        };
        let i = paths.iter().position(|p| p == node).expect("port node");
        l[i][i] = *v;
        port_of.insert(name.as_str(), (i, sign));
    }
    for (_, l1, l2, k) in &parsed.couplings {
        let ((i, si), (j, sj)) = (port_of[l1.as_str()], port_of[l2.as_str()]);
        let m = si * sj * k * (l[i][i] * l[j][j]).sqrt();
        l[i][j] = m;
        l[j][i] = m;
    }
    l
}

/// The return node of a parsed network: a last port named [`RET_PIN`]
/// (the `ret_pin` variant), else ground `0`.
#[cfg(test)]
fn return_of(parsed: &Parsed) -> &str {
    match parsed.ports.last() {
        Some(p) if p == RET_PIN => RET_PIN,
        _ => GROUND,
    }
}

/// The number of matrix ports (every port but a trailing `ret` pin).
#[cfg(test)]
fn path_count(parsed: &Parsed) -> usize {
    parsed.ports.len() - usize::from(return_of(parsed) == RET_PIN)
}

/// Rebuild the Maxwell matrix from a parsed network (test-only):
/// `C_ii = Σ` of every capacitor at node *i*, `C_ij = −C_mutual(i, j)`
/// (the return, `0` or a trailing `ret` pin, is not a matrix port).
#[cfg(test)]
pub fn maxwell_from(parsed: &Parsed) -> Vec<Vec<f64>> {
    let n = path_count(parsed);
    let idx = |node: &str| parsed.ports[..n].iter().position(|p| p == node);
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
            SpiceOptions::default(),
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
        let net = capacitance_netlist(&[], &s(&["a", "b", "c"]), &c, &sig, SpiceOptions::default())
            .unwrap();
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
        let t = capacitance_netlist(&[], &s(&["x", "y", "z"]), &c, &sig, SpiceOptions::default())
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
        let t = capacitance_netlist(
            &[],
            &s(&["sig+", "sig-"]),
            &c,
            &row_sums(&c),
            SpiceOptions::default(),
        )
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
        let e = capacitance_netlist(
            &[],
            &s(&["a", "b"]),
            &c,
            &row_sums(&c),
            SpiceOptions::default(),
        );
        assert!(matches!(e, Err(CliError::SpiceUnsupported { .. })), "{e:?}");
        // Negative row sum → negative ground capacitance.
        let c = vec![vec![1e-12, -2e-12], vec![-2e-12, 3e-12]];
        let e = capacitance_netlist(
            &[],
            &s(&["a", "b"]),
            &c,
            &row_sums(&c),
            SpiceOptions::default(),
        );
        let Err(err) = e else { panic!("accepted") };
        assert_eq!(err.code(), "invalid_spec");
        assert!(err.to_string().contains("C(a, ground)"), "{err}");
        // But a tiny positive off-diagonal is noise, dropped not rejected.
        let c = vec![vec![2e-12, 1e-24], vec![1e-24, 2e-12]];
        capacitance_netlist(
            &[],
            &s(&["a", "b"]),
            &c,
            &row_sums(&c),
            SpiceOptions::default(),
        )
        .unwrap();
    }

    #[test]
    fn malformed_matrices_are_rejected() {
        let e = capacitance_netlist(&[], &[], &[], &[], SpiceOptions::default());
        assert!(matches!(e, Err(CliError::SpiceUnsupported { .. })));
        let e = capacitance_netlist(
            &[],
            &s(&["a", "b"]),
            &[vec![1.0]],
            &[1.0],
            SpiceOptions::default(),
        );
        assert!(matches!(e, Err(CliError::SpiceUnsupported { .. })));
        let e = capacitance_netlist(
            &[],
            &s(&["a"]),
            &[vec![f64::NAN]],
            &[1.0],
            SpiceOptions::default(),
        );
        assert!(matches!(e, Err(CliError::SpiceUnsupported { .. })));
        let e = capacitance_netlist(
            &[],
            &s(&["a"]),
            &[vec![0.0]],
            &[0.0],
            SpiceOptions::default(),
        );
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
        let t = inductance_netlist(
            &s(&["hdr"]),
            &s(&["core"]),
            &[vec![2.5e-10]],
            SpiceOptions::default(),
        )
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
        let net =
            inductance_netlist(&[], &s(&["core", "tube"]), &l, SpiceOptions::default()).unwrap();
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
        let t = inductance_netlist(&[], &s(&["a", "b"]), &l, SpiceOptions::default())
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
        let t = inductance_netlist(&[], &s(&["a", "b", "c"]), &l, SpiceOptions::default())
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
            let e = inductance_netlist(&[], &s(paths), l, SpiceOptions::default());
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
        let t = inductance_netlist(&[], &s(&["path 1", "0"]), &l, SpiceOptions::default())
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
        let t = capacitance_netlist(
            &s(&["spec: a\nb"]),
            &s(&["x"]),
            &c,
            &[1e-12],
            SpiceOptions::default(),
        )
        .unwrap()
        .render();
        assert!(t.starts_with("* spec: a b\n"));
        parse(&t).unwrap();
    }

    const POS_K: SpiceOptions = SpiceOptions {
        ret_pin: false,
        positive_k: true,
    };
    const RET: SpiceOptions = SpiceOptions {
        ret_pin: true,
        positive_k: false,
    };

    /// Asserts the port-level matrix of `t` equals `l` (signs included).
    fn assert_l_round_trips(t: &str, l: &[Vec<f64>]) {
        let back = inductance_from(&parse(t).unwrap());
        assert_eq!(back.len(), l.len());
        for i in 0..l.len() {
            for j in 0..l.len() {
                assert!(
                    (back[i][j] - l[i][j]).abs() <= 1e-15 * l[i][i],
                    "L[{i}][{j}]: {} vs {}\n{t}",
                    back[i][j],
                    l[i][j]
                );
            }
        }
    }

    fn body(t: &str) -> Vec<&str> {
        t.lines().filter(|x| !x.starts_with('*')).collect()
    }

    #[test]
    fn positive_k_flips_the_second_of_two_paths() {
        let l = vec![vec![2e-9, -0.6e-9], vec![-0.6e-9, 1e-9]];
        let plain = inductance_netlist(&[], &s(&["a", "b"]), &l, SpiceOptions::default())
            .unwrap()
            .render();
        let t = inductance_netlist(&[], &s(&["a", "b"]), &l, POS_K)
            .unwrap()
            .render();
        let k = -0.6e-9 / (2e-9f64 * 1e-9).sqrt();
        assert!(k < 0.0);
        // b's inductor is declared return -> node; k is exactly negated.
        assert_eq!(
            body(&t),
            [
                ".subckt LEXTRACT a b",
                "L1_0 a 0 2e-9",
                "L2_0 0 b 1e-9",
                &format!("K1_2 L1_0 L2_0 {:e}", -k),
                ".ends LEXTRACT",
            ]
        );
        assert!(
            plain.contains(&format!("K1_2 L1_0 L2_0 {k:e}\n")),
            "{plain}"
        );
        assert!(
            t.contains(
                "* flipped: \"b\" (L2_0 declared 0 -> b; its i(L2_0) is minus the path's \
                 reference current)\n"
            ),
            "{t}"
        );
        assert!(t.contains("so every k is positive"), "{t}");
        // The port-level matrix, negative mutual included, is unchanged.
        assert_l_round_trips(&t, &l);
        assert_l_round_trips(&plain, &l);
    }

    #[test]
    fn positive_k_without_negative_couplings_changes_only_comments() {
        let l = vec![vec![4e-10, 1e-10], vec![1e-10, 1e-10]];
        let plain = inductance_netlist(&[], &s(&["core", "tube"]), &l, SpiceOptions::default())
            .unwrap()
            .render();
        let t = inductance_netlist(&[], &s(&["core", "tube"]), &l, POS_K)
            .unwrap()
            .render();
        assert_eq!(body(&t), body(&plain));
        assert!(t.contains("* flipped: none (every emitted k is already positive)\n"));
        // One path: nothing to couple, trivially satisfied.
        let t = inductance_netlist(&[], &s(&["core"]), &[vec![2.5e-10]], POS_K)
            .unwrap()
            .render();
        assert_eq!(
            body(&t),
            [
                ".subckt LEXTRACT core",
                "L1_0 core 0 2.5e-10",
                ".ends LEXTRACT"
            ]
        );
    }

    #[test]
    fn positive_k_balanced_three_path_cycle_is_switched() {
        // Two negative edges around the a-b-c triangle: balanced, so flipping
        // b and c (a is the unflipped root) makes all three k positive.
        let l = vec![
            vec![3e-9, -1e-9, -0.5e-9],
            vec![-1e-9, 2e-9, 0.4e-9],
            vec![-0.5e-9, 0.4e-9, 1e-9],
        ];
        let t = inductance_netlist(&[], &s(&["a", "b", "c"]), &l, POS_K)
            .unwrap()
            .render();
        let p = parse(&t).unwrap();
        let ends: Vec<(&str, &str)> = p
            .inds
            .iter()
            .map(|(_, a, b, _)| (a.as_str(), b.as_str()))
            .collect();
        assert_eq!(ends, [("a", "0"), ("0", "b"), ("0", "c")]);
        assert_eq!(p.couplings.len(), 3);
        assert!(p.couplings.iter().all(|c| c.3 > 0.0), "{t}");
        assert_l_round_trips(&t, &l);
    }

    #[test]
    fn positive_k_frustrated_cycle_is_a_clear_error() {
        let rej = |l: &[Vec<f64>]| {
            let e = inductance_netlist(&[], &s(&["a", "b", "c"]), l, POS_K);
            let Err(err) = e else {
                panic!("accepted {l:?}")
            };
            assert!(matches!(err, CliError::SpiceUnsupported { .. }), "{err:?}");
            assert_eq!(err.code(), "invalid_spec");
            err.to_string()
        };
        // Three negative edges (odd): the classic frustrated triangle.
        let m = -0.3e-9;
        let msg = rej(&[vec![1e-9, m, m], vec![m, 1e-9, m], vec![m, m, 1e-9]]);
        assert!(msg.contains("--spice-positive-k"), "{msg}");
        assert!(msg.contains("odd number of negative couplings"), "{msg}");
        assert!(msg.contains("the cycle b -> a -> c -> b"), "{msg}");
        assert!(
            msg.contains("K(b, a) = ") && msg.contains("K(c, b) = "),
            "{msg}"
        );
        // One negative edge (also odd).
        let msg = rej(&[
            vec![1e-9, -0.3e-9, 0.2e-9],
            vec![-0.3e-9, 1e-9, 0.2e-9],
            vec![0.2e-9, 0.2e-9, 1e-9],
        ]);
        assert!(msg.contains("odd number of negative couplings"), "{msg}");
        // The same matrix is fine without the flag.
        inductance_netlist(
            &[],
            &s(&["a", "b", "c"]),
            &[vec![1e-9, m, m], vec![m, 1e-9, m], vec![m, m, 1e-9]],
            SpiceOptions::default(),
        )
        .unwrap();
    }

    #[test]
    fn positive_k_ignores_dropped_couplings() {
        // One negative edge, but it is round-off noise and dropped, so the
        // emitted graph is a path (no cycle) and needs no flip at all.
        let noise = -1e-22;
        let l = vec![
            vec![1e-9, 0.3e-9, noise],
            vec![0.3e-9, 1e-9, 0.2e-9],
            vec![noise, 0.2e-9, 1e-9],
        ];
        let t = inductance_netlist(&[], &s(&["a", "b", "c"]), &l, POS_K)
            .unwrap()
            .render();
        assert!(t.contains("* flipped: none"), "{t}");
        assert!(t.contains("* dropped: K(a, c) = "), "{t}");
        // Two disconnected components: each root stays unflipped.
        let l = vec![
            vec![1e-9, -0.3e-9, 0.0, 0.0],
            vec![-0.3e-9, 1e-9, 0.0, 0.0],
            vec![0.0, 0.0, 1e-9, -0.1e-9],
            vec![0.0, 0.0, -0.1e-9, 1e-9],
        ];
        let t = inductance_netlist(&[], &s(&["a", "b", "c", "d"]), &l, POS_K)
            .unwrap()
            .render();
        let flipped: Vec<&str> = t.lines().filter(|x| x.starts_with("* flipped:")).collect();
        assert_eq!(flipped.len(), 2, "{t}");
        assert!(
            flipped[0].starts_with("* flipped: \"b\"")
                && flipped[1].starts_with("* flipped: \"d\"")
        );
        assert_l_round_trips(&t, &l);
    }

    #[test]
    fn ret_pin_capacitance_is_a_last_port_and_collision_safe() {
        let c = vec![
            vec![5e-12, -1e-12, -2e-12, -0.5e-12],
            vec![-1e-12, 4e-12, -0.5e-12, -0.25e-12],
            vec![-2e-12, -0.5e-12, 7e-12, -1e-12],
            vec![-0.5e-12, -0.25e-12, -1e-12, 3e-12],
        ];
        let names = s(&["ret", "Ret", "RET", "x"]);
        let t = capacitance_netlist(&[], &names, &c, &row_sums(&c), RET)
            .unwrap()
            .render();
        assert!(
            t.contains(".subckt CEXTRACT ret_2 Ret_3 RET_4 x ret\n"),
            "{t}"
        );
        assert!(t.contains("* node: \"ret\" -> ret_2\n"), "{t}");
        assert!(
            t.contains("* return: the ground reference is the last port ret"),
            "{t}"
        );
        let p = parse(&t).unwrap();
        assert!(
            p.caps.iter().all(|(_, a, b, _)| a != "0" && b != "0"),
            "{t}"
        );
        assert_eq!(p.caps.iter().filter(|c| c.2 == "ret").count(), 4);
        let back = maxwell_from(&p);
        for i in 0..4 {
            for j in 0..4 {
                assert!(
                    (back[i][j] - c[i][j]).abs() <= 1e-15 * c[i][i],
                    "C[{i}][{j}]"
                );
            }
        }
        // Default: `ret` is an ordinary name and ground stays `0`.
        let t = capacitance_netlist(&[], &names, &c, &row_sums(&c), SpiceOptions::default())
            .unwrap()
            .render();
        assert!(t.contains(".subckt CEXTRACT ret Ret_2 RET_3 x\n"), "{t}");
        assert!(!t.contains("* return:"));
    }

    #[test]
    fn ret_pin_inductance_with_and_without_positive_k() {
        let l = vec![vec![2e-9, -0.6e-9], vec![-0.6e-9, 1e-9]];
        let paths = s(&["Ret", "b"]);
        let t = inductance_netlist(&[], &paths, &l, RET).unwrap().render();
        let k = -0.6e-9 / (2e-9f64 * 1e-9).sqrt();
        assert_eq!(
            body(&t),
            [
                ".subckt LEXTRACT Ret_2 b ret",
                "L1_0 Ret_2 ret 2e-9",
                "L2_0 b ret 1e-9",
                &format!("K1_2 L1_0 L2_0 {k:e}"),
                ".ends LEXTRACT",
            ]
        );
        assert!(
            t.contains("which is the last port ret (not global node 0)"),
            "{t}"
        );
        assert!(
            t.contains("every inductor is declared node -> ret, so k"),
            "{t}"
        );
        assert_l_round_trips(&t, &l);
        let both = SpiceOptions {
            ret_pin: true,
            positive_k: true,
        };
        let t = inductance_netlist(&[], &paths, &l, both).unwrap().render();
        assert_eq!(
            body(&t),
            [
                ".subckt LEXTRACT Ret_2 b ret",
                "L1_0 Ret_2 ret 2e-9",
                "L2_0 ret b 1e-9",
                &format!("K1_2 L1_0 L2_0 {:e}", -k),
                ".ends LEXTRACT",
            ]
        );
        assert_l_round_trips(&t, &l);
    }
}
