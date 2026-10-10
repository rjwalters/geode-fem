//! Field-character identification of dielectric-sphere eigenmodes
//! (issue #1022).
//!
//! A complex eigensolve on the open sphere (dielectric ball + vacuum gap +
//! PML shell) returns the physical quasi-modes mixed with PML-continuum
//! modes, and the eigenvalue alone does not say which is which: on the
//! bundled 774-node fixture several unrelated modes can sit at the same
//! complex distance from an analytic Mie root. This module classifies a
//! mode from its **eigenvector**:
//!
//! - where the energy `∫ ε_r |E|² dV` sits (ball, vacuum gap, PML shell);
//! - how much of the in-ball field is radial (`E·r̂`; zero for an exact TE
//!   multipole);
//! - the fraction of the in-ball energy captured by the Hermitian
//!   projection onto each low-order vector spherical multipole family
//!   (TE and TM, `l = 1` and `l = 2`), see [`MultipoleFamily`];
//! - and, from the eigenvalues, the multiplet a mode belongs to
//!   ([`cluster_wavenumbers`]): an order-`l` multipole is `(2l + 1)`-fold
//!   degenerate on the exact sphere, split only by the mesh.
//!
//! # Why the projection needs no fitted radial profile
//!
//! Inside the ball the medium is uniform, so *every* eigenvector of
//! `curl curl E = λ ε E` with eigenvalue `λ = k²` is there a superposition
//! of the regular multipoles `M_lm`, `N_lm` with radial argument
//! `ρ = n k r` and the mode's **own** complex `k = √λ`, whatever happens in
//! the gap and the PML. For `l = 1` the angular functions are `Y = â·r̂`
//! (three Cartesian `â`); for `l = 2`, `Y = r̂ᵀ S r̂` (five symmetric
//! traceless `S`). With `g = r ∇Y` the tangential gradient,
//!
//! ```text
//! TE (magnetic multipole):  M = j_l(ρ) · (r̂ × g)
//! TM (electric multipole):  N = l(l+1) · j_l(ρ)/ρ · Y r̂ + (ρ j_l(ρ))'/ρ · g
//! ```
//!
//! and `(ρ j_l)' = ρ j_{l−1} − l j_l`. The four families are mutually
//! orthogonal over the ball, so the four captured fractions add up to at
//! most 1 (up to quadrature and discretization error) and the remainder
//! is `l ≥ 3` content.

use faer::c64;

use crate::analytic::mie::{MiePolarisation, spherical_j_c};
use crate::elements::nedelec::{TET_QUAD4_A, TET_QUAD4_B};
use crate::elements::nedelec_p2::tet_barycentric_gradients;
use crate::mesh::{
    PHYS_PML_SHELL, PHYS_SPHERE_INTERIOR, R_PML_INNER, SphereFixture, TET_LOCAL_EDGES,
};

/// A vector-spherical-multipole family of the dielectric ball: a
/// polarisation and an angular order `l` (`1` or `2`). The family has
/// `2l + 1` members.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MultipoleFamily {
    /// `TE` = magnetic multipole (`E ∝ M_lm`, no radial `E`); `TM` =
    /// electric multipole (`E ∝ N_lm`).
    pub pol: MiePolarisation,
    /// Angular order, `1` or `2`.
    pub l: usize,
}

impl MultipoleFamily {
    /// The four families the projection covers, in report order:
    /// TE₁, TM₁, TE₂, TM₂.
    pub const ALL: [MultipoleFamily; 4] = [
        MultipoleFamily {
            pol: MiePolarisation::TE,
            l: 1,
        },
        MultipoleFamily {
            pol: MiePolarisation::TM,
            l: 1,
        },
        MultipoleFamily {
            pol: MiePolarisation::TE,
            l: 2,
        },
        MultipoleFamily {
            pol: MiePolarisation::TM,
            l: 2,
        },
    ];

    /// Number of degenerate members, `2l + 1`.
    pub fn multiplicity(self) -> usize {
        2 * self.l + 1
    }

    /// Short label, e.g. `"TE_1"`.
    pub fn label(self) -> String {
        format!("{}_{}", self.pol.as_str(), self.l)
    }

    fn slot(self) -> usize {
        Self::ALL
            .iter()
            .position(|f| *f == self)
            .expect("MultipoleFamily with l outside {1, 2}")
    }

