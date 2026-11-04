# Nested-DOF shape-optimization baseline, 2.4 GHz bent patch (#1087)

The artifacts in this directory are `L{0,1}_<label>.toml`, one per design freedom and level. The generator is `crates/geode-core/examples/patch_conformal_nested_dof.rs`. The tests that pin them are in `crates/geode-core/tests/patch_conformal_nested_dof.rs`. The geometry dumps are in `../geometry/` (see `../geometry/README.md`). Every number here is at UPML σ₀ = 1.

## These results hold only on the mesh level they were optimized on

**Read every worst-of-band value here as a statement about one discretization: L0 or L1 at σ₀ = 1.** None of them describes the converged antenna. #1103 must re-optimize on L2 or finer and verify at p = 2. Until it does, the results are conditional on the mesh level.

### L1 is not converged

- The #1084 baseline already shows a 35 MHz L0 → L1 shift of the initial bent dip.
- The #1085 refinement study (PR #1107, in flight when this was written) shows the initial bent dip still rising with every refinement step: 2.312 GHz on L1, 2.342 GHz on L3, 2.357 GHz with p = 2 on L1. It has not converged by L3.
- Because the dip rises under every h- and p-refinement, the converged initial bent dip is at least 2.3566 GHz. That is at most 43 MHz below the ISM band. The upper end is undetermined: the local convergence order falls along the ladder (1.18, then 0.50), and the extrapolations span about 2.37–2.43 GHz. The finest-triple estimate lies in band, so once converged the initial antenna may already resonate in band.
- L1 sits at least 45 MHz below the converged dip, and L0 at least 85 MHz below it. The detune the optimizer has to correct may therefore be much smaller than L1 suggests. On L1 it looks like about 88 MHz, while the real gap is at most 43 MHz and may be zero.

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

## Gate G2: re-optimization on L2, verification on L3 and with p = 2 on L1 (#1103)

> **Idealized non-uniform-thickness ablation, not a manufacturable design.** Every design in this section uses radial (gap) DOFs, which thicken the FR-4 under the patch from 1.6 mm to 3.7 mm on average (up to 4.7–5.5 mm), with the pinned feed left at the bottom of a 1.6 mm crater. Operator ruling (2026-10-13): the antenna must keep a standard, uniform 1.6 mm board, and only the etched copper may be optimized. The manufacturable fixed-thickness design (outline, feed inset, in-plane slots) is #1120. No L2 outline-only run was made here; #1120 covers it.

