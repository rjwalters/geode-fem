# Nested-DOF shape-optimization baseline, 2.4 GHz bent patch (#1087)

The artifacts in this directory are `L{0,1}_<label>.toml`, one per design freedom and level. The generator is `crates/geode-core/examples/patch_conformal_nested_dof.rs`. The tests that pin them are in `crates/geode-core/tests/patch_conformal_nested_dof.rs`. The geometry dumps are in `../geometry/` (see `../geometry/README.md`). Every number here is at UPML σ₀ = 1.

## These results hold only on the mesh level they were optimized on

**Read every worst-of-band value here as a statement about one discretization: L0 or L1 at σ₀ = 1.** None of them describes the converged antenna. #1103 must re-optimize on L2 or finer and verify at p = 2. Until it does, the results are conditional on the mesh level.

### L1 is not converged

- The #1084 baseline already shows a 35 MHz L0 → L1 shift of the initial bent dip.
- The #1085 refinement study (PR #1107, in flight when this was written) shows the initial bent dip still rising with every refinement step: 2.312 GHz on L1, 2.342 GHz on L3, 2.357 GHz with p = 2 on L1. It has not converged by L3.
- Its extrapolations bracket the converged initial dip at about 2.357–2.369 GHz. Where it lands depends on the convergence order assumed.
- L1 sits about 55 MHz below that estimate, and L0 about 95 MHz below it. These offsets are comparable to the 83.5 MHz ISM band width.

### The optimized designs do not transfer across levels

The PR #1104 Judge measured the transfer on an AWS r8g.4xlarge at head `36ee5400`. A throwaway patch applied each committed `design_vector` to a different mesh and ran the 9-point in-band verification sweep. These numbers come from that review and are not in a committed artifact:

| Design | Evaluated on | Worst in-band \|S₁₁\| | Min volume ratio |
|---|---|---|---|
| L0 combined_64 | L0 (its own mesh) | −13.785 dB (equals the artifact) | 0.250 |
| L1 combined_64 | L1′, a sibling L1 mesh at the same resolution | −14.37 dB (holds) | 0.258 |
| L1 combined_64 | L0 | −6.04 dB; the dip moves to about 2.35 GHz | 0.319 |
| L0 combined_64 | L1′ | −5.53 dB | 0.216 (below the 0.25 guard) |
| x = 0 | L1′ | −1.456 dB (pinned L1: −1.47 dB) | 1.0 |

L1′ was regenerated from `reference/gmsh/patch_conformal_L1.yaml` with arm64 gmsh 4.12.1. It has 23,203 nodes against the pinned 23,246 and a different sha256, so the L1 fixture is not bit-reproducible across gmsh versions.

The L1 optimum survives a perturbation of the mesh at the same resolution. It does not survive a change of resolution: about 8 dB is lost in each direction. Between L0 and L1 the optimized dip moves by about 100 MHz, roughly 3× the 35 MHz baseline shift.

**The optimizer is fitting discretization error as well as the antenna.** On each level it tunes the design against that level's own resonance offset. A design matched on an unconverged mesh is therefore mismatched on a finer one.

### What the ladder does show

- **The match needs spatial variation.** A uniform gap change (P = 1) and the 2 × 2 gap (P = 4) stay above −2.5 dB on both levels. A spatially varying gap alone, with no outline motion, reaches −10 dB (P = 16 on L1, P = 48 on both levels). So the data show that the match is not a one-parameter result. They do not show that it needs a freeform outline.
- **The P = 1 negative looks physical, not a guard artifact.** A uniform lift pulls the dip further below the band, to 2.20 GHz or lower.

### Where the distortion guard binds

Every run except L1 outline-only ends `distortion_limited` at the 0.25 minimum volume ratio. The final gradient norm is still about 13 % of the initial one (L1 combined: 3.18e-2 against 2.43e-1). These designs are therefore guard-limited, not stationary points.

The guard binds at the mesh and geometry singularities, which is where discretization error is worst:

- **gap_16 and gap_48:** the worst tet (`worst_tet_final`) is in the air just above the surface, at the rim of the feed cutoff, about 4.2–4.4 mm from the feed.
- **gap_1, gap_4 and combined:** the worst tet is near the patch corners.
- **The feed "crater":** in every run the steepest surface slope (|∇lift| ≈ 0.9–1.4) is at the pinned feed. There the patch rises about 3 mm within 0.8–4 mm of the probe.

The L0 locations were read from the artifacts by the PR #1104 Judge. #1103 should check whether the binding tets and the crater stay put under refinement.

### The substrate thickness is a design variable

The radial DOFs move the patch away from the ground plane, so they change the substrate thickness under the patch:

- **L1 combined:** a mean lift of 2.03 mm takes the FR-4 from 1.6 mm to about 3.6 mm on average. At the pinned probe the thickness stays 1.6 mm, so the feed sits at the bottom of a crater about 3 mm deep.
- **Uniform gap (P = 1):** the lift is 2.9–3.3 mm.

Two consumers need to know this:

- **The Meep matched-geometry cross-check (#1088)** must model a variable-thickness substrate, not a 1.6 mm sheet.
- **Manufacturing** must take the thickness profile as part of the design. The optimized antenna is not a 1.6 mm FR-4 board with a reshaped patch.
