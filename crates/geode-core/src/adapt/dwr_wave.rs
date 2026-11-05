//! Wave-port DWR (Epic #835 Phase 4, issue #1266, step 3b): the `dwr-p2`
//! estimate of a wave-port S-parameter goal.
//!
//! Wave ports are not baked into [`DrivenOperator`]: they enter as the
//! rank-`N_w` modal-Robin term `Σ_q j·y_q f_q f_qᵀ` that the sweeps fold in by
//! Sherman–Morrison–Woodbury (SMW) on top of the factored base operator. The
//! full operator is `A = A_base + Σ_q j·y_q f_q f_qᵀ`; it is **complex
//! symmetric** (`Aᵀ = A`), so the dual of an `S_kj` goal is again a solve with
//! `A`, here with the same SMW correction. [`WavePortOperator`] packages the
//! base factorization, the SMW data and the modal fluxes at one `ω`, so the
//! p=1 solution, the p=2 residual and the p=2 dual all use the *same* operator
//! the sweeps use, with no second factorization of a different system.
//!
//! The p=1 solution being judged and the p=2 operator carry different port
//! modes (each is the face mode of its own order), so the p=2 residual is
//! `r = b₂ − A₂ E_h` with the p=2 excitation `b₂`. As in [`super::dwr::dwr_p2`]
//! the estimate is a *p=2 error* estimate of the p=2 goal.

use faer::c64;

use crate::adapt::dwr::{DwrError, DwrEstimate, DwrEstimator, dwr_from_residual_and_weight_shared};
use crate::assembly::hcurl_space::HcurlSpace;
use crate::driven::ports::WavePort;
use crate::driven::ports::mixed::{
    ModalChannel, ModalSmw, channel_admittances, dot_t, interior_fluxes,
};
use crate::driven::ports::wave_p2::modal_channels_on_space;
use crate::driven::solve::{DrivenError, DrivenOperator, FactoredDrivenOperator};
use crate::mesh::TetMesh;

fn drv(e: DrivenError) -> DwrError {
    DwrError::Driven(e.to_string())
}

/// A wave-port driven operator `A_base + Σ_q j·y_q f_q f_qᵀ` factored at one
/// `ω`, on one [`HcurlSpace`] (p=1 or p=2).
pub struct WavePortOperator<'a> {
    space: &'a HcurlSpace,
    omega: f64,
    fac: FactoredDrivenOperator<'a>,
    channels: Vec<ModalChannel>,
    fluxes_int: Vec<Vec<c64>>,
    ys: Vec<c64>,
    smw: ModalSmw,
    i2f: Vec<usize>,
}

impl<'a> WavePortOperator<'a> {
    /// Factor `op` (assembled on `space` with the walls / volume but **no**
    /// lumped ports, zero source) at `omega` and fold in the modal terms of
    /// `ports`. `pec_interior_mask` is the mask `op` was assembled with.
    ///
    /// # Errors
    ///
    /// [`DwrError::Driven`] for an invalid port, a singular SMW capacitance
    /// matrix or a failed factorization.
    pub fn new(
        space: &'a HcurlSpace,
        mesh: &TetMesh,
        op: &'a DrivenOperator,
        pec_interior_mask: &[bool],
        ports: &[WavePort],
        omega: f64,
    ) -> Result<Self, DwrError> {
        if op.n_dofs() != space.n_dofs() {
            return Err(DwrError::NotP2Enrichment(
                "operator and space have different DOF counts",
            ));
        }
        let channels = modal_channels_on_space(space, mesh, 0, ports).map_err(drv)?;
        let (_, ys) = channel_admittances(ports, &channels, omega);
        let fluxes_int = interior_fluxes(&channels, pec_interior_mask);
        let fac = op.factor_at(omega).map_err(drv)?;
        let n_int = op.n_interior();
        let mut iters = Vec::new();
        let mut bs = |b: &[c64], x: &mut [c64]| fac.back_solve(b, x).map(|()| 0usize);
        let smw =
            ModalSmw::prepare(&fluxes_int, &ys, n_int, omega, &mut bs, &mut iters).map_err(drv)?;
        Ok(Self {
            space,
            omega,
            fac,
            channels,
            fluxes_int,
            ys,
            smw,
            i2f: op.interior_to_full(),
        })
    }

    /// Number of wave channels (ports × modes).
    pub fn n_channels(&self) -> usize {
        self.channels.len()
    }

    /// `A⁻¹ g` on interior vectors (`Aᵀ = A`, so this is also the dual solve).
    fn solve_int(&self, g: &[c64]) -> Result<Vec<c64>, DwrError> {
        let mut bs = |b: &[c64], x: &mut [c64]| self.fac.back_solve(b, x).map(|()| 0usize);
        let mut iters = Vec::new();
        self.smw
            .solve(&self.fluxes_int, g, &mut bs, &mut iters)
            .map_err(drv)
    }

    /// `A x` on interior vectors, modal terms included.
    fn apply_int(&self, x: &[c64]) -> Vec<c64> {
        let mut ax = vec![c64::new(0.0, 0.0); x.len()];
        self.fac.spmv_a(x, &mut ax);
        for (f, &y) in self.fluxes_int.iter().zip(&self.ys) {
            let scaled = c64::new(0.0, 1.0) * y * dot_t(f, x);
            for (a, &fr) in ax.iter_mut().zip(f) {
                *a += fr * scaled;
            }
        }
        ax
    }

