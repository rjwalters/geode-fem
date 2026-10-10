# Transmon junction port: a `LumpedPort`-like formulation for the eigen path

**Issue:** #1003 (design spike, Epic #476) · **Date:** 2026-10-10 · **Palace source:**
`awslabs/palace` at `fba6a5b` (the changeset in the committed run log
`reference/fixtures/transmon_palace/results_p1/palace_run_v22.log:12`) · **geode:**
`origin/main` at `d7c95504`.

The operator ruling on #1003 asked for a spike toward a port formulation like Palace's
`LumpedPort`, one that removes the 3.4528 GHz transmon mode without matching against the
Palace oracle. This note answers the eight questions in the second curation pass. It
recommends a formulation and splits the build into phases.

> **The main finding contradicts the spike's premise.** The evidence below says that the
> 3.4528 GHz mode is very probably **not** a port artifact. It is very probably the
> transmon's own **qubit (plasma) mode**. It is missing from Palace's table only because
> Palace's eigensolver searches **above** `Target = 4.5 GHz` by design. The mode that *is*
> a port-formulation artifact is the **17.49 GHz "junction LC" mode**. That mode is in
> Palace's spectrum too, because Palace uses the same distributed surface term that geode
> does.
>
> The recommended formulation (A2, a rank-1 port-voltage term) removes the 17.49 GHz
> mode by construction. It leaves the 3.4528 GHz mode and the five cavity modes
> unchanged to the printed precision. A probe on the 133k fixture measured this (§3.3).
> So the target the ruling wrote down ("remove 3.4528 GHz without the oracle") cannot be
> met by any `LumpedPort`-like formulation. The target needs an operator decision
> (§9, question O1).

---

## 0. Verified, measured, and inferred

| Claim | Status | Evidence |
|---|---|---|
| Palace's `LumpedPort` with `L`/`C` enters the eigenproblem as a **distributed, isotropic** tangential surface mass on the port attribute. It is **not** a directional term and **not** an eliminated port-voltage form. | **Verified from source** | §1 |
| geode's `K_port`/`M_port` scalings equal Palace's per-square scalings. `transmon.rs:42-45` ("matches Palace's `LumpedPort`/`LumpedElement`") is correct. The `results.toml` hypothesis of "eliminated port unknowns" is wrong. | **Verified from source** | §1 |
| Palace's eigensolve only accepts eigenvalues **at or above** `Target`. A 3.45 GHz eigenpair cannot appear at `Target = 4.5`. | **Verified from source** (SLEPc region filter plus Palace's schema text). Palace was not re-run. | §2 |
| On the 133k fixture, the 3.4528 GHz mode carries **100.00 %** of its port energy in the uniform port voltage (`V²/L̃`). The 17.49 GHz mode carries **0.00 %**. | **Measured** (geode probe, local) | §3.1 |
| Palace's own port EPR for its 17.49 GHz mode is `2.49e-08`, which means zero port voltage. That is consistent with the 0.00 % above. | **Committed data** (`results_p1/port-EPR.csv`) | §3.1 |
| The 3.4528 GHz frequency is within 0.28 % of the lumped-circuit qubit prediction `1/(2π√(L(C_Σ+C_J))) = 3.4625 GHz`, using geode's committed electrostatic `C_Σ = 136.68 fF`. It also obeys the `1/√L` law (0.7081, committed). | **Computed from committed numbers** | §3.2 |
| That the 3.4528 GHz mode is *the* physical qubit mode. | **Inferred** (strongly). The decisive check is a Palace run with `Target` below 3.45 GHz (phase P1). | §3.2, §8 |
| A2 removes the 17.49 GHz mode and keeps 3.4528 GHz and the cavity modes. Under A2, `Gᵀ b = 0` exactly. With the plain bulk projector, A2 returns six modes at `N = 6` with no oracle filter. | **Measured** (geode probe, local, uncommitted code) | §3.3 |
| A1 (directional surface term) does not remove the 17.49 GHz mode. It moves it to 17.3777 GHz. | **Measured** (geode probe) | §3.3 |
| A3 (Lagrange multiplier) is algebraically A2 after Schur elimination. | **Derived, not run** | §3.4 |