    /// The `i`-th member field (`i < 2l + 1`) at the point `x`, for the
    /// in-ball radial argument `ρ = n·k·|x|`. Not normalized; the
    /// projection only uses the span.
    ///
    /// # Panics
    ///
    /// Panics if `l` is not 1 or 2, or `i ≥ 2l + 1`.
    pub fn member_field(self, i: usize, n_inside: f64, k: c64, x: [f64; 3]) -> [c64; 3] {
        assert!(
            i < self.multiplicity(),
            "member index {i} out of range for l = {}",
            self.l
        );
        let r = (x[0] * x[0] + x[1] * x[1] + x[2] * x[2]).sqrt();
        if r < 1e-12 {
            return [c64::new(0.0, 0.0); 3];
        }
        let rhat = [x[0] / r, x[1] / r, x[2] / r];
        // Angular function Y and its tangential gradient g = r ∇Y.
        let (y, g) = match self.l {
            1 => {
                let mut a = [0.0; 3];
                a[i] = 1.0;
                let y = dot3(a, rhat);
                (
                    y,
                    [a[0] - y * rhat[0], a[1] - y * rhat[1], a[2] - y * rhat[2]],
                )
            }
            2 => {
                let s = quadrupole_basis(i);
                let sr = [dot3(s[0], rhat), dot3(s[1], rhat), dot3(s[2], rhat)];
                let y = dot3(rhat, sr);
                (
                    y,
                    [
                        2.0 * (sr[0] - y * rhat[0]),
                        2.0 * (sr[1] - y * rhat[1]),
                        2.0 * (sr[2] - y * rhat[2]),
                    ],
                )
            }
            l => panic!("MultipoleFamily supports l = 1, 2 (got {l})"),
        };
        let rho = k * c64::new(n_inside * r, 0.0);
        let jl = spherical_j_c(self.l, rho);
        match self.pol {
            MiePolarisation::TE => {
                let t = cross3(rhat, g);
                [
                    jl * c64::new(t[0], 0.0),
                    jl * c64::new(t[1], 0.0),
                    jl * c64::new(t[2], 0.0),
                ]
            }
            MiePolarisation::TM => {
                let lf = self.l as f64;
                // (ρ j_l)' = ρ j_{l−1} − l j_l.
                let d_rho_jl = rho * spherical_j_c(self.l - 1, rho) - c64::new(lf, 0.0) * jl;
                let radial = c64::new(lf * (lf + 1.0) * y, 0.0) * jl / rho;
                let tang = d_rho_jl / rho;
                [
                    radial * c64::new(rhat[0], 0.0) + tang * c64::new(g[0], 0.0),
                    radial * c64::new(rhat[1], 0.0) + tang * c64::new(g[1], 0.0),
                    radial * c64::new(rhat[2], 0.0) + tang * c64::new(g[2], 0.0),
                ]
            }
        }
    }
}

/// The five symmetric traceless matrices spanning the `l = 2` angular
/// functions `r̂ᵀ S r̂`: `xy`, `yz`, `zx`, `x² − y²`, `2z² − x² − y²`.
fn quadrupole_basis(i: usize) -> [[f64; 3]; 3] {
    match i {
        0 => [[0.0, 1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 0.0]],
        1 => [[0.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]],
        2 => [[0.0, 0.0, 1.0], [0.0, 0.0, 0.0], [1.0, 0.0, 0.0]],
        3 => [[1.0, 0.0, 0.0], [0.0, -1.0, 0.0], [0.0, 0.0, 0.0]],
        _ => [[-1.0, 0.0, 0.0], [0.0, -1.0, 0.0], [0.0, 0.0, 2.0]],
    }
}

fn dot3(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// Hermitian inner product `conj(u)·v` of two complex 3-vectors.
fn herm3(u: &[c64; 3], v: &[c64; 3]) -> c64 {
    u[0].conj() * v[0] + u[1].conj() * v[1] + u[2].conj() * v[2]
}

/// Field-character metrics of one eigenmode on the sphere fixture. All
/// fractions are of the real, un-stretched energy `∫ ε_r |E|² dV`
/// (`ε_r = n²` in the ball, 1 elsewhere; the PML stretch is deliberately
/// left out so the split measures where the field is, not how the PML
/// weights it).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SphereModeCharacter {
    /// Share of the total energy inside the dielectric ball.
    pub ball_energy_fraction: f64,
    /// Share of the total energy in the vacuum gap (between the ball and
    /// the PML inner radius).
    pub gap_energy_fraction: f64,
    /// Share of the total energy inside the PML shell.
    pub pml_energy_fraction: f64,
    /// Share of the in-ball energy carried by the radial component
    /// `E·r̂`. Zero for an exact TE multipole.
    pub radial_fraction: f64,
    /// Share of the in-ball energy captured by the projection onto each
    /// family of [`MultipoleFamily::ALL`], in that order.
    pub overlaps: [f64; 4],
}

impl SphereModeCharacter {
    /// Captured in-ball energy fraction for `family`.
    pub fn overlap(&self, family: MultipoleFamily) -> f64 {
        self.overlaps[family.slot()]
    }

    /// The family with the largest captured fraction, and that fraction.
    pub fn dominant(&self) -> (MultipoleFamily, f64) {
        let (slot, frac) = self
            .overlaps
            .iter()
            .copied()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(&b.1).expect("finite overlaps"))
            .expect("four families");
        (MultipoleFamily::ALL[slot], frac)
    }

    /// One-line summary for `--nocapture` audit tables.
    pub fn summary(&self) -> String {
        let (dom, _) = self.dominant();
        format!(
            "E[ball/gap/pml] = {:.3}/{:.3}/{:.3}, radial = {:.3}, overlap TE1/TM1/TE2/TM2 = \
             {:.3}/{:.3}/{:.3}/{:.3} → {}",
            self.ball_energy_fraction,
            self.gap_energy_fraction,
            self.pml_energy_fraction,
            self.radial_fraction,
            self.overlaps[0],
            self.overlaps[1],
            self.overlaps[2],
            self.overlaps[3],
            dom.label()
        )
    }
}