    /// Interior excitation of channel `j`: `2j·y·a_inc·f`.
    fn excitation_int(&self, j: usize) -> Vec<c64> {
        let coeff = c64::new(0.0, 2.0) * self.ys[j] * self.channels[j].a_inc;
        self.fluxes_int[j].iter().map(|&f| f * coeff).collect()
    }

    fn scatter(&self, x_int: &[c64]) -> Vec<c64> {
        let mut full = vec![c64::new(0.0, 0.0); self.space.n_dofs()];
        for (k, &f) in self.i2f.iter().enumerate() {
            full[f] = x_int[k];
        }
        full
    }

    /// The full-length solution (`space.n_dofs()`) with channel `j` excited.
    ///
    /// # Errors
    ///
    /// [`DwrError::Driven`] if a back-solve fails.
    ///
    /// # Panics
    ///
    /// Panics if `j >= n_channels()`.
    pub fn solve_excitation(&self, j: usize) -> Result<Vec<c64>, DwrError> {
        assert!(j < self.channels.len(), "channel out of range");
        let x = self.solve_int(&self.excitation_int(j))?;
        Ok(self.scatter(&x))
    }

    /// Full-length modal flux `f_k` of channel `k`.
    pub fn flux(&self, k: usize) -> &[f64] {
        &self.channels[k].flux
    }

    /// `(ã_j^{-1} · √y_k/√ω …)` scale of `S_kj = a_k·√y_k/(a_inc,j·√y_j)`
    /// (the power-wave normalization of the sweeps), without the offset.
    pub fn s_scale(&self, k: usize, j: usize) -> c64 {
        let ww = |q: usize| self.ys[q].sqrt() / self.omega.sqrt();
        ww(k) / (self.channels[j].a_inc * ww(j))
    }

    /// The `S_kj` goal on this space: weights `f_k · s_scale`, offset `−δ_kj`.
    pub fn s_goal(&self, k: usize, j: usize) -> crate::adapt::goal::SParamGoal {
        let s = self.s_scale(k, j);
        let w = self.channels[k].flux.iter().map(|&f| s * f).collect();
        let offset = if k == j {
            c64::new(-1.0, 0.0)
        } else {
            c64::new(0.0, 0.0)
        };
        crate::adapt::goal::SParamGoal::from_weights(format!("s{}{}", k + 1, j + 1), w, offset)
    }
}

/// `dwr-p2` estimate of a wave-port goal.
///
/// `enr` is the **p=2** [`WavePortOperator`]; `x_h_p1` the p=1 solution of
/// the same excitation (`space.n_edges()` long, e.g. from
/// [`WavePortOperator::solve_excitation`] on the p=1 space); `excited` the
/// excited channel; `dual_rhs_p2` the goal's `∂J/∂x` on the p=2 space (an
/// [`SParamGoal`](crate::adapt::goal::SParamGoal) from
/// [`WavePortOperator::s_goal`]). The residual is `r = b₂ − A₂ E_h` with
/// `E_h` prolonged hierarchically, the dual `A₂ z = ∂J/∂x` (complex
/// symmetric), and the weight `z` with its Whitney DOFs removed; see
/// [`super::dwr::dwr_p2`].
///
/// # Errors
///
/// [`DwrError::NotP2Enrichment`] if `enr` is not p=2 or a length is wrong;
/// [`DwrError::Driven`] if a back-solve fails.
pub fn dwr_p2_wave(
    enr: &WavePortOperator<'_>,
    x_h_p1: &[c64],
    excited: usize,
    dual_rhs_p2: &[c64],
) -> Result<DwrEstimate, DwrError> {
    let space = enr.space;
    if space.order() != crate::elements::ElementOrder::P2 {
        return Err(DwrError::NotP2Enrichment("space must be p=2"));
    }
    if x_h_p1.len() != space.n_edges() || dual_rhs_p2.len() != space.n_dofs() {
        return Err(DwrError::NotP2Enrichment("vector length mismatch"));
    }
    if excited >= enr.n_channels() {
        return Err(DwrError::NotP2Enrichment("excited channel out of range"));
    }
    let zero = c64::new(0.0, 0.0);
    let x_full = space.prolong_p1(x_h_p1);
    let x_int: Vec<c64> = enr.i2f.iter().map(|&f| x_full[f]).collect();
    let ax = enr.apply_int(&x_int);
    let b = enr.excitation_int(excited);
    let mut residual = vec![zero; space.n_dofs()];
    for (k, &f) in enr.i2f.iter().enumerate() {
        residual[f] = b[k] - ax[k];
    }
    let g: Vec<c64> = enr.i2f.iter().map(|&f| dual_rhs_p2[f]).collect();
    let z = enr.solve_int(&g)?;
    let mut weight = enr.scatter(&z);
    for ge in 0..space.n_edges() {
        weight[space.edge_dofs(ge)[0] as usize] = zero;
    }
    dwr_from_residual_and_weight_shared(
        &residual,
        &weight,
        space.n_tets(),
        |t| space.tet_dofs(t),
        DwrEstimator::P2Enrichment,
    )
}
