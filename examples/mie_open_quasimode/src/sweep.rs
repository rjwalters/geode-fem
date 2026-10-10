//! σ₀ continuation sweep of the matched-UPML TE₁ triplets (issue #1026).
//!
//! At `σ₀ = 25` the frozen-ω matched-UPML pencil has two complete TE₁
//! triplets, about 26 % low and 19 % high on `Re(k)` against the
//! open-space TE₁,₁ root, and the field-character projection of
//! `geode_core::postproc::mode_character` cannot say which (if either) is
//! the TE₁,₁ quasi-mode: it fixes the polarisation and `l`, not the radial
//! order. This sweep follows each TE₁ triplet **by identity** between
//! `σ₀ = 5`, where the triplet is unambiguous, and `σ₀ = 25`, in both
//! directions.
//!
//! # Method
//!
//! Λ stays frozen at `ω₀ = Re(k)` of the analytic TE₁,₁ root (1.88074) at
//! every step, exactly as in the three-point benchmark, so successive rows
//! differ only in `σ₀`. Each step is the same sparse shift-invert solve
//! (the [`N_NEAR_SHIFT`] converged pairs nearest
//! `σ = ω₀²`), classified with `classify_sphere_modes`.
//!
//! A **branch** is a 3-dimensional eigenvector subspace carried from step
//! to step. The eigenvectors of successive steps live in the same space
//! (the same mesh and edge numbering; only the PML tensors change), so they
//! are compared directly, with the Euclidean inner product on the edge
//! DOFs:
//!
//! - **capture** of a mode `v` by a branch with orthonormal basis `Q`:
//!   `‖Qᴴ v‖² / ‖v‖²`.
//! - the branch's continuation at the next step is the three modes with
//!   the largest capture over all edge DOFs, and the **link overlap** is
//!   `‖Q_prevᴴ Q_next‖_F² / 3` (1 = the same subspace). The same two
//!   numbers restricted to the edges of the dielectric ball (where the
//!   medium does not change with `σ₀`) are reported as `*_ball`.
//! - `fourth_capture_*` is the largest capture among the remaining modes:
//!   a value close to `third_capture_*` means the continuation is
//!   ambiguous at that step (a crossing or a hybridisation).
//!
//! The members of a near-degenerate triplet rotate freely within the
//! triplet between steps, so only the subspace is compared. The projection
//! coefficients onto `MultipoleFamily::member_field` are not used for
//! linking: they depend on each mode's own `k`, which is exactly what
//! changes along a branch.
//!
//! Every identified TE₁ triplet that is not exactly some branch's
//! continuation seeds a new branch; its row records its overlap with every
//! branch's current subspace (`seed_overlaps`). Rows also record how many
//! continuation members belong to an identified TE₁ triplet of the step
//! (`members_in_te1_triplet`), which follows a branch through a crossing
//! that the subspace overlap alone cannot resolve.
//!
//! The Euclidean overlap does not separate two **different** TE₁ triplets
//! of one step well: they share the angular structure, and the
//! eigenvectors of the complex-symmetric pencil are not orthogonal in this
//! inner product. `[[step]].te1_cross_overlap` records that baseline.
//!
//! Writes `benchmarks/mie_sphere/open_sigma_sweep.toml`.

use std::fs;
use std::path::PathBuf;

use burn::prelude::Backend;
use faer::c64;

use geode_core::analytic::mie::MieRootComplex;
use geode_core::mesh::{PHYS_SPHERE_INTERIOR, SphereFixture};
use geode_core::postproc::mode_character::{ClassifiedMode, MultipoleFamily};
use geode_util::fixture::BackendInfo;

use crate::{N_INSIDE, N_NEAR_SHIFT, solve_frozen_omega_with_vectors};

/// The `l = 1` magnetic-dipole family.
const TE1: MultipoleFamily = MultipoleFamily::ALL[0];

