# Can Palace model the Design A Floquet cell? — desk research for the #1581 cross-check

**Issue:** #1629 (research note; sizes the Phase-4 cross-check in #1581, Epic #1252) ·
**Date:** 2026-11-14 · **geode:** `origin/main` at `8f31f5f9`.

**Status.** Answered (desk research against Palace's source and in-tree documentation at a
stated version; no Palace run, no config written — both out of scope here). One verdict, at
the end. "Design A" here means the Epic #1252 Phase-4 TCDA/Floquet array element
(`benchmarks/tcda_baseline/`, `benchmarks/tcda_design/`,
`crates/geode-core/tests/tcda_design.rs`) — not the unrelated conformal patch-antenna
"design A" of `benchmarks/patch_antenna_conformal/`.

**Palace version.** All Palace citations below are read at the repository's vendored pin
`fba6a5b9d17ef09db2907e9c4c6f92d123d131c0`, which Palace self-reports as
`v0.17.0-99-gfba6a5b9d` — 99 commits past upstream `v0.17.0` (2026-06-28), not a tagged
release. The pin is set by all three Dockerfiles
(`reference/palace/docker/Dockerfile:47`, `Dockerfile.cuda:97`, `Dockerfile.tuned:86`) and
confirmed in the committed fixture (`reference/fixtures/transmon_palace/results_controls_1123/*/palace.json:56`,
`GitTag`). Line numbers are into the Palace tree at that commit; `docs/src/…` is the
source of the published palace.readthedocs.io pages.

> **Verdict: the cross-check is possible as specified.** Palace at the pin has
> Bloch-phased periodic boundaries in driven solves, lumped ports inside such a cell with
> S-parameters referenced to a user-set resistance, and the scan angle set per run (one
> Bloch vector per invocation). geode's `|Γ_active|` maps to Palace's lumped-port
> power-wave S-parameter with `R = z_ref` (feed-side view) or the Floquet-port (0,0)-order
> S-parameter (aperture-side view). Two caveats, neither scope-reducing: a scan grid is N
> Palace runs (one angle per run), and the mixed lumped+Floquet S-parameter path is the
> newest code in the pin — drive and observe with the same port type, or validate the
> mixed path first.

## 1. (a) Periodic boundaries with a Floquet/Bloch phase in driven simulations — **yes**

Palace supports periodic boundary pairs (donor/receiver attributes) with a
Floquet/Bloch phase. The phase is folded into the bulk time-harmonic Maxwell equations by
substituting `E(x) = E_p(x)·e^(−i k_p·x)` — "exact periodic boundary conditions are
applied" to `E_p`, with `k_p` the user-specified Bloch wavevector; the magnetic flux is
recovered as `B_p = −(1/iω)∇×E_p + (1/ω) k_p × E_p` (Palace `docs/src/reference.md:79-104`,
§ "Floquet periodic boundary conditions"; phase-factor sign `e^(−ik_p·x)` since 0.15.0,
`CHANGELOG.md:333-335`). The user-facing keyword is
`config["Boundaries"]["Periodic"]` with `BoundaryPairs` plus an optional global
`"FloquetWaveVector"` (`docs/src/guide/boundaries.md:87-100`; config schema
`palace/utils/configfile.hpp:497-510`).

This is wired into the **driven** solver, not just eigenmode: `SpaceOperator` assembles the
`k_p`-dependent bulk terms (curl-curl cross term and `k×μ⁻¹k` mass term,
`palace/models/spaceoperator.cpp:1045-1058`; `palace/models/materialoperator.hpp:104-105`), a
Robin absorption coefficient on Floquet port faces (`spaceoperator.cpp:1038-1039`), and the
driven postprocessor adds the `B += k_F(ω)/ω × E` correction
(`palace/drivers/drivensolver.cpp:138-141` and `:212-216`; the frequency-sweep path again at
`:292-341`, `:471-475`). Feature history: Floquet periodic BCs landed in 0.14.0
(2025-08-20, `CHANGELOG.md:402-404`); the current `BoundaryPairs` + `FloquetWaveVector`
config shape in 0.15.0 (2025-12-02, `CHANGELOG.md:308-312`); driven Floquet ports in 0.17.0
(2026-06-28, `CHANGELOG.md:81-83`).