/// Classify the eigenmode with full-length edge-DOF vector `edge_field`
/// (one entry per `fixture.mesh.edges()` edge; PEC edges zero) and complex
/// wavenumber `k = √λ` on the sphere fixture.
///
/// `k` must be the mode's **own** eigen-wavenumber (see the module docs):
/// it fixes the in-ball radial profile `j_l(n k r)` exactly, with no sign
/// convention to choose, because the in-ball field solves
/// `curl curl E = k² n² E` for that `k`.
///
/// Integration uses the degree-2 four-point tet rule on the first-order
/// Whitney interpolant. The eigenvector's normalization and global phase
/// drop out of every returned quantity.
///
/// # Panics
///
/// Panics if `edge_field.len()` is not the mesh's edge count.
pub fn sphere_mode_character(
    fixture: &SphereFixture,
    edge_field: &[c64],
    n_inside: f64,
    k: c64,
) -> SphereModeCharacter {
    let mesh = &fixture.mesh;
    let tet_edges = mesh.tet_edges();
    let n_edges = mesh.edges().len();
    assert_eq!(
        edge_field.len(),
        n_edges,
        "edge_field must be the full-length edge-DOF vector"
    );

    let mut energy = [0.0_f64; 3]; // ball, gap, pml
    let mut radial_energy = 0.0_f64;

    // Per-family Gram matrix G_ij = <B_i, B_j> and moments c_i = <B_i, E>
    // over the ball, Hermitian inner product with weight ε_r dV. (ε_r is
    // uniform in the ball, so it cancels in the fractions; it is kept so
    // the bookkeeping matches `energy[0]`.)
    let mut grams: Vec<Vec<Vec<c64>>> = MultipoleFamily::ALL
        .iter()
        .map(|f| vec![vec![c64::new(0.0, 0.0); f.multiplicity()]; f.multiplicity()])
        .collect();
    let mut moments: Vec<Vec<c64>> = MultipoleFamily::ALL
        .iter()
        .map(|f| vec![c64::new(0.0, 0.0); f.multiplicity()])
        .collect();

    for (t, tet) in mesh.tets.iter().enumerate() {
        let coords: [[f64; 3]; 4] = std::array::from_fn(|i| mesh.nodes[tet[i] as usize]);
        let (grad, signed_vol) = tet_barycentric_gradients(&coords);
        let w = signed_vol.abs() / 4.0;
        let dofs: [c64; 6] = std::array::from_fn(|e| {
            let (idx, sign) = tet_edges[t][e];
            edge_field[idx as usize] * c64::new(sign as f64, 0.0)
        });
        let centroid_r = {
            let c: [f64; 3] = std::array::from_fn(|d| {
                0.25 * (coords[0][d] + coords[1][d] + coords[2][d] + coords[3][d])
            });
            dot3(c, c).sqrt()
        };
        let tag = fixture.tet_physical_tags[t];
        let in_ball = tag == PHYS_SPHERE_INTERIOR;
        let region = if in_ball {
            0
        } else if tag == PHYS_PML_SHELL && centroid_r > R_PML_INNER {
            2
        } else {
            1
        };
        let eps_r = if in_ball { n_inside * n_inside } else { 1.0 };

        for q in 0..4 {
            let lambda: [f64; 4] =
                std::array::from_fn(|p| if p == q { TET_QUAD4_A } else { TET_QUAD4_B });
            let x: [f64; 3] = std::array::from_fn(|d| {
                lambda[0] * coords[0][d]
                    + lambda[1] * coords[1][d]
                    + lambda[2] * coords[2][d]
                    + lambda[3] * coords[3][d]
            });
            let mut e = [c64::new(0.0, 0.0); 3];
            for (slot, &(a, b)) in TET_LOCAL_EDGES.iter().enumerate() {
                for (d, e_d) in e.iter_mut().enumerate() {
                    let wgt = lambda[a] * grad[b][d] - lambda[b] * grad[a][d];
                    *e_d += dofs[slot] * c64::new(wgt, 0.0);
                }
            }
            let e2 = herm3(&e, &e).re;
            energy[region] += w * eps_r * e2;
            if !in_ball {
                continue;
            }
            let r = dot3(x, x).sqrt();
            if r > 1e-12 {
                let er = (e[0] * c64::new(x[0] / r, 0.0))
                    + (e[1] * c64::new(x[1] / r, 0.0))
                    + (e[2] * c64::new(x[2] / r, 0.0));
                radial_energy += w * eps_r * er.norm_sqr();
            }
            for (fi, fam) in MultipoleFamily::ALL.iter().enumerate() {
                let m = fam.multiplicity();
                let basis: Vec<[c64; 3]> = (0..m)
                    .map(|i| fam.member_field(i, n_inside, k, x))
                    .collect();
                for i in 0..m {
                    moments[fi][i] += c64::new(w * eps_r, 0.0) * herm3(&basis[i], &e);
                    for j in 0..m {
                        grams[fi][i][j] += c64::new(w * eps_r, 0.0) * herm3(&basis[i], &basis[j]);
                    }
                }
            }
        }
    }

    let total = energy[0] + energy[1] + energy[2];
    let ball = energy[0];
    let mut overlaps = [0.0_f64; 4];
    if ball > 0.0 {
        for fi in 0..4 {
            // Captured energy = c^H G^{-1} c.
            let coeff = solve_small(&grams[fi], &moments[fi]);
            let captured: c64 = moments[fi]
                .iter()
                .zip(coeff.iter())
                .map(|(c, a)| c.conj() * a)
                .sum();
            overlaps[fi] = captured.re / ball;
        }
    }
    let frac = |x: f64| if total > 0.0 { x / total } else { 0.0 };
    SphereModeCharacter {
        ball_energy_fraction: frac(energy[0]),
        gap_energy_fraction: frac(energy[1]),
        pml_energy_fraction: frac(energy[2]),
        radial_fraction: if ball > 0.0 {
            radial_energy / ball
        } else {
            0.0
        },
        overlaps,
    }
}

