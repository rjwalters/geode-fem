//! Self-consistent-Λ Picard iteration of the matched-UPML TE₁ triplets
//! (issue #1030).
//!
//! The frozen-ω pencil `K(Λ⁻¹(ω)) x = k² M(ε_r·Λ(ω)) x` is only the
//! physical quasi-mode problem at `ω = Re(k)`. The three-point benchmark
//! and the σ₀ continuation (issue #1026) freeze Λ at the analytic TE₁,₁
//! root `ω₀ = 1.88074`; this mode asks whether solving for Λ
//! **self-consistently** changes the σ₀ trade-off (accurate `Re(k)` at
//! small σ₀, accurate `Q` only near the mixing window).
//!
//! # Method
//!
//! For every σ₀ of the grid:
//!
//! 1. Solve at `ω₀` (the same sparse shift-invert solve as the
//!    benchmark) and identify every complete TE₁ triplet
//!    (`classify_sphere_modes` + `te1_triplets`).
//! 2. Picard: re-freeze Λ at `ω_{i+1} = mean Re(k)` of the current
//!    triplet, re-solve, re-identify. Stop when `|ω_{i+1} − ω_i| <`
//!    [`OMEGA_TOL`] (`converged`), when `ω_{i+1}` returns within
//!    [`OMEGA_TOL`] of an earlier non-adjacent freeze (`cycled`), when a
//!    solve has no usable triplet (`lost`), or after `max_iters` refreshes
//!    (`oscillating` if the last three `Δω` alternate in sign, otherwise
//!    `not_converged`).
//!
//! Each σ₀ runs two selection policies:
//!
//! - **`identity`**: the triplet re-identified at every iterate is the
//!   complete TE₁ triplet nearest the analytic root (`select_multiplet`,
//!   the selection of the three-point benchmark and its single Picard
//!   refresh). Iterate 0 is therefore the `open_results.toml` multiplet.
//! - **`branch`**: one run per TE₁ triplet of the `ω₀` solve; every
//!   iterate takes the triplet of the new solve with the largest
//!   eigenvector-subspace overlap with the previous iterate's triplet (the
//!   continuation rule of the σ₀ sweep, with `ω` instead of `σ₀` as the
//!   continuation parameter).
//!
//! A refresh is a **hop** when the chosen triplet's overlap with the
//! previous iterate's triplet (all edge DOFs) is below [`LINK_MIN`]: the
//! iterate moved to a different triplet rather than following one. Clean
//! σ₀-continuation links are `>= 0.99`, and two different TE₁ triplets of
//! one solve overlap by `0.26 – 0.59` in this inner product
//! (`open_sigma_sweep.toml`, `te1_cross_overlap`), so the threshold sits in
//! the gap. `continuity_*` is the overlap of the final triplet with the
//! iterate-0 triplet.
//!
//! # Which run is TE₁,₁
//!
//! The identity check fixes polarisation and `l`, not the radial order.
//! On the 774-node fixture the σ₀ continuation of issue #1026 attributes
//! the ω₀-solve triplets: below its mixing window the lowest-`Re(k)` TE₁
//! triplet is the tracked TE₁,₁ branch, above the window the lowest one is
//! **inferred** to be TE₁,₁ (across the window, not tracked through), and
//! inside the window the attribution is undecidable; the second (high)
//! triplet is a separate branch at every σ₀. `te11_attribution` records
//! that. No continuation exists on `sphere_fine`, so there the attribution
//! is `not_tracked`.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use burn::prelude::Backend;
use faer::c64;

use geode_core::analytic::mie::MieRootComplex;
use geode_core::assembly::nedelec::NedelecScatterMap;
use geode_core::mesh::SphereFixture;
use geode_core::postproc::mode_character::{ClassifiedMode, select_multiplet};
use geode_util::fixture::BackendInfo;

use crate::sweep::{
    MIXING_WINDOW, TE1, edge_masks, float_list, mean_k, orthonormal_basis, subspace_overlap,
    te1_triplets, tf,
};
use crate::{N_INSIDE, N_NEAR_SHIFT, solve_frozen_omega_with_vectors, tet_edge_indices};

/// Picard stopping tolerance on the freeze frequency.
pub(crate) const OMEGA_TOL: f64 = 1e-3;

