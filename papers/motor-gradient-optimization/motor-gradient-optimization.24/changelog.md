# Changelog: motor-gradient-optimization.23 → .24

Input: geode-fem#1231, the v2 list after the Zenodo v1 release (motor items). `.23` is the published text and is not edited.

- Validation-anchor symbol: the full-current mean of the initial design was written $\mu_0$ ("Common validation and credit"), the symbol of vacuum permeability in Appendix A. It is now $\bar T_{r,1}(\theta_0)$, the paper's own symbol for that mean, in both places (`optimization.tex`). The value 1.44084 N m is unchanged.
- SLSQP's BFGS curvature estimate joins the sentence naming the search models that are fitted to coarse values (C's radial-basis surrogate, COBYQA's interpolation model).
- Abstract: "all 45 of the 50 paired comparisons where both succeeded" now reads "in all 45 start-target pairs where both reached the target (5 of 50 are censored)" (nit 2 of the #1229 review). The abstract also drops "and only validated scores count" (the credit rule in Common validation and credit says it) and "single-threaded", to stay within the 250-word cap the verifier enforces.
- Factor sets (nit 1): the abstract's "1.9 to 2.9 with 18" is a four-start figure, beside the five-start 1.71 and 2.70. The abstract now says "(four starts)", and the results paragraph that gives 2.68--4.10 and 1.91--2.92 says they cover the starts where COBYQA reaches the primary target (five in S8, four in S18) and that the abstract's 2.70 and 4.11 are the five-start S18 values. No number changed.
- Verifiers: `verify_exp2*.py` assert the new wording and accept the `.23` corresponding-author footnote (they failed on `.23`); `verify_exp2b.py` and `verify_exp2c.py` accept a later revision directory beside the evidence package `.22` that the data statement links, and `verify_exp2c.py` checks `_progress.json` against the directory's own number.

Already fixed in `.23`, so not changed: the $\mu_{r,1}$ versus $\mu_r = 1.05$ clash (the paper writes $\bar T_{r,1}$ for the mean) and the `verify_exp2b.py` success message that named only the zenodo-doi.

Deferred: the Fig. 3 "budget" label clipping the top frame (`figures/src/render_exp2.py`). The figure is hash-bound in `evidence-manifest.json`, and the change could not be checked in a built PDF on this host.

No number, claim, title, author, affiliation, reference, table or DOI changed; no computation run.

Not built here: this host has no TeX toolchain, so `main.pdf` is absent from this directory. Build with `papers/build.sh` before the Zenodo re-upload. `verify.py` asserts `main.pdf` and passes through its other checks only up to that point.
