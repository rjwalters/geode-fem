# ICEM motor reference data

This is preparation for milestone M2 of the motor optimization paper. It
pins the published model's numerical inputs and stored outputs. It does
not implement or validate a GEODE motor solve.

- `sources.json`: immutable upstream commit, source URLs, byte counts and SHA-256 hashes.
- `reference_data.json`: extracted M27 B-H points, geometry defaults and
  optimization bounds, winding groups, saved initial/final currents and
  torque profiles, with their population statistics.
- `conventions.json`: units, sector normalization, phase/angle conventions,
  boundary identifiers, material interpretation and reproduction caveats.
- `extract_reference.py`: source-hash verification and deterministic extraction.
- [`geometry/`](geometry/README.md): initial-design NURBS export converted to a tagged
  gmsh 60-degree sector mesh family (issue #1162). It holds the tag table, exact areas and mesh stats.
- [`forward/`](forward/README.md): the GEODE forward reproduction of the initial
  design's torque waveforms, with L0 to L3 convergence and sensitivity (issue #1168).
  It is the reproduction gate. Mean torque agrees to 0.16%. Ripple does not
  agree: it is 10% to 11% higher under load and 29% lower for cogging.
- [`forward_harmonic/`](forward_harmonic/README.md): the same study with the
  reference's harmonic-truncated rotor-stator coupling, orders n = 3:6:45
  (issue #1170). With it, the L3 load ripple is within 1.3% to 1.7% of the
  reference. Cogging matches in shape but is 13% larger in amplitude.

The source is the authors' [ICEM snapshot at b145aaf4](https://github.com/MichaelWiesheu/ICEMsnapshot/tree/b145aaf4ab1a6424828bc941eaf172f48fa0d458),
associated with [Wiesheu et al., arXiv:2402.19065](https://arxiv.org/abs/2402.19065).
The snapshot includes initial and final multipatch NURBS exports. These
are pinned in the manifest; converting them to a GEODE mesh remains work
for M2. Shape-control displacements cannot be recovered from scalar
dimensions alone.

## Reproduce the extraction

From the repository root:

```sh
uv run --with scipy==1.18.1 python benchmarks/motor_reference/extract_reference.py --fetch --check
```

The first run downloads approximately 55 MB, mostly the stored optimization
history, into ignored `_upstream/`. Existing files must match their hashes;
the extractor never executes MATLAB code. To regenerate the JSON after
an intentional extractor change, omit `--check`. SciPy reads the MAT files;
MATLAB itself is not required for this extraction.

Checks include all source hashes, the 47-point monotone H(B) table, binding
history parameters to source registration order using initial values and
bounds, current scaling, angle grids and agreement between the rounded CSV
and full-precision MAT torque. These are data-integrity checks, not a new
physics simulation.

## Reproduction traps resolved here

The optimizer's initial slot depth and corner radius are both 1 mm; the
geometry function defaults differ. Its saved initial peak current is
9.677657374 A, despite a 10 A setup value. The optimized saved profile uses
11.575697538 A and a different load angle. Reproduce each profile with its
own geometry and excitation before comparing design quality.

The model is a 60-degree sector with six slots. Its torque includes axial
length and no full-machine multiplier. The 20-degree saved waveform period
is distinct from the geometric sector and from the electrical period.

M27 uses PCHIP H(B), not a monotone reluctivity law. H/B decreases at low
field. For a consistent magnetic tangent, require positive transverse and
longitudinal eigenvalues, not a positive derivative of reluctivity. The
upstream zero-field clamp and high-field extrapolation need explicit
treatment in the GEODE implementation.

## Source attribution

Upstream credits Michael Wiesheu, Theodor Komann, Melina Merkel, Sebastian
Schöps, Stefan Ulbrich and Idoia Cortes Garcia. Its README declares
GPL-3.0-or-later; the upstream license text is retained as
`UPSTREAM_LICENSE.txt` for the extracted upstream data. Upstream source
files are retrieved into the ignored cache with their original contents.
No upstream MATLAB solver implementation is vendored into GEODE's Rust
source. The extraction script is independently written for this repository.