/// A refresh whose chosen triplet overlaps the previous iterate's triplet
/// by less than this (all edge DOFs) is a hop.
const LINK_MIN: f64 = 0.8;

/// Decision rule of issue #1030: `|Re(k) error| <=` this…
const RULE_RE_K: f64 = 0.05;

/// …and `Q / Q_analytic` within this band.
const RULE_Q_BAND: (f64, f64) = (0.67, 1.5);

/// What one fixture run is configured with.
pub(crate) struct ScConfig {
    /// `"coarse"` or `"fine"` (the `--fixture` value).
    pub fixture: &'static str,
    /// Mesh file name, for the TOML.
    pub mesh_file: &'static str,
    /// σ₀ grid.
    pub sigmas: Vec<f64>,
    /// Maximum number of Picard refreshes after the ω₀ solve.
    pub max_iters: usize,
    /// The exact command that regenerates the artifact.
    pub command: String,
    /// Output path.
    pub path: PathBuf,
    /// Assemble through the sparse `[nnz]` path (fine fixture).
    pub sparse_assembly: bool,
    /// Whether the #1026 σ₀ continuation attribution applies (774-node
    /// fixture only).
    pub attribute: bool,
}

/// One classified solve with its TE₁ triplets.
struct Solve {
    omega: f64,
    modes: Vec<ClassifiedMode>,
    triplets: Vec<Vec<usize>>,
    q_full: Vec<Vec<Vec<c64>>>,
    q_ball: Vec<Vec<Vec<c64>>>,
    ks: Vec<c64>,
    /// Index into `triplets` of the triplet nearest the analytic root.
    nearest_root: Option<usize>,
    wall_s: f64,
}

/// Selection policy (see the module docs).
#[derive(Clone, Copy, PartialEq)]
enum Policy {
    Identity,
    Branch,
}

impl Policy {
    fn as_str(self) -> &'static str {
        match self {
            Policy::Identity => "identity",
            Policy::Branch => "branch",
        }
    }
}

/// Outcome of one Picard run.
#[derive(Clone, Copy, PartialEq)]
enum Status {
    Converged,
    Cycled,
    Oscillating,
    NotConverged,
    Lost,
}

impl Status {
    fn as_str(self) -> &'static str {
        match self {
            Status::Converged => "converged",
            Status::Cycled => "cycled",
            Status::Oscillating => "oscillating",
            Status::NotConverged => "not_converged",
            Status::Lost => "lost",
        }
    }
}

/// One Picard iterate (a solve at `omega` and the triplet chosen in it).
struct Iterate {
    omega: f64,
    k: c64,
    q: f64,
    /// Index of the chosen triplet among the solve's TE₁ triplets (by
    /// `Re(k)`).
    rank: usize,
    n_triplets: usize,
    /// Overlap with the previous iterate's triplet (NaN at iterate 0).
    link_full: f64,
    link_ball: f64,
    /// Largest overlap of the previous iterate's triplet with any OTHER
    /// triplet of this solve (NaN at iterate 0 or with one triplet).
    best_other_link: f64,
    /// The chosen triplet is also the one nearest the analytic root.
    nearest_root_is_chosen: bool,
}

struct Run {
    policy: Policy,
    sigma_0: f64,
    iterates: Vec<Iterate>,
    status: Status,
    hops: usize,
    continuity_full: f64,
    continuity_ball: f64,
    attribution: &'static str,
}

impl Run {
    fn last(&self) -> &Iterate {
        self.iterates
            .last()
            .expect("a run has at least one iterate")
    }

    fn rel_re_k(&self, root: &MieRootComplex) -> f64 {
        (self.last().k.re - root.re_k) / root.re_k
    }

    fn q_ratio(&self, root: &MieRootComplex) -> f64 {
        self.last().q / root.q()
    }

    /// The numeric part of the decision rule plus convergence and no hop
    /// (the TE₁,₁ attribution is a separate field).
    fn meets_rule(&self, root: &MieRootComplex) -> bool {
        let qr = self.q_ratio(root);
        self.status == Status::Converged
            && self.hops == 0
            && self.rel_re_k(root).abs() <= RULE_RE_K
            && (RULE_Q_BAND.0..=RULE_Q_BAND.1).contains(&qr)
    }