/// Log every mode whose TE₁ overlap is at least this (diagnostic log
/// only; nothing in the table depends on it).
const TE1_CONTENT_LOG: f64 = 0.1;

/// Sweep grid: `σ₀ = start, start + step, …, end` (inclusive).
pub(crate) struct SweepGrid {
    pub start: f64,
    pub end: f64,
    pub step: f64,
}

impl SweepGrid {
    fn values(&self) -> Vec<f64> {
        let n = ((self.end - self.start) / self.step).round() as usize;
        (0..=n).map(|i| self.start + i as f64 * self.step).collect()
    }
}

fn inner(a: &[c64], b: &[c64]) -> c64 {
    a.iter().zip(b).map(|(x, y)| x.conj() * *y).sum()
}

fn norm(a: &[c64]) -> f64 {
    a.iter().map(|x| x.norm_sqr()).sum::<f64>().sqrt()
}

fn masked(v: &[c64], mask: &[bool]) -> Vec<c64> {
    v.iter()
        .zip(mask)
        .map(|(x, &m)| if m { *x } else { c64::new(0.0, 0.0) })
        .collect()
}

/// Orthonormal basis (modified Gram–Schmidt, applied twice, Hermitian
/// inner product) of the span of `vs`, each restricted to `mask`.
/// Numerically dependent directions are dropped.
fn orthonormal_basis(vs: &[&[c64]], mask: &[bool]) -> Vec<Vec<c64>> {
    let mut q: Vec<Vec<c64>> = Vec::new();
    for v in vs {
        let mut w = masked(v, mask);
        let norm0 = norm(&w);
        for _ in 0..2 {
            for b in &q {
                let c = inner(b, &w);
                for (wi, bi) in w.iter_mut().zip(b) {
                    *wi -= c * *bi;
                }
            }
        }
        let n = norm(&w);
        if n > 0.0 && n > 1e-10 * norm0 {
            q.push(w.iter().map(|x| *x / n).collect());
        }
    }
    q
}

/// `‖Qᴴ v‖² / ‖v‖²` for `v` restricted to `mask`.
fn capture(q: &[Vec<c64>], v: &[c64], mask: &[bool]) -> f64 {
    let vm = masked(v, mask);
    let n2 = vm.iter().map(|x| x.norm_sqr()).sum::<f64>();
    if n2 == 0.0 {
        return 0.0;
    }
    q.iter().map(|b| inner(b, &vm).norm_sqr()).sum::<f64>() / n2
}

/// `‖Qaᴴ Qb‖_F² / max(dim)` between two orthonormal bases.
fn subspace_overlap(qa: &[Vec<c64>], qb: &[Vec<c64>]) -> f64 {
    let d = qa.len().max(qb.len());
    if d == 0 {
        return 0.0;
    }
    let s: f64 = qa
        .iter()
        .flat_map(|a| qb.iter().map(move |b| inner(a, b).norm_sqr()))
        .sum();
    s / d as f64
}

/// Every complete TE₁ triplet of a solve, as sorted index lists into
/// `modes`, ordered by `Re(k)`.
fn te1_triplets(modes: &[ClassifiedMode]) -> Vec<Vec<usize>> {
    let mut ids: Vec<usize> = Vec::new();
    let mut out: Vec<Vec<usize>> = Vec::new();
    for m in modes {
        if let Some(id) = m.multiplet_id
            && m.is_member_of(TE1)
            && !ids.contains(&id)
        {
            ids.push(id);
            out.push(
                (0..modes.len())
                    .filter(|&i| modes[i].multiplet_id == Some(id))
                    .collect(),
            );
        }
    }
    out.sort_by(|a, b| {
        mean_k(modes, a)
            .re
            .partial_cmp(&mean_k(modes, b).re)
            .unwrap()
    });
    out
}

/// Mean `k` of a group of modes.
fn mean_k(modes: &[ClassifiedMode], idx: &[usize]) -> c64 {
    let s: c64 = idx.iter().map(|&i| modes[i].k).sum();
    s / idx.len() as f64
}