The probe that produced every "measured" row is committed for reproducibility at
[`transmon-lumped-port-formulation.probe-1003.rs`](./transmon-lumped-port-formulation.probe-1003.rs).
It is **scratch evidence**: cargo does not build it, and it is not a shipped feature. Its
header explains how to run it. Full run: 114 s wall and 4.5 GB peak RSS on the M3 Ultra
dev host, direct LU, on the committed fixture
(`sha256 5b3ff4c3…33dd`). No cloud was used.

---

## 1. How Palace's `LumpedPort` with `L`/`C` enters the eigenproblem (question 1)

It is a **distributed surface impedance over the full tangential trace**: the same
structure as geode's `S_Γ`, with the same scalings.

- `palace/models/lumpedportoperator.cpp`, `LumpedPortOperator::AddStiffnessBdrCoefficients`
  (:552-572). For each port element it computes `Ls = data.L * data.GetToSquare(*elem)`
  and calls `fb.AddMaterialProperty(<port bdr attributes>, coeff / Ls)`, a **scalar**
  coefficient.
- Same file, `AddMassBdrCoefficients` (:596-616): `Cs = data.C / data.GetToSquare(*elem)`
  and `fb.AddMaterialProperty(..., coeff * Cs)`, again **scalar**. `AddDampingBdrCoefficients`
  (:574-594) does the same for `R`.
- `palace/models/lumpedportoperator.hpp:60-63`: `GetToSquare = w / l * n_elems`. So the
  stiffness coefficient is `1/Ls = l/(w·L)` and the mass coefficient is `Cs = C·l/w` (for
  one element). These are exactly geode's `k_scale = ℓ/(w·L̃)` and
  `m_scale = C̃·ℓ/w` (`crates/geode-core/src/eigen/transmon.rs:157`, `:166`).
- `palace/models/spaceoperator.cpp`. `SpaceOperator::AddStiffnessBdrCoefficients` (:966-972)
  and `AddRealMassBdrCoefficients` (:996-1003) route the port coefficients into `fb`.
  `GetStiffnessMatrix` (:376-395) and `GetMassMatrix` (:455-484) pass `fb` to
  `AddIntegrators` (:269-302). There a scalar `fb` becomes
  `a.AddBoundaryIntegrator<VectorFEMassIntegrator>(*fb)` (:302): the boundary mass of the
  Nédélec trace space, `∫_Γ c · (n×u)·(n×v) dS`. That is our `S_Γ`
  (`crates/geode-core/src/driven/ports/lumped.rs:149`, `assemble_port_surface_mass`).
  That a boundary `VectorFEMassIntegrator` on an ND space acts on the tangential trace is
  the MFEM convention; I did not trace it into libCEED.
- The committed run log shows the distributed form in use: `Configuring Robin impedance
  BC for lumped ports at attributes: 4: Ls = 1.486e-08 H/sq, Cs = 5.500e-15 F/sq`
  (`palace_run_v22.log:48-49`).
- The `Direction` and the rank-1 voltage functional enter **only post-processing and
  excitation**. `LumpedPortData::InitializeLinearForms` (:163-222) builds
  `V = (1/w)∫ E·l̂ dS` (:200-205, a `VectorFEBoundaryLFIntegrator` with the element's
  `GetModeCoefficient`). `GetVoltage` (:297) and the EPR in
  `palace/models/postoperator.cpp`, `MeasureLumpedPortsEig` (:1182-1235), use it. It never
  enters `K` or `M`.
- Lumped-port attributes are explicitly **excluded** from interior-boundary cracking
  (`palace/utils/geodata.cpp`, `AddInterfaceBdrElements`, :2814-2833). So the port patch
  is one sheet, as in geode.

**Verdict on the contradiction.** `transmon.rs:42-45` is right: geode's lumped reactive
shunt has the same structure and scalings as Palace's. The `results.toml` `[spurious_mode]`
sentence that Palace "represents the port DOFs differently (eliminated port unknowns…)"
(:218-221) is wrong, and this PR corrects it. The 17.4901 GHz mode agrees to 0.001 % in
both solvers, while the cavity modes agree to about 0.03 %. That is what an identical port
operator would produce.

