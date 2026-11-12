use super::*;

/// Surface-impedance model `Z_s(ω)` for the Leontovich boundary
/// condition (Epic #193, issue #204).
///
/// The Leontovich BC replaces a thick-conductor interior (skin depth
/// `δ = √(2/ωμσ)` too fine to mesh) with the impedance condition
/// `E_t = −Z_s(ω) n̂ × H` on the conductor surface (`n̂` the outward
/// normal of the *computational* domain, pointing into the conductor).
/// In the weak form this is the surface term `+(iω/Z_s) ∮ v · E_t dS`
/// — the Silver-Müller term with `1/η₀ → 1/Z_s(ω)`.
#[derive(Debug, Clone, Copy)]
pub enum SurfaceImpedanceModel {
    /// Frequency-independent complex surface impedance. `Fixed(η₀)`
    /// (`= 1` in natural units) reproduces the first-order
    /// Silver-Müller absorbing boundary exactly.
    Fixed(c64),
    /// Built-in **good-conductor** model
    ///
    /// ```text
    /// Z_s(ω) = (1 + i) √(ωμ / 2σ) = (1 + i) / (σ δ),   δ = √(2 / ωμσ),
    /// ```
    ///
    /// in natural units (`μ = 1`). `sigma` is the conductor's electrical
    /// conductivity in natural units `1/length`
    /// (`σ_nat = σ_SI · Z₀ · L_unit`), the same normalization as the
    /// volumetric `sigma_tet` of [`driven_solve_with_sigma`]. Note the
    /// **ω-dependence**: `Z_s ∝ √ω · (1+i)`, so the weak-form
    /// coefficient `iω/Z_s = (1+i)√(ωσ/2) = (1+i)/δ` scales as `√ω`.
    GoodConductor {
        /// Conductivity σ in natural units (`1/length`). Must be `> 0`.
        sigma: f64,
    },
    /// Built-in **London superconductor** model (Epic #475, issue #604)
    ///
    /// ```text
    /// Z_s(ω) = iωμλ_L,   μ = 1 natural units,
    /// ```
    ///
    /// the purely reactive (kinetic-inductance) surface impedance of a
    /// superconductor in the local London limit, with `λ_L` the London
    /// penetration depth. Unlike the good-conductor `√ω (1+i)` dispersion,
    /// the London impedance is **frequency-linear and lossless**.
    ///
    /// `lambda_l` is a **length** in natural (mesh) units, the same
    /// normalization as the mesh coordinates: `λ_L_nat = λ_L_SI / L_unit`
    /// (e.g. λ_L = 90 nm on a micrometre-unit mesh is `lambda_l = 0.09`)
    /// — the length-based sibling of the `GoodConductor` σ normalization
    /// `σ_nat = σ_SI · Z₀ · L_unit` above.
    ///
    /// The weak-form coefficient is special: `iω/Z_s = iω/(iωλ_L) = 1/λ_L`
    /// — **real, positive and frequency-independent** (the iω cancels
    /// exactly), so [`Self::weak_coefficient`] returns `1/λ_L` directly
    /// instead of evaluating the `iω/Z_s` quotient numerically (which
    /// would degrade to 0/0 as ω → 0 and trip the `|Z_s| = 0` singular
    /// guard).
    ///
    /// `lambda_l` must be strictly positive and finite: the `λ_L → 0` PEC
    /// limit must be expressed through the PEC edge mask (exactly like
    /// `Z_s → 0` for the other models), not through a vanishing
    /// penetration depth.
    London {
        /// London penetration depth λ_L in natural units (mesh length
        /// unit): `λ_L_nat = λ_L_SI / L_unit`. Must be `> 0` and finite.
        lambda_l: f64,
    },
    /// **Rough good conductor** (Epic #756, issue #758): the
    /// [`Self::GoodConductor`] impedance scaled by a real, frequency-
    /// dependent roughness loss factor `K(ω) ≥ 1`
    /// ([`SurfaceRoughness::loss_factor`]):
    ///
    /// ```text
    /// Z_s,rough(ω) = K(ω) · (1 + i) √(ωμ / 2σ).
    /// ```
    ///
    /// The **whole complex** `Z_s` is scaled — resistive and reactive
    /// (internal-inductance) parts alike — the convention of the
    /// Hammerstad–Jensen (1980) and Huray (2010) papers, which define `K`
    /// as a multiplier on the conductor's surface impedance, and of the
    /// common SI field solvers. Every consumer (dense / direct assembly,
    /// the AMS SPD proxy, the matrix-free operator and the PROM sweep)
    /// reads `Z_s` through [`Self::weak_coefficient`], so roughness
    /// composes with all of them without further changes: since `K` is
    /// real and positive, `iω/Z_s,rough = (1/K)·iω/Z_s,smooth`.
    ///
    /// With `K ≡ 1` (zero RMS roughness, zero Huray spheres) this is
    /// **bit-identical** to [`Self::GoodConductor`].
    RoughConductor {
        /// Conductivity σ in natural units (`1/length`), as for
        /// [`Self::GoodConductor`]. Must be `> 0`.
        sigma: f64,
        /// The roughness model and its parameters (natural units).
        roughness: SurfaceRoughness,
    },
}