/// A tracked branch and its current subspace (all edge DOFs and in-ball).
struct Branch {
    id: usize,
    q_full: Vec<Vec<c64>>,
    q_ball: Vec<Vec<c64>>,
}

/// One `(pass, σ₀, branch)` row of the sweep table.
struct SweepRow {
    /// `"forward"` (increasing σ₀) or `"backward"`.
    pass: &'static str,
    sigma_0: f64,
    branch: usize,
    /// `true` on the step that seeded the branch (link fields are NaN).
    seed: bool,
    /// The three continuation modes, by `Re(k)`.
    members: Vec<ClassifiedMode>,
    /// The three continuation modes are exactly one complete TE₁ triplet.
    identified: bool,
    /// Number of continuation members that belong to some complete TE₁
    /// triplet of the step, and the mean `k` of the triplet holding most
    /// of them.
    members_in_te1_triplet: usize,
    holding_triplet_k: Option<c64>,
    link_overlap_full: f64,
    link_overlap_ball: f64,
    third_capture_full: f64,
    fourth_capture_full: f64,
    third_capture_ball: f64,
    fourth_capture_ball: f64,
    /// Ranking by in-ball capture picks the same three modes.
    ball_choice_agrees: bool,
    /// Seed rows: `(branch id, overlap)` of the new triplet with every
    /// existing branch's current subspace, all edge DOFs.
    seed_overlaps: Vec<(usize, f64)>,
}

impl SweepRow {
    fn mean_ball_fraction(&self) -> f64 {
        self.members
            .iter()
            .map(|m| m.character.ball_energy_fraction)
            .sum::<f64>()
            / self.members.len() as f64
    }