Side finding: Palace's run log reports `Added 4913 elements in 2 iterations of local
bisection` and `6014 duplicate vertices` for the cracked PEC sheets (`:16-19`).
Palace's discrete mesh is therefore **not** bit-identical to geode's (183,891 ND unknowns
against 156,863 edges). "Identical mesh" in the committed docs means the identical *input*
mesh. The ≤0.032 % agreement still stands.

## 2. Why Palace has no 3.45 GHz mode (question 2)

**Palace never searched below 4.5 GHz.** The curation reasoned that "a 3.45 GHz eigenpair
would be nearer the target than 26.08, so it is absent from Palace's discrete spectrum".
That is false for Palace's SLEPc configuration:

- With no `R` (so no damping matrix `C`), `palace/drivers/eigensolver.cpp` (:286-300)
  takes the linear branch: `eigen->SetShiftInvert(target * target)` and, for SLEPc,
  `SetWhichEigenpairs(TARGET_REAL)`.
- `palace/linalg/slepc.cpp`: `region = true` by default (:331). `SetWhichEigenpairs(TARGET_REAL)`
  does **not** clear it (:591-593). Only the `*_MAGNITUDE` types and JD do. Then
  `SlepcEigenvalueSolver::Customize` (:432-447), for a real positive shift, calls
  `ConfigureRG(GetRG(), sr / gamma, +∞, −∞, +∞)`, an `RGINTERVAL` that admits only
  `Re μ ≥ σ²`. Eigenvalues below the target are **filtered out**.
- The ARPACK branch (:292-296) uses `LARGEST_REAL` of `1/(μ − σ²)`, which is positive only
  for `μ > σ²`. So it selects above the target too.
- The schema says so outright. `scripts/schema/config-schema.json:2214-2217` defines
  `Target` as the "(Nonzero) frequency target **above which** to search for eigenvalues, GHz."
- The run used SLEPc (`palace_run_v22.log:63`, `Configuring SLEPc eigenvalue solver`).

Of the curation's three candidate causes (formulation, element values, patch geometry),
the answer is **none of them**. The cause is the solve window. The paper draft's claim
that "Palace's projected eigensolve hunted at σ = 4.5 GHz and found nothing below 5.15"
(`papers/transmon-benchmark/BRIEF.md:438`,
`papers/transmon-benchmark/transmon-benchmark.3/main.tex:657-680`) misreads `Target`.
It needs correcting (phase P3).

**Not yet verified:** whether Palace *does* return about 3.45 GHz when asked
(`Target ≈ 2 GHz`). That is phase P1, a local docker run. Docker was not running on the
spike host, and the image was not built.

## 3. The mechanism in our pencil, and what each candidate does (question 3)

### 3.1 Two different port-coupled modes

On the 133k fixture the junction patch is a 1 μm × 1 μm square split into **4 triangles**.
It has 5 nodes, and **only one of them (the centre) is not on PEC**. It has 8 edges, of
which 6 are interior (probe output). `ê = +ŷ` and `ℓ = w = 1 μm`.

For any field the isotropic port energy splits, by Cauchy-Schwarz, as

```text
(ℓ/(w L̃)) ∫_Γ |E_t|² dS  =  V²/L̃  +  (non-uniform remainder ≥ 0),      V = (1/w) ∫_Γ E·ê dS.
```

The probe measured the uniform share `(V²/L̃) / ((ℓ/(wL̃)) xᵀS_Γx)` for each eigenvector of
the shipped pencil (σ = 4.5 and 17.5 GHz):

| mode (GHz) | stiffness participation `p` | uniform share `V²/L̃ ÷ S_Γ energy` | Palace port-EPR |
|---|---:|---:|---:|
| **3.4528** | 0.9942 | **1.0000** | not in Palace's window |
| 5.1528 | 0.0005 | 1.0000 | −4.71e-04 |
| 15.4650 | 0.0000 | 1.0000 | −1.71e-05 |
| **17.4901** | 1.0000 | **0.0000** | **+2.49e-08** |
| 18.6927 | 0.0000 | 1.0000 | −2.98e-09 |
| 20.7034 | 0.0000 | 1.0000 | −1.17e-08 |

Palace's EPR is `½ L |I|² / E` with `I = V/(iωL)` built from the same `V` functional
(§1), so it measures the uniform part only. The two solvers agree: the 17.49 GHz mode has
**zero port voltage**.

**Mechanism of the 17.49 GHz mode (measured plus derived).** Take the nodal hat function
`ψ_c` of the single free patch node. `∇ψ_c` is curl-free (`K ∇ψ_c = 0`), but the isotropic
`S_Γ` sees its in-plane gradient, so `K_port ∇ψ_c ≠ 0` (probe: `max|Gᵀ S_Γ| = 1.0`).
Its port voltage is `(1/w)∫_Γ ∂ψ_c/∂y dS = (1/w)∮ ψ_c (ê·ν) ds = 0`, because `ψ_c`
vanishes on the patch boundary. The Rayleigh quotient of a field confined to the patch is
`k_scale/m_scale = 1/(L̃C̃)`, which is `f_LC = 17.60 GHz`. The volume mass of the adjacent
tets lowers it to 17.49 GHz. So this mode is a **sheet-internal LC resonance**: it lives
on a patch-bump gradient that a real two-terminal junction cannot carry. This explains
several earlier measurements at once:

- the #509 measurement that "the junction mode" is 99.99 % gradient (div-ratio 50.17);
- the #514 finding that exactly **one** gradient direction `û` must be re-admitted (one
  free patch node, one bump);
- Palace's near-zero EPR.

The model predicts one such mode per free patch node. A finer port mesh would show a
cluster just below `f_LC`. That is a cheap falsifier, but it was not run.

**Mechanism of the 3.4528 GHz mode (inferred).** It carries all of its port energy as the
uniform voltage, and it is solenoidal with respect to the interior gradient
(div-ratio 6.18e-15, #514). Both facts fit a quasi-static field `E ≈ −∇φ_island` with
`φ = V` on the PEC island. `φ_island` is not in `image(d⁰_interior)` because island nodes
are PEC (Dirichlet). So this field is "divergence-free" for every projector built on
interior `G`. Its curl vanishes, so `p ≈ 1`, and its frequency is set by `L` against the
whole island capacitance. That is the transmon plasma mode.

### 3.2 The 3.4528 GHz mode matches the lumped-circuit qubit

Inputs, all committed:

- `L = 14.860 nH` and `C_J = 5.5 fF`;
- `C_Σ = 136.68 fF`, the island self-capacitance to ground on the same mesh with the
  tensor ε (`benchmarks/transmon_quantum/results.toml`, `[capacitance.tensor_eps]`).

```text
f_qubit,LC = 1 / (2π √(L (C_Σ + C_J))) = 1 / (2π √(14.86e-9 × 142.18e-15)) = 3.4625 GHz
geode eigenmode                                                         = 3.4528 GHz  (−0.28 %)
L-doubling ratio (committed, #514)                         0.7081  vs  1/√2 = 0.7071
```

A Rayleigh-Ritz eigenvalue lies at or below the quotient of any trial field, and the
electrostatic trial `∇φ_island` is in the Nédélec space. So a full-wave value slightly
**below** the quasi-static LC estimate is the expected direction. The repo's own
`omega01_koch_ghz = 3.383` (Phase C) is the anharmonic `0→1` transition of this same
oscillator.

The paper draft's §"The Absent Qubit Mode: An Expected Negative" therefore looks
inverted. The qubit mode is present in geode, and Palace was not asked to look for it.

### 3.3 Candidates on the 133k fixture (measured, probe)

| formulation | σ = 4.5 GHz, ungauged, `N = 8` | σ = 17.5 GHz, ungauged, `N = 8` | bulk projector `P`, σ = 4.5, `N = 6` (non-null) |
|---|---|---|---|
| **shipped `S_Γ`** | 6 near-kernel, **3.4528**, 5.1528 | 2 near-kernel, 3.4528, 5.1528, 15.4650, **17.4901**, 18.6927, 20.7034 | 3.4528, 5.1528, 15.4650, 18.6927, 20.7034, 26.0883. The 17.49 GHz mode is deflated (#509). Residuals up to **5.5e-3**, because `P` is not exact when `Gᵀ K_port ≠ 0`. |
| **A1** directional `∫(N_i·ê)(N_j·ê)` | 6 near-kernel, 3.4528, 5.1528 | 2 near-kernel, 3.4528, 5.1528, 15.4650, **17.3777**, 18.6927, 20.7034 | not run |
| **A2** rank-1 `(1/L̃) b bᵀ`, `C̃ b bᵀ` | 6 near-kernel, 3.4528, 5.1528 | 3 near-kernel, 3.4528, 5.1528, 15.4650, 18.6927, 20.7034. **No mode between 15.47 and 18.69 GHz.** | **3.4528, 5.1528, 15.4650, 18.6927, 20.7034, 26.0883**, residuals **≤ 1.7e-9** |

Here `b_i = (1/w)∫_Γ N_i·ê dS`, which is `assemble_port_flux / w`
(`crates/geode-core/src/driven/ports/lumped.rs:185`). Palace uses the same voltage
functional (§1).

The probe also checked its own construction. Its Whitney surface mass matches
`assemble_port_surface_mass` exactly (max diff 0). `S_Γ u = f` for the uniform-field
interpolant `u` (5.6e-17), and `uᵀS_Γu = area`. `max|Gᵀ b| = 0.0` exactly.

- **A1** removes the coupling of the patch patterns transverse to `ê`. The centre bump
  still has a `∂ψ/∂y` component, so its artifact survives, shifted by 0.64 %. A1 does not
  fix the problem.
- **A2** keeps only the energy `V²/L̃` and `C̃V²`. The uniform field reproduces the
  distributed energy exactly, and that is why 3.4528 and every cavity mode are unchanged
  to all printed digits. Every zero-voltage patch pattern loses its port stiffness. The
  centre bump becomes an exact null vector of `K` and drops into the gradient kernel.
- **A3** is covered in §3.4.

The cheapest discriminating measurement for each candidate is the uniform share and the
mode list at σ = 17.5 GHz, as in the tables above. A synthetic-pencil version of the same
check needs a patch with at least one free interior node (phase P2).

### 3.4 A3: a Lagrange multiplier or Schur form

Add the port voltage `V` (or current `I`) as an unknown with the constraint `V = bᵀx`. The
pencil gains one row and column per port. Eliminating `V` gives exactly the A2 pencil, so
the spectrum and the effect on both modes are the same as A2. The cost:

- The block system is saddle-point (indefinite even at `σ < 0`). That breaks the SPD
  assumptions of `InnerSolver::MatrixFree` CG, AMS (#526/#559) and the SPD proxy
  `K + |σ|M`.
- Every reduced-space consumer (sensitivities, participation, London walls) needs the
  extra unknown threaded through.

A3 only pays off if the junction later becomes a **circuit** with internal state, for
example multi-element or nonlinear junctions, or a port voltage needed as a primary
unknown. Neither applies to the linear `L ∥ C` v1. A3 is not recommended now.

## 4. Effect on the physical spectrum (question 4)

Under A2, 5.1528, 15.4650, 18.6927, 20.7034 and 26.0883 GHz are unchanged to 4 decimals
(measured). Their stored relative errors against Palace (≤ 0.032 %) do not move. The
17.4901 GHz mode is removed. This analysis says it should be removed, because it is an
artifact of the distributed port. But it is in Palace's committed spectrum, so an A2 geode
**will no longer match Palace's six modes**. It will match five of them, plus the
3.4528 GHz mode that Palace was not asked for.

- **A1** keeps all modes, but moves the 17.49 GHz artifact by 0.64 %. It would break the
  ≤1 % agreement with Palace on that mode only partially, and it fixes nothing.
- **A2** cannot shift the 3.4528 GHz mode or the cavity modes, because their port fields
  are uniform to 4 digits (share 1.0000). On a port patch with a strongly non-uniform
  field, A2 and `S_Γ` would differ. On this fixture they do not.

The curation's acceptance asked to retain "the 17.49 GHz junction mode … ≤ 1 %". That
conflicts with A2, and this note says the conflict is real (O1).

## 5. Coupling to the port-aware gauge in `projection.rs` (question 5)

- **Under A2, `Gᵀ K_port = (Gᵀb)(bᵀ)/L̃ = 0` and `Gᵀ M_port = 0` exactly** (measured
  `max|Gᵀ b| = 0`). The reason: `(Gᵀb)_n = (1/w)∫_Γ ∇λ_n·ê dS = (1/w)∮ λ_n (ê·ν) ds`. On a
  rectangular port, that integrand is zero on the two sides parallel to `ê` and on the PEC
  terminals (where `λ_n = 0` for free nodes `n`).
- As a result, `image(d⁰_interior)` is exactly the kernel of `K + K_port`, and the
  **plain bulk projector** `MOrthogonalGradientProjector`
  (`crates/geode-core/src/eigen/projection.rs`, used by `solve_transmon_eigenmodes_projected`,
  :1352) is exact. Measured: six non-null modes, residuals ≤ 1.7e-9, against up to 5.5e-3
  for the shipped `S_Γ` under the same `P`.
- No re-admission is needed. `PortAwareGradientProjector` (:473, the `û` extract) and
  `PortSubspaceGradientProjector` (:628) exist **only** to keep the 17.49 GHz bump mode,
  and A2 removes that mode.
- `PortSubspaceGradientProjector::port_nodes` (:647-672) reads nodes adjacent to the
  **support** of `K_port`. It does not evaluate the product `Gᵀ K_port`. Under A2 it would
  still pick the centre node, because the spokes are in `b`'s support. It would then
  re-admit `∇ψ_c`, which is an exact `λ = 0` vector under A2, and
  `smallest_nonnull_eigenpairs` would drop it. That is harmless but wasted work.
  Recommendation: route A2 through the bulk projected solve, and keep `P'`/`P_V` for the
  `S_Γ` shunt. The curation's expectation that the port nodes "shrink to the terminal
  nodes" holds for `Gᵀ K_port` itself, and the terminal nodes are PEC, so the set is
  empty. It does not hold for the current support heuristic.
- **AMS and matrix-free.** `b bᵀ` is PSD, so `K + |σ|M` stays SPD and AMS stays valid. A
  matrix-free apply is `K x + (bᵀx/L̃) b`, so no fill. The assembled form adds
  `nnz(b)² = 36` entries here, and at most O(10⁴) for a refined port.
- **A3** breaks SPD everywhere (§3.4).

## 6. Driven consistency (question 6)

The driven `LumpedPort` (`crates/geode-core/src/driven/ports/lumped.rs`, module docs :1-60)
already uses **both** structures:

- the distributed `S_Γ` Robin term for `R`, the same as Palace's
  `AddDampingBdrCoefficients`;
- the rank-1 flux `f = w·b` for excitation and voltage readout (`assemble_port_flux`, :185).

The driven path has the same zero-voltage patch patterns. Under `S_Γ` they are damped by
`R` instead of resonating. Making the driven Robin term rank-1 would **diverge from
Palace's driven `LumpedPort`**, and that would break the like-for-like basis of the driven
parity work (Epic #193/#202).

**This note does not decide the question.** It is listed as operator question O2. The
spike recommends A2 for the eigen path only, opt-in, with the driven path unchanged until
the operator decides.

## 7. Re-baselining (question 7)

The ruling places the new form **beside** `LumpedReactiveShunt`. The policy proposed here:
**`S_Γ` stays the default until P4**, and A2 is an explicit opt-in. The default switch is
an operator call (O3).

There are 35 references to `LumpedReactiveShunt` outside `transmon.rs` (by
`git grep -c`): `tests/transmon_eigenmode.rs` 18, `src/eigen/projection.rs` 4,
`tests/transmon_eigen_sensitivity.rs` 3, `tests/london_eigen_kinetic_inductance.rs` 3,
`tests/transmon_quantum.rs` 2, `examples/transmon_bench.rs` 2, the #956 diagnostic 2, and
`src/eigen/cavity.rs` 1. None of them changes while A2 is opt-in.

If and when A2 becomes the default, the following would move:

- `benchmarks/transmon_eigen/results.toml`:
  - `[modes.junction_lc]` (17.490) would be retired or reclassified;
  - the 3.4528 GHz mode would move from `[spurious_mode]` to a physical-mode entry, gated
    by P1;
  - `[tripwires.junction_l_doubling]` (17.49 → 12.37) targets the artifact and should
    retarget the 3.4528 GHz mode (committed 0.7081);
  - the five cavity modes do not move.
- `benchmarks/transmon_bench_cpu/results_like_for_like_local.toml`: the `[issue_950]`
  cells would need re-running and relabelling. `modes_class = "spurious_port"` on
  3.452751 is wrong under this analysis. The geode-versus-Palace mode lists cannot be
  compared until Palace is re-run below the qubit (P1).
- `tests/transmon_eigenmode.rs`: `PALACE_JUNCTION_MODE_GHZ` and its doc ("the physical
  Josephson-junction resonance"), `characterize_spurious_3p45_mode`, and the port-aware
  release tests that assert the 17.49 GHz mode is retained.
- `transmon_quantum` (electrostatic, no eigen pencil), the London tests and the
  sensitivity tests: no numeric change.
- Prose:
  - `papers/transmon-benchmark/` §"The Absent Qubit Mode" and every "spurious 3.45 GHz"
    passage;
  - `docs/research/geode-vs-palace-comparison.md` (an outreach document, :45 and the
    table);
  - `docs/research/transmon-paper-reframe.md:91`.

## 8. Phases (question 8)

Filed under Epic #476 with `loom:triage` only:

| phase | issue | scope | depends on |
|---|---|---|---|
| **P1** | #1043 | Palace (in-repo docker, local, no cloud) on the committed fixture with `Target` below the qubit (for example 2.0 GHz, `N = 8`), plus an L-doubling run. **Decisive test** of the qubit-mode hypothesis. | none |
| **P2** | #1044 | `LumpedPortVoltageShunt` (A2) beside `LumpedReactiveShunt`, opt-in; bulk-projected route; synthetic test with a free interior patch node; the 133k release test reproducing §3.3 with no oracle filter. | none (can run in parallel with P1) |
| **P3** | #1045 | Re-baseline and correct prose: `results.toml` classification, like-for-like labels, test docs, the paper's absent-qubit section, the comparison and reframe docs. | P1 (and P2 for the A2 numbers) |
| **P4** | #1046 | Default switch for the eigen path, and the driven-consistency decision (O2, O3). **Operator call.** | P1, P2, P3 |

## 9. Recommendation, falsifiers, and open operator questions

**Recommendation: A2, the rank-1 port-voltage term, for the eigen path, opt-in.**

- It is the exact lumped two-terminal element (`V²/L`, `CV²`).
- It is real symmetric PSD and keeps every SPD and AMS path.
- It makes the plain bulk projector exact.
- On the fixture it removes the one mode that this analysis identifies as a
  port-formulation artifact, without touching any other mode.

**What would falsify it:**

1. P1 Palace at `Target ≈ 2 GHz` returns **no** mode within 1 % of 3.4528 GHz. That would
   mean Palace's identical `S_Γ` operator does not support it, which contradicts §1, so
   the cause would have to be geometry or mesh handling (for example cracking). The
   qubit-mode reading would then be dead and the question reopens.
2. A refined port patch (≥ 2 free nodes) under `S_Γ` does **not** show a cluster just
   below `f_LC`. That would weaken the sheet-internal explanation of 17.49 GHz.
3. Under A2 on a refined patch, any cavity mode moves by > 0.1 %. That would make A2 an
   approximation, not an identity.

**Open operator questions (not decided here):**

- **O1. The target.** The ruling asked to remove the 3.4528 GHz mode without the oracle.
  The evidence says that mode is the physical qubit mode and must be **kept**, and that
  the mode a `LumpedPort`-like formulation removes is 17.49 GHz. Should the acceptance
  become "under A2, at σ = 4.5 GHz, `N = 6`, return 3.4528 plus the five shared Palace
  cavity modes, with no oracle filter, the 17.49 GHz bump mode absent by construction,
  and 3.4528 GHz confirmed by P1"? If P1 falsifies the hypothesis, the original target
  comes back.
- **O2. Driven consistency.** Keep the driven `LumpedPort` on `S_Γ` (Palace parity), or
  move it to rank-1 as well (§6)?
- **O3. Default switch.** Make A2 the eigen default after P1 to P3, or keep it opt-in
  permanently beside the Palace-parity `S_Γ`? Either way, the published "like for like"
  claims must say which port model each row uses.
- **O4. Published claims.** The transmon paper draft (`transmon-benchmark.3`) and the
  Palace-outreach comparison document state that no qubit mode exists and that Palace
  confirmed its absence. Should those be held or corrected before P1 reports?