/// Conductor surface-roughness model (Epic #756, issue #758): a real
/// loss factor `K(ω) = P_rough / P_smooth ≥ 1` multiplying a good
/// conductor's surface impedance (see
/// [`SurfaceImpedanceModel::RoughConductor`]).
///
/// Both models depend on frequency only through the skin depth
///
/// ```text
/// δ(ω) = √(2 / ωμσ)        (μ = 1 natural units; δ_SI = 1/√(π f μ₀ σ)),
/// ```
///
/// and all lengths are in natural (mesh) units, the same normalization as
/// the mesh coordinates: `ℓ_nat = ℓ_SI / L_unit` (areas `/ L_unit²`). In
/// these units `δ_nat = δ_SI / L_unit` exactly, since
/// `ω_nat · σ_nat = (ω L/c)(σ η₀ L) = ω μ₀ σ L²`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SurfaceRoughness {
    /// **Hammerstad–Jensen** empirical model (E. Hammerstad and
    /// Ø. Jensen, "Accurate models for microstrip computer-aided design,"
    /// *IEEE MTT-S Int. Microwave Symp. Digest*, 1980, pp. 407–409):
    ///
    /// ```text
    /// K = 1 + (2/π) · atan(1.4 · (Δ/δ)²),
    /// ```
    ///
    /// with `Δ` the RMS surface roughness. `K → 1` as `Δ/δ → 0` (low
    /// frequency or smooth foil) and saturates at `K → 2` as
    /// `Δ/δ → ∞`.
    HammerstadJensen {
        /// RMS roughness `Δ` in natural (mesh) length units. Must be
        /// finite and `≥ 0` (`0` reproduces the smooth conductor
        /// exactly).
        rms: f64,
    },
    /// **Huray "snowball" model**, single sphere size, on a flat base
    /// (P. G. Huray, O. Oluwafemi, J. Loyer, E. Bogatin and X. Ye,
    /// "Impact of copper surface texture on loss: a model that works,"
    /// *DesignCon* 2010; P. G. Huray, *The Foundations of Signal
    /// Integrity*, Wiley 2009, ch. 6), in the "cannonball" form of
    /// Y. Shlepnev and C. Nwachukwu, "Roughness characterization for
    /// interconnect analysis," *IEEE Int. Symp. EMC*, 2011:
    ///
    /// ```text
    /// K = A_matte/A_flat + (3/2) · (N · 4πa² / A_tile) / (1 + δ/a + δ²/(2a²)),
    /// ```
    ///
    /// implemented with the flat-base ratio `A_matte/A_flat = 1` (the
    /// cannonball convention) and one sphere radius `a`, `N` spheres per
    /// tile of area `A_tile`. The `3/2` is the high-frequency loss of a
    /// conducting sphere in a uniform tangential field (`6πa²` of
    /// flat-surface loss per sphere, i.e. `3/2` of its surface area).
    /// `K → 1` as `δ/a → ∞` (low frequency) or `N → 0`, and `K` increases
    /// monotonically with frequency to `1 + (3/2)·N·4πa²/A_tile`.
    Huray {
        /// Sphere ("snowball") radius `a` in natural (mesh) length units.
        /// Must be finite and `> 0`.
        ball_radius: f64,
        /// Number of spheres `N` per tile (real; fitted values are often
        /// fractional). Must be finite and `≥ 0` (`0` is smooth).
        n_balls: f64,
        /// Tile (flat base) area `A_tile` in natural units (mesh length
        /// squared). Must be finite and `> 0`.
        tile_area: f64,
    },
}

impl SurfaceRoughness {
    /// Skin depth `δ = √(2/ωσ)` (natural units, `μ = 1`) of a conductor
    /// of natural-unit conductivity `sigma` at natural angular frequency
    /// `omega` (`k₀`). `+∞` at `ω = 0`.
    pub fn skin_depth(omega: f64, sigma: f64) -> f64 {
        (2.0 / (omega * sigma)).sqrt()
    }