/// Solve the small dense complex system `A x = b` by Gaussian elimination
/// with partial pivoting (`A` is a 3×3 or 5×5 Hermitian positive-definite
/// Gram matrix here). A numerically singular pivot yields a zero
/// coefficient for that direction.
fn solve_small(a: &[Vec<c64>], b: &[c64]) -> Vec<c64> {
    let n = b.len();
    let mut a: Vec<Vec<c64>> = a.to_vec();
    let mut b: Vec<c64> = b.to_vec();
    let scale = a
        .iter()
        .flat_map(|row| row.iter())
        .map(|v| v.norm())
        .fold(0.0_f64, f64::max);
    let mut singular = vec![false; n];
    for col in 0..n {
        let piv = (col..n)
            .max_by(|&i, &j| {
                a[i][col]
                    .norm()
                    .partial_cmp(&a[j][col].norm())
                    .expect("finite Gram entries")
            })
            .expect("non-empty pivot range");
        if a[piv][col].norm() <= 1e-13 * scale {
            singular[col] = true;
            continue;
        }
        a.swap(col, piv);
        b.swap(col, piv);
        let pivot_row = a[col].clone();
        let pivot_rhs = b[col];
        for row in (col + 1)..n {
            let factor = a[row][col] / pivot_row[col];
            for (entry, pivot_entry) in a[row].iter_mut().zip(pivot_row.iter()).skip(col) {
                *entry -= factor * *pivot_entry;
            }
            b[row] -= factor * pivot_rhs;
        }
    }
    let mut x = vec![c64::new(0.0, 0.0); n];
    for col in (0..n).rev() {
        if singular[col] {
            continue;
        }
        let mut acc = b[col];
        for c in (col + 1)..n {
            acc -= a[col][c] * x[c];
        }
        x[col] = acc / a[col][col];
    }
    x
}

/// Group complex wavenumbers into multiplets by single linkage in the
/// complex `k`-plane: two modes are linked when `|k_i − k_j| ≤ link_tol`,
/// and a cluster is a connected component. Returns the clusters as lists
/// of indices into `ks`, each sorted ascending, ordered by their smallest
/// index.
///
/// On the exact sphere an order-`l` multipole is `(2l + 1)`-fold
/// degenerate; the unstructured mesh splits it by a small amount, so
/// with `link_tol` between that splitting and the distance to the next
/// multiplet the cluster size reads off `2l + 1`.
pub fn cluster_wavenumbers(ks: &[c64], link_tol: f64) -> Vec<Vec<usize>> {
    let n = ks.len();
    let mut cluster_of: Vec<Option<usize>> = vec![None; n];
    let mut clusters: Vec<Vec<usize>> = Vec::new();
    for seed in 0..n {
        if cluster_of[seed].is_some() {
            continue;
        }
        let id = clusters.len();
        let mut members = vec![seed];
        cluster_of[seed] = Some(id);
        let mut head = 0;
        while head < members.len() {
            let i = members[head];
            head += 1;
            for j in 0..n {
                if cluster_of[j].is_none() && (ks[i] - ks[j]).norm() <= link_tol {
                    cluster_of[j] = Some(id);
                    members.push(j);
                }
            }
        }
        members.sort_unstable();
        clusters.push(members);
    }
    clusters
}

/// Smallest captured in-ball energy fraction for a mode to count as a
/// member of a [`MultipoleFamily`]: a strict majority.
///
/// Measured on the bundled 774-node fixture with the matched UPML
/// (`σ₀ ∈ {5, 10, 25}`, Λ frozen at the TE₁,₁ root, the 60 modes nearest
/// the shift): every mode that belongs to a clean `l = 1` magnetic-dipole
/// triplet captures 0.64 – 0.99 in TE₁, and every other mode at most 0.10.
/// 0.5 sits in that gap and is the natural "more than every other family
/// combined" line; it is not tuned to a particular mode.
pub const MIN_FAMILY_OVERLAP: f64 = 0.5;

