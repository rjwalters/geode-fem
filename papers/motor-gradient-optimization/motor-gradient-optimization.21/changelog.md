# Revision 20 → 21

Input: coauthor Crutcher Dunnavant's review of v18, relayed by the operator (geode-fem#1231),
2026-10-22. It asked for two changes: the "model" terminology and a future-work paragraph. V21 is
v20 with those two changes and nothing else:
- no number, figure datum, table value, reference entry or DOI changed;
- no solver, optimizer or other computation was run;
- v20 and earlier versions are unchanged.

## 1. "Model" terminology

The note: "model" suggests a fittable or trainable object rather than a simulation. The ML-style
pair "training model" / "validation model" is renamed, along with "training" used as an adjective.
Nothing in this study is trained.

| v20 | v21 |
|---|---|
| training model (L1 or L0 mesh, interface order 45) | **coarse simulation** |
| validation model (L2 mesh, order 93, 20 angles) | **validation simulation** |
| training forward(s) / evaluation(s) / objective / outputs / values / gradients | coarse forward(s) / evaluation(s) / objective / outputs / values / gradients |
| training resolution | coarse resolution |
| Table `tab:methods` column "Training mesh" | "Optimization mesh". "Coarse" would be ambiguous here: the text calls L0 "the coarse mesh" in the E/A and D/B contrasts. |
| training currents (the objective's three currents, against the held-out 0.25 and 0.75) | objective currents |
| "trains on order-45 coupling torque and validates on order 93" (twice) | "'s coarse simulation uses order-45 coupling torque and its validation simulation order 93" |
| $F_{\mathrm{train}}$, $\mu_{\mathrm{train}}$ (Appendix B) | $F_r$, $\mu_{r,1}$, the notation the protocol already defines for $r\in\{\mathrm{L0},\mathrm{L1}\}$ |
| "the model" in the cost-estimate appendix paragraph (4 uses) | "cost model", since this one is a predictive model |

**Definition:** both terms are defined once, in the protocol paragraph, now titled "Coarse and
validation simulations." (it was "Training and validation models."). The paragraph gives each one's
mesh level and interface order and says the optimizer never sees the validation simulation. It
replaces the v20 sentences "Training here means querying the cheap model, not fitting: apart from
the surrogate of method C, nothing is fitted to data." with two sentences: "Unqualified, *model*
means a formulation of the simulation, such as the field or interface model; no simulation is trained
or fitted to torque data. Of the models the optimizers use, only their own search models are fitted,
to coarse values: C's radial-basis surrogate and COBYQA's interpolation model."

The first draft of this definition (the first v21 commit) read "nothing is trained, and no simulation
is fitted to data". The Judge (PR #1242) found that it overclaimed and dropped v20's caveat about
method C, so it was rewritten. The new wording is scoped so that it is literally true for the whole
paper:
- "Unqualified" leaves out the qualified uses: the cost model, the search models, and the
  magnetic-circuit and reduced models.
- "Of the models the optimizers use" leaves out the fits that are not the optimizers' own: the
  appendix cost model, the slotless angular fit's three coefficients, and the derivative-check fits.
- "torque data" leaves out the interpolation of the tabulated M27 B–H law, which is a material
  input; no simulation is fitted to torques.

The abstract and introduction describe the pair (coarse, finer,
never seen) but don't define it, because L0–L2 and the interface order are introduced only in
Section 3.

**Counts:** 62 edit sites and 64 string replacements, all listed in `V21_EDITS` in `verify_exp2b.py`.
At the word level:
- 69 tokens of the "train" family were removed: 38 in `main.tex`, including 5 `\mathrm{train}`
  subscripts and 2 uses of "trains", and 31 in `optimization.tex`. The only one left is the new
  "no simulation is trained".
- 20 uses of "model(s)" were renamed (11 in `main.tex`, 9 in `optimization.tex`). The new definition
  sentences add 5 uses ("*model*", "interface model", "models the optimizers use", "search models",
  "interpolation model").
- 4 uses of "model" were qualified as "cost model".
- 3 uses of "cheap"/"cheaper" that were attached to the renamed model were dropped.

**Judgment calls:**
- Abstract: "query a cheap training model" became "query a coarse simulation". "Cheap" was dropped;
  "finer" carries the contrast. The abstract has 248 words (v20: 249; the cap is 250).
- Introduction: "on one validation model finer than any training model" became "on one validation
  simulation, finer than any coarse simulation an optimizer queries".
- Discussion: "D is a coarse finite-element model, not such a baseline" became "D searches a coarse
  finite-element simulation, not such a baseline". This "model" could be read as a numerics use, but
  the sentence contrasts D with fitted reduced models, which is exactly the ambiguity the note
  raises.
- Appendix C: "at the study's training tolerance" became "at the coarse simulation's Newton
  tolerance".
- Section 5: "training time per training forward" became "coarse-simulation time per coarse
  forward".
- Appendix E: "this coarse-model explanation" (the coarse saved reference) became "this
  coarse-reference explanation", so that it doesn't collide with the newly defined term.
- Cost table column "Model" (entries such as "L2, order 93") became "Simulation".
- Table `tab:methods`: "Optimization mesh" is wider than "Training mesh" and made the table 0.83 pt
  overfull, so the table sets `\tabcolsep` to 5pt. The column "Optimizer or search model" is kept,
  because C's surrogate is a fitted search model.
- Kept, as physics, numerics or genuinely fitted uses: field model; harmonic, conforming and
  interface models; model–estimator pairs; "survives a better model: the adjoint differentiates the
  chosen discrete model" (introduction); Motor model and Forward-model controls (section titles);
  isogeometric reference model; magnetic-circuit, reduced and waveform models (Discussion and
  `tab:methods`); COBYQA's "first model step" (its interpolation model is fitted); the slotless
  model's "three fitted coefficients".

**Figures:** two rendered legends used the old word, so their sources are relabelled and re-rendered
with the README's pinned toolchain (`uv run --with numpy==2.5.4 --with matplotlib==3.11.2`). Before the
edit, the same command reproduced v20's committed PDFs and PNGs byte for byte, and every table.
- `render_exp2.py`: "open: L0 training" → "open: L0 coarse simulation" (Fig. `exp2-time-to-target`;
  its caption now reads "Filled markers: L1 coarse simulation; open markers: L0").
- `render_decomp.py`: "training forwards and other work" → "coarse forwards and other work" (Fig.
  `decomp`), plus two docstring words.

No other figure, and no table, changed bytes.

## 2. Future work

The note: cheap gradients open up higher-order and parallel optimization methods (Hessian estimates,
particle swarms). A second note: synchronous reluctance motors. One paragraph now closes the
Discussion, after the limits paragraph and before the Conclusion:

> Cheap, verified gradients also make methods affordable that this study did not test.
> Second-order information is one: Hessian-vector products through a second-order adjoint, or
> quasi-Newton methods driven by exact gradients beyond the BFGS update inside SLSQP.
> Gradient-enhanced population and multistart searches are another, such as particle swarms or
> CMA-style searches seeded or steered by adjoint gradients, with runs in parallel across starts;
> every run here used one single-threaded core. Each still pays for validation and coarse forwards,
> which already bound the end-to-end gain (Fig. 6), and can gain beyond that only by needing fewer of
> them or, in wall time but not in compute, by running them in parallel. A natural next problem for
> the same pipeline is the rotor flux-barrier design of synchronous reluctance machines, a classic
> shape-optimization target because their torque ripple is high; gradient-based free-form shape
> optimization has already been applied to such a machine (Gangl et al., 2022).

The citation is `gangl2022multi`, which the related work already cites. `refs.bib` is unchanged and
`main.bbl` is byte-identical to v20's. The limits paragraph's "CMA-ES and other population or
multi-start methods remain untested" is kept.

## Package

- Data statement: the package link and its display text name `.21`.
- `README.md`:
  - heading and State line name v21;
  - package link and archive paragraph name `.21`;
  - the prose's "training" uses follow the new terms. Record keys and file names are unchanged.
- `../.anvil.json`: `artifact_verify.cwd` → `motor-gradient-optimization.21`.
- `_progress.json`: iteration 21, revised from 20.
- `compile-log.txt` is carried over unchanged, as in v19 and v20.

**Thread metadata:**
- `BRIEF.md` CURRENT and `status_of_inputs` name v21.
- `EXPERIMENTS.md`'s archive line cites v21's data statement.
- The row in `papers/README.md` names v21.

## Verifiers

Pins on old wording now assert the new wording; none was dropped or weakened.
- `verify_exp2.py`:
  - "at the objective currents";
  - package link `.21`;
  - its PASS messages and comments say "coarse" (Judge, non-blocking). The record keys
    `paired_training_F`, `train_level` and `session['training']` are the records' own names and are
    unchanged.
- `verify_exp2b.py`:
  - "one coarse evaluation fewer";
  - "every coarse evaluation costs a coarse forward … coarse-simulation time per coarse forward";
  - "like every coarse forward in the preregistered coarse configuration";
  - `PKG` → `.21`, and no link to `.1`–`.20` may remain.
- `verify_exp2c.py`:
  - "validation and coarse forwards bound/now bound";
  - "cold coarse evaluation";
  - "base coarse forwards";
  - the coarse-forward medians;
  - "Their cost model counts only coarse forwards", "the cost model's 0.27 to 0.31" and "differ from
    their cost model by";
  - the cost-table row "Coarse forward & L1, order 45";
  - `PKG` → `.21`.

**New v21 check** (`verify_exp2b.py`, alongside the v20 one):
- no "train" token remains apart from "no simulation is trained";
- "training model", "validation model", "cheap model", "Training mesh" and "coarse-model" are absent;
- both definition sentences are present;
- `gangl2022multi` is cited exactly twice.

Inside the repository, the check also undoes the 64 listed v21 edits (62 terminology sites, the
future-work paragraph and the package pointer). The result must equal v20's `main.tex` (pinned by
SHA-256) and `optimization.tex` exactly. The v20 check then runs on that recovered v20 text and must
give back v19's `main.tex`, as before. A package downloaded on its own prints a NOTE and skips both
comparisons.

**Mutation check:** run in a scratch copy of the thread, then reverted. A one-character edit to the
future-work paragraph, and one to the abstract, each make `verify_exp2b.py` fail.

## Results

- **Build:** `papers/build.sh`:
  - 43 pages (v20: 42). The main text, through the data statement, now ends on p. 19 (v20: p. 18),
    because the new paragraph pushes the end of the data statement onto p. 19, where Appendix A
    still starts.
  - 0 overfull boxes, and no undefined references or citations.
  - `main.bbl` is byte-identical to v20's.
- **Verifiers (extract mode, `python3.14 -I -B`):**
  - `verify.py`: 23 PASS (v20: 22; the new line is the v21 check), exit 0. It chains the other three.
  - `verify_exp2.py`, `verify_exp2b.py` and `verify_exp2c.py` each exit 0 when run on their own.
  - A lone copy of the package also passes, with the two NOTEs.
- **Lints:** no `[PENDING` marker.