    /// The roughness loss factor `K` at skin depth `delta` (same length
    /// unit as the model's parameters). `delta = +∞` (DC) gives exactly
    /// `1`.
    pub fn loss_factor_at_skin_depth(&self, delta: f64) -> f64 {
        match *self {
            SurfaceRoughness::HammerstadJensen { rms } => {
                let r = rms / delta;
                1.0 + std::f64::consts::FRAC_2_PI * (1.4 * r * r).atan()
            }
            SurfaceRoughness::Huray {
                ball_radius: a,
                n_balls,
                tile_area,
            } => {
                let ratio = n_balls * 4.0 * std::f64::consts::PI * a * a / tile_area;
                let d = delta / a;
                1.0 + 1.5 * ratio / (1.0 + d + 0.5 * d * d)
            }
        }
    }

    /// The roughness loss factor `K(ω)` of a conductor of natural-unit
    /// conductivity `sigma` at natural angular frequency `omega`.
    pub fn loss_factor(&self, omega: f64, sigma: f64) -> f64 {
        self.loss_factor_at_skin_depth(Self::skin_depth(omega, sigma))
    }

    /// `true` when every parameter is finite and in range (`rms ≥ 0`;
    /// `ball_radius > 0`, `n_balls ≥ 0`, `tile_area > 0`).
    pub fn is_valid(&self) -> bool {
        match *self {
            SurfaceRoughness::HammerstadJensen { rms } => rms.is_finite() && rms >= 0.0,
            SurfaceRoughness::Huray {
                ball_radius,
                n_balls,
                tile_area,
            } => {
                ball_radius.is_finite()
                    && ball_radius > 0.0
                    && n_balls.is_finite()
                    && n_balls >= 0.0
                    && tile_area.is_finite()
                    && tile_area > 0.0
            }
        }
    }
}

/// A differentiable parameter of a [`SurfaceImpedanceModel`] wall
/// (Epic #841 Phase 2b, issue #1260): the `θ` of
/// [`SurfaceImpedanceModel::d_weak_coefficient`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WallParam {
    /// Conductivity `σ` of a [`SurfaceImpedanceModel::GoodConductor`] or
    /// [`SurfaceImpedanceModel::RoughConductor`] wall (natural units).
    Sigma,
    /// Hammerstad–Jensen RMS roughness `Δ`.
    RmsRoughness,
    /// Huray sphere radius `a` (at fixed sphere density `N/A_tile`).
    BallRadius,
    /// Huray sphere density `ρ = N / A_tile` (`N = ρ·A_tile` at fixed tile).
    BallDensity,
    /// London penetration depth `λ_L`.
    LondonDepth,
}

/// `∂c_Γ/∂θ` of one wall parameter ([`SurfaceImpedanceModel::d_weak_coefficient`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WeakCoefficientGradient {
    /// `∂(iω/Z_s)/∂θ` at the given `ω`.
    pub dc: c64,
    /// `true` for a roughness parameter whose loss factor has reached its
    /// plateau (`K → 2` for Hammerstad–Jensen, `K → 1 + (3/2)·N·4πa²/A_tile`
    /// for Huray; within `10⁻³` of the plateau excess), so the gradient is
    /// ≈ 0 by saturation rather than by an error. Always `false` for `σ`
    /// and `λ_L`.
    pub saturated: bool,
}

impl SurfaceRoughness {
    /// `(K, δ·∂K/∂δ)` at skin depth `delta`.
    fn k_and_delta_dk(&self, delta: f64) -> (f64, f64) {
        match *self {
            SurfaceRoughness::HammerstadJensen { rms } => {
                let r = rms / delta;
                let x = 1.4 * r * r;
                let k = 1.0 + std::f64::consts::FRAC_2_PI * x.atan();
                // dK/dx = (2/π)/(1+x²); δ·dx/dδ = −2x.
                (k, -std::f64::consts::FRAC_2_PI * 2.0 * x / (1.0 + x * x))
            }
            SurfaceRoughness::Huray {
                ball_radius: a,
                n_balls,
                tile_area,
            } => {
                let ratio = n_balls * 4.0 * std::f64::consts::PI * a * a / tile_area;
                let d = delta / a;
                let den = 1.0 + d + 0.5 * d * d;
                (
                    1.0 + 1.5 * ratio / den,
                    -1.5 * ratio * d * (1.0 + d) / (den * den),
                )
            }
        }
    }