Both committed examples exercise it: `examples/cylinder/floquet.json` (eigenmode with
`FloquetWaveVector`) and `examples/dielectric_grating/dielectric_grating_uniform.json:3-63`
— a **`"Type": "Driven"`** 2–12 GHz sweep with an oblique `FloquetWaveVector` and two
Floquet ports.

## 2. (b) A lumped port inside such a cell — **yes**

Nothing forbids combining `LumpedPort` with `Periodic`/`FloquetPort`: the config parser
reads them side by side (`palace/utils/configfile.cpp:892-898`) and the validator rejects
only duplicate port *indices* across port types, not their combination
(`palace/utils/configfile.cpp:1421-1460`). Mixed lumped+Floquet S-parameters are computed
with an explicit √2 power-normalization bridge (Floquet ports normalize incident waves to
unit time-averaged power, lumped/wave to unit peak power;
`palace/models/postoperator.cpp:1312-1368`). The only stacking rule is per-attribute: a
boundary attribute may carry at most one of wave port / Floquet port / surface current
(`spaceoperator.cpp:194-226`) — the lumped feed is a different, interior surface, and the
Floquet phase lives in the bulk equations and the donor/receiver constraints only
(`docs/src/reference.md:81-85`), so it does not touch a feed placed inside the cell.
Palace reports per-port `V`, `I` and `S` (`port-V.csv`/`port-I.csv`,
`palace/models/postoperatorcsv.cpp:955-956`; `port-S.csv`, `:1112`).

**Caution:** the mixed-config S-parameter path is the newest code in this area — the pin's
own changelog carries an *unreleased* entry fixing "incorrect values" in `port-S.csv` for
mixed Floquet + lumped/wave configurations (`CHANGELOG.md:19-22`, under "In progress").
Tagged 0.17.0 predates the fix. For a cross-check, drive and observe with the same port
type (avoiding the bridge entirely), or sanity-check S-matrix unitarity on a trivial case
before trusting a mixed run. See §5 (AC4).

## 3. (c) The scan angle per solve — **yes, one angle per run**

The Bloch vector is a single global input: one `FloquetWaveVector` per simulation
(`palace/utils/configfile.hpp:497-510`). Setting it is how the scan angle is set; there is
no in-run angle sweep — nothing varies `k_F` per excitation index or per frequency point
except optional linear scaling with frequency, `k_F(f) = k_F,ref · f/f_ref`, which keeps
the *incidence angle constant* across a sweep (`"FloquetReferenceFrequency"`,
`docs/src/guide/boundaries.md:179-183`; `configfile.hpp:505-509`; nondimensionalized at
`palace/utils/iodata.cpp:737-742`). The grating example uses exactly this for a swept
oblique incidence (`examples/dielectric_grating/dielectric_grating_uniform.json:31-63`).

So a geode scan grid (one factorization per angle in-process,
`FloquetCell::scan_cut`, `crates/geode-core/src/driven/floquet.rs:1639-1645`) maps to **N
Palace runs, one per angle** — the same one-invocation-per-config pattern this repo's
Palace benchmark tooling already uses (`benchmarks/gpu_driven_scaling/`). Each run
re-assembles the `k_F`-dependent terms, so there is no reuse loss to model; the cost
difference is process overhead, not capability.

## 4. (d) The observable compared with geode's `|Γ_active|`

**Geode side (fixed by the code):** `FloquetCell::edge_scan_impedance(omega, theta, phi,
edge)` returns the scan impedance `Z = −V/I` of a unit delta-gap current on one full edge
DOF — a lumped feed in a Bloch-phased cell (`crates/geode-core/src/driven/floquet.rs:1606`);
`Γ = (Z − z_ref)/(Z + z_ref)` with real feed-line impedance `z_ref`
(`crates/geode-core/tests/common/tcda_design.rs:16`), wrapped by `gamma_active`
(`crates/geode-core/tests/common/tcda_baseline.rs:339`) and reduced over a scan grid by
`max_gamma_active`.

**Palace counterpart, feed-side (primary):** the lumped-port S-parameter.
`LumpedPortData::GetSParameter` projects the solution field onto the port mode,
`s·E` with the linear form `∫ E·(E_inc/Z_s) dS` = `V/V_inc`, where `V_inc` encodes the
port's resistance (`palace/models/lumpedportoperator.cpp:178-188`, `:284-295`);
`MeasureSParameter` assembles these into **Kurokawa power-wave generalized S-parameters
with per-port reference impedance — for lumped ports, the user-specified `R`**
(`palace/models/postoperator.cpp:1330-1344`, output to `port-S.csv`). For a real reference
impedance this is exactly geode's definition: `|S[p][p]| = |(Z − R)/(Z + R)|`. The
cross-check sets the feed port's `R = z_ref`, drives that port, and reads `|S[p][p]|` per
angle/frequency.