/// Single-linkage tolerance on `|Δk|` (units of `1/R_s`) for
/// [`classify_sphere_modes`] multiplets.
///
/// Measured on the same solves as [`MIN_FAMILY_OVERLAP`]: the mesh splits a
/// TE₁ triplet by at most 0.0155 nearest-neighbour (the `σ₀ = 25` triplet
/// at `k ≈ 1.39 + 0.66j`; 0.0045 at `σ₀ = 5`), while two distinct
/// multiplets of the **same** family are at least 0.4 apart. Clustering
/// all modes regardless of family is not reliable at this tolerance or any
/// other: at `σ₀ = 25` a TM₁ triplet and a TE₂-dominant quintuplet sit
/// 0.019 apart and merge into a group of eight.
pub const MULTIPLET_LINK_TOL: f64 = 0.02;

/// One eigenmode with its field character and multiplet bookkeeping, as
/// produced by [`classify_sphere_modes`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClassifiedMode {
    /// Pencil eigenvalue `λ = k²`.
    pub lambda: c64,
    /// Principal `k = √λ` (`Re k ≥ 0`).
    pub k: c64,
    /// Field-character metrics.
    pub character: SphereModeCharacter,
    /// The family capturing at least [`MIN_FAMILY_OVERLAP`] of the in-ball
    /// energy, if any.
    pub family: Option<MultipoleFamily>,
    /// Size of the mode's cluster among **all** classified modes
    /// ([`MULTIPLET_LINK_TOL`]). Reported for audit only; see that
    /// constant for why it cannot identify a mode by itself.
    pub k_cluster_size: usize,
    /// Size of the mode's cluster among the modes of the **same family**;
    /// 0 when `family` is `None`.
    pub multiplet_size: usize,
    /// Identifier of that same-family cluster (shared by its members,
    /// unique across families); `None` when `family` is `None`.
    pub multiplet_id: Option<usize>,
}

impl ClassifiedMode {
    /// `Q = Re k / (2 |Im k|)`.
    pub fn q(&self) -> f64 {
        self.k.re / (2.0 * self.k.im.abs().max(1e-300))
    }

    /// Distance to an analytic root in the `(Re k, |Im k|)` plane (`|Im|`
    /// folds out the time-convention sign).
    pub fn distance_to(&self, re_k: f64, im_k: f64) -> f64 {
        (self.k.re - re_k).hypot(self.k.im.abs() - im_k.abs())
    }

    /// The identity check: the mode's dominant family is `family` **and**
    /// it sits in a same-family multiplet of exactly `2l + 1` members.
    pub fn is_member_of(&self, family: MultipoleFamily) -> bool {
        self.family == Some(family) && self.multiplet_size == family.multiplicity()
    }

    /// Why [`Self::is_member_of`] rejects this mode for `family`, or
    /// `None` if it accepts.
    pub fn rejection(&self, family: MultipoleFamily) -> Option<String> {
        if self.is_member_of(family) {
            return None;
        }
        Some(match self.family {
            Some(f) if f == family => format!(
                "{} multiplet has {} member(s), expected {}",
                family.label(),
                self.multiplet_size,
                family.multiplicity()
            ),
            Some(f) => format!(
                "field is {} (overlap {:.3}); {} overlap is {:.3}",
                f.label(),
                self.character.overlap(f),
                family.label(),
                self.character.overlap(family)
            ),
            None => format!(
                "no family reaches {MIN_FAMILY_OVERLAP} of the in-ball energy; {} overlap is {:.3}",
                family.label(),
                self.character.overlap(family)
            ),
        })
    }

    /// One-line audit row: `k`, `Q`, multiplicity and field character.
    pub fn summary(&self) -> String {
        format!(
            "k = {:.4} {:+.4}j  Q = {:.3}  k-cluster {}  multiplet {}×{}  | {}",
            self.k.re,
            self.k.im,
            self.q(),
            self.k_cluster_size,
            self.multiplet_size,
            self.family
                .map_or_else(|| "none".to_string(), |f| f.label()),
            self.character.summary()
        )
    }
}