    /// `(∂K/∂θ, saturated)` of the roughness parameter `param` at skin
    /// depth `delta`, or `None` when `param` is not a parameter of this
    /// model.
    fn d_loss_factor(&self, delta: f64, param: WallParam) -> Option<(f64, bool)> {
        const SAT: f64 = 1.0e-3;
        match (*self, param) {
            (SurfaceRoughness::HammerstadJensen { rms }, WallParam::RmsRoughness) => {
                let r = rms / delta;
                let x = 1.4 * r * r;
                // dK/dΔ = (2/π)/(1+x²) · dx/dΔ,  dx/dΔ = 2x/Δ = 2.8 Δ/δ².
                let dx = 2.8 * rms / (delta * delta);
                let dk = std::f64::consts::FRAC_2_PI * dx / (1.0 + x * x);
                // Fraction of the (K−1) plateau (=1) not yet reached.
                let sat = 1.0 - std::f64::consts::FRAC_2_PI * x.atan() < SAT;
                Some((dk, sat))
            }
            (
                SurfaceRoughness::Huray {
                    ball_radius: a,
                    n_balls,
                    tile_area,
                },
                WallParam::BallRadius | WallParam::BallDensity,
            ) => {
                let four_pi = 4.0 * std::f64::consts::PI;
                let ratio = n_balls * four_pi * a * a / tile_area;
                let d = delta / a;
                let den = 1.0 + d + 0.5 * d * d;
                // Fraction of the plateau excess not yet reached: 1 − 1/D.
                let sat = (1.0 - 1.0 / den) < SAT && ratio > 0.0;
                let dk = match param {
                    WallParam::BallRadius => 1.5 * ratio / (a * den) * (2.0 + d * (1.0 + d) / den),
                    _ => 1.5 * four_pi * a * a / den,
                };
                Some((dk, sat))
            }
            _ => None,
        }
    }
}

impl SurfaceImpedanceModel {
    /// Closed-form `∂c_Γ/∂θ` of the weak-form coefficient
    /// `c_Γ(ω) = iω/Z_s(ω)` ([`Self::weak_coefficient`]) with respect to
    /// the wall parameter `param` (Epic #841 Phase 2b, issue #1260).
    ///
    /// With `c = c₀(σ)/K`, `c₀ = (1+i)/δ`, `δ = √(2/ωσ)`:
    ///
    /// ```text
    /// ∂c/∂σ = c/(2σ) · (1 + δ ∂K/∂δ / K)      // the ∂K/∂δ term: K depends on σ through δ
    /// ∂c/∂θ = −c · ∂K/∂θ / K                  // roughness parameters (Δ, a, ρ)
    /// ∂c/∂λ_L = −1/λ_L²                       // London: c = 1/λ_L, ω-independent
    /// ```
    ///
    /// Returns `None` when `param` is not a parameter of this model (e.g.
    /// `Sigma` on `London`, `RmsRoughness` on a Huray wall).
    ///
    /// # Errors
    ///
    /// Propagates [`Self::weak_coefficient`]'s singular-impedance error.
    pub fn d_weak_coefficient(
        &self,
        omega: f64,
        param: WallParam,
    ) -> Result<Option<WeakCoefficientGradient>, DrivenError> {
        let c = self.weak_coefficient(omega)?;
        let g = |dc: c64, saturated: bool| Some(WeakCoefficientGradient { dc, saturated });
        Ok(match (*self, param) {
            (SurfaceImpedanceModel::GoodConductor { sigma }, WallParam::Sigma) => {
                g(c / (2.0 * sigma), false)
            }
            (SurfaceImpedanceModel::RoughConductor { sigma, roughness }, WallParam::Sigma) => {
                let delta = SurfaceRoughness::skin_depth(omega, sigma);
                let (k, d_dk) = roughness.k_and_delta_dk(delta);
                g(c / (2.0 * sigma) * (1.0 + d_dk / k), false)
            }
            (SurfaceImpedanceModel::RoughConductor { sigma, roughness }, p) => {
                let delta = SurfaceRoughness::skin_depth(omega, sigma);
                match roughness.d_loss_factor(delta, p) {
                    Some((dk, sat)) => {
                        let k = roughness.loss_factor_at_skin_depth(delta);
                        g(-c * (dk / k), sat)
                    }
                    None => None,
                }
            }
            (SurfaceImpedanceModel::London { lambda_l }, WallParam::LondonDepth) => {
                g(c64::new(-1.0 / (lambda_l * lambda_l), 0.0), false)
            }
            _ => None,
        })
    }