    fn line(&self, root: &MieRootComplex) -> String {
        let l = self.last();
        format!(
            "σ₀ = {:>5} {:<8} start {:.4}{:+.4}j → k = {:.4}{:+.4}j ({:+.1}%) Q = {:.3} \
             (×{:.2})  {} after {} refresh(es), {} hop(s), continuity {:.3}  [{}]{}",
            self.sigma_0,
            self.policy.as_str(),
            self.iterates[0].k.re,
            self.iterates[0].k.im,
            l.k.re,
            l.k.im,
            self.rel_re_k(root) * 100.0,
            l.q,
            self.q_ratio(root),
            self.status.as_str(),
            self.iterates.len() - 1,
            self.hops,
            self.continuity_full,
            self.attribution,
            if self.meets_rule(root) {
                "  MEETS RULE"
            } else {
                ""
            }
        )
    }
}

/// Mean Q of a triplet (the convention of `family_multiplets_q` and the
/// sweep's `q`).
fn mean_q(modes: &[ClassifiedMode], idx: &[usize]) -> f64 {
    idx.iter().map(|&i| modes[i].q()).sum::<f64>() / idx.len() as f64
}

/// The #1026 attribution of the ω₀-solve triplet of the given rank.
fn attribution(cfg: &ScConfig, sigma_0: f64, rank: usize) -> &'static str {
    if !cfg.attribute {
        return "not_tracked";
    }
    if (MIXING_WINDOW.0..=MIXING_WINDOW.1).contains(&sigma_0) {
        "undecidable"
    } else if rank > 0 {
        "other_branch"
    } else if sigma_0 < MIXING_WINDOW.0 {
        "te11_tracked"
    } else {
        "te11_inferred"
    }
}

/// Solver context shared by every solve of one fixture run.
struct Ctx<'a, B: Backend> {
    device: &'a B::Device,
    f: &'a SphereFixture,
    root: &'a MieRootComplex,
    masks: (Vec<bool>, Vec<bool>),
    scatter: Option<NedelecScatterMap>,
    t_all: Instant,
}

impl<B: Backend> Ctx<'_, B> {
    /// One classified frozen-ω solve with its TE₁ triplets.
    fn solve(&self, sigma_0: f64, omega: f64) -> Solve {
        let (device, f, root, masks, t_all) =
            (self.device, self.f, self.root, &self.masks, &self.t_all);
        eprintln!(
            "--- [{:.0} s] solve σ₀ = {sigma_0}, ω = {omega:.6}",
            t_all.elapsed().as_secs_f64()
        );
        let t = Instant::now();
        let (modes, vecs) = solve_frozen_omega_with_vectors::<B>(
            device,
            f,
            sigma_0,
            omega,
            false,
            self.scatter.as_ref(),
        );
        let triplets = te1_triplets(&modes);
        let basis = |mask: &[bool]| -> Vec<Vec<Vec<c64>>> {
            triplets
                .iter()
                .map(|t| {
                    let tv: Vec<&[c64]> = t.iter().map(|&i| vecs[i].as_slice()).collect();
                    orthonormal_basis(&tv, mask)
                })
                .collect()
        };
        let q_full = basis(&masks.1);
        let q_ball = basis(&masks.0);
        let ks: Vec<c64> = triplets.iter().map(|t| mean_k(&modes, t)).collect();
        let nearest_root = select_multiplet(&modes, TE1, root.re_k, root.im_k).map(|m| {
            let mut m = m;
            m.sort_unstable();
            triplets
                .iter()
                .position(|t| *t == m)
                .expect("the nearest-root multiplet is a TE_1 triplet")
        });
        let wall_s = t.elapsed().as_secs_f64();
        eprintln!(
            "    {} modes, TE_1 triplets at {}; nearest root: {:?}; {wall_s:.1} s",
            modes.len(),
            ks.iter()
                .map(|k| format!("{:.4}{:+.4}j", k.re, k.im))
                .collect::<Vec<_>>()
                .join(", "),
            nearest_root
        );
        Solve {
            omega,
            modes,
            triplets,
            q_full,
            q_ball,
            ks,
            nearest_root,
            wall_s,
        }
    }
}

