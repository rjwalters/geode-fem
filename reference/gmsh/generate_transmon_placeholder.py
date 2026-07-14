#!/usr/bin/env python3
"""Generate + validate the PLACEHOLDER transmon mesh fixture (issue #485).

This is the toolchain-gap fallback for
``reference/julia/generate_transmon_fixture.jl``: DeviceLayout.jl cannot
be loaded in the Loom build environment (Cairo/Pango_jll precompile
failure), so the real transmon geometry is operator-generated. Until then
this wrapper builds a schema-faithful STAND-IN from
``transmon_placeholder.geo`` — a two-pad + junction + trace + two-port
capacitor on a sapphire slab in a vacuum box — carrying the IDENTICAL
physical-group names the ``mesh::transmon`` adapter consumes. The real
DeviceLayout mesh is a drop-in replacement (same group names, updated
counts).

Mirrors ``generate_spiral_fixture.py`` exactly: offline generation (gmsh
is NOT a CI dependency), quality gates (no inverted tets, min dihedral,
edge budget, all groups present), provenance recorded next to the mesh.

Usage:
  python3 reference/gmsh/generate_transmon_placeholder.py \
      reference/gmsh/transmon_placeholder_smoke.yaml \
      crates/geode-core/tests/fixtures/transmon_smoke.msh

Exit status is non-zero if generation or any quality gate fails.
"""

from __future__ import annotations

import hashlib
import sys
from datetime import datetime, timezone
from pathlib import Path

# Reuse the spiral wrapper's MSH parser + quality helpers (stdlib-only,
# no behavior fork).
sys.path.insert(0, str(Path(__file__).resolve().parent))
from generate_spiral_fixture import (  # noqa: E402
    parse_msh,
    parse_simple_yaml,
    quality_report,
)
import subprocess  # noqa: E402

SCRIPT_DIR = Path(__file__).resolve().parent
GEO_SCRIPT = SCRIPT_DIR / "transmon_placeholder.geo"

# Quality gates (same class as the patch/spiral smoke meshes).
MAX_EDGES = 150_000
MIN_DIHEDRAL_DEG = 5.0

# Physical groups the placeholder must carry — the exact DeviceLayout
# schema the `mesh::transmon` adapter asserts against.
EXPECTED_GROUPS = [
    (3, 1, "substrate"),
    (3, 2, "vacuum"),
    (2, 11, "metal"),
    (2, 12, "port_1"),
    (2, 13, "port_2"),
    (2, 14, "lumped_element"),
    (2, 15, "exterior_boundary"),
]

# YAML keys forwarded to the .geo as -setnumber overrides.
GEO_PARAMS = {
    "geom": [
        "chip_x", "chip_y", "h_sub", "h_vac", "margin",
        "pad_w", "pad_h", "pad_gap", "trace_w", "port_s",
    ],
    "mesh": ["lc_metal", "lc_far"],
}


def run_gmsh(params: dict, output: Path) -> str:
    version = subprocess.run(
        ["gmsh", "--version"], capture_output=True, text=True, check=True
    )
    gmsh_version = (version.stdout + version.stderr).strip()

    cmd = ["gmsh", "-3", "-format", "msh41", "-o", str(output)]
    for section, keys in GEO_PARAMS.items():
        for key in keys:
            if section in params and key in params[section]:
                cmd += ["-setnumber", key, str(float(params[section][key]))]
    cmd.append(str(GEO_SCRIPT))

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
            failures.append(f"physical group {name!r} (dim={dim}, tag={tag}) has no elements")
    n_tagged = sum(report["tet_phys_counts"].values()) - report["tet_phys_counts"].get(0, 0)
    if n_tagged != report["n_tets"]:
        failures.append(
            f"only {n_tagged} of {report['n_tets']} tets carry a 3D physical tag"
        )
    return failures


def main(argv: list) -> int:
    if len(argv) != 3:
        sys.stderr.write(__doc__ or "")
        return 2
    yaml_path = Path(argv[1])
    out_path = Path(argv[2])

    params = parse_simple_yaml(yaml_path)
    gmsh_version = run_gmsh(params, out_path)

    nodes, tets, tris, group_names, _ = parse_msh(out_path)
    report = quality_report(nodes, tets, tris, group_names)

    print("\n--- mesh quality report (PLACEHOLDER transmon) ---")
    print(f"nodes:            {report['n_nodes']}")
    print(f"tets:             {report['n_tets']}")
    print(f"tagged triangles: {report['n_tris']}")
    print(f"unique edges:     {report['n_edges']}  (budget {MAX_EDGES})")
    print(f"inverted tets:    {report['inverted_tets']}")
    print(f"min dihedral:     {report['min_dihedral_deg']:.2f} deg  (gate {MIN_DIHEDRAL_DEG})")
    print(f"total volume:     {report['total_volume']:.6g} um^3")
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

    sha = hashlib.sha256(out_path.read_bytes()).hexdigest()
    prov_path = out_path.with_suffix(".provenance.txt")
    prov = [
        f"fixture:        {out_path.name}",
        "kind:           PLACEHOLDER (schema-faithful stand-in — NOT DeviceLayout.jl)",
        f"sha256:         {sha}",
        f"generated:      {datetime.now(timezone.utc).isoformat(timespec='seconds')}",
        f"gmsh version:   {gmsh_version}",
        "script:         reference/gmsh/transmon_placeholder.geo",
        "wrapper:        reference/gmsh/generate_transmon_placeholder.py",
        f"parameters:     {yaml_path.name}",
        "real-fixture:   reference/julia/generate_transmon_fixture.jl (operator-assisted)",
        "swap-note:      replace with the DeviceLayout mesh + update the",
        "                per-group counts in tests/transmon_mesh.rs when the",
        "                operator generates the real fixture.",
        "",
        "parameter values:",
    ]
    for section in ("geom", "mesh", "materials"):
        if section in params:
            prov.append(f"  {section}:")
            for k, v in params[section].items():
                prov.append(f"    {k}: {v}")
    prov += [
        "",
        "physical-group element counts (adapter test assertions):",
    ]
    for dim, tag, name in EXPECTED_GROUPS:
        counts = report["tet_phys_counts"] if dim == 3 else report["tri_phys_counts"]
        kind = "tets" if dim == 3 else "tris"
        prov.append(f"  ({dim},{tag:>2}) {name:<18} {counts.get(tag, 0):>7} {kind}")
    prov += [
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
    print(f"\nOK — wrote {out_path} and {prov_path}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