/// Classify a set of eigenpairs of the sphere pencil.
///
/// `modes` holds `(λ, full-length edge-DOF vector)` pairs (PEC edges
/// zero-filled). Each mode gets its [`SphereModeCharacter`] (evaluated at
/// its own `k = √λ`), its dominant family if one captures at least
/// [`MIN_FAMILY_OVERLAP`], and its multiplet size by
/// [`cluster_wavenumbers`] at [`MULTIPLET_LINK_TOL`], once among all modes
/// and once among the modes of the same family. Output order matches
/// `modes`.
///
/// The multiplet sizes only see the modes passed in: a multiplet cut by the
/// eigensolver's "nearest `n` to the shift" truncation is reported short
/// and fails [`ClassifiedMode::is_member_of`], which is the conservative
/// outcome.
pub fn classify_sphere_modes(
    fixture: &SphereFixture,
    modes: &[(c64, &[c64])],
    n_inside: f64,
) -> Vec<ClassifiedMode> {
    let ks: Vec<c64> = modes
        .iter()
        .map(|(lambda, _)| crate::eigen::wavenumber::principal_sqrt(*lambda))
        .collect();
    let mut out: Vec<ClassifiedMode> = modes
        .iter()
        .zip(ks.iter())
        .map(|((lambda, field), k)| {
            let character = sphere_mode_character(fixture, field, n_inside, *k);
            let (dom, frac) = character.dominant();
            ClassifiedMode {
                lambda: *lambda,
                k: *k,
                character,
                family: (frac >= MIN_FAMILY_OVERLAP).then_some(dom),
                k_cluster_size: 0,
                multiplet_size: 0,
                multiplet_id: None,
            }
        })
        .collect();
    for cluster in cluster_wavenumbers(&ks, MULTIPLET_LINK_TOL) {
        for &i in &cluster {
            out[i].k_cluster_size = cluster.len();
        }
    }
    let mut next_id = 0;
    for fam in MultipoleFamily::ALL {
        let members: Vec<usize> = (0..out.len())
            .filter(|&i| out[i].family == Some(fam))
            .collect();
        let member_ks: Vec<c64> = members.iter().map(|&i| ks[i]).collect();
        for cluster in cluster_wavenumbers(&member_ks, MULTIPLET_LINK_TOL) {
            for &j in &cluster {
                out[members[j]].multiplet_size = cluster.len();
                out[members[j]].multiplet_id = Some(next_id);
            }
            next_id += 1;
        }
    }
    out
}

/// The identified `family` multiplet nearest an analytic root: among the
/// modes passing [`ClassifiedMode::is_member_of`], take the one nearest
/// `(re_k, |im_k|)` and return the indices (into `modes`) of its whole
/// multiplet, nearest member first. `None` if no complete multiplet of
/// that family was found.
pub fn select_multiplet(
    modes: &[ClassifiedMode],
    family: MultipoleFamily,
    re_k: f64,
    im_k: f64,
) -> Option<Vec<usize>> {
    let by_dist = |a: &usize, b: &usize| {
        modes[*a]
            .distance_to(re_k, im_k)
            .partial_cmp(&modes[*b].distance_to(re_k, im_k))
            .expect("finite distances")
    };
    let best = (0..modes.len())
        .filter(|&i| modes[i].is_member_of(family))
        .min_by(by_dist)?;
    let mut members: Vec<usize> = (0..modes.len())
        .filter(|&i| modes[i].multiplet_id == modes[best].multiplet_id)
        .collect();
    members.sort_by(by_dist);
    Some(members)
}