/// The solve at `(σ₀, ω)` from the per-σ₀ cache, solving on a miss (the
/// identity run and the branch run from the same triplet share solves).
fn cached<'c, B: Backend>(
    cache: &'c mut HashMap<u64, Solve>,
    ctx: &Ctx<'_, B>,
    sigma_0: f64,
    omega: f64,
) -> &'c Solve {
    cache
        .entry(omega.to_bits())
        .or_insert_with(|| ctx.solve(sigma_0, omega))
}

/// Run the self-consistent mode over the configured grid and write the
/// artifact.
pub(crate) fn run_selfconsistent<B: Backend>(
    device: &B::Device,
    f: &SphereFixture,
    root: &MieRootComplex,
    cfg: &ScConfig,
) {
    let scatter = cfg
        .sparse_assembly
        .then(|| NedelecScatterMap::new(&tet_edge_indices(f)));
    let ctx = Ctx::<B> {
        device,
        f,
        root,
        masks: edge_masks(f),
        scatter,
        t_all: Instant::now(),
    };
    let t_all = &ctx.t_all;
    let mut runs: Vec<Run> = Vec::new();
    let mut solves: Vec<(f64, f64, f64, usize, Vec<c64>)> = Vec::new();

    for &sigma_0 in &cfg.sigmas {
        let mut cache: HashMap<u64, Solve> = HashMap::new();
        let omega0 = root.re_k;
        let (n0, nearest0) = {
            let s0 = cached(&mut cache, &ctx, sigma_0, omega0);
            (s0.triplets.len(), s0.nearest_root)
        };
        let mut starts: Vec<(Policy, usize)> = Vec::new();
        if let Some(r) = nearest0 {
            starts.push((Policy::Identity, r));
        }
        starts.extend((0..n0).map(|r| (Policy::Branch, r)));
        if starts.is_empty() {
            eprintln!("  σ₀ = {sigma_0}: no TE_1 triplet at ω₀ — no runs");
        }

        for (policy, rank0) in starts {
            let mut iterates: Vec<Iterate> = Vec::new();
            let (mut prev_full, mut prev_ball, first_full, first_ball) = {
                let s0 = cached(&mut cache, &ctx, sigma_0, omega0);
                let t = &s0.triplets[rank0];
                iterates.push(Iterate {
                    omega: omega0,
                    k: s0.ks[rank0],
                    q: mean_q(&s0.modes, t),
                    rank: rank0,
                    n_triplets: s0.triplets.len(),
                    link_full: f64::NAN,
                    link_ball: f64::NAN,
                    best_other_link: f64::NAN,
                    nearest_root_is_chosen: s0.nearest_root == Some(rank0),
                });
                (
                    s0.q_full[rank0].clone(),
                    s0.q_ball[rank0].clone(),
                    s0.q_full[rank0].clone(),
                    s0.q_ball[rank0].clone(),
                )
            };
            let mut hops = 0;
            let mut freezes = vec![omega0];
            let status = loop {
                let cur = iterates.last().expect("iterate");
                let next = cur.k.re;
                if (next - cur.omega).abs() < OMEGA_TOL {
                    break Status::Converged;
                }
                let n = freezes.len();
                if n >= 2
                    && freezes[..n - 1]
                        .iter()
                        .any(|w| (next - w).abs() < OMEGA_TOL)
                {
                    break Status::Cycled;
                }
                if iterates.len() > cfg.max_iters {
                    let d: Vec<f64> = freezes
                        .windows(2)
                        .map(|w| w[1] - w[0])
                        .chain(std::iter::once(next - cur.omega))
                        .collect();
                    let alt = d.len() >= 3
                        && d[d.len() - 3..]
                            .windows(2)
                            .all(|w| w[0].signum() != w[1].signum());
                    break if alt {
                        Status::Oscillating
                    } else {
                        Status::NotConverged
                    };
                }
                freezes.push(next);
                let s = cached(&mut cache, &ctx, sigma_0, next);
                let links: Vec<(f64, f64)> = (0..s.triplets.len())
                    .map(|r| {
                        (
                            subspace_overlap(&prev_full, &s.q_full[r]),
                            subspace_overlap(&prev_ball, &s.q_ball[r]),
                        )
                    })
                    .collect();
                let pick = match policy {
                    Policy::Identity => s.nearest_root,
                    Policy::Branch => (0..links.len())
                        .max_by(|a, b| links[*a].0.partial_cmp(&links[*b].0).unwrap()),
                };
                let Some(pick) = pick else {
                    break Status::Lost;
                };
                let best_other = (0..links.len())
                    .filter(|&r| r != pick)
                    .map(|r| links[r].0)
                    .fold(f64::NAN, f64::max);
                if links[pick].0 < LINK_MIN {
                    hops += 1;
                }
                iterates.push(Iterate {
                    omega: s.omega,
                    k: s.ks[pick],
                    q: mean_q(&s.modes, &s.triplets[pick]),
                    rank: pick,
                    n_triplets: s.triplets.len(),
                    link_full: links[pick].0,
                    link_ball: links[pick].1,
                    best_other_link: best_other,
                    nearest_root_is_chosen: s.nearest_root == Some(pick),
                });
                prev_full = s.q_full[pick].clone();
                prev_ball = s.q_ball[pick].clone();
            };
            let (continuity_full, continuity_ball) = if iterates.len() == 1 {
                (1.0, 1.0)
            } else {
                (
                    subspace_overlap(&first_full, &prev_full),
                    subspace_overlap(&first_ball, &prev_ball),
                )
            };
            let run = Run {
                policy,
                sigma_0,
                iterates,
                status,
                hops,
                continuity_full,
                continuity_ball,
                attribution: attribution(cfg, sigma_0, rank0),
            };
            eprintln!("  RUN {}", run.line(root));
            runs.push(run);
        }

        let mut done: Vec<&Solve> = cache.values().collect();
        done.sort_by(|a, b| a.omega.partial_cmp(&b.omega).unwrap());
        for s in done {
            solves.push((sigma_0, s.omega, s.wall_s, s.modes.len(), s.ks.clone()));
        }
    }

    let total_s = t_all.elapsed().as_secs_f64();
    write_artifact(
        &BackendInfo::of::<B>(device),
        f,
        root,
        cfg,
        &runs,
        &solves,
        total_s,
    );
    eprintln!("\nSelf-consistent summary ({} fixture):", cfg.fixture);
    for r in &runs {
        eprintln!("  {}", r.line(root));
    }
    eprintln!("total wall time {total_s:.0} s");
}

