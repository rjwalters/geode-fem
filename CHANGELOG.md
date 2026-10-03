# Changelog

All notable changes to GEODE-FEM are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

#### `geode` CLI

- Wave ports compose with Leontovich walls, including rough walls: a lossy guide's S-parameters carry the conductor attenuation, validated against Pozar Eq. 3.96 (TE10 α_c) and the roughness factor K(f). A Silver-Müller wall that shares an edge with a wave-port rim is rejected with `invalid_spec` (#776).
- **Hybrid wave ports: microstrip, stripline and inhomogeneous port faces** (#807, Epic #778 Phase 5).
  - **Routing.** `check` and `driven` route each wave port by its cross-section (`wave_ports[].route`). A face touching more than one material, or carrying a floating conductor (a `pec` strip sheet crossing it, or a carved-out strip), goes to the hybrid path: the full-vector E_t–E_z port modes, re-solved and tracked at every frequency. Before, an inhomogeneous face was `invalid_spec` naming #778, and a coax/strip face silently lost its TEM mode. Homogeneous faces without a conductor keep the geometric TE path bit for bit. The TE-only TM-cutoff guard (#808) applies to geometric ports only; hybrid ports use core's completeness check, and their rim must still lie on a conductor wall. Lossy faces use the complex-symmetric pencil; dispersive specs use the dispersive spec sweeps, so the faces and the volume read ε(ω) from the same per-tet vector.
  - **Defaults** (operator decision on #804): complex evanescent pairs are carried; `wave_ports[].hybrid.n_termination_evanescent` defaults to 4 (clamped on a small face, with a warning); the accuracy estimate is on, with `hybrid.accuracy_threshold` (default 0.5 %) applied to β and, on a lossy face, to α. Every warning goes to the report's new `warnings[]` and to stderr.
  - **Line impedance.** `wave_ports[].impedance_definition` defaults to `power_current` (Z_PI); `power_voltage` and `voltage_current` are opt-ins documented as path- and frequency-dependent. Z_TE is never offered for hybrid ports. `--touchstone` renormalizes hybrid channels to `reference_ohm` with the per-row `z_line_ohm`, which is complex on a lossy face. A hybrid port without a floating conductor is rejected for `--touchstone`.
  - **Line-impedance accuracy estimate** (#807 review: Z₀ is never silently wrong). The β estimate is the wrong proxy for Z: ε_eff = C/C₀ is a ratio, so the strip-edge singular error cancels in it, while Z = 1/(c√(C·C₀)) keeps all of it. Every channel with a line impedance now reports `hybrid.z_line_accuracy`: `z_line_ohm` re-evaluated on the h/2 face the β estimate already solves, Richardson-extrapolated with the rate observed over h, h/2, h/4 (about 1–1.5 at a strip edge, not the O(h²) of β; the conservative singular rate 1 when it cannot be observed). Above `wave_ports[].hybrid.impedance_accuracy_threshold` (default 1 %: a 1 % Z error renormalizes a matched line to |S11| ≈ 0.005) it raises `impedance_accuracy_above_threshold`, in `warnings[]` and on stderr, on `check` and on `driven` (including `--touchstone`) runs. The warning gives the refine factor and the strip-edge cell size from the observed rate, and says to grade toward the conductor edges. A failed refined solve, a missing match or a degenerate cluster gives `impedance_accuracy_unavailable`. The estimate covers discretization only: a shielded line's Z₀ also depends on the shield box (physics; about −3.3 % for an 8h × 5h box, −1 % for 16h × 10h, −0.3 % for 40h × 40h), which it does not flag. Measured on the 8h × 5h microstrip: estimates within 1.05× of the true Z error at three face levels (10.8 % vs 10.4 %, 6.4 % vs 6.3 %, 3.6 % vs 3.6 %), where the β estimate is 0.2–0.5 %.
  - **Report consumers: branch on `wave_ports[].route` before reading `medium`** — a hybrid port's `medium` (and `modes`, the TM-guard fields) is `null`.
  - **Report** (additive, schema v1). Every wave channel gets `eps_eff` and `normalization` (`power`, `pseudo_power` or `evanescent`; evanescent means not power-normalized). Hybrid channels add `hybrid`: E_z energy fraction, β and α accuracy estimates, residual and floor, tracking overlap, cluster size, `coupled_mode` even/odd, line impedances, and `z_line_ohm`. Rows of a lossy spec get `sigma_max` (measured passivity, with a `passivity` warning above 1 + 1e-6). Wave ports get `route` and `hybrid` (reason, conductors, free face unknowns, h, options, observed rates, worst tracking overlap); a hybrid port's `medium` is `null`. `geode check` previews every hybrid port's face solve at every frequency (`wave_ports[].hybrid.frequencies[]`) without the 3-D solve.
  - **Still `invalid_spec`.** μ_r ≠ 1 or anisotropic ε on a hybrid face; `sweep.adaptive` with a hybrid port (non-affine in ω, #774); `absorbing_regions` with a hybrid port; a dispersive geometric fill alongside hybrid ports; `power_voltage` / `voltage_current` on a conductor with no voltage path; internal port planes that differ across the plane; and the hybrid-only fields on geometric ports.
  - **Cookbook.** `examples/driven/microstrip_line.json` with its Gmsh geometry `microstrip_line.geo`: a 50 Ω shielded microstrip with hybrid ports, written to Touchstone. A 20h × 12h shield graded toward the strip edges gives Z_PI = 49.32 Ω (1.4 % below 50 Ω: 0.6 % shield offset, 0.9 % mesh error, estimated 0.88 %) with no impedance warning.
  - **Measured** (`tests/hybrid_wave_port.rs`):
    - slab-loaded guide β within 0.031 % of the LSE/LSM oracle (the accuracy estimate tracks it: 3.13e-4 vs 3.13e-4);
    - a Hammerstad–Jensen 50 Ω microstrip gives Z_PI = 48.5 Ω and Touchstone |S11| ≤ 0.014, while the Z_TE tripwire would give |Γ| ≈ 0.61;
    - microstrip eps_eff is −0.09 % from Hammerstad–Jensen (20h shield);
    - coupled microstrip: even first, eps_eff 3.27 vs odd 2.88, even→odd conversion ≤ 3e-5;
    - a tan δ = 0.02 line has σ_max = 0.997 / 0.995 and |S21| = e^{−αL} to 1e-5.

#### `geode-core`

- `solve_wave_port_sweep_with_mode` takes a `surfaces` argument (impedance walls folded into the base operator); an empty slice is bit-identical to before. New `analytic::waveguide::te10_conductor_attenuation` oracle (#776).
- New `analytic::port_modes`: a p=1 Whitney + P1 mixed E_t–E_z port-mode solver (`solve_hybrid_port_modes`) for PEC-shielded cross-sections with real per-triangle ε. It returns every propagating mode plus the first K evanescent modes, including fast modes. The exact β² = 0 null space is deflated out of the Krylov space, every pair carries an explicit residual, and modes are normalized by the B-form with `xᵀBx = β²` for propagating modes. A short or uncertified window returns an explicit `Shortfall` error. A complex-conjugate pair inside the window returns a `ComplexPair` error. New `analytic::loaded_guide` closed-form LSE/LSM oracle for the slab-loaded rectangular guide. Measured on the slab guide (ε_r 2.25 and 4): β error ≤ 0.43 % at h = b/16, rate ≈ 2 in β², and the mode set matches the oracle one-to-one with no ceiling pileup. Honest limits: spurious complex LSE/LSM evanescent pairs, with Im β² = O(h); and P1-carried TM modes (uniform TM₂₁ β error 1.8 % at h = b/16). The fiber `mixed_pencil` solver is unchanged; its Arnoldi gains an optional projector (#803, Epic #778 Phase 1).
- **Hybrid wave ports** (#804, Epic #778 Phase 2): a `HybridWavePort` drives the 3-D solve with modes of an inhomogeneous PEC cross-section, re-solved per frequency with the p=1 mixed solver and tracked by continuity (B-pairing overlap, one-to-one; lost identity is an explicit error, never a guess). The modal flux is `f̂ = S_p(E_t − (j/β)∇_tE_z)` normalized by the unconjugated B-form, so the existing rank-N SMW, drive and `√β` power weights carry over unchanged. Additive API: `WavePortSpec` (geometric or hybrid), `solve_wave_port_spec_sweep_with_mode` and `solve_mixed_port_spec_sweep_with_mode` return the S points plus per-port reports (β, E_z energy fraction, tracking overlap, accuracy estimate) and report-level warnings. Every propagating face mode must be a reported channel, else `InvalidPort`; hybrid ports carry TM content, so the TE-only TM-cutoff guard of #808 does not apply to them. Mesh-induced complex evanescent pairs are terminated as a reciprocal 2×2 block with a warning that names the port and modes and gives a predicted split resolution. A per-mode accuracy estimate (an h/2 face re-solve with nested prolongation, `analytic::port_mode_accuracy`) warns above 0.5 % with a refine-to-h hint. `DrivenRom::build_with_wave_port_specs` rejects hybrid ports, because their port operator is not affine in ω. `HybridPortOpts::carry_complex_pairs` (default off, so P1 behaviour is unchanged) returns complex pairs as `HybridComplexPair`. Measured: the homogeneous limit matches the `PortMedium` path to `|ΔS| = 4e-15`; on the slab-loaded section, β is within 0.22 % of the oracle, `|S11|` ≤ 1.0e-2, `|S21 − e^{−jβL}|` ≤ 1.2e-2, and reciprocity and energy hold to round-off; the accuracy estimate is within a factor of 1.11 of the true error over 46 modes.
- **Interior PEC strips on port faces and the quasi-TEM mode, 2-D part** (#805, Epic #778 Phase 3).
  - `analytic::port_modes::HybridPecMasks` treats the rim, carved-out thick strips (holes) and zero-thickness PEC sheets with one rule: an edge is PEC if it is on the boundary or in an extra mask; a node is PEC if it touches a PEC edge.
  - The p=1 hybrid solver returns the quasi-TEM mode, which lies far from the deflated β² = 0 null space.
  - New `mode_line_quantities` gives the line impedances Z_PI, Z_PV and Z_VI from the discrete Ampère current and a path voltage. These are distinct from the #775 wave impedance.
  - New `analytic::microstrip` adds:
    - the Hammerstad–Jensen ε_eff/Z₀, Kirschning–Jansen dispersion and Pozar α_d closed forms;
    - a graded, optionally mirror-symmetric shielded-strip face builder (sheet or thick strips);
    - a C4v square-coax face;
    - a 2-D P1 electrostatic capacitance reference on the same mesh.
  - Measured:
    - the homogeneous TEM has β² = k₀²ε to 2e-13;
    - the low-frequency ε_eff tends to C(ε)/C(1) with an O(k₀²) gap (ratio 4.00 per halving of k₀);
    - against Hammerstad–Jensen, ε_eff is within 0.53 % and Z_PI within 1.33 % over w/h ∈ {0.5, 1, 2}, ε_r ∈ {4.4, 9.8}, with box doubling under 0.41 %;
    - against Kirschning–Jansen, within 0.47 % up to f·h = 5 GHz·mm.
  - Solver changes, which leave the P1/P2 tests unchanged:
    - a default multiplicity verification pass (`HybridPortOpts::verify_multiplicity`). It re-runs Arnoldi from an independent start with the found modes deflated, and recovers missed copies of exactly repeated eigenvalues. A single start returned only one of the two degenerate TEM modes of a homogeneous two-strip face.
    - B-orthogonalization within degenerate clusters. Copies found on a C4v mesh paired at 4e-2 before it.
    - a residual round-off floor (`HybridPortMode::residual_floor`, reported per mode and counted in `floor_accepted`). Low-frequency faces (k₀W ≤ 0.1) previously failed with `Shortfall` even though the eigenpair was accurate.
  - The 3-D part (driving with the quasi-TEM mode) is a follow-up phase.
- **Lossy and dispersive substrates on hybrid port faces** (#806, Epic #778 Phase 4).
  - New `analytic::lossy_port_modes`: the complex-symmetric p=1 mixed pencil for complex per-triangle ε (tan δ, Djordjevic–Sarkar, Debye, Drude), solved by a complex shift-invert Arnoldi. It keeps the null-space deflation, coverage certificate, explicit `Shortfall`, multiplicity pass and residual floor of the real solver. β is complex on the outgoing branch (Im β ≤ 0). "Propagating" means Re β² > 0, i.e. Re β > |Im β|. Modes are normalized unconjugated, zᵀBz = β², so Sᵀ = S.
  - `HybridPortFace::from_volume_lossy` / `new_lossy` build lossy faces. `solve_{wave,mixed}_port_spec_sweep_dispersive_with_mode` re-assemble the volume and re-solve the port faces from the same per-tet ε(ω) at every frequency, with mode tracking across frequency.
  - A face built from the volume must match the volume's ε bit for bit; a lossless face on a lossy volume is now an `InvalidPort` error. Gain (Im ε > 0) is refused.
  - `HybridChannelReport` gains `beta_sq_im` and an α accuracy estimate. A failed refined solve on this path reports the estimate as unavailable instead of aborting the sweep.
  - `FaerComplexEigensolver::smallest_complex_pencil_pairs` adds a general complex (A, B) eigenpair entry point, used as the dense oracle. `SlabLoadedGuide::continued_root` gives complex LSE/LSM roots by continuation in ε.
  - Measured:
    - Re β is within 0.44 % and α within 0.41 % of the continued root at h = b/32 (28 modes, O(h²) rates);
    - first-order α matches the exact lossy solve to 3.4e-6;
    - the microstrip α_d is −0.86 % from Pozar;
    - the dense oracle agrees to 3.3e-13;
    - on the 3-D lossy straight section, |S21| matches e^{−αL} to 8.5e-4 and reciprocity holds to 1e-14; σ_max(S) ≤ 0.981 is measured, not guaranteed (pseudo-power normalization);
    - a Djordjevic–Sarkar slab over 8 frequencies tracks continuously;
    - the tan δ = 0 regression reproduces Phase 1/2 to 7.5e-15 (S) and 6.7e-13 (β²);
    - the accuracy estimate is within 1.08× (β) and 1.30× (α) of the true error.
- **Hybrid-port follow-ups for the CLI** (#807, Epic #778 Phase 5):
  - `solve_hybrid_port_face_sweep` runs the port-face half of a hybrid spec sweep without the 3-D solve: modes, tracking, completeness, termination window, accuracy and warnings. Its report and warnings equal the 3-D sweep's bit for bit, on both the real and lossy paths.
  - A failed refined (h/2) accuracy solve on the real path now degrades to `AccuracyUnavailable`; it no longer aborts the sweep (#815 review note 1). The lossy path already did this.
  - New opt-in `PortAccuracyOpts::alpha_threshold` raises the new `PortWarningKind::AttenuationAccuracyAboveThreshold` for a lossy channel whose α estimate exceeds it (#819 review). It is off by default, so the Phase 4 goldens are unchanged; the CLI sets it to the β threshold.
  - Lossy faces report complex line impedances, `HybridChannelReport::line_lossy` (`HybridComplexLineReport`, unconjugated `Z_PI = 2P/Σ I_c²`). These equal the real Z_PI in the lossless limit (3e-15), and on an exact lossy TEM line they scale as √(ε′/ε) (3e-13).
  - With `sigma_tet`, a hybrid face built from the volume now sees the ε − jσ/ω of the conducting tets it bounds; before, it ignored σ. The port β equals the dispersive sweep with that ε(ω) bit for bit, and S matches to 4e-15.
  - The internal `HybridState` report assembly moved into `into_report`; the arithmetic is unchanged.
  - **Line-impedance accuracy estimate** (#807 review): `HybridChannelReport::impedance_accuracy` (`ImpedanceAccuracy`: `Z_PI`, and `Z_PV` / `Z_VI` with a voltage path) re-evaluates the line quantities on the h/2 face (refined conductor = coarse conductor nodes plus the midpoints of its PEC edges; the voltage path through the edge midpoints) and extrapolates with the per-channel rate observed over h, h/2, h/4, clamped to [0.5, 2] (`SINGULAR_IMPEDANCE_RATE` = 1 when unobserved). New `PortAccuracyOpts::impedance_threshold` (default `DEFAULT_IMPEDANCE_ACCURACY_THRESHOLD` = 1 %) and `::impedance` (`LineImpedance`, the definition checked) raise `PortWarningKind::ImpedanceAccuracyAboveThreshold` (refine factor, h and strip-edge cell targets); `ImpedanceAccuracyUnavailable` says why an estimate is missing. Real and lossy paths. A lossy face's complex line currents now apply the full complex T_ε of the face ε, so a dispersive face's Re ε(ω) is honoured (before, the displacement term used the build-time Re ε; identical for a non-dispersive face).
- **3-D microstrip / stripline sections with hybrid wave ports** (#817, Epic #778 Phase 3b).
  - `HybridPortFace::from_volume_with_pec` / `with_interior_pec`: a face edge is PEC if it is on the rim or its 3-D edge is eliminated by the volume PEC mask, so zero-thickness strips (PEC sheets inside the volume) reach the port solve. Floating conductors are found automatically (`HybridPortFace::conductors`, ordered by 3-D centroid), each with a shortest shield-to-conductor voltage path. Without a mask the face is rim-only, as before.
  - New `mesh::extrude_tri_mesh` (and `_layers`): a conforming sorted-vertex prism split of any `TriMesh` into right-handed tets, with per-tet source triangle and slab and an `ExtrudedEdge` classification for z-dependent PEC masks. New `driven::ports::strip_line_section` builds a straight shielded strip-line section and its two hybrid ports.
  - Every propagating channel on a strip face reports `HybridLineReport` (Z_PI, Z_PV, Z_VI, signed conductor currents and voltages) per port and per frequency, for #807. With several conductors these are the per-line modal impedances (`Z_PI = 2P/Σ|I_c|²`), which are Z_e / Z_o for a symmetric pair.
  - Exactly degenerate clusters (e.g. the TEM pair of a homogeneous two-strip line) are tracked as subspaces by principal angles and rotated by orthogonal Procrustes, and get a canonical basis at the first frequency (orthogonal conductor currents: odd / even). A split cluster or a non-canonical basis is a warning; a reported-channel count that splits a cluster is `InvalidPort`.
  - The sweep consumes `multiplicity_certified`: an uncertified solve is retried with twice the Krylov cap, then continues with a `MultiplicityUncertified` warning naming the port and ω (new `HybridWavePortOpts::verify_multiplicity`, default on). Each channel reports `residual`, `residual_floor` and `floor_accepted` at every frequency; each point reports the certificate, retries, repeated copies, degenerate clusters and the floor-accepted count.
  - The Phase 4 lossy path gets the same treatment: lossy faces take the volume PEC mask (`from_volume_lossy(..)?.with_interior_pec(..)`); exactly degenerate lossy clusters are tracked as subspaces with complex-orthogonal Procrustes rotations (`RᵀR = I`, keeping `zᵀBz = β²`; Denman–Beavers inverse square root) and get the canonical odd/even basis from the complex-symmetric current matrix. The certificate, floor and cluster reports and warnings apply as well. Lossy faces do not report line impedances yet (`line: None`): a lossy line's Z is complex, and choosing its definition is #807's job.
  - The E_t-only `PortFaceProjection::solve_modes` count subtracts one harmonic field per hole (`n_holes`); hybrid ports reject more reported + termination channels than the face's free transverse DOF count (`HybridPortFace::max_modes`). New `PortFaceError::InvalidPecMask`.
  - Measured (`tests/hybrid_strip_line.rs`): on a straight shielded microstrip (ε_r 4.4, w/h 1), `|S11|` ≤ 5.1e-3, `|S21 − e^{−jβL}|` ≤ 8.3e-3 and reciprocity holds to 2e-14, while the port β and Z_PI equal the 2-D P3a solve to 2e-13. A homogeneous stripline gives exact TEM (β to 3e-13, Z_PI = Z_qs to 2e-13). A carved thick strip and a masked one agree to 1e-13 in 3-D S. Across a width step w/h 1 → 2, `|S11|` follows the Z_PI ratio within 0.30 %, where Z_TE would predict an 18× smaller reflection. Coupled microstrip even/odd ε_eff and Z match the 2-D capacitance matrix within 3.4e-4 at k₀h = 0.01. The degenerate stripline pair is canonical to 1e-12 and tracked with overlaps of 1 − 2e-12; a C4v coax TE11-like pair (one conductor) keeps the solver basis with a warning and is tracked to 1 − 2e-15. On the P3a 20h shield, Z_PI is −1.43 % from Hammerstad–Jensen. On a lossy microstrip (tan δ 0.02), the port β equals the 2-D lossy solve to 1.4e-13; a lossy degenerate stripline pair keeps β² = k₀²ε to 3e-13, with tracking overlaps of 1 − 2e-12.
- `eigen::lanczos::SparseShiftInvertLanczos::smallest_eigenpairs_checked` (with `ConvergenceCheck` and `CheckedEigenpairs`) returns only Ritz pairs whose true residual `‖K x − λ M x‖ / (max(|λ|, |σ|)‖M x‖)` meets a tolerance. When the first pass leaves a pair unconverged, it extends the same Krylov run (no restart, bounded by a step cap) until a converged pair confirms it. It reports what it withheld (`rejected`, `shortfall()`). When the first pass already converges, the result is bit-identical to `smallest_eigenpairs`. The plain `smallest_eigenpairs` is unchanged, so the cavity, transmon, projection and CLI `eigen` solves keep their results and cost exactly (#798).

### Fixed

#### `geode` CLI

- A wave-port sweep that reaches a port's TM cutoff is now rejected with `invalid_spec` by `check` and `driven` (pure wave, mixed and adaptive specs). Before, it gave silently wrong S-parameters. The port modal solve carries TE modes only, so above the lowest TM cutoff (TM₁₁ in a rectangular guide) a propagating TM channel had no termination. A height step driven above TM₁₁ changed `|S|` by 0.44 when its feed section was lengthened, against 3.6e-3 below TM₁₁, and its power balance still summed to 1. The limit is 5 % below an estimate of the TM cutoff: the port face's P1 Dirichlet `E_z` eigenvalue, Richardson-extrapolated over two uniform face refinements with the measured order. The face value alone is a Rayleigh-Ritz upper bound and sits above the 3-D Nédélec model's own TM cutoff, which falls below the continuum on a coarse axial mesh (8 × 4 face of a 2 × 1 guide: face 3.661, analytic 3.512, 3-D 3.349 with one tet layer of 0.5; the limit is 3.337). Filled ports scale the limit by `1/√(Re ε_n·μ_t)`, using the axial `ε_n`. Fills with `Re ε_n·μ_t ≤ 0` have no TM cutoff and are always rejected. The message names the port, the limit and the first offending frequency in Hz, and points to #778 / #804. `check` / `driven` reports gain `wave_ports[].tm_k_c`, `tm_k_c_face` and `tm_limit_hz` (additive in schema v1). Two existing CLI tests swept an ε_r ≈ 1.5 guide to k₀ = 3.0, past its filled TM₁₁ (2.87); they now stop at k₀ = 2.7, below the filled limit 2.72. Their straight, uniformly filled guides do not couple TE₁₀ to TM₁₁, so the old rows were not numerically damaged (#808).
- A wave port whose rim is not entirely on `pec` or `leontovich` walls is now rejected with `invalid_spec` by `check` and `driven`, naming the port, the number of open rim edges and the surface groups they lie on. The port modes are solved with a PEC rim, so on any other rim (for example a PMC symmetry plane) they were silently the modes of the wrong cross-section (#808).
- `geode eigen` on a lossy or open pencil now reports high-Q modes with a finite, accurate `q` and `k0_im`. Before, a mode with `Q ≳ 6.7e7` came out as `k0_im = 0`, `q: null` ("lossless"), and from `Q ≈ 1e7` the Ritz values themselves degraded: on a uniformly filled cavity with `tan δ = 1e-10` (`Q = 1e10`) the solver's own `Q(λ)` was 88 % low with residuals `≈ 1e-7`. Both now agree with the exact `½·cot(δ/2)` to `≤ 8.2e-14` relative at `Q = 2e8` and `1e10`, with residuals `≤ 1.2e-13` (#830).

#### `geode-core`

- New `PortFaceProjection::lowest_tm_cutoff` (the port face's P1 TM cutoff, with optional open rim edges; a Rayleigh-Ritz upper bound), `PortFaceProjection::tm_cutoff_estimate` / `TmCutoffEstimate` (the face value extrapolated over two uniform refinements, and `guard_k_c`, `TM_GUARD_MARGIN` = 5 % below it) and `PortMedium::tm_cutoff_k0` (the filled TM cutoff from the axial `ε_n`). Wave ports remain TE-only (#808).
- `analytic::waveguide::solve_dielectric_modes2` now returns the guided modes of weakly-guiding fibers (for example SMF-28). Every in-window eigenpair of its pencil obeys the exact Rayleigh identity `r = 1 − n_eff²/⟨ε⟩_x`, so its curl ratio is below `(ε_max − ε_min)/ε_max` (`7.8e-3` for SMF-28). The old fixed curl-energy floor (`3e-2`) sat above that bound, so it rejected every guided mode. What it returned as the "fundamental" was an unconverged Lanczos Ritz vector, or nothing, depending on platform rounding and mesh. The floor now scales with the contrast and is capped at the old value, so the Si/SiO₂ floor is unchanged. The solver also rejects in-window pairs that break the Rayleigh identity (unconverged Ritz vectors). The #449 grad–div audit is re-measured on the genuine fundamentals, and its verdict is corrected in `docs/formulation_audit_reduced_vs_full_vector.md` (#791).
- `analytic::waveguide::solve_dielectric_modes2` and `solve_dielectric_modes` (p=1) no longer pass unconverged Lanczos Ritz pairs to their mode filters. The Lanczos solve returned its requested window after `n_request + 8` steps without a convergence check, and the tail of that window reached the filters. The #791 Rayleigh-identity check caught only grossly unconverged pairs: its violation is second order in the residual, so a pair with residual `1e-2` could pass it. Both solvers now use the residual-checked solve, with tolerance `1e-8` and a cap of 6× the old step budget. Unconverged in-window pairs extend the run until they converge; the rest are withheld, and the solver log counts them. The Rayleigh check stays as a second line and now also applies at p=1. Converged results are unchanged (SMF-28 timing mesh 1.448239, Si slab 2.910844, the SOI, mixed-pencil and audit benchmarks), and the SMF-28 timing solve takes 0.35 s instead of 0.25 s (#798).
- `analytic::waveguide::solve_waveguide_modes`, `solve_waveguide_modes_with_opts` and `solve_rect_waveguide_modes` (the metallic port-mode solver behind wave ports), and `solve_rect_waveguide_modes2_cutoffs`, use the residual-checked Lanczos solve as well. They return only converged modes. An undercount, or an unconverged pair below the last returned mode, triggers the existing budget-doubling retry, and the error names the shortfall. On the `b/16` guide, `n_modes = 12` returned TE₃₁ off by `1.06e-8` and merged the near-degenerate pairs at `k_c² ≈ 39.31` / `49.34` (found in PR #809). It now matches the `n_modes = 20` solve to `1e-12`. The same tail also dropped degenerate partners: the circular guide's second TE₁₁ polarization, and the 2:1 rectangle's TE₂₀/TE₀₁ pair in the p=2 cutoffs. Both are now returned. Wave-port S-parameters change only at round-off (≤ 1e-14), except where a returned port mode was itself unconverged. The CLI vacuum golden `PURE_WAVE_BITS` is re-recorded: its 8×4 port face's second mode (the near-degenerate TE₂₀/TE₀₁ pair) came from one unchecked pass at residual `3.6e-4`. That moved S by up to `6e-9` and made the golden platform-dependent (main failed it on macOS/aarch64). macOS and Linux now agree to `1.6e-15` (#798).
- `analytic::waveguide::solve_dielectric_modes` (p=1) now uses the contrast-scaled curl-energy floor from #791 (`clamp(1e-2 · (ε_max − ε_min)/ε_min, 1e-6, 3e-2)`). The fixed `3e-2` rejected every guided mode at weak index contrast (Δn/n ≲ 1.5 %); for example, the p=1 SMF-28 fundamental (`r = 9.2e-4`) is now returned. Si/SiO₂ cross-sections keep exactly `3e-2` (#794).
- `FaerComplexEigensolver` (and so `self_consistent_k` / `self_consistent_k_vector_tracked`) no longer hangs on complex pencils above about 500 DOF. It called faer 0.24's generalized complex QZ, which turns the matrices into NaN when many shifts come from a degenerate eigenvalue cluster (the Nédélec gradient null space) and then runs its `30·n` sweep cap without reporting an error; a 3300-DOF Mie pencil ran for 14.8 h. The solver now computes the full spectrum by dense shift-invert (LU of `A − σB`, then faer's standard complex Schur QR of `(A − σB)⁻¹B`). The eigenvalues match the old solver's to about `1e-12` relative, and the 3300-DOF pencil takes about 11 s. Pencils larger than the new public `eigen::complex::MAX_DENSE_COMPLEX_DIM` (6000) are refused with the new `EigenError::DenseTooLarge { dim, max }`. Non-finite input or output returns `EigenError::FaerGevd` instead of NaN eigenvalues. `EigenError` is not `#[non_exhaustive]`, so a downstream exhaustive `match` on it needs a new arm (#796).
- `FaerDenseEigensolver` (`smallest_eigenvalues` / `smallest_eigenpairs`) is no longer many times slower from about 590 DOF, and no longer returns inaccurate or spuriously complex eigenvalues on real pencils of a few hundred DOF and up. It called faer 0.24's generalized real QZ, which from 590 DOF spins its aggressive-early-deflation window QZ to an iteration cap (5.7 s at 600 DOF against 0.36 s at 560), and whose blocked path lost accuracy on symmetric-definite FEM pencils: on sub-blocks of the bundled Mie pencil `(Re K, Re M)` its physical eigenvalues were off the symmetric reference by `9e-5` at 250 DOF, `8e-4` at 600 and up to `0.57` at 1200, with spurious complex-conjugate pairs. The solver now uses dense shift-invert (LU of `K + τM`, then faer's standard real Schur QR of `(K + τM)⁻¹M`, with complex-shift fallbacks), which matches the symmetric reference to about `1e-12` relative and takes 0.15 s at 600 DOF and 8 s at 3300 with eigenvectors (46 s before). Pencils larger than the new public `eigen::dense::MAX_DENSE_REAL_DIM` (8000) are refused with `EigenError::DenseTooLarge`, whose message now names the real and the complex sparse solvers; non-finite input returns `EigenError::FaerGevd`. Ordering, the `ComplexEigenvalue` / `SingularPencil` errors and the M-normalized eigenvectors are unchanged (#800).
- The release test `transmon_eigen_sensitivity::real_eigen_sensitivity_release` now FD-validates a physical mode and runs in CI (the `core-ignored` job). On the μm-unit 133k-tet transmon mesh it checked a gradient null-space mode (λ ≈ 1e-17, 0.0002 GHz) and failed on Linux and macOS. The test's mode picker skipped null-space modes with an absolute `λ ≤ 1e-3` cutoff, but every physical λ on a μm mesh is about 1e-8, so it skipped them all and fell back to index 0. The picker now uses the σ-relative criterion `λ ≤ PecCavitySettings::DEFAULT_NULL_TOL_REL · σ`, which the production cavity solvers and the `geode` CLI already used, and panics if no physical mode was returned. The test's interior-node finder also assumed the unit cube; it now chooses a node by topology (all incident edges unconstrained). The picked mode is now λ = 6.25e-9 (3.77 GHz). The material gradient matches the FD and the closed form `−λ/ε` to the printed digits, and the geometry gradient matches the FD to a relative 1.9e-5. The tolerances are unchanged. A new regression test checks that the same cavity meshed in metres and in micrometres gives the same picked mode. No library code changed (#814).
- Two eigen thresholds no longer depend on the mesh length unit (#826). `eigen::self_consistent::q_factor` (now public, with `Q_LOSSLESS_REL_TOL`) reported `Q = ∞` when `|Im k| < 1e-12`, an absolute bound in rad per mesh unit: on a μm mesh at 5 GHz every mode with `Q ≳ 5e7` was infinite, against `Q ≈ 5e13` in metres. The mode is now lossless only when `|Im k| ≤ 16ε·|Re k|`. `FaerDenseEigensolver` rejected a kept complex pair only when `|Im λ| > 1e-9·max(|Re λ|, 1)`, so below `|λ| = 1` the test was absolute and at `λ ≈ 1e-8` a 10 % imaginary part passed as real. The floor is now the pencil scale `τ` (the shift scale, `∝ 1/L²` like `λ`), which still keeps round-off null eigenvalues real. The fiber `analytic::mixed_pencil::solve_mixed_modes` had the same `max(|β²|, 1)` floor in its real-eigenvalue and duplicate tests: on an nm mesh (`β² ≈ 3.5e-5`) the duplicate test merged all 14 in-window modes of the SMF-28 decoupled ladder into one. Both tests are now relative (every kept `β²` is in the window, so positive). Each fix has a two-unit regression test. Eigenvalues, modes and `Q` values are bit-identical; only the classification moves, and only where an old floor was active (`|Im k| < 1e-12`, `|Re λ| < 1`, `β² < 1`). For a pencil with `τ > 1`, an eigenvalue below `τ` may now carry up to `1e-9·τ` of imaginary round-off (before, `1e-9`).
- `Im k` no longer cancels at high `Q` (#830). Every `λ → k` conversion (`lossy_cavity::principal_k0`, the self-consistent driver's `k = √λ`, and `geode_util::eigen::k_from_lambda`) and the complex Lanczos's M-bilinear norm `β = √(wᵀMw)` computed `Im √z = √(½(|z| − Re z))`. That form loses `≈ log₁₀(2Q²)` digits (relative error `≈ ε·Q²`: `2e-5` at `Q = 1e6`) and is exactly 0 from `Q ≈ 6.7e7`, in any mesh unit. All four now call one shared helper, the new public `eigen::wavenumber::principal_sqrt`, which evaluates only the non-cancelling half (`Re k = √(½(|z| + Re z))`, `Im k = Im z / (2 Re k)`; mirrored for `Re z < 0`). Against `mpmath` at 50 digits it is correctly rounded on every test point from `Q = 1e2` to `1e14`, in mm-like and m-like units and in all four quadrants. Branch and sign conventions are unchanged (`Re k ≥ 0`, `sign Im k = sign Im λ`, `Im k = +0` exactly for real `λ ≥ 0`, so a lossless mode still has `Q = ∞`). In the complex Lanczos the lost `Im β` left the basis M-normalized only to `≈ √ε` on weakly lossy pencils, so its Ritz values degraded from `Q ≈ 1e7` up (see the `geode` CLI entry). At moderate `Q` every `Im k` changes in its last bits only: over the committed sphere Mie/PML fixtures (`Q ≤ 1.2e3`) the old `Q` was off its `mpmath` value by up to `2.2e-10` relative and the new one by `≤ 4.5e-16`. No golden or tolerance changed. `analytic::waveguide`'s private `principal_sqrt_c64` (the leaky `n_eff` of the complex PML port pencil) has the same form and is left for #831, because PR #812 is editing that file.
- `eigen::lossy_cavity::solve_lossy_cavity_modes` (the lossy/open `geode eigen` path) no longer fails on a spurious Ritz value near `σ` (#830). The complex-symmetric Lanczos has no interlacing guarantee, and its full Ritz set can contain values that approximate no eigenvalue. On the uniformly filled `tan δ = 0.1` sphere (x86-64 Linux, 4 threads) one such value, `λ = 0.4756 − 0.0767j` with relative residual 11, sat between the converged modes, displaced one from the closest-to-`σ` selection and failed the `residual_tol` gate. Whether it appears depends on the last bits of the iteration. At 2 threads, 1 of 12 loss levels failed both before the `β` fix above (at `tan δ = 0.08`) and after it (at `0.12`). At 4 threads, the committed `0.1` case failed after it. The selection now passes over candidates with relative residual `≥` the new `SPURIOUS_RESIDUAL_REL` (1.0, i.e. no correct digit), but only when that yields `n_modes` modes that all pass `residual_tol`. Otherwise it falls back to the old selection, so every failing run fails as before. Residuals are now computed for every candidate (two SpMVs each). New field `LossyCavityModes::n_spurious_filtered`; `LossyCavityModes` is not `#[non_exhaustive]`, so a downstream struct literal needs the new field. The same sweep plus `tan δ = 0.01`, `5e-9` and `1e-10` (30 runs) now passes, with `Q` within `2.1e-13` of `½·cot(δ/2)`.

#### `geode-util`

- `eigen::dense_lowest_eigenvalues` / `dense_lowest_eigenpairs` no longer call faer 0.24's generalized real QZ (`generalized_eigen` → `qz_real`). That QZ is inaccurate in the middle and upper spectrum of these pencils, and it returns spurious complex pairs, which the helpers silently dropped, shifting every later index. On the 3300-DOF sphere-PEC pencil, against a symmetric Cholesky reference, the old QZ was 1.4e-3 off at λ[379], 0.45 at worst over the physical spectrum, and dropped 32 complex eigenvalues; the compared λ[0..375] were already accurate to about 2e-12. The helpers now delegate to `FaerDenseEigensolver` (dense shift-invert, #800), which is within 1.2e-12 over the whole physical spectrum. They return `Result<_, EigenError>`, so a complex eigenvalue among the requested lowest modes is an `EigenError::ComplexEigenvalue` error, not a silent drop. `dense_lowest_eigenpairs` now does the per-cluster M-Gram–Schmidt its docs always promised (the old code only M-normalized). `dense_lowest_eigenvalues` no longer computes eigenvectors, so the sphere-PEC reference solve drops from about 42 s to about 5 s in release (#813).
- `eigen::q_factor` (and so the CLI eigen report's `q`, and `q_factor_from_lambda`) now calls `geode_core::eigen::self_consistent::q_factor`: its lossless cutoff is relative to `Re k` rather than the absolute `|Im k| > 1e-12`, so `Q` does not depend on the mesh length unit (#826).

#### Tests and CI

- The 22 `#[ignore = "faer 0.24 qz_real panics under debug-assertions"]` reasons are gone. No in-tree solver reaches `qz_real` any more; only `faer_qz_debug_overflow_guard` calls it, on purpose. The `derham_gauge_invariance` full-spectrum GEVD also moved off a direct `generalized_eigen` call. Tests that pass in debug in under about 30 s are un-ignored. The rest are ignored in debug builds only (`#[cfg_attr(debug_assertions, ignore = "slow in debug …")]`) and run in the default tier in release. As a result, the eigensolve tests of `eigensolver`, `nedelec_cavity`, `cube_cavity_{jax,julia,onnx}_reference` and `sphere_pec_{jax,julia,numpy,onnx,tfjava}_reference` now run in CI. `arpack.yml` runs `sparse_eigensolver`'s default tier, and `cube-cavity-tolerance.yml` runs `cube_cavity_numpy_reference`'s default tier, instead of their `--ignored` tiers, which would now select nothing (#813).

## [0.7.0] - 2026-10-01

This release completes the `geode` CLI's EDA-flow epic (#702). It adds:
- lossy and open-cavity eigenmodes;
- design sensitivities for every analysis;
- a scalable iterative driven path (an AMS preconditioner) with an adaptive frequency sweep and parallel frequency points;
- published JSON Schemas, a cookbook and prebuilt binaries.

It also fixes several precision and convergence bugs that could silently degrade results.

### Added

#### `geode` CLI

- `geode eigen` solves lossy and open-cavity problems (lossy dielectrics, UPML), reporting complex frequency, f and Q per mode (#706, #737).
- **Design sensitivities:** a new `sensitivity` spec section. Every gradient is computed by an adjoint, and an optional `fd_check` verifies each one against finite differences (#707, #741, #739, #750). Available observables:
  - capacitance: two-terminal C at P2, as `c_farad_p2`;
  - inductance: L_ij;
  - lossless eigen frequencies;
  - driven: |S11|², for one lumped port.
- **Scalable driven solves** (#708):
  - `solver.preconditioner` selects `jacobi` (default), `ilu0` or `ams` for iterative solves (#742, #744, #752).
  - `ams` is a Hiptmair–Xu auxiliary-space preconditioner with an exact nodal coarse solve. On the spiral inductor it converges where Jacobi and ILU(0) stall. At 228k edges it takes 33 s and 1.7 GB, against direct LU's 73 s and 17.8 GB. A spec with floating PEC conductors is rejected up front with `invalid_spec`.
  - `sweep.adaptive {tolerance, max_snapshots}` runs a Galerkin reduced-order sweep. Interpolated rows carry an error indicator. A frequency that misses tolerance, or whose reduced solve is singular, falls back to an exact solve (#745, #747, #753).
  - `--jobs N` solves frequency points in parallel. Each job gets `max(1, threads/N)` threads, so jobs don't oversubscribe the CPU (#745, #753, #755, #762).
  - `--progress` writes JSONL progress events to stderr (#745).
- **Mesh:** `geode mesh --analysis capacitance|inductance` emits starter specs with nets, ground and contacts (#720, #727).
- **SPICE:** `--spice-positive-k` and `--spice-ret-pin` export variants (#723, #733).
- **`geode check`:** reports `anchor_nnz_ratio` and `above_anchor` resource-estimate fields, plus an AMS memory estimate (#713, #735, #752).
- **Touchstone:** the output path is checked before the solve, and the export is round-trip-checked with scikit-rf in CI (#713, #735).

#### Distribution (#709)

- Published JSON Schemas for the spec and the report (`geode schema`, `crates/geode-cli/schemas/`, with a drift test), plus an examples cookbook in `crates/geode-cli/examples/` (#728).
- Prebuilt `geode` binaries for linux x86_64/aarch64 (glibc ≥ 2.35) and macOS arm64, attached to each GitHub Release, plus a Gmsh-equipped container (`docker/geode-cli/`) (#726).

### Changed

- **Iterative driven solves fail honestly:** a solve whose recomputed explicit residual misses `solver.tol` now fails with `solve_failed`, even if the recursive residual converged. The error suggests loosening `tol` (#744, #749).
- **`--threads 1` and `GEODE_NUM_THREADS=1` now mean a serial LU.** Previously faer's LU still used every core. Results at one thread can change at roundoff (#753, #755, #762).
- **`--jobs` bit-identity:** `--jobs N` matches `--jobs 1` bit for bit only at the same threads per factorization. Otherwise the results agree to roundoff, about 1e-14 relative (#753).

### Fixed

- **Nédélec precision:** material and source weights are now uploaded at backend precision (f64). Before this, ε was rounded to f32 (#740, #746).
- **Surface triangles:** a surface triangle that is not a tet face returns `SurfaceNotOnMesh` instead of panicking, with a per-tet-face check and an integration fixture (#725, #729, #732, #751).
- **SPICE `ret` pin docs:** the floating `ret` caveat (it needs a DC path to `0`) is now documented, and the README usage lines are fixed (#736, #738).

### API notes (geode-core, pre-1.0)

- `waveguide_mode_reduce` now returns `Result`.
- `RomError::UnsupportedOperator` is removed.
- The capacitance sensitivity observable is renamed `c_farad` → `c_farad_p2` (#748).
- New: `driven::solve_ams` and `IterativePreconditioner::Ams`, `eigen::parallel::ParallelismGuard::cap` and `with_thread_budget`.

The spec and report schemas stay at v1; every CLI change is additive.

## [0.6.0] - 2026-09-29

This release turns the `geode` CLI into a static parasitic-extraction tool
for EDA flows (Epic #702): layout geometry goes in, and tagged meshes,
Maxwell capacitance and inductance matrices, and SPICE subcircuits come
out, alongside the Touchstone S-parameters added in 0.5.0.

### Added

#### `geode` CLI — EDA flows (Epic #702)

- `geode mesh`: turns a versioned, tool-neutral layout description
  (rectilinear conductor polygons, a dielectric layer stack, lumped gap
  ports, PEC or UPML outer boundary) into an MSH 4.1 mesh with
  automatically named physical groups, plus a starter problem spec, by
  driving an external `gmsh` binary (`--gmsh` / `GEODE_GMSH` / `PATH`,
  `--gmsh-timeout`). The report records the Gmsh version and sha256 of the
  layout, script and mesh. Golden test: the spiral inductor rebuilt from a
  layout, L −4.3 % vs Mohan / −6.3 % vs MoM (#704, #712).
- `geode capacitance`: Maxwell capacitance matrix between named conductor
  surfaces, with one or more grounded surfaces. Golden test: coaxial and
  triaxial lines, ≤ 0.2 % vs analytic on the benchmark mesh (#705, #716).
- `geode capacitance --spice <PATH>`: exports the matrix as a mutual-C
  SPICE `.subckt` (ground branches from row sums, noise-level branches
  dropped and listed, significant sign violations rejected), verified by
  an ngspice AC admittance check (#715, #717).
- `geode inductance`: static inductance matrix for open current paths
  (conductor volume from a source face to a sink face), with a new
  `materials[].mu_r`. Source and sink must sit on the same connected
  piece of the required PEC wall, checked before solving; a non-physical
  L (non-SPD or L_ii ≤ 0) fails with `solve_failed`. Golden test: coax
  and triax, ≤ 0.35 % vs analytic on the benchmark mesh, including a
  μ_r = 4 core (#714, #718).
- `geode inductance --spice <PATH>`: exports the L matrix as an
  `LEXTRACT` `.subckt` (one inductor per path to the PEC return,
  `K` couplings with sign preserved), verified by an ngspice AC check
  (#719, #722).
- Spec and report stay schema v1; all changes are additive.

#### Library

- `geode_core::assembly::current_path`: general open-path current
  excitation. A P1 conduction solve gives J = −σ∇φ normalised to 1 A,
  discretely divergence-free and verified against the analytic
  wire/loop builders. Grounded-component analysis with a per-path
  current-balance check (`CurrentPathError::UnbalancedGround`) (#718).
- Electrostatic face-flux helpers made public (#718).

### Changed

- `materials[].eps_r` now defaults to vacuum (1) for every analysis, so a
  spec that omits it now parses where it used to fail. A non-unit `mu_r`
  is rejected outside `geode inductance` (#718).
- CI installs Gmsh (4.12.1 on Ubuntu) for the `geode-cli` job and fails,
  rather than skips, the Gmsh tests if it is missing. It also runs the
  inductance golden benchmark tier in release mode (#712, #718).

### Fixed

- `geode mesh` now hollows thick conductors (cut out of the dielectric
  and any sheets they pierce), so Leontovich R/Q match the benchmark:
  spiral R −5.9 % → +0.16 %, Q +6.7 % → +0.76 %. Coplanar-wall merging
  is disabled so no cavity face is left untagged, and new mesh-time and
  solve-time guards reject untagged or dangling surface triangles
  (#721, #724).

## [0.5.0] - 2026-09-28

This release completes the `geode` CLI's second phase: alongside `check`
and `driven`, it now computes cavity eigenmodes (`eigen`), extracts L/R/Q
with a quasi-static f→0 L₀ extrapolation (`extract`), handles open
radiating problems with wave ports and UPML / Silver-Müller boundaries,
exports fields and far-field patterns, and writes Touchstone files for
EDA tools. Every committed benchmark artifact produced through a Burn
backend was audited for float precision. Three had been generated in
f32, and CI now enforces f64 provenance. The audit also uncovered a
singular σ = 0 shift that had silently broken the sparse Mie eigensolve;
it is fixed and now guarded.

### Added

#### `geode` CLI — Phase 2 (Epic #680)

- `geode eigen`: lossless PEC-cavity eigenmodes via sparse shift-invert
  Lanczos, with a residual acceptance gate (`eigen.residual_tol`) so a
  non-converged solve fails with `solve_failed` instead of reporting wrong
  modes. Golden test: dielectric sphere in a PEC cavity vs analytic Mie
  roots (#681, #685).
- `geode extract`: per-frequency L/R/Q, the quasi-static L₀ via
  Richardson extrapolation (optional `l0_rel_tol` gate) and SRF. Golden
  test: SLCFET 3HP, L₀ −2.6 % vs MoM-PEEC (#682, #686).
- Open boundaries and wave ports in the problem spec: box UPML
  (`absorbing_regions`), Silver-Müller walls, and wave ports built from a
  tagged mesh face. Golden test: patch antenna, f_res −6.5 % vs the
  Balanis cavity model (#683, #688).
- `--outdir`: opt-in VTU field export and NTFF far-field results
  (directivity, gain, efficiency, principal-plane cuts). The NTFF box is
  validated before the sweep (#684, #694, #697, #699).
- `--touchstone <PATH>` on `driven` / `extract`: Touchstone 2.0 `.sNp`
  output (S-parameters, RI, per-port `[Reference]` impedances), referenced
  from the report with its sha256; wave-port specs and `eigen` are
  rejected up front (#703, #711).
- `geode check` reports `resources`: nnz(A) and order-of-magnitude
  memory/time estimates for the chosen solver, calibrated on a 1.16M-DOF
  run and documented with measured error bars (#703, #711).
- Spec and report stay schema v1; all changes are additive.

#### Library

- `geode_core::eigen::pec_cavity`: tagged mesh + named PEC group +
  materials → eigenmodes (#685).
- `geode_core::driven::extraction::extrapolate_l0`: public, panic-free
  replacement for two duplicated private helpers (#686).
- Wave-port construction from a tagged planar mesh face
  (`driven::ports::wave_face`) (#688).
- `EigenError::DegenerateShift`: the sparse complex and projected
  shift-invert Lanczos solvers now reject a degenerate σ ≈ 0 solve that
  collapses onto the gradient null space instead of returning spurious
  modes (#696, #701).
- Shared benchmark-provenance helpers in `geode-util` (`BackendInfo`,
  oracle-block carry-through on regeneration) (#689).

### Changed

- CI: a `benchmark_provenance` guard classifies every committed benchmark
  artifact and requires f64 provenance on Burn-backed ones. CI also now
  runs the default tiers of the spiral, SLCFET and patch benchmark tests,
  which never ran there before (#692, #698, #700).

### Fixed

- Benchmark artifacts that were generated on the f32 `wgpu` backend,
  now regenerated in f64:
  - SLCFET 3HP: Q and R had been ~2× off (#687, #689).
  - patch antenna: negligible drift (#690, #695).
- `examples/mie_sphere` used a singular σ = 0 Lanczos shift and on main
  returned only null-space modes. It now uses σ = 1.0, and the committed
  results were regenerated (TM₁,₁ ~5.7 %, TE₁,₁ ~1 %, TM₂,₁ 0.5–2.3 %)
  (#691, #693).
- The same σ = 0 bug in the ignored sparse Mie test and the
  `mie_end_to_end` benchmark; earlier sparse timings from that
  benchmark are invalid (#696, #701).
- A stray tracked `.loom-managed` worktree marker (#679).

## [0.4.0] - 2026-09-27

This release gives GEODE-FEM a headless command-line driver: a `geode`
binary that takes a mesh plus a JSON/TOML problem spec and emits a
versioned JSON report, so design flows and agents can run validated solves
without writing Rust. It also closes two latent hazards found along the
way — a spiral-inductor reference artifact that had silently been generated
in f32, and lint debt outside `geode-core` hidden by a narrowly scoped CI
gate.

### Added

#### `geode` CLI — Phase 1 (#673)

- New `crates/geode-cli` crate producing the `geode` binary, installable
  with `cargo install --locked --git … --rev <sha>` (#676).
- `geode check` validates a spec and mesh without solving (physical-group
  resolution, DOF counts); `geode driven` runs a lumped-port frequency
  sweep with PEC / Leontovich boundaries, direct-LU or iterative COCG, and
  reports Z/Y/S per frequency plus per-port L/R/Q/S11. `eigen` and
  `extract` are reserved and return `not_implemented`.
- Versioned problem-spec v1 (JSON or TOML) and report v1 schemas with
  explicit units, unknown-field rejection, stable error codes, and
  provenance (crate version, git sha, backend, mesh SHA-256); documented in
  `crates/geode-cli/README.md`.
- `geode --version` prints the git sha; the process exits non-zero on
  solver failure or non-convergence; `--backend` confirms the compiled-in
  backend and `--threads N` caps threading.
- Golden tests re-express the spiral-inductor benchmark as CLI input (smoke
  tier in default CI; full-mesh tier `#[ignore]`d) and assert agreement
  with both the committed results and the library (~1e-12).
- `geode_core::mesh::read_tagged_tet_mesh` / `TaggedTetMesh`: a public
  tagged-mesh reader with physical-group name lookup; the sphere, spiral,
  patch, and transmon fixture loaders now share it.

### Changed

- CI clippy gate widened from `geode-core` to the whole workspace
  (`--workspace --tests --features geode-core/arpack`), and a new CI job
  runs `cargo test -p geode-cli`; pre-existing lints fixed across
  `geode-util`, `geode-validation`, and the example crates (#675, #677).
- Vendored `.anvil/` tool install untracked and gitignored — it is a
  copy-installed tool tree, not project source.
- Loom orchestration tooling upgraded to 0.18.0.

### Fixed

- `benchmarks/spiral_inductor/results.toml` regenerated in f64. The
  committed artifact had been produced on the f32 `wgpu` backend, leaving
  R/Q at 1 GHz ~16–18 % off (L unaffected); no physics changed. Regeneration
  now carries the `[oracles.palace]` block through, a default-CI check
  requires `float_dtype = "F64"`, and the CLI golden test checks R/Q against
  the artifact (#674, #678).
- New Rust 1.98 clippy lint (`chunks_exact_to_as_chunks`) fixed in
  `geode-util` and `geode-validation` (#676, #677).
- Stale references to the retired `geode-cli` crate in `geode-app` docs.

## [0.3.0] - 2026-07-29

This release repositions GEODE-FEM as a differentiable-by-construction
complement to Palace: the full 2×2 sensitivity matrix (material + geometry ×
scalar + H(curl) EM) is FD-validated and mutation-tested, and it is exercised
end-to-end by a driven-Maxwell shape-adjoint chain that culminates in
many-DOF freeform inverse design of a curved conformal radiator. Alongside
the adjoint work, a matrix-free eigensolve path with a full Hiptmair–Xu AMS
preconditioner opens the road past the direct-LU memory wall, and a suite of
validated benchmarks lands: slotless-PM Arkkio torque T(θ) at 0.71 %, SMF-28
LP01 modal b at 0.88 %, and transmon capacitance/EPR extraction. The
workspace itself goes standalone (vendored Palace docker, external refs
scrubbed) and gains the `geode-app` harness, standalone example crates, and
the `geode-util` pre-core staging crate.

### Added

#### Differentiable sensitivities — the full 2×2 matrix (Epic #569)

- Discrete-adjoint material sensitivity ∂(observable)/∂ε on the scalar path
  (#570, #573) and geometry/shape gradient ∂(observable)/∂X (#571, #575),
  both FD-validated.
- H(curl) driven-EM material adjoint ∂/∂ε (#579) and geometry/shape adjoint
  ∂(EM observable)/∂(node coords) (#581), completing the matrix; complex-ε
  (loss-tangent) sensitivities on the driven adjoint (#598).
- Hellmann–Feynman eigenvalue sensitivities ∂λ/∂p (#600) and
  inductance-matrix reluctivity sensitivity ∂L/∂ν via the self-adjoint
  energy form (#615).

#### Driven shape-adjoint chain → freeform inverse design

- |S11|² shape-adjoint objective closure and the first driven-Maxwell
  optimization loop (#627); complex-ε lossy shape adjoint (#630);
  pinned-feed (#632) and moving-feed (#644) lumped-port terminations in the
  shape adjoint; box-UPML tensor-material adjoint at fixed Λ (#643).
- Composed open-radiator shape adjoint + full inverse-design capstone
  (#645); high-DOF freeform boundary parametrization with mesh-morph
  regularizer (#654); curved conformal radiator fixture (#653) and many-DOF
  freeform inverse design on it (#655).
- FDTD-density head-to-head baseline plus measured Meep runtime-scaling
  evidence for the intractability axis (#656, #657).

#### Matrix-free scale path and AMS preconditioning (Epic #547)

- Matrix-free Nédélec curl-curl + mass matvecs on Burn (#483), GPU-resident
  COCG over the matrix-free operator (#487), and
  `SolverMode::IterativeMatrixFree` on the driven pencil (#495).
- Matrix-free shift-invert Lanczos to scale past the direct-LU memory wall
  (#525), AMS-lite H(curl) preconditioner (#528), and a matrix-free MINRES
  inner solver for interior/indefinite shifts (#537).
- Full Hiptmair–Xu AMS cycle: three-space AMS as the SPD preconditioner for
  indefinite MINRES (#560), the vector-nodal ΠᵀAΠ block (#553), an
  O(node_dim) few-sweep coarse solve (#554) with a smoothed-aggregation AMG
  alternative (#566), and the `transmon_bench` scale harness (#552).
- Distributed groundwork: geometric k-way edge-DOF partitioner + halo map
  (#642) and a distributed matrix-free Krylov abstraction with a
  single-process mock collective (#646).
- Opt-in `InnerSolver::DirectCustomOrder` — custom fill-reducing LU ordering
  via faer's public deeper API (#543, #544).

#### Second-order elements

- 20-DOF second-order Nédélec tet element (#617) with an opt-in p=2 driven
  forward path + material adjoint (#621), p=2 PEC-cube eigensolve with a
  frequency-convergence gate (#622), and a p=2 shape adjoint via a dual
  element twin (#623).
- P2 Lagrange tets on the scalar magnetostatic path, retiring the 2.44 %
  B-field miss to 0.211 % at O(h²) (#472, #474), and on the electrostatic
  path with the adjoint retained (#608).

#### New solver physics

- 3-D electrostatic solver + Maxwell capacitance-matrix extraction (#481)
  and 3-D vector magnetostatics + inductance-matrix extraction (#512), with
  a 2-D scalar magnetostatic Poisson solver as the oracle rung (#460).
- Transient EM solver — generalized-α time integration with broadband
  S-parameters via DFT (#489); adaptive fast frequency sweep via greedy
  Galerkin PROM (#610).
- London superconductor surface BC (per-tag λ_L) on the driven and eigenmode
  paths, with ∂λ/∂λ_L via Hellmann–Feynman (#609).
- Divergence-free M-orthogonal projection for the eigen path, port-aware so
  the junction LC mode survives (#513, #515), plus a tree-cotree gauge
  module with its spectrum limitation pinned (#508).

#### Validated benchmarks

- Motor torque (Epic #448): PM magnetization sources and air-gap oracle
  (#463), Maxwell-stress + Arkkio torque extractors vs the loop T = m × B
  oracle (#464), and the driven slotless-PM locked-rotor T(θ) benchmark at
  0.71 % (#465), on multi-band annular meshes with ν heterogeneity oracles
  (#462).
- Optical fiber (Epic #339): full-vector mixed E_t–E_z Nédélec–Lagrange
  dielectric modal pencil hitting SMF-28 LP01 b within 0.88 % (≤ 1 % gate),
  with a regression tripwire for the old reduced-pencil artifact (#473,
  #477) and an audit of the ε-coupling term that pencil dropped (#461);
  analytic-cladding DtN fiber solver (#447); chromatic dispersion D(λ) + ZDW
  vs the analytic oracle (#482).
- Transmon (Epics #475/#476 groundwork): fixture ingestion + shared MSH tag
  scanners (#488, #494), Josephson junction as a lumped reactive shunt in
  the eigensolve (#496), EPR quantization + qubit parameters (#511), a
  differentiable capacitance→E_C chain (#586) driving gradient-based
  transmon-parameter optimization (#588) and island-pad shape optimization
  on the real 133k-tet mesh (#590), with harmonic mesh-morphing island
  deformation (#599).

#### New `geode-util` pre-core staging crate (Epic #414)

- Introduced `geode-util`, a pre-core staging layer above `geode-core` that
  collects shared helpers previously scattered across `geode-validation` and
  the example crates (module map `repo` / `convert` / `interop` / `fixture`
  / `viz`): fixture-repository helpers (#417), interop decoders (#418),
  edge-DOF → nodal reconstruction (#419), and the shared fixture
  TOML/pvd/sweep harness (#423). `geode-validation` now consumes these and
  retains only genuine validation-harness code, with JSON fixture
  loader/schema and serde glue migrated in Epic #429 (#434, #435, #437).

### Changed

- The workspace is fully standalone: Palace docker tooling vendored in-repo
  and all external repository references scrubbed (#542).
- Examples restructured into standalone top-level crates (mie, patch
  antenna, spiral inductor, SLCFET, waveguides, fibers; #402–#412) driven by
  the new `geode-app` clap harness with arg groups and a lifecycle seam
  (#400); example crates now depend on `geode-util` instead of
  `geode-validation`.
- Bunsen integration closed out (Epic #355): named shape contracts wired
  across the P1 gather cluster, assembly, and basis/eigensolver paths (#466,
  #467, #469) and the git pin swapped to crates.io bunsen 0.28.0 (#507).
- Assembly and factorization hot paths parallelized: rayon on the Nédélec
  host-side pattern/slot build (#538) and in faer's sparse-LU factorization
  (#521); M·v_j caching drops O(k²) reorthogonalization SpMVs (#510).
- Orchestration/authoring tooling (Loom, Anvil, Repo Skills) kept current
  across the cycle (#540, #597, #625, #662, #667, #670, #671).

### Fixed

- Transmon fixtures resolve mesh groups by name, with the real DeviceLayout
  fixture swapped in (#494).
- `geode-validation` rustdoc lints repaired and the `cargo doc -D warnings`
  gate widened to the whole workspace (#541).
- Dead macos-13 CI leg removed from cube-cavity-tolerance, ending 24 h hangs
  (#443).

### Removed

- Deleted the orphaned `examples/_support` (`geode-examples-support`) crate
  after its last consumer moved to `geode_util::viz` (Epic #414 Phase 3,
  #426, #427).
- Deleted the external-reference MoM baseline crates as part of the
  standalone move (#542).

## [0.2.0] - 2026-06-25

### Changed

#### geode-core public API reorganized into a hierarchical module tree (Epic #377 — BREAKING)

- The crate's public surface, previously a flat set of root re-exports
  (`geode_core::<item>`), is now organized into directory-backed module
  groups: `backend`, `traits`, `mesh`, `elements`, `derham`, `assembly`,
  `solver`, `eigen`, `driven`, `analytic`, `postproc`, `interop`, and
  `prelude`. Every public item now lives at its canonical path
  `geode_core::<module>::<item>` (children #378–#386).
- **All deprecated flat-root re-export shims have been removed.** Code that
  imported items via `geode_core::<item>` must migrate to the canonical
  module path or `use geode_core::prelude::*;`. The only re-exports that
  remain at the crate root are the core traits
  `geode_core::{Element, Mesh, Operator}` (also available via
  `geode_core::traits::*` and the prelude).
- `silvermuller_self_consistent` has moved to `eigen::self_consistent`
  (canonical path `geode_core::eigen::self_consistent::*`), with no compat
  shim — it is a quasimode-`k` eigenpencil finder and now lives alongside
  the other eigensolvers.
- `geode_core::prelude` is finalized as the recommended ergonomic surface:
  glob-import it (`use geode_core::prelude::*;`) to pull in the high-traffic
  entry points (mesh constructors/readers, assembly/eigen/driven/analytic
  types, core traits) from their canonical paths.

This is a breaking change for downstream callers; the workspace minor
version is bumped accordingly. See epic #377 and children #378–#387.

## [0.1.0] - 2026-06-15

### Summary

Initial public release. GEODE-FEM is a Burn-based Rust FEM/DG electromagnetic
solver. The 0.1 milestone closes four foundational epics (#88, #193, #226,
#234) and lands a Krylov + iterative-solver sweep on top, bringing the project
to the point where the driven solver hits Palace 3D parity on a spiral
inductor benchmark and the wave-port path validates against analytic
mode-matching cross-checks.

### Added

#### Solver core (Epic #88 — Burn bring-up)

- Workspace skeleton with three crates: `geode-core` (solver primitives),
  `geode-cli` (`geode` binary), and `geode-validation` (cross-backend
  comparison harness).
- Burn-tensor assembly layer with `wgpu` default backend and opt-in `ndarray`
  / `cuda` / `autodiff` backends; `unsafe_code = "deny"` at the crate
  boundary.
- Whitney / Nédélec / P1 element kernels including the shared
  `whitney_face` surface-mass module (#221).
- Sparse `[nnz]` pattern-slot Nédélec assembly for the driven path, lifting
  the 46k-edge dense-scatter cap (#220).
- De Rham `d⁰` rank classifier replacing the older spurious-mode heuristic
  (#124).
- ARPACK FFI eigensolver (vendored `dsaupd_c` / `dseupd_c` bindings, no
  bindgen required) behind the `arpack` Cargo feature.
- Pure-Rust sparse shift-invert Lanczos path (faer sparse LU) as the
  default eigensolver.
- Phase I/J cross-backend Mie reference suite: NumPy (#179), Julia (#181),
  JAX (#180), TF-Java (#183), and ONNX expressibility audits (#178, #182).

#### Driven solver (Epic #193)

- Deterministic driven solve `A(ω)x = b` with volumetric current source
  (#194).
- Conductivity term σ via ω-independent damping matrix C (#196).
- Matched (full Sacks) UPML lifted into the Burn assembly layer and into
  `driven_solve` (#205).
- Palace-style uniform lumped port for `driven_solve` with R termination
  and V/I bookkeeping (#206).
- Leontovich surface-impedance BC for thick conductors (#207).
- Driven Mie scattering benchmark Q_ext / Q_sca vs ka with matched UPML
  against the analytic series (#195).
- Z(ω) → L/R/Q/S₁₁ extraction and assembly-reusing frequency sweep over
  port-driven solves (#209).
- Layered-stack spiral inductor mesh generation (gmsh) with tag adapter to
  port / Leontovich / UPML inputs (#217).
- N-port S-matrix extraction over factor-once / multi-RHS port-driven
  solves (#219).
- Spiral inductor L/Q benchmark — FEM sweep vs Mohan analytic and MoM PEEC
  baselines (#211).
- SLCFET 3HP spiral capstone hitting the 5 % bar on quasi-static L₀
  comparison (#230).

#### Patch antenna (Epic #226)

- Probe-fed FR-4 patch-antenna gmsh fixture with box-UPML open-radiator
  adapter (#231).
- Patch-antenna S11 / resonance / bandwidth / efficiency benchmark vs the
  cavity-model oracle (#232).
- Love-equivalence near-to-far-field transform → patch radiation pattern,
  directivity, gain (#229).
- Impedance-matched patch feed delivering a real −10 dB return loss and
  bandwidth (#237).
- NTFF pattern artifact for the impedance-matched patch fixture
  (G = D·η_matched) (#252).

#### Wave-port BC (Epic #234)

- 2D transverse modal eigensolver for waveguide port cross-sections
  (#240).
- Wave-port boundary condition and wave-port S-parameters (#234 Phase 2)
  (#245).
- 2D waveguide modal pencil moved onto the sparse Lanczos path; drops the
  faer-QZ debug-overflow workaround (#253).
- True mesh height-step waveguide fixture and single-mode S-parameter
  validation (#248).
- Multi-mode waveguide modal eigensolve with outgoing-β branch and
  wrapper unification (#254).
- Rank-N SMW wave-port BC, multi-mode `waveguide_mode_reduce`, and block
  S-matrix (#255).
- Bi-modal straight-section wave-port validation (#256).
- Bi-modal height-step with analytic mode-matching cross-check (#257).
- Deterministic eigenvector sign pin in `solve_rect_waveguide_modes`
  (#262).
- General-cross-section 2D modal eigensolver (#265).

#### Iterative solvers and oracle parity (post-epic sweep)

- Krylov iterative solver path (COCG + Jacobi) for the driven
  complex-symmetric system (#243).
- Krylov iterative solver wired through sweep pipelines (#264).
- ILU(0) preconditioner for the COCG Krylov path (#267).
- Palace 3D oracle scaffolding: config generator, result ingester, and
  patch-benchmark wiring (#239).
- Palace 3D oracle parity for the spiral inductor benchmark (#266).
- Matched (full Sacks) UPML on the eigenmode path with quasi-mode Q vs
  `mie_open` complex roots (#223).
- Fine Mie sphere fixture — on-resonance driven Q_ext / Q_sca below 5 %
  (#224).

### Changed

- Build profile keeps dense linear algebra (faer) and tensor backends
  (Burn / wgpu) optimized in debug and test builds; project crates remain
  no-opt for fast iteration.
- README refreshed for the driven + multi-mode wave-port era (#274).

### Fixed

- ARPACK iterations are now deterministic (fixed-seed v₀ + rng) for all
  reference eigensolves (#191).
- `upload_mesh` honors `B::FloatElem` instead of forcing f32 (#99).
- Backend cfg robust to feature unification via precedence selection
  (#76).

### Removed

- A1's deprecated wave-port shims dropped following the multi-mode
  migration (#268).