    /// One-line log / summary row.
    fn line(&self, root: &MieRootComplex) -> String {
        let (re, im, q) = means(&self.members);
        let holding = match self.holding_triplet_k {
            Some(k) => format!(
                "{} in TE1 triplet @ {:.4}{:+.4}j",
                self.members_in_te1_triplet, k.re, k.im
            ),
            None => "0 in a TE1 triplet".to_string(),
        };
        let seed = if self.seed {
            format!(
                " (seed; overlaps {})",
                self.seed_overlaps
                    .iter()
                    .map(|(b, o)| format!("b{b} {o:.3}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        } else {
            String::new()
        };
        format!(
            "{} b{} σ₀ = {:>6.3}  k = {re:.4}{im:+.4}j ({:+.1}%)  Q = {q:.3}  TE1 {:.3}–{:.3}  \
             E ball {:.2}  link full/ball {:.4}/{:.4}  3rd/4th full {:.3}/{:.3} ball {:.3}/{:.3}{}  \
             {holding}  {}{seed}",
            &self.pass[..1],
            self.branch,
            self.sigma_0,
            (re - root.re_k) / root.re_k * 100.0,
            min_of(&self.members, |m| m.character.overlap(TE1)),
            max_of(&self.members, |m| m.character.overlap(TE1)),
            self.mean_ball_fraction(),
            self.link_overlap_full,
            self.link_overlap_ball,
            self.third_capture_full,
            self.fourth_capture_full,
            self.third_capture_ball,
            self.fourth_capture_ball,
            if self.ball_choice_agrees {
                ""
            } else {
                " [ball ranking differs]"
            },
            if self.identified { "id" } else { "--" },
        )
    }
}

/// Per-step summary for `[[step]]`.
struct StepInfo {
    pass: &'static str,
    sigma_0: f64,
    n_modes: usize,
    /// Mean `k` of every complete TE₁ triplet, by `Re(k)`.
    triplet_ks: Vec<c64>,
    /// Subspace overlap between the first two TE₁ triplets (NaN if fewer).
    te1_cross_overlap: f64,
}

/// `(in-ball edges, all edges)` masks.
fn edge_masks(f: &SphereFixture) -> (Vec<bool>, Vec<bool>) {
    let n_edges = f.mesh.edges().len();
    let mut ball = vec![false; n_edges];
    for (t, row) in f.mesh.tet_edges().iter().enumerate() {
        if f.tet_physical_tags[t] == PHYS_SPHERE_INTERIOR {
            for &(e, _) in row {
                ball[e as usize] = true;
            }
        }
    }
    (ball, vec![true; n_edges])
}

/// Run the continuation sweep (forward, then backward over the same grid)
/// and write `open_sigma_sweep.toml`.
pub(crate) fn run_sweep<B: Backend>(
    device: &B::Device,
    f: &SphereFixture,
    root: &MieRootComplex,
    grid: &SweepGrid,
) {
    let masks = edge_masks(f);
    let sigmas = grid.values();
    let (mut rows, mut steps) = run_pass::<B>(device, f, root, &sigmas, "forward", &masks);
    let reversed: Vec<f64> = sigmas.iter().rev().copied().collect();
    let (back_rows, back_steps) = run_pass::<B>(device, f, root, &reversed, "backward", &masks);
    rows.extend(back_rows);
    steps.extend(back_steps);

    write_sweep(&BackendInfo::of::<B>(device), root, grid, &rows, &steps);

    eprintln!("\nSweep summary:");
    for r in &rows {
        eprintln!("  {}", r.line(root));
    }
}

/// One continuation pass over `sigmas` in the given order.
fn run_pass<B: Backend>(
    device: &B::Device,
    f: &SphereFixture,
    root: &MieRootComplex,
    sigmas: &[f64],
    pass: &'static str,
    masks: &(Vec<bool>, Vec<bool>),
) -> (Vec<SweepRow>, Vec<StepInfo>) {
    let (mask_ball, mask_full) = (&masks.0[..], &masks.1[..]);
    let omega0 = root.re_k;
    let mut branches: Vec<Branch> = Vec::new();
    let mut rows: Vec<SweepRow> = Vec::new();
    let mut steps: Vec<StepInfo> = Vec::new();

    for (step, &sigma_0) in sigmas.iter().enumerate() {
        eprintln!("=== {pass} step {step}: σ₀ = {sigma_0} (Λ frozen at ω₀ = {omega0:.5}) ===");
        let (modes, vecs) = solve_frozen_omega_with_vectors::<B>(device, f, sigma_0, omega0, false);
        for m in modes
            .iter()
            .filter(|m| m.character.overlap(TE1) >= TE1_CONTENT_LOG)
        {
            eprintln!("  TE_1 content: {}", m.summary());
        }
        let triplets = te1_triplets(&modes);
        let triplet_q: Vec<Vec<Vec<c64>>> = triplets
            .iter()
            .map(|t| {
                let tv: Vec<&[c64]> = t.iter().map(|&i| vecs[i].as_slice()).collect();
                orthonormal_basis(&tv, mask_full)
            })
            .collect();
        let triplet_ks: Vec<c64> = triplets.iter().map(|t| mean_k(&modes, t)).collect();
        let te1_cross_overlap = if triplet_q.len() >= 2 {
            subspace_overlap(&triplet_q[0], &triplet_q[1])
        } else {
            f64::NAN
        };
        for k in &triplet_ks {
            eprintln!(
                "  identified TE_1 triplet, mean k = {:.4}{:+.4}j",
                k.re, k.im
            );
        }
        if triplet_q.len() >= 2 {
            eprintln!("  overlap between the first two TE_1 triplets: {te1_cross_overlap:.4}");
        }
        steps.push(StepInfo {
            pass,
            sigma_0,
            n_modes: modes.len(),
            triplet_ks: triplet_ks.clone(),
            te1_cross_overlap,
        });

        // Continue every tracked branch.
        let mut claimed: Vec<usize> = Vec::new();
        for br in &mut branches {
            let ranked = |q: &[Vec<c64>], mask: &[bool]| {
                let mut caps: Vec<(usize, f64)> = (0..modes.len())
                    .map(|i| (i, capture(q, &vecs[i], mask)))
                    .collect();
                caps.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
                caps
            };
            let top3 = |caps: &[(usize, f64)]| {
                let mut t: Vec<usize> = caps.iter().take(3).map(|c| c.0).collect();
                t.sort_unstable();
                t
            };
            let nth = |caps: &[(usize, f64)], n: usize| caps.get(n).map_or(0.0, |c| c.1);
            let caps_full = ranked(&br.q_full, mask_full);
            let caps_ball = ranked(&br.q_ball, mask_ball);
            let top = top3(&caps_full);
            let top_vecs: Vec<&[c64]> = top.iter().map(|&i| vecs[i].as_slice()).collect();
            let q_full = orthonormal_basis(&top_vecs, mask_full);
            let q_ball = orthonormal_basis(&top_vecs, mask_ball);
            let identified = triplets.iter().position(|t| *t == top);
            if let Some(ti) = identified {
                claimed.push(ti);
            }
            // Which identified triplet holds the most continuation members.
            let holding = triplets
                .iter()
                .enumerate()
                .map(|(ti, t)| (top.iter().filter(|i| t.contains(i)).count(), ti))
                .max_by_key(|&(n, _)| n)
                .filter(|&(n, _)| n > 0);
            let mut members: Vec<ClassifiedMode> = top.iter().map(|&i| modes[i]).collect();
            members.sort_by(|a, b| a.k.re.partial_cmp(&b.k.re).unwrap());
            let row = SweepRow {
                pass,
                sigma_0,
                branch: br.id,
                seed: false,
                members,
                identified: identified.is_some(),
                members_in_te1_triplet: holding.map_or(0, |h| h.0),
                holding_triplet_k: holding.map(|h| triplet_ks[h.1]),
                link_overlap_full: subspace_overlap(&br.q_full, &q_full),
                link_overlap_ball: subspace_overlap(&br.q_ball, &q_ball),
                third_capture_full: nth(&caps_full, 2),
                fourth_capture_full: nth(&caps_full, 3),
                third_capture_ball: nth(&caps_ball, 2),
                fourth_capture_ball: nth(&caps_ball, 3),
                ball_choice_agrees: top3(&caps_ball) == top,
                seed_overlaps: Vec::new(),
            };
            eprintln!("  {}", row.line(root));
            rows.push(row);
            br.q_full = q_full;
            br.q_ball = q_ball;
        }

        // Every identified triplet that is not exactly a branch's
        // continuation seeds a new branch.
        for (ti, t) in triplets.iter().enumerate() {
            if claimed.contains(&ti) {
                continue;
            }
            let seed_overlaps: Vec<(usize, f64)> = branches
                .iter()
                .map(|b| (b.id, subspace_overlap(&b.q_full, &triplet_q[ti])))
                .collect();
            let id = branches.len();
            let tv: Vec<&[c64]> = t.iter().map(|&i| vecs[i].as_slice()).collect();
            let mut members: Vec<ClassifiedMode> = t.iter().map(|&i| modes[i]).collect();
            members.sort_by(|a, b| a.k.re.partial_cmp(&b.k.re).unwrap());
            let row = SweepRow {
                pass,
                sigma_0,
                branch: id,
                seed: true,
                members,
                identified: true,
                members_in_te1_triplet: 3,
                holding_triplet_k: Some(triplet_ks[ti]),
                link_overlap_full: f64::NAN,
                link_overlap_ball: f64::NAN,
                third_capture_full: f64::NAN,
                fourth_capture_full: f64::NAN,
                third_capture_ball: f64::NAN,
                fourth_capture_ball: f64::NAN,
                ball_choice_agrees: true,
                seed_overlaps,
            };
            eprintln!("  {}", row.line(root));
            rows.push(row);
            branches.push(Branch {
                id,
                q_full: triplet_q[ti].clone(),
                q_ball: orthonormal_basis(&tv, mask_ball),
            });
        }
    }

    (rows, steps)
}

fn means(ms: &[ClassifiedMode]) -> (f64, f64, f64) {
    let n = ms.len() as f64;
    (
        ms.iter().map(|m| m.k.re).sum::<f64>() / n,
        ms.iter().map(|m| m.k.im).sum::<f64>() / n,
        ms.iter().map(|m| m.q()).sum::<f64>() / n,
    )
}

fn min_of(ms: &[ClassifiedMode], f: impl Fn(&ClassifiedMode) -> f64) -> f64 {
    ms.iter().map(f).fold(f64::INFINITY, f64::min)
}

fn max_of(ms: &[ClassifiedMode], f: impl Fn(&ClassifiedMode) -> f64) -> f64 {
    ms.iter().map(f).fold(f64::NEG_INFINITY, f64::max)
}

fn sweep_path() -> PathBuf {
    geode_util::repo::repo_root()
        .join("benchmarks")
        .join("mie_sphere")
        .join("open_sigma_sweep.toml")
}

/// A TOML float (`nan` where a field does not apply).
fn tf(x: f64) -> String {
    if x.is_nan() {
        "nan".to_string()
    } else {
        format!("{x:.6}")
    }
}

fn float_list(xs: impl Iterator<Item = f64>) -> String {
    xs.map(|x| format!("{x:.6}")).collect::<Vec<_>>().join(", ")
}

fn write_sweep(
    backend: &BackendInfo,
    root: &MieRootComplex,
    grid: &SweepGrid,
    rows: &[SweepRow],
    steps: &[StepInfo],
) {
    let path = sweep_path();
    let mut s = String::new();
    s.push_str(&format!(
        "# Auto-generated by `cargo run -p mie_open_quasimode --release -- --sigma-sweep \
         --sigma-start {} --sigma-end {} --sigma-step {}`.\n\
         # Do NOT edit by hand — regenerate after any intentional change.\n\
         # Issue #1026: sigma_0 continuation of the matched-UPML TE_1 triplets.\n\n",
        grid.start, grid.end, grid.step
    ));
    s.push_str("[meta]\n");
    s.push_str(
        "description = \"Matched (full Sacks) UPML sigma_0 continuation (issue #1026): every \
         TE_1 triplet of the frozen-omega eigenpencil followed by eigenvector-subspace overlap \
         between sigma_0 = 5 and 25, forward and backward, on the 774-node sphere fixture.\"\n",
    );
    backend.push_meta(&mut s);
    s.push_str(&format!(
        "generated_at_commit = \"{}\"\n",
        geode_util::repo::current_commit()
    ));
    s.push_str("pml_kernel = \"matched_full_sacks\"\n");
    s.push_str(&format!("n_inside = {N_INSIDE}\n"));
    s.push_str(&format!("omega_freeze = {:.15e}\n", root.re_k));
    s.push_str(&format!("analytic_re_k = {:.15e}\n", root.re_k));
    s.push_str(&format!("analytic_im_k = {:.15e}\n", root.im_k));
    s.push_str(&format!("analytic_q = {:.15e}\n", root.q()));
    s.push_str(&format!("n_near_shift = {N_NEAR_SHIFT}\n"));
    s.push_str(&format!("sigma_start = {}\n", grid.start));
    s.push_str(&format!("sigma_end = {}\n", grid.end));
    s.push_str(&format!("sigma_step = {}\n", grid.step));
    s.push_str("notes = [\n");
    for n in NOTES {
        s.push_str(&format!("  \"{n}\",\n"));
    }
    s.push_str("]\n\n");

    for st in steps {
        s.push_str("[[step]]\n");
        s.push_str(&format!("pass = \"{}\"\n", st.pass));
        s.push_str(&format!("sigma_0 = {}\n", st.sigma_0));
        s.push_str(&format!("n_modes = {}\n", st.n_modes));
        s.push_str(&format!("n_te1_triplets = {}\n", st.triplet_ks.len()));
        s.push_str(&format!(
            "te1_triplets_re_k = [{}]\n",
            float_list(st.triplet_ks.iter().map(|k| k.re))
        ));
        s.push_str(&format!(
            "te1_triplets_im_k = [{}]\n",
            float_list(st.triplet_ks.iter().map(|k| k.im))
        ));
        s.push_str(&format!(
            "te1_cross_overlap = {}\n\n",
            tf(st.te1_cross_overlap)
        ));
    }

    for r in rows {
        let (re, im, q) = means(&r.members);
        s.push_str("[[row]]\n");
        s.push_str(&format!("pass = \"{}\"\n", r.pass));
        s.push_str(&format!("sigma_0 = {}\n", r.sigma_0));
        s.push_str(&format!("branch = {}\n", r.branch));
        s.push_str(&format!("seed = {}\n", r.seed));
        s.push_str(&format!("identified = {}\n", r.identified));
        s.push_str(&format!("re_k = {re:.6}\n"));
        s.push_str(&format!("im_k = {im:.6}\n"));
        s.push_str(&format!(
            "re_k_min = {:.6}\n",
            min_of(&r.members, |m| m.k.re)
        ));
        s.push_str(&format!(
            "re_k_max = {:.6}\n",
            max_of(&r.members, |m| m.k.re)
        ));
        s.push_str(&format!("rel_re_k = {:.6}\n", (re - root.re_k) / root.re_k));
        s.push_str(&format!("q = {q:.6}\n"));
        s.push_str(&format!(
            "te1_overlap_min = {:.6}\n",
            min_of(&r.members, |m| m.character.overlap(TE1))
        ));
        s.push_str(&format!(
            "te1_overlap_max = {:.6}\n",
            max_of(&r.members, |m| m.character.overlap(TE1))
        ));
        let mean_of = |f: &dyn Fn(&ClassifiedMode) -> f64| {
            r.members.iter().map(f).sum::<f64>() / r.members.len() as f64
        };
        s.push_str(&format!(
            "ball_energy_fraction = {:.6}\n",
            mean_of(&|m| m.character.ball_energy_fraction)
        ));
        s.push_str(&format!(
            "gap_energy_fraction = {:.6}\n",
            mean_of(&|m| m.character.gap_energy_fraction)
        ));
        s.push_str(&format!(
            "pml_energy_fraction = {:.6}\n",
            mean_of(&|m| m.character.pml_energy_fraction)
        ));
        s.push_str(&format!(
            "members_in_te1_triplet = {}\n",
            r.members_in_te1_triplet
        ));
        if let Some(k) = r.holding_triplet_k {
            s.push_str(&format!("holding_triplet_re_k = {:.6}\n", k.re));
            s.push_str(&format!("holding_triplet_im_k = {:.6}\n", k.im));
        }
        s.push_str(&format!(
            "link_overlap_full = {}\n",
            tf(r.link_overlap_full)
        ));
        s.push_str(&format!(
            "link_overlap_ball = {}\n",
            tf(r.link_overlap_ball)
        ));
        s.push_str(&format!(
            "third_capture_full = {}\n",
            tf(r.third_capture_full)
        ));
        s.push_str(&format!(
            "fourth_capture_full = {}\n",
            tf(r.fourth_capture_full)
        ));
        s.push_str(&format!(
            "third_capture_ball = {}\n",
            tf(r.third_capture_ball)
        ));
        s.push_str(&format!(
            "fourth_capture_ball = {}\n",
            tf(r.fourth_capture_ball)
        ));
        s.push_str(&format!("ball_choice_agrees = {}\n", r.ball_choice_agrees));
        if r.seed {
            s.push_str(&format!(
                "seed_overlap_branches = [{}]\n",
                r.seed_overlaps
                    .iter()
                    .map(|(b, _)| b.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
            s.push_str(&format!(
                "seed_overlaps = [{}]\n",
                float_list(r.seed_overlaps.iter().map(|(_, o)| *o))
            ));
        }
        s.push('\n');
    }

    fs::create_dir_all(path.parent().expect("sweep parent")).expect("mkdir");
    fs::write(&path, s).expect("write open_sigma_sweep.toml");
    eprintln!("wrote {}", path.display());
}

/// `[meta].notes` of the sweep table: method and verdict.
const NOTES: &[&str] = &[
    "Pencil: K(Λ⁻¹(ω₀)) x = k² M(ε_r·Λ(ω₀)) x, Λ frozen at ω₀ = 1.88074 (Re k of the analytic TE_1,1 root) at every step; sparse shift-invert at σ = ω₀², the n_near_shift converged pairs nearest the shift.",
    "Branch tracking: a branch is a 3-dimensional eigenvector subspace. Each step's continuation is the three modes with the largest capture ‖Qᴴv‖²/‖v‖² by the previous step's subspace (Euclidean inner product on all edge DOFs). link_overlap_full = ‖Q_prevᴴ Q_next‖_F²/3; *_ball is the same on the in-ball edge DOFs, where the medium does not change with sigma_0. third_capture / fourth_capture are the smallest capture inside and the largest outside the continuation; a fourth_capture close to third_capture marks an ambiguous step. members_in_te1_triplet counts continuation members that belong to a complete TE_1 triplet of the step (holding_triplet_* is that triplet).",
    "identified = true when the three continuation modes are exactly one complete TE_1 triplet (MIN_FAMILY_OVERLAP, MULTIPLET_LINK_TOL). Every complete triplet that is not exactly a branch continuation seeds a new branch (seed = true; seed_overlaps against every existing branch). Branch ids are per pass.",
    "Verdict (issue #1026), hypothesis 1: the 19 %-high sigma_0 = 25 triplet (k ≈ 2.241 + 0.368j) is NOT the continuation of the sigma_0 = 5 TE_1,1 triplet. It is a separate branch, identified at every step from sigma_0 = 6 (k ≈ 2.835 + 0.410j, +51 %) to 25 with step links >= 0.998, and it never shares a mode with the low branch; it is also not the open-sphere TE_1,2 root (Re k = 4.08). Below sigma_0 = 6 it is no longer a complete triplet in the solve.",
    "Verdict, continued: the 26 %-low sigma_0 = 25 triplet (k ≈ 1.386 + 0.666j) IS the continuation of the sigma_0 = 5 TE_1,1 triplet (k ≈ 1.855 + 0.192j), but only through a crossing. Between sigma_0 ≈ 12.75 and 16.75 the TE_1 triplet is near-degenerate with a TM_2 quintuplet and the modes hybridise (TE_1 overlap of individual modes falls to 0.4 – 0.8, the TM_2 modes pick up 0.1 – 0.4 TE_1), so per-mode identity is undecidable there: fourth_capture reaches 0.8 – 0.9 at sigma_0 step 0.25 and also at step 0.1 (not a step-size effect). Across the window, the tracker started at either end carries one (step 0.25) or two (step 0.1) of its three modes into the identified triplet at the other end, and never into the high triplet; Re(k) and Im(k) of the identified triplet vary smoothly through the window. Outside the window both ends are clean (links >= 0.99, identified at every step).",
    "Consequence: on the 774-node fixture with Λ frozen at ω₀ = 1.88074, the matched-UPML eigen path moves TE_1,1 from 1.4 % low (sigma_0 = 5) to 26 % low (sigma_0 = 25) in Re(k), with Q falling from 4.8 through the analytic 1.95 (between sigma_0 = 13.5 and 13.75) to 1.04. No radial-order discriminator was added: the branches are told apart by continuation, not by a property of a single solve.",
    "Baseline for reading overlaps: two different TE_1 triplets of the same step overlap by te1_cross_overlap (about 0.5 – 0.6 here), because they share the angular structure and the eigenvectors of the complex-symmetric pencil are not orthogonal in this inner product. Step-to-step links along a clean branch are >= 0.99.",
];