The artifacts are `L2_<label>.toml` (the L2 optimizations), `verify/<design>_on_<level>[_p2].toml` (fresh forward sweeps of a design carried onto another fixture, written by the example's `--verify-only` mode), the geometry dumps `../geometry/L2_*.json`, and the logs `../logs/nested_dof_L2_*.log` and `../logs/nested_dof_verify/`. `python3 ../scripts/nested_dof_g2.py` prints the tables below. The tests are the `g2_*` functions in `crates/geode-core/tests/patch_conformal_nested_dof.rs`. Every run is at UPML σ₀ = 1, the value #1085 recommends (its residual is |ΔS₁₁| ≤ 0.021), so no second σ₀ was run.

The verification closest to the converged antenna is p = 2 on L1 ("P2/L1"; 804,460 DOFs). #1085 puts its initial dip (2.3566 GHz) above L3's (2.3422 GHz) and closest to the converged value.

### The converged initial patch is far from −10 dB

| level | order | worst of 9 in-band points (dB) | best (dB, at 2.40 GHz) |
|---|---|---|---|
| L1 | p1 | −1.47 | −3.77 |
| L2 | p1 | −1.75 | −4.97 |
| L3 | p1 | −2.04 | −6.29 |
| L1 | p2 | −2.42 | −8.34 |

The in-band match improves with refinement, as #1085 found, but even at P2/L1 the worst in-band point is −2.4 dB. The optimization problem is real at every fidelity. These points reproduce the #1085 convergence runs at 2.40 and 2.4835 GHz exactly.

### The L2 optimizations

| run | start | worst initial (dB) | worst final, 9-pt (dB) | wide-sweep dip | steps / terminal | wall |
|---|---|---|---|---|---|---|
| combined_64_cold (48 radial + 16 outline) | x = 0 | −1.75 | **−14.07** | 2.425 GHz | 24 / distortion_limited | 4.6 h |
| combined_64_warm | L1 combined optimum | −9.85 | **−15.87** | 2.450 GHz | 15 / distortion_limited | 3.6 h |
| gap_48_warm (48 radial) | L1 gap_48 optimum × 0.5 | −9.67 | **−13.29** | 2.425 GHz | 20 / distortion_limited | 4.0 h |

The L1 gap_48 optimum violates the volume guard on L2 (0.178 < 0.25), so its warm start is halved (0.597). The FD gate passes at `x0` and `x_final` on every run (worst relative error 5.5e-6). All three runs end `distortion_limited`. The cold run's final |grad| is 9.5 % of its initial value; the warm combined one is at 1.6 %.

### Verification and transfer (worst of the 9 in-band points, dB)

| design | own level | L0 | L1 | L2 | L3 | P2/L1 | loss to P2/L1 |
|---|---|---|---|---|---|---|---|
| L1 combined_64 (#1087) | L1 | −6.04 | **−14.12** | −9.85 | −7.57 | −5.69 | 8.43 dB |
| L2 combined_64 cold | L2 | | | **−14.07** | −12.47 | **−9.16** | 4.91 dB |
| L2 combined_64 warm | L2 | | | **−15.87** | −11.48 | −8.26 | 7.61 dB |

- **The L1 optimum degrades monotonically with fidelity:** −14.12 → −9.85 → −7.57 → −5.69 dB from L1 to P2/L1. The #1087 "about 8 dB" (L0 ↔ L1) is confirmed against the converged side: 8.4 dB to P2/L1.
- **The L2 optimum transfers about twice as well:** 1.6 dB lost to L3 and 4.9 dB to P2/L1. It still loses, so the optimizer is still fitting L2's discretization error.
- **The warm start carries the L1 bias.** The warm-started combined optimum is the best design on L2 (−15.87 dB) but transfers worse than the cold one: −11.48 dB on L3 and −8.26 dB on P2/L1, a 7.6 dB loss. Starting from the L1 optimum keeps it near a design that fits L1's discretization error. The cold start, the clean L2 optimum, is the gate G2 design.
- **Where the residual comes from.** The L2 optimum's resonance rises with fidelity: 2.425 GHz on L2, 2.450 on L3, 2.500 on P2/L1. That is the same upward drift as the initial dip under refinement (2.329 → 2.357 GHz from L2 to P2/L1, #1085), but about 2.7× larger on the optimized, thicker board. On P2/L1 the misses are at the band bottom: −9.16 dB at 2.40 GHz and −9.999 dB at 2.4104 GHz. From 2.4209 GHz up every point is −10.9 dB or better.

### Gate G2: not met on the finest verification

**The cold-start L2 combined optimum reaches −14.07 dB on L2 and holds on L3 at −12.47 dB, but with p = 2 on L1 it is −9.16 dB. That misses −10 dB by 0.84 dB at 2.40 GHz.** Read this as a statement about the idealized non-uniform-thickness ablation. The trend (loss to P2/L1: 8.4 dB from L1, 4.9 dB from L2) suggests that optimizing on a finer level, or with a margin at the band bottom, would meet it. That was not run, because the design freedom is not manufacturable (#1120).

### Where the distortion guard binds, and whether it stays put

- **Combined designs:** the binding tets are in the air just outside the ±s (radiating) edges, 1.7–3.1 mm outside the patch. For the L1 optimum this is (s, y) = (−22.1, +0.2) mm on L1 and (−21.4, −3.9) mm on L2. On L3 the same surface's worst tet jumps to the feed-cutoff rim, 3.7 mm from the feed, with ratio 0.227, below the guard. **The binding site does not stay put under refinement:** it is a property of each mesh's tets, not of the surface.
- **Gap-only (gap_48):** the guard binds at the feed-cutoff rim, 3.9 mm from the feed. That is where #1087 found it on L0/L1 (4.2–4.4 mm), so this site does stay put.
- **The crater is a surface property and does stay put.** The same L1 coefficients give the same steepest slope on L1, L2 and L3 (|∇lift| = 0.91–0.92 at 2.4–2.7 mm from the feed) and the same mean thickness (3.63–3.65 mm). The L2 cold optimum has a gentler crater (0.70 at 2.6 mm) and a thicker board (3.73 mm mean, 4.70 mm max). gap_48 has the steepest crater (1.59 at 2.4 mm, 5.49 mm max thickness).

### Cost

AWS: pool1 (r8g.4xlarge) ran the two warm optimizations concurrently, with 8 threads each. g2v (r8g.8xlarge) ran the cold optimization and every verification. An L2 band gradient takes about 455 s at 8 threads. A P2/L1 solve takes about 490 s at 16 threads with 132 GiB peak RSS, and an L3 solve about 5 min at 12 threads with 70 GiB. pool1 was stopped externally at 22:19Z mid-run and both warm runs were restarted from scratch. They are deterministic, and the restarted runs reproduced the killed runs' trajectories step for step.
