# Pilot figure layout review

This preview uses only the archived 36-run development-host pilot at Git commit `43db1c626bdc2367f896905d7807979789ee22b3`. The host was not idle. It is not a main-study result and establishes no optimization speedup. No finalized manuscript is changed.

The plot separates the three design starts into rows and the two design spaces into columns. Colors identify the methods; solid and dashed lines show the two timing repeats. Those repeats are not independent designs. Every trace is retained, including a run that reached an earlier target and later failed. A cross marks failure, a vertical tick a budget stop, and a star T99 attainment. Curves are steps and end at the actual stop cost, never extrapolated to the full budget. Runs with no feasible validation are counted in their panel rather than silently omitted.

`render.py` reads the immutable Git blobs and recomputes the validation error margin, mean feasibility, target flags and first target times. `verified-plot-data.json` records all source hashes and the exact plotted points. The pilot's feasibility rule is the one it actually used; the prospective main study adds the adopted geometric admissibility requirement.

Regenerate from the repository with the same plotting versions as v6:

```sh
uv run --with numpy==2.5.4 --with matplotlib==3.11.2 python papers/motor-gradient-optimization/refs/pilot-figure-preview/render.py
```

The main-study figure must use its own frozen data and checks, include the L0 methods and the 60/120-FE views, and preserve failed or censored outcomes. This preview supplies a readable layout, not substitute evidence.