**Palace counterpart, aperture-side (secondary):** the Floquet-port (0,0)-order
S-parameter. Floquet ports decompose the scattered field into diffraction orders and
report per-order S-parameters in a TE/TM basis with the incident field subtracted on the
driving port (`IsIncidentMode`/`GetAllSParameters`,
`palace/models/floquetportoperator.hpp:114`, `:160-163`;
`MeasureFloquetPorts`, `palace/models/postoperator.cpp:1257-1310`; CSV
`port-floquet-S.csv`, `postoperatorcsv.cpp:1234`, columns like
`S[P1(0,0)TE][1]`). Under TE incidence, `|S[P(0,0)TE]|` is the phased-array active
reflection coefficient for plane-wave incidence at the `k_F`-set scan angle — geode's
aperture-side counterpart — but referenced to the TE/TM modal (Dirichlet-to-Neumann)
impedance and normalized to unit time-averaged power, not to a lumped `z_ref`. Comparing
it against the feed-side `|Γ_active|` therefore also validates the feed/aperture
relationship, not just the number.

If a run mixes drive and observe types, apply the √2 bridge factor of §2
(`postoperator.cpp:1362-1368`); same-type runs need no correction.

## 5. Which version this verdict applies to (AC4)

The verdict above is stated for the **vendored pin** `fba6a5b9d`
(`v0.17.0-99-gfba6a5b9d`). Against upstream tagged releases:

- Floquet periodic BCs (§1) exist since 0.14.0, the current config shape since 0.15.0, and
  driven Floquet ports (§1, §4) since 0.17.0 (`CHANGELOG.md:81-83`, `:308-312`, `:333-335`,
  `:402-404`) — so the capability core holds for any Palace ≥ 0.17.0.
- The mixed lumped+Floquet S-parameter **fix** (§2) is *not* in any tag at pin time — it
  sits under "In progress" (`CHANGELOG.md:19-22`) and is part of what the pin's 99 extra
  commits carry. Tagged 0.17.0 contains the bug it fixes.

Consequence: run the cross-check with the vendored image (`palace:cpu`/`cuda`/`tuned` built
from `reference/palace/docker/`), which has the fix — or use released ≥ 0.17.0 with
same-type drive/observe, which never exercises the fixed path.

## 6. Verdict and the resulting scope for #1581

**Cross-check possible as specified.** Neither a reduced scope nor a missing capability:
every element of the Phase-4 criterion (Bloch-phased periodic faces, lumped feed, scanned
beam, a `|Γ_active|`-comparable observable) exists in Palace at the pin, cited above. The
shape of the future cross-check is: the TCDA unit cell meshed with periodic donor/receiver
pairs, `FloquetWaveVector` set per scan angle (N runs for a scan grid, optionally with
`FloquetReferenceFrequency` to hold the angle across a frequency sweep), a lumped feed
port with `R = z_ref`, and `|S[p][p]|` from `port-S.csv` compared against
`|Γ_active|`; optionally a second, aperture-side series driven by a Floquet port reading
`|S[P(0,0)]|` from `port-floquet-S.csv`. #1581 can be sized as that benchmark run; the
p=2 re-centre half of #1581 is untouched by this note.

## Sources

- Palace source and in-tree docs at `fba6a5b9d17ef09db2907e9c4c6f92d123d131c0`
  (`github.com/awslabs/palace`), cited as `palace/…:line` and `docs/src/…:line` above;
  `docs/src/` files are the sources of the published palace.readthedocs.io pages.
- Pin provenance: `reference/palace/docker/{Dockerfile:47,Dockerfile.cuda:97,Dockerfile.tuned:86}`;
  `reference/fixtures/transmon_palace/results_controls_1123/*/palace.json:56`.
- Geode side: `crates/geode-core/src/driven/floquet.rs:1600-1660`;
  `crates/geode-core/tests/common/tcda_design.rs:16`;
  `crates/geode-core/tests/common/tcda_baseline.rs:339-368`.
- Format precedent: `docs/research/geode-vs-palace-comparison.md` (contains no
  periodic/Floquet/Bloch content; not overlapped by this note).