fn write_artifact(
    backend: &BackendInfo,
    f: &SphereFixture,
    root: &MieRootComplex,
    cfg: &ScConfig,
    runs: &[Run],
    solves: &[(f64, f64, f64, usize, Vec<c64>)],
    total_s: f64,
) {
    let mut s = String::new();
    s.push_str(&format!(
        "# Auto-generated by `{}`.\n\
         # Do NOT edit by hand — regenerate after any intentional change.\n\
         # Issue #1030: self-consistent-Λ Picard iteration of the matched-UPML TE_1 triplets.\n\
         # Wall-time fields (`*wall_s`) are measurements and differ between runs; every \
         other value is deterministic.\n\n",
        cfg.command
    ));
    s.push_str("[meta]\n");
    s.push_str(&format!(
        "description = \"Matched (full Sacks) UPML eigen path with Λ solved self-consistently \
         (issue #1030): Picard re-freeze of Λ at the chosen TE_1 triplet's mean Re(k), per sigma_0, \
         on the {} fixture ({}).\"\n",
        cfg.fixture, cfg.mesh_file
    ));
    backend.push_meta(&mut s);
    s.push_str(&format!(
        "generated_at_commit = \"{}\"\n",
        geode_util::repo::current_commit()
    ));
    s.push_str("pml_kernel = \"matched_full_sacks\"\n");
    s.push_str(&format!("fixture = \"{}\"\n", cfg.fixture));
    s.push_str(&format!("mesh_file = \"{}\"\n", cfg.mesh_file));
    s.push_str(&format!("n_nodes = {}\n", f.mesh.n_nodes()));
    s.push_str(&format!("n_tets = {}\n", f.mesh.n_tets()));
    s.push_str(&format!("n_edges = {}\n", f.mesh.edges().len()));
    s.push_str(&format!(
        "assembly = \"{}\"\n",
        if cfg.sparse_assembly {
            "sparse_nnz"
        } else {
            "dense"
        }
    ));
    s.push_str(&format!("n_inside = {N_INSIDE}\n"));
    s.push_str(&format!("n_near_shift = {N_NEAR_SHIFT}\n"));
    s.push_str(&format!("analytic_re_k = {:.15e}\n", root.re_k));
    s.push_str(&format!("analytic_im_k = {:.15e}\n", root.im_k));
    s.push_str(&format!("analytic_q = {:.15e}\n", root.q()));
    s.push_str(&format!("omega_0 = {:.15e}\n", root.re_k));
    s.push_str(&format!(
        "sigma_values = [{}]\n",
        cfg.sigmas
            .iter()
            .map(|x| x.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    ));
    s.push_str(&format!("max_iters = {}\n", cfg.max_iters));
    s.push_str(&format!("omega_tol = {OMEGA_TOL}\n"));
    s.push_str(&format!("link_min = {LINK_MIN}\n"));
    s.push_str(&format!("rule_re_k = {RULE_RE_K}\n"));
    s.push_str(&format!(
        "rule_q_band = [{}, {}]\n",
        RULE_Q_BAND.0, RULE_Q_BAND.1
    ));
    s.push_str(&format!("n_solves = {}\n", solves.len()));
    s.push_str(&format!("total_wall_s = {total_s:.1}\n"));
    s.push_str("notes = [\n");
    for n in NOTES {
        s.push_str(&format!("  \"{n}\",\n"));
    }
    s.push_str("]\n\n");

    for r in runs {
        let l = r.last();
        s.push_str("[[run]]\n");
        s.push_str(&format!("sigma_0 = {}\n", r.sigma_0));
        s.push_str(&format!("policy = \"{}\"\n", r.policy.as_str()));
        s.push_str(&format!("start_rank = {}\n", r.iterates[0].rank));
        s.push_str(&format!("te11_attribution = \"{}\"\n", r.attribution));
        s.push_str(&format!("status = \"{}\"\n", r.status.as_str()));
        s.push_str(&format!("refreshes = {}\n", r.iterates.len() - 1));
        s.push_str(&format!("hops = {}\n", r.hops));
        s.push_str(&format!("start_re_k = {:.6}\n", r.iterates[0].k.re));
        s.push_str(&format!("start_im_k = {:.6}\n", r.iterates[0].k.im));
        s.push_str(&format!("start_q = {:.6}\n", r.iterates[0].q));
        s.push_str(&format!("final_omega = {:.6}\n", l.omega));
        s.push_str(&format!("final_re_k = {:.6}\n", l.k.re));
        s.push_str(&format!("final_im_k = {:.6}\n", l.k.im));
        s.push_str(&format!("final_q = {:.6}\n", l.q));
        s.push_str(&format!("rel_re_k = {:.6}\n", r.rel_re_k(root)));
        s.push_str(&format!("q_ratio = {:.6}\n", r.q_ratio(root)));
        s.push_str(&format!("continuity_full = {:.6}\n", r.continuity_full));
        s.push_str(&format!("continuity_ball = {:.6}\n", r.continuity_ball));
        s.push_str(&format!(
            "in_mixing_window = {}\n",
            cfg.attribute && (MIXING_WINDOW.0..=MIXING_WINDOW.1).contains(&r.sigma_0)
        ));
        s.push_str(&format!("meets_rule = {}\n", r.meets_rule(root)));
        let it = &r.iterates;
        s.push_str(&format!(
            "iter_omega = [{}]\n",
            float_list(it.iter().map(|i| i.omega))
        ));
        s.push_str(&format!(
            "iter_re_k = [{}]\n",
            float_list(it.iter().map(|i| i.k.re))
        ));
        s.push_str(&format!(
            "iter_im_k = [{}]\n",
            float_list(it.iter().map(|i| i.k.im))
        ));
        s.push_str(&format!(
            "iter_q = [{}]\n",
            float_list(it.iter().map(|i| i.q))
        ));
        s.push_str(&format!(
            "iter_rank = [{}]\n",
            it.iter()
                .map(|i| i.rank.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ));
        s.push_str(&format!(
            "iter_n_te1_triplets = [{}]\n",
            it.iter()
                .map(|i| i.n_triplets.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ));
        let tfl = |g: &dyn Fn(&Iterate) -> f64| {
            it.iter().map(|i| tf(g(i))).collect::<Vec<_>>().join(", ")
        };
        s.push_str(&format!("iter_link_full = [{}]\n", tfl(&|i| i.link_full)));
        s.push_str(&format!("iter_link_ball = [{}]\n", tfl(&|i| i.link_ball)));
        s.push_str(&format!(
            "iter_best_other_link = [{}]\n",
            tfl(&|i| i.best_other_link)
        ));
        s.push_str(&format!(
            "iter_nearest_root_is_chosen = [{}]\n",
            it.iter()
                .map(|i| i.nearest_root_is_chosen.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ));
        s.push('\n');
    }

    for (sigma_0, omega, wall_s, n_modes, ks) in solves {
        s.push_str("[[solve]]\n");
        s.push_str(&format!("sigma_0 = {sigma_0}\n"));
        s.push_str(&format!("omega = {omega:.6}\n"));
        s.push_str(&format!("n_modes = {n_modes}\n"));
        s.push_str(&format!(
            "te1_triplets_re_k = [{}]\n",
            float_list(ks.iter().map(|k| k.re))
        ));
        s.push_str(&format!(
            "te1_triplets_im_k = [{}]\n",
            float_list(ks.iter().map(|k| k.im))
        ));
        s.push_str(&format!("wall_s = {wall_s:.1}\n\n"));
    }

    fs::create_dir_all(cfg.path.parent().expect("artifact parent")).expect("mkdir");
    fs::write(&cfg.path, s).expect("write self-consistent artifact");
    eprintln!("wrote {}", cfg.path.display());
}

/// `[meta].notes`: method (the verdict lives in the crate docs, the
/// `sphere_matched_upml_eigenmode` test header and the CHANGELOG).
const NOTES: &[&str] = &[
    "Each sigma_0 starts from the solve with Λ frozen at omega_0 (Re k of the analytic TE_1,1 root). A Picard refresh re-freezes Λ at the mean Re(k) of the current TE_1 triplet, re-solves (sparse shift-invert at the new omega², n_near_shift converged pairs) and re-identifies. status: converged = |omega_{i+1} - omega_i| < omega_tol; cycled = omega_{i+1} returned within omega_tol of an earlier non-adjacent freeze; oscillating / not_converged = max_iters refreshes reached, with / without the last three Δω alternating in sign; lost = a solve had no usable TE_1 triplet. final_* is the last iterate (for converged runs: self-consistent to omega_tol).",
    "policy = identity: every iterate takes the complete TE_1 triplet nearest the analytic root (select_multiplet; the selection of open_results.toml and its single Picard refresh), so iterate 0 is the open_results.toml multiplet. policy = branch: one run per TE_1 triplet of the omega_0 solve (start_rank = its index by Re k); every iterate takes the triplet with the largest eigenvector-subspace overlap with the previous iterate's triplet.",
    "A hop is a refresh whose chosen triplet overlaps the previous iterate's triplet (all edge DOFs, ‖Q_prevᴴ Q‖_F²/3) by less than link_min: the iterate moved to a different triplet. Clean sigma_0-continuation links are >= 0.99 and two different TE_1 triplets of one solve overlap by 0.26 - 0.59 (open_sigma_sweep.toml te1_cross_overlap), so link_min sits in that gap. iter_best_other_link is the previous triplet's largest overlap with any other triplet. continuity_* = overlap of the final triplet with the iterate-0 triplet.",
    "k and Q of a triplet are the means over its three members (Q = mean of Re k / 2|Im k|), as in open_sigma_sweep.toml; open_results.toml fem_* is the member nearest the root, which differs from the mean by up to the triplet spread.",
    "te11_attribution (774-node fixture only, from the issue #1026 continuation of the omega_0 solves): te11_tracked = lowest-Re(k) TE_1 triplet below the mixing window (tracked from sigma_0 = 5); te11_inferred = lowest-Re(k) triplet above the window (INFERRED to be TE_1,1 across the window by shared membership and elimination, not tracked through); undecidable = inside the window [12.75, 16.75]; other_branch = the high triplet, a separate branch. not_tracked = no sigma_0 continuation exists on this fixture.",
    "Decision rule (issue #1030): recommend an eigen-path sigma_0 for TE_1,1 only if, with Λ self-consistent, |rel_re_k| <= rule_re_k AND q_ratio in rule_q_band, on a converged, non-hopped, identified TE_1,1 triplet on the finest fixture measured. meets_rule checks everything but the attribution.",
];
