# Motor paper, revision 5

State: **REVISED; fresh independent reviews and audit pending.** Author: Robb Walters. No matched optimization or time-to-target result is claimed.

This revision addresses the v4 reviews at `fea6ba5b` (Opus 38/44, Fable 36/44, both blocked). The missing committed evidence log was a packaging error: the v4 local verification did not establish clean-checkout reproducibility. V4 is preserved as reviewed; its original reports are archived verbatim in `.4.review/`. V5 carries and explicitly tracks the required log. Its complete package is checked from a Git archive before handoff.

## Rebuild

From this directory:

```sh
uv run --with numpy==2.5.4 --with matplotlib==3.11.2 --with meshio==5.3.5 python figures/src/render_v5.py
bash ../../build.sh .
python3 -I -B verify.py
```

The local class and committed `main.bbl` make the paper standalone. The renderer produces six manuscript figures, three manuscript tables, inherited supplements, and `figures/opt120-waveforms.pdf`. The new waveform supplement compares h45/h93 coupling torque and conforming outer-band Arkkio on the fresh L3 Opt120 meshes at fixed unscaled full current over 40 angles. Its left panel preserves each mean; its right panel subtracts each curve's own 40-angle mean to reveal ripple. It is not a reference-current reproduction comparison.

The stdlib verifier independently reconstructs the numerical claims from raw arrays, without importing renderer helpers. It checks 115 input hashes, original forward/gradient controls, 704 distinct FD cases, 592 Taylor windows, 3,240 candidate solve statuses, reference refinement, and the new size normalizations, matched-current ratios, refinement increments and paired-estimator comparisons. This does not rerun FEM or replace independent scientific review.

## Frozen evidence

`evidence-manifest.json` records each copied input's source path, full source commit and SHA-256. Snapshots match their **pinned source commits**, not necessarily a later main branch. In particular, the harmonic-study README predates a subsequent update note.

- Original evidence: `581ee7ad40c9646f43e0d6df3de7b5a87d06f798` and `910593bad6f64f9f01671c2632b57034e72b9c61`.
- Candidate validation: `224332c526be1191183af82b610973c7778de720`. V5 adds the L1 fresh-export h45 record and alternate fresh-construction L2/L3 h45 records to the prior L2/L3, current-policy and angular controls. One invalid moved-mesh record is retained.
- Independent reference refinement: `8acd21ecf85d89fe077275a3c04ca70b53963d43`.

The upstream source is ICEMsnapshot `b145aaf4ab1a6424828bc941eaf172f48fa0d458`; its license and attribution are preserved. Reference runs use Octave 11.3.0, GeoPDEs `53ae5ca1ce16fe81057047831cecbaa859c49080`, and nurbs 1.4.3. The source benchmark READMEs provide mesh and solver commands and environment details.

## Interpretation

The 0.10% cogging agreement is between finite-resolution results, with larger remaining refinement changes; it is not an accuracy bound. Opt120's ripple reduction survives, but its magnitude changes with interface order. Its mean-torque comparison changes with both motor size and current policy. The 1.4971 N m normalized mean removes both explicit size factors; length-only normalization gives 1.6434 N m. The matched-reference-current objective reduction is 10,170-fold in GEODE versus 11,982-fold in the saved reference; 7,631-fold instead holds unscaled current fixed. Reference refinement at Opt120 remains unperformed.

The prospective protocol follows #1185 amendments 1–6 and the amendment-6 addendum. The baseline is an empirical RBF surrogate with a specified linear current interpolation; an inexpensive physical model remains an open comparison. No smoke-run result enters this paper. See `changelog.md` for every review response and `../EXPERIMENTS.md` for the experiment handoff.
