# Derived payloads stay outside Git

The native archives retain requests, results, CAD plans, source/build/process
receipts and mesh reports. Each `omitted-payloads.json` records the original
path, SHA-256 and byte count of omitted compiled executables or meshes. Original
result and manifest records are unchanged; an archive with omitted bytes is
not a complete runtime artifact. The synthetic capture likewise requires mesh
hydration before its full capture replay. The 38-byte synthetic build fixture
is text, not a compiled executable or numerical evidence.

Regenerate on **macOS arm64** with the pinned Gmsh wheel. The four historical
meshes come from the exact source commit
`63b3e3d8bbc30504f845eebb5b78e8be6a2ee12f`:

```sh
export GEODE_REFINEMENT_MESH_CACHE=/path/outside/git/new-mesh-cache
uv run --with numpy==2.5.4 --with gmsh==4.15.2 python -I \
  benchmarks/motor_optimization/final_refinement/smoke/regenerate_meshes.py \
  --out "$GEODE_REFINEMENT_MESH_CACHE"
```

This extracts the pinned geometry sources into a temporary directory. It reads
or downloads only the two hash-pinned upstream NURBS exports and executes
`build_sector_mesh.py --levels 2 3` and `build_design_mesh.py --levels 2`
with the archived conforming and scalar/offset repair plans. Each resulting
file must match its original size and SHA-256; a platform difference fails,
never silently updates a golden hash. Linux Gmsh 4.15.2 was observed to produce
a different triangulation (L2 36,069 rather than 36,083 nodes), so it cannot
regenerate these exact historical bytes. CI regenerates on macOS arm64, passes
the digest-named files as a one-day Actions artifact, and runs the full stdlib
replay suite on Linux. No platform-specific digest is substituted.
The regeneration receipt records the operating system, architecture and Python
version. The generator refuses other platforms before creating output; replay
on Linux uses the verified cache without invoking Gmsh.
No optimizer or FEM solve runs. Tests require this cache and copy bytes only
after hash verification; no missing-evidence tests are skipped.

The native-result archives are digest-only until hydrated. Test code uses
`mesh_test_support.hydrate(extracted_archive)`; the synthetic capture uses the
same helper at its archive root. Production preparation instead requires an
explicit `--l3-mesh` and verifies its bytes against the reviewed construction
report. Production capture/result verifiers still require actual mesh bytes.

For paired executables, retain the historical `1c6784e7e1fc8fe0564b1d793a6c4061588028f5`
source pin and build receipts. On a clean isolated checkout of that commit,
run its `refinement_build.py --repo "$PWD" --reviewed-commit "$COMMIT" --out
"$NEW_EXTERNAL_BUILD"` using isolated Python from that checkout, as described
in the parent README. A rebuild has its own receipts and digests: compiler or
platform changes need not reproduce the old Mach-O bytes. `replay.py` in the
archive checks historical digest/source/process-record consistency and
explicitly reports `binary_bytes_verified: false`; it cannot authorize launch.
The production build/runtime verifier is unchanged and requires executable bytes.

Historical evidence JSON documents keep their original hashes and test counts
as revision-specific records. The current archive hashes are pinned in the
corresponding regression tests. No history rewrite is implied by this cleanup.
