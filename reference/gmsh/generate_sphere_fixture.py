#!/usr/bin/env python3
"""Generate + validate the sphere-in-vacuum FEM mesh fixtures (issue #215).

Offline generation wrapper around ``mesh_scripts/sphere.geo`` following
the ``generate_spiral_fixture.py`` pattern (issue #210 / PR #217): gmsh
is NOT a CI dependency — the generated ``.msh`` is committed as a test
fixture and this script records provenance (gmsh version, script,
parameters, quality metrics) next to it.

Pipeline:
  1. invoke the ``gmsh`` CLI on ``mesh_scripts/sphere.geo`` with
     ``-setnumber`` overrides for the two characteristic lengths,
  2. parse the MSH 4.1 ASCII output (stdlib only, shared with the
     spiral wrapper) and run mesh-quality sanity checks:
       - every tet has positive signed volume (no inverted tets),
       - minimum dihedral angle is reported (and gated),
       - unique-edge count stays within the direct-sparse-LU budget,
       - all expected physical groups are present and populated,
  3. write a ``<output>.provenance.txt`` next to the mesh.

Usage:
  # The fine driven-benchmark fixture (issue #215):
  python3 reference/gmsh/generate_sphere_fixture.py \
      --lc-sphere 0.13 --lc-buffer 0.23 \
      crates/geode-core/tests/fixtures/sphere_fine.msh

  # The bundled coarse fixture (defaults match mesh_scripts/sphere.geo):
  python3 reference/gmsh/generate_sphere_fixture.py \
      crates/geode-core/tests/fixtures/sphere.msh

Exit status is non-zero if generation or any quality gate fails.
"""

from __future__ import annotations

import argparse
import hashlib
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

# Shared MSH parsing + tet-quality helpers (same directory).
from generate_spiral_fixture import parse_msh, quality_report

SCRIPT_DIR = Path(__file__).resolve().parent
GEO_SCRIPT = SCRIPT_DIR.parent.parent / "mesh_scripts" / "sphere.geo"

# Quality gates.
#
# MAX_EDGES: the fine fixture must stay below the 46,340 dense-i32 cap
# (sqrt(i32::MAX)) so a future Burn-path parity run remains *possible*
# (issue #215 scope note: fine-fixture Burn parity itself is deferred
# until after #218), and comfortably within the direct sparse-LU budget
# the spiral fixture already demonstrated (54k edges).
MAX_EDGES = 46_000
MIN_DIHEDRAL_DEG = 5.0  # reject pathologically flat tets

# Physical groups the fixture must carry: (dim, tag, name) — keep in
# sync with the PHYS_* constants in crates/geode-core/src/mesh/sphere.rs.
EXPECTED_GROUPS = [
    (3, 1, "sphere_interior"),
    (3, 2, "vacuum_gap"),
    (3, 5, "pml_shell"),
    (2, 3, "outer_boundary"),
    (2, 4, "sphere_surface"),
    (2, 6, "pml_interface"),
]


def run_gmsh(lc_sphere: float, lc_buffer: float, output: Path) -> str:
    """Invoke the gmsh CLI; returns the gmsh version string."""
    version = subprocess.run(
        ["gmsh", "--version"], capture_output=True, text=True, check=True
    )
    gmsh_version = (version.stdout + version.stderr).strip()

    cmd = [
        "gmsh",
        "-3",
        "-format",
        "msh41",
        "-o",
        str(output),
        "-setnumber",
        "lc_sphere",
        str(lc_sphere),
        "-setnumber",
        "lc_buffer",
        str(lc_buffer),
        str(GEO_SCRIPT),
    ]

    print(f"+ {' '.join(cmd)}")
    proc = subprocess.run(cmd, capture_output=True, text=True)
    if proc.returncode != 0 or "Error" in proc.stdout or "Error" in proc.stderr:
        sys.stderr.write(proc.stdout)
        sys.stderr.write(proc.stderr)
        raise RuntimeError(f"gmsh failed (exit {proc.returncode})")
    return gmsh_version


