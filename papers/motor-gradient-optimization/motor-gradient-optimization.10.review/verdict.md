# motor-gradient-optimization.10 — review verdict

Fresh, independent re-scores of v10 at `c08ce9e6` (origin/main). The v7 scores (Opus 37/44, Fable 37/44) do not carry over. The full reports are archived verbatim beside this index. Reviewers ran every build and verify step in a scratch copy; nothing in the repo was modified.

| Reviewer | Total | Advance | Critical flags |
|---|---|---|---|
| Opus | **35/44** | true | none |
| Fable | **35/44** | true | none |

Rubric: installed `.anvil/skills/paper/rubric.md`, /44, advance threshold ≥35. `verify.py` and `verify_exp2.py` exit 0 in both reviewers' copies.

| # | Dimension | Opus | Fable |
|---|---|---|---|
| 1 | Rigor | 5/6 | 5/6 |
| 2 | Evidence | 4/6 | 5/6 |
| 3 | Clarity of contribution | 4/5 | 4/5 |
| 4 | Related work | 4/5 | 4/5 |
| 5 | Reproducibility | 4/5 | 4/5 |
| 6 | Figures & tables | 4/4 | 3/4 |
| 7 | Prose & structure | 3/4 | 3/4 |
| 8 | Citation hygiene | 4/5 | 4/5 |
| 9 | Rhetorical economy | 3/4 | 3/4 |

## Required findings

- **Both: title and conclusion credit the gradients** (optimization.tex L22–24). No SLSQP arm uses finite-difference gradients. Retitle, or add an FD-SLSQP arm or a cost estimate built from the runs' gradient counts.
- **Opus R2: validation charges dominate charged time.** Report validation counts and their share of charged time. Qualify the space-A D/A sentence (optimization.tex L176–178) and "D does not close that gap" (main.tex L794–795). Opus's rough estimate puts that contrast at 0.82–1.42 once validation is removed.
- **Opus R3: the space-B grouped-offset gradients are unverified in the paper.** main.tex L612–614 says they are not included. Cite `benchmarks/motor_optimization/runs/fd_L1_space_{a,b}.json`.
- **Opus R4: stale pre-§7 sentences** at main.tex L636–637 and L757–759.
- **Fable R2: method C is underspecified** (optimization.tex L19–21).
- **Fable R3: the shape-freedom dependence is buried.** Every large ratio is in space B; say so in the abstract.
- **Fable R4: the conclusion restates the abstract.**

Both reviewers give literature leads for `paper-litsearch`: Gangl et al. 2015, plus gradient vs gradient-free shape-optimization benchmarks. Both also note the uncited Gmsh/GeoPDEs. Opus flags the length (26 pages) and the headline first appearing on page 15.