/// Edge DOFs (`∫ E·t dl` along each globally oriented mesh edge, 3-point
/// Gauss–Legendre) of a continuous field — the first-order Nédélec
/// interpolant. Used to build synthetic multipole fields for the unit and
/// negative tests of the identity check.
pub fn interpolate_edge_field(
    fixture: &SphereFixture,
    field: impl Fn([f64; 3]) -> [c64; 3],
) -> Vec<c64> {
    // 3-point Gauss–Legendre on [0, 1].
    const GL: [(f64, f64); 3] = [
        (0.112_701_665_379_258_3, 5.0 / 18.0),
        (0.5, 8.0 / 18.0),
        (0.887_298_334_620_741_7, 5.0 / 18.0),
    ];
    let mesh = &fixture.mesh;
    mesh.edges()
        .iter()
        .map(|e| {
            let a = mesh.nodes[e[0] as usize];
            let b = mesh.nodes[e[1] as usize];
            let t = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let mut acc = c64::new(0.0, 0.0);
            for &(s, w) in GL.iter() {
                let x = [a[0] + s * t[0], a[1] + s * t[1], a[2] + s * t[2]];
                let f = field(x);
                acc += c64::new(w, 0.0)
                    * (f[0] * c64::new(t[0], 0.0)
                        + f[1] * c64::new(t[1], 0.0)
                        + f[2] * c64::new(t[2], 0.0));
            }
            acc
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::read_sphere_fixture;

    const N_INSIDE: f64 = 1.5;

    /// A fixed complex combination of a family's members (so the test
    /// field is a generic member of the multiplet, not a basis vector).
    fn synthetic(fam: MultipoleFamily, k: c64, x: [f64; 3]) -> [c64; 3] {
        let coeffs = [
            c64::new(0.7, 0.2),
            c64::new(-0.3, 0.5),
            c64::new(0.4, -0.6),
            c64::new(0.1, 0.9),
            c64::new(-0.8, 0.3),
        ];
        let mut out = [c64::new(0.0, 0.0); 3];
        for (i, coeff) in coeffs.iter().enumerate().take(fam.multiplicity()) {
            let b = fam.member_field(i, N_INSIDE, k, x);
            for d in 0..3 {
                out[d] += *coeff * b[d];
            }
        }
        out
    }

    #[test]
    fn synthetic_multipoles_are_classified_as_themselves() {
        // Each analytic family, interpolated onto the fixture's edges with
        // the in-ball formula, must be recognised as itself and not as
        // any of the other three. This is the mutation check for the
        // identity test: a TM_1, TE_2 or TM_2 field does not pass as TE_1.
        let f = read_sphere_fixture().expect("fixture load");
        let k = c64::new(1.880_74, -0.481_81); // open-space TE_1,1 root
        for fam in MultipoleFamily::ALL {
            let x = interpolate_edge_field(&f, |p| synthetic(fam, k, p));
            let ch = sphere_mode_character(&f, &x, N_INSIDE, k);
            eprintln!("{}: {}", fam.label(), ch.summary());
            assert_eq!(ch.dominant().0, fam, "{} misclassified", fam.label());
            assert!(
                ch.overlap(fam) > 0.9,
                "{} self-overlap {:.3} ≤ 0.9",
                fam.label(),
                ch.overlap(fam)
            );
            for other in MultipoleFamily::ALL {
                if other != fam {
                    assert!(
                        ch.overlap(other) < 0.05,
                        "{} leaks {:.3} into {}",
                        fam.label(),
                        ch.overlap(other),
                        other.label()
                    );
                }
            }
            let sum: f64 = ch.overlaps.iter().sum();
            assert!(sum < 1.0 + 1e-6, "captured fractions sum to {sum} > 1");
            if fam.pol == MiePolarisation::TE {
                assert!(
                    ch.radial_fraction < 0.05,
                    "{} radial fraction {:.3}",
                    fam.label(),
                    ch.radial_fraction
                );
            }
        }
    }

    #[test]
    fn character_is_invariant_under_complex_rescaling() {
        let f = read_sphere_fixture().expect("fixture load");
        let k = c64::new(1.8, 0.6);
        let fam = MultipoleFamily::ALL[0];
        let x = interpolate_edge_field(&f, |p| synthetic(fam, k, p));
        let scaled: Vec<c64> = x.iter().map(|v| *v * c64::new(-3.0, 4.0)).collect();
        let a = sphere_mode_character(&f, &x, N_INSIDE, k);
        let b = sphere_mode_character(&f, &scaled, N_INSIDE, k);
        assert!((a.ball_energy_fraction - b.ball_energy_fraction).abs() < 1e-12);
        assert!((a.radial_fraction - b.radial_fraction).abs() < 1e-12);
        for i in 0..4 {
            assert!((a.overlaps[i] - b.overlaps[i]).abs() < 1e-12);
        }
    }

    #[test]
    fn energy_fractions_partition_and_locate_the_field() {
        // A field confined to r > R_PML_INNER has no ball energy; the
        // three fractions always sum to 1.
        let f = read_sphere_fixture().expect("fixture load");
        let x = interpolate_edge_field(&f, |p| {
            let r = dot3(p, p).sqrt();
            let amp = if r > R_PML_INNER + 0.1 { 1.0 } else { 0.0 };
            [c64::new(amp, 0.0), c64::new(0.0, 0.0), c64::new(0.0, 0.0)]
        });
        let ch = sphere_mode_character(&f, &x, N_INSIDE, c64::new(1.8, 0.6));
        let sum = ch.ball_energy_fraction + ch.gap_energy_fraction + ch.pml_energy_fraction;
        assert!((sum - 1.0).abs() < 1e-12, "fractions sum to {sum}");
        assert_eq!(ch.ball_energy_fraction, 0.0);
        assert!(ch.pml_energy_fraction > 0.9, "{}", ch.summary());
    }

    #[test]
    fn cluster_wavenumbers_reads_multiplet_sizes() {
        let ks = [
            c64::new(1.870, 0.10),
            c64::new(2.400, 0.30),
            c64::new(1.872, 0.10),
            c64::new(1.871, 0.101),
            c64::new(2.405, 0.30),
            c64::new(3.0, 0.0),
        ];
        let clusters = cluster_wavenumbers(&ks, 0.01);
        assert_eq!(clusters, vec![vec![0, 2, 3], vec![1, 4], vec![5]]);
        // Single linkage chains: 0 – 0.006 – 0.012 link at tol 0.007.
        let chain = [
            c64::new(0.0, 0.0),
            c64::new(0.006, 0.0),
            c64::new(0.012, 0.0),
        ];
        assert_eq!(cluster_wavenumbers(&chain, 0.007), vec![vec![0, 1, 2]]);
        assert!(cluster_wavenumbers(&[], 0.1).is_empty());
    }

    #[test]
    fn identity_check_accepts_a_triplet_and_rejects_wrong_modes() {
        // Three TE_1 members (a mesh-split triplet), three TM_1 members
        // sitting right next to them in k, and a lone TE_1 field far away.
        let f = read_sphere_fixture().expect("fixture load");
        let te1 = MultipoleFamily::ALL[0];
        let tm1 = MultipoleFamily::ALL[1];
        let mut fields: Vec<(c64, Vec<c64>)> = Vec::new();
        let base = c64::new(1.85, 0.19);
        for (i, dk) in [0.0, 0.002, 0.004].iter().enumerate() {
            let k = base + c64::new(*dk, 0.0);
            fields.push((
                k * k,
                interpolate_edge_field(&f, |p| te1.member_field(i, N_INSIDE, k, p)),
            ));
        }
        for (i, dk) in [0.008, 0.010, 0.012].iter().enumerate() {
            let k = base + c64::new(*dk, 0.0);
            fields.push((
                k * k,
                interpolate_edge_field(&f, |p| tm1.member_field(i, N_INSIDE, k, p)),
            ));
        }
        let lone = c64::new(2.6, 0.4);
        fields.push((
            lone * lone,
            interpolate_edge_field(&f, |p| te1.member_field(0, N_INSIDE, lone, p)),
        ));
        let modes: Vec<(c64, &[c64])> = fields.iter().map(|(l, v)| (*l, v.as_slice())).collect();
        let classified = classify_sphere_modes(&f, &modes, N_INSIDE);
        for c in &classified {
            eprintln!("{}", c.summary());
        }

        // The six near-degenerate modes form one k-cluster, so the
        // eigenvalues alone cannot tell the triplet from its neighbours.
        for c in &classified[..6] {
            assert_eq!(c.k_cluster_size, 6);
        }
        for c in &classified[..3] {
            assert!(c.is_member_of(te1), "{}", c.summary());
            assert!(c.rejection(te1).is_none());
            assert!(!c.is_member_of(tm1));
        }
        // Wrong polarisation, same place in the spectrum: rejected.
        for c in &classified[3..6] {
            assert!(!c.is_member_of(te1), "{}", c.summary());
            let why = c.rejection(te1).expect("rejected");
            assert!(why.contains("TM_1"), "{why}");
            assert!(c.is_member_of(tm1));
        }
        // Right field, wrong multiplicity: rejected.
        assert_eq!(classified[6].family, Some(te1));
        assert_eq!(classified[6].multiplet_size, 1);
        assert!(!classified[6].is_member_of(te1));
        let why = classified[6].rejection(te1).expect("rejected");
        assert!(why.contains("1 member"), "{why}");

        // Selection returns the whole triplet, nearest member first, and
        // never the TM_1 modes, even when they are nearer the target.
        let picked = select_multiplet(&classified, te1, 1.862, 0.19).expect("triplet");
        assert_eq!(picked, vec![2, 1, 0]);
        let nearest_any = (0..classified.len())
            .min_by(|a, b| {
                classified[*a]
                    .distance_to(1.862, 0.19)
                    .partial_cmp(&classified[*b].distance_to(1.862, 0.19))
                    .unwrap()
            })
            .unwrap();
        assert_eq!(nearest_any, 5, "the nearest mode overall is a TM_1 member");
        assert!(select_multiplet(&classified, MultipoleFamily::ALL[2], 1.862, 0.19).is_none());
    }

    #[test]
    fn a_mode_with_no_majority_family_is_unclassified() {
        // A TE_1 + TM_1 mixture balanced so that both families capture the
        // same share of the in-ball energy: neither reaches 0.5.
        let f = read_sphere_fixture().expect("fixture load");
        let te1 = MultipoleFamily::ALL[0];
        let tm1 = MultipoleFamily::ALL[1];
        let k = c64::new(1.85, 0.19);
        let te = interpolate_edge_field(&f, |p| te1.member_field(0, N_INSIDE, k, p));
        let tm = interpolate_edge_field(&f, |p| tm1.member_field(1, N_INSIDE, k, p));
        let mix = |scale: f64| -> Vec<c64> {
            te.iter()
                .zip(tm.iter())
                .map(|(a, b)| *a + *b * c64::new(scale, 0.0))
                .collect()
        };
        // The families are orthogonal, so the captured energies are
        // E_TE and scale²·E_TM: one trial fixes the balancing scale.
        let trial = sphere_mode_character(&f, &mix(1.0), N_INSIDE, k);
        let scale = (trial.overlap(te1) / trial.overlap(tm1)).sqrt();
        let field = mix(scale);
        let classified = classify_sphere_modes(&f, &[(k * k, field.as_slice())], N_INSIDE);
        let c = &classified[0];
        eprintln!("{}", c.summary());
        let (o_te, o_tm) = (c.character.overlap(te1), c.character.overlap(tm1));
        assert!((o_te - o_tm).abs() < 1e-3, "unbalanced: {o_te} vs {o_tm}");
        assert!(
            o_te > 0.4 && o_te < MIN_FAMILY_OVERLAP,
            "TE_1 overlap {o_te}"
        );
        assert_eq!(c.family, None);
        assert_eq!(c.multiplet_size, 0);
        assert!(!c.is_member_of(te1) && !c.is_member_of(tm1));
        let why = c.rejection(te1).expect("rejected");
        assert!(why.contains("no family"), "{why}");
    }
}