def check_quality(report: dict) -> list:
    failures = []
    if report["inverted_tets"]:
        failures.append(f"{report['inverted_tets']} inverted tets (signed volume <= 0)")
    if report["min_dihedral_deg"] < MIN_DIHEDRAL_DEG:
        failures.append(
            f"min dihedral angle {report['min_dihedral_deg']:.2f} deg "
            f"< gate {MIN_DIHEDRAL_DEG} deg"
        )
    if report["n_edges"] > MAX_EDGES:
        failures.append(f"{report['n_edges']} edges exceeds budget {MAX_EDGES}")
    for dim, tag, name in EXPECTED_GROUPS:
        if report["group_names"].get((dim, tag)) != name:
            failures.append(f"missing physical group (dim={dim}, tag={tag}) {name!r}")
            continue
        counts = report["tet_phys_counts"] if dim == 3 else report["tri_phys_counts"]
        if counts.get(tag, 0) == 0:
            failures.append(
                f"physical group {name!r} (dim={dim}, tag={tag}) has no elements"
            )
    n_tagged = sum(report["tet_phys_counts"].values()) - report[
        "tet_phys_counts"
    ].get(0, 0)
    if n_tagged != report["n_tets"]:
        failures.append(
            f"only {n_tagged} of {report['n_tets']} tets carry a 3D physical tag"
        )
    return failures


def main(argv: list) -> int:
    parser = argparse.ArgumentParser(
        description="Generate + validate a sphere-in-vacuum mesh fixture."
    )
    parser.add_argument(
        "--lc-sphere",
        type=float,
        default=0.23,
        help="characteristic length inside the dielectric sphere (default: "
        "0.23, the committed coarse fixture)",
    )
    parser.add_argument(
        "--lc-buffer",
        type=float,
        default=0.4,
        help="characteristic length in the vacuum gap + PML shell "
        "(default: 0.4, the committed coarse fixture)",
    )
    parser.add_argument("output", type=Path, help="output .msh path")
    args = parser.parse_args(argv[1:])

    gmsh_version = run_gmsh(args.lc_sphere, args.lc_buffer, args.output)

    nodes, tets, tris, group_names, _ = parse_msh(args.output)
    report = quality_report(nodes, tets, tris, group_names)

    print("\n--- mesh quality report ---")
    print(f"nodes:            {report['n_nodes']}")
    print(f"tets:             {report['n_tets']}")
    print(f"tagged triangles: {report['n_tris']}")
    print(f"unique edges:     {report['n_edges']}  (budget {MAX_EDGES})")
    print(f"inverted tets:    {report['inverted_tets']}")
    print(
        f"min dihedral:     {report['min_dihedral_deg']:.2f} deg  "
        f"(gate {MIN_DIHEDRAL_DEG})"
    )
    print(f"total volume:     {report['total_volume']:.6g}")
    for dim, tag, name in EXPECTED_GROUPS:
        counts = report["tet_phys_counts"] if dim == 3 else report["tri_phys_counts"]
        kind = "tets" if dim == 3 else "tris"
        print(f"  ({dim},{tag:>2}) {name:<18} {counts.get(tag, 0):>7} {kind}")

    failures = check_quality(report)
    if failures:
        print("\nQUALITY GATES FAILED:")
        for f in failures:
            print(f"  - {f}")
        return 1

    sha = hashlib.sha256(args.output.read_bytes()).hexdigest()
    prov_path = args.output.with_suffix(".provenance.txt")
    prov = [
        f"fixture:        {args.output.name}",
        f"sha256:         {sha}",
        f"generated:      {datetime.now(timezone.utc).isoformat(timespec='seconds')}",
        f"gmsh version:   {gmsh_version}",
        f"script:         mesh_scripts/sphere.geo",
        f"wrapper:        reference/gmsh/generate_sphere_fixture.py",
        "",
        "parameter values:",
        f"  lc_sphere: {args.lc_sphere}",
        f"  lc_buffer: {args.lc_buffer}",
        "",
        "quality report:",
        f"  nodes: {report['n_nodes']}",
        f"  tets: {report['n_tets']}",
        f"  unique edges: {report['n_edges']} (budget {MAX_EDGES})",
        f"  inverted tets: {report['inverted_tets']}",
        f"  min dihedral angle: {report['min_dihedral_deg']:.2f} deg",
        "",
    ]
    prov_path.write_text("\n".join(prov))
    print(f"\nOK — wrote {args.output} and {prov_path}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