    /// Evaluate `Z_s(ω)`.
    pub fn z_s(&self, omega: f64) -> c64 {
        match *self {
            SurfaceImpedanceModel::Fixed(z) => z,
            SurfaceImpedanceModel::GoodConductor { sigma } => {
                // (1 + i) √(ωμ / 2σ), μ = 1 natural units.
                let a = (omega / (2.0 * sigma)).sqrt();
                c64::new(a, a)
            }
            // iωμλ_L, μ = 1 natural units: purely reactive.
            SurfaceImpedanceModel::London { lambda_l } => c64::new(0.0, omega * lambda_l),
            SurfaceImpedanceModel::RoughConductor { sigma, roughness } => {
                // K(ω) · (1 + i) √(ωμ / 2σ): the full complex Z_s scaled.
                if !roughness.is_valid() {
                    // Poison Z_s so `weak_coefficient` reports it singular.
                    return c64::new(f64::NAN, f64::NAN);
                }
                let a = (omega / (2.0 * sigma)).sqrt();
                let k = roughness.loss_factor(omega, sigma);
                c64::new(k * a, k * a)
            }
        }
    }

    /// The weak-form surface coefficient `iω / Z_s(ω)` multiplying the
    /// real surface mass `S_Γ`. For [`SurfaceImpedanceModel::Fixed`]
    /// with `Z_s = η₀ = 1` this is `i k₀` — exactly the Silver-Müller
    /// factor; for the good-conductor model it is
    /// `(1+i)√(ωσ/2) = (1+i)/δ` (divided by `K(ω)` for the rough
    /// conductor); for the London model it is the real,
    /// frequency-independent `1/λ_L` (evaluated **directly**, not as the
    /// `iω/(iωλ_L)` quotient, so it stays finite and exact at any ω,
    /// including ω = 0).
    ///
    /// # Errors
    ///
    /// Returns [`DrivenError::SurfaceImpedanceSingular`] when `Z_s(ω)`
    /// is zero or non-finite (e.g. `Fixed(0)`, `GoodConductor` with
    /// `σ ≤ 0`, `RoughConductor` with out-of-range roughness parameters
    /// ([`SurfaceRoughness::is_valid`]), or `London` with `λ_L ≤ 0` /
    /// non-finite). The
    /// `Z_s → 0` (`λ_L → 0`) PEC limit must be expressed through the
    /// PEC edge mask, not through a vanishing impedance.
    pub fn weak_coefficient(&self, omega: f64) -> Result<c64, DrivenError> {
        if let SurfaceImpedanceModel::London { lambda_l } = *self {
            // iω/Z_s = iω/(iωλ_L) = 1/λ_L exactly — real and
            // ω-independent. Returned directly so the quotient never
            // degrades to 0/0 at small ω.
            if !(lambda_l.is_finite() && lambda_l > 0.0) {
                return Err(DrivenError::SurfaceImpedanceSingular {
                    z_re: 0.0,
                    z_im: omega * lambda_l,
                    omega,
                });
            }
            return Ok(c64::new(1.0 / lambda_l, 0.0));
        }
        let z = self.z_s(omega);
        let singular = |z: c64| DrivenError::SurfaceImpedanceSingular {
            z_re: z.re,
            z_im: z.im,
            omega,
        };
        if !(z.re.is_finite() && z.im.is_finite()) {
            return Err(singular(z));
        }
        let abs2 = z.re * z.re + z.im * z.im;
        if abs2 == 0.0 {
            return Err(singular(z));
        }
        // iω / Z_s = iω · conj(Z_s) / |Z_s|².
        Ok(c64::new(omega * z.im / abs2, omega * z.re / abs2))
    }
}

/// One Leontovich impedance surface: a set of conforming boundary
/// triangles sharing a surface-impedance model.
///
/// Multiple [`SurfaceImpedanceBc`]s with different models may be passed
/// to [`driven_solve_with_surface_impedance`] (per-surface `Z_s(ω)`,
/// e.g. different conductor metals on different walls).
#[derive(Debug, Clone, Copy)]
pub struct SurfaceImpedanceBc<'a> {
    /// Boundary triangles (0-based node indices into `mesh.nodes`,
    /// any winding). Each triangle must be a face of the tet mesh —
    /// its three edges must appear in the global edge table.
    pub triangles: &'a [[u32; 3]],
    /// Surface impedance `Z_s(ω)` on these triangles.
    pub model: SurfaceImpedanceModel,
}
