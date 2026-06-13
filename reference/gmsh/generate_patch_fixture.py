#!/usr/bin/env python3
"""Generate + validate the patch-antenna FEM mesh fixture (issue #227).

Offline generation wrapper around ``patch_antenna.geo``: gmsh is NOT a
CI dependency — the generated ``.msh`` is committed as a test fixture and
this script records provenance (gmsh version, script, parameters, quality
metrics) next to it.

Pipeline:
  1. read patch + substrate + air/UPML + mesh parameters from a flat
     two-level YAML file (stdlib parser — no PyYAML needed),
  2. invoke the ``gmsh`` CLI on ``patch_antenna.geo`` with ``-setnumber``
     overrides,
  3. parse the MSH 4.1 ASCII output (stdlib only) and run mesh-quality
     sanity checks:
       - every tet has positive signed volume (no inverted tets),
       - minimum dihedral angle is reported (and gated),
       - unique-edge count stays within the sparse-LU budget,
       - all expected physical groups are present and populated,
  4. write a ``<output>.provenance.txt`` next to the mesh.

Usage:
  python3 reference/gmsh/generate_patch_fixture.py \
      reference/gmsh/patch_2g4_benchmark.yaml \
      crates/geode-core/tests/fixtures/patch_2g4.msh

Exit status is non-zero if generation or any quality gate fails.
"""

from __future__ import annotations

import hashlib
import math
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

SCRIPT_DIR = Path(__file__).resolve().parent
GEO_SCRIPT = SCRIPT_DIR / "patch_antenna.geo"

# Quality gates.
MAX_EDGES = 150_000  # sparse-LU affordability target (issue #227; #218
#                      lifted the old ~46k dense cap)
MIN_DIHEDRAL_DEG = 5.0  # reject pathologically flat tets

# Physical groups the fixture must carry: (dim, tag, name).
EXPECTED_GROUPS = [
    (3, 1, "substrate"),
    (3, 2, "air"),
    (3, 3, "upml"),
    (2, 11, "port"),
    (2, 12, "patch"),
    (2, 13, "ground"),
    (2, 14, "outer_boundary"),
]

# YAML keys (under `patch:` / `domain:` / `mesh:`) forwarded to the .geo
# as -setnumber parameters.
GEO_PARAMS = {
    "patch": ["patch_w", "patch_l", "h", "sub_pad", "probe_w", "probe_inset"],
    "domain": ["air_margin", "pml_thick"],
    "mesh": ["lc_patch", "lc_sub", "lc_port", "lc_far", "dist_far"],
}


def parse_simple_yaml(path: Path) -> dict:
    """Parse the flat two-level YAML subset used by the parameter files.

    Supports:  ``section:`` headers, two-space-indented ``key: value``
    scalars (float / int / string), and ``#`` comments. This is NOT a
    general YAML parser — it covers exactly the schema of the patch
    parameter files so the wrapper has no dependencies beyond the
    Python stdlib (mirrors ``generate_spiral_fixture.py``).
    """
    data: dict = {}
    section: dict | None = None
    for raw in path.read_text().splitlines():
        line = raw.split("#", 1)[0].rstrip()
        if not line.strip():
            continue
        indented = line.startswith("  ")
        key, _, value = line.strip().partition(":")
        key = key.strip()
        value = value.strip()
        if not indented:
            if value:  # top-level scalar
                data[key] = _coerce(value)
                section = None
            else:  # section header
                section = {}
                data[key] = section
        else:
            if section is None:
                raise ValueError(f"{path}: indented key {key!r} outside any section")
            section[key] = _coerce(value)
    return data


def _coerce(value: str):
    value = value.strip().strip('"').strip("'")
    try:
        return int(value)
    except ValueError:
        pass
    try:
        return float(value)
    except ValueError:
        pass
    return value


def run_gmsh(params: dict, output: Path) -> str:
    """Invoke the gmsh CLI; returns the gmsh version string."""
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


# ---------------------------------------------------------------------------
# Minimal MSH 4.1 ASCII parsing (mirrors the geode-core reader's needs).
# ---------------------------------------------------------------------------


def _section(text: str, name: str) -> str:
    start = text.index(f"${name}") + len(name) + 1
    end = text.index(f"$End{name}")
    return text[start:end]


def parse_msh(path: Path):
    """Returns (nodes, tets, tris, group_names, entity_phys)."""
    text = path.read_text()

    group_names = {}
    if "$PhysicalNames" in text:
        lines = [l for l in _section(text, "PhysicalNames").splitlines() if l.strip()]
        for line in lines[1:]:
            dim, tag, name = line.split(maxsplit=2)
            group_names[(int(dim), int(tag))] = name.strip().strip('"')

    # $Entities -> (dim, entity_tag) -> physical tag
    entity_phys = {}
    ent_lines = [l for l in _section(text, "Entities").splitlines() if l.strip()]
    n_pts, n_crv, n_srf, n_vol = (int(t) for t in ent_lines[0].split())
    row = 1
    for _ in range(n_pts):  # points: tag x y z numPhys phys...
        toks = ent_lines[row].split()
        if int(toks[4]) >= 1:
            entity_phys[(0, int(toks[0]))] = int(toks[5])
        row += 1
    for dim, count in ((1, n_crv), (2, n_srf), (3, n_vol)):
        for _ in range(count):  # tag bbox(6) numPhys phys... numBnd ...
            toks = ent_lines[row].split()
            if int(toks[7]) >= 1:
                entity_phys[(dim, int(toks[0]))] = int(toks[8])
            row += 1

    nodes = {}
    node_lines = [l for l in _section(text, "Nodes").splitlines() if l.strip()]
    n_blocks = int(node_lines[0].split()[0])
    row = 1
    for _ in range(n_blocks):
        n_in_block = int(node_lines[row].split()[3])
        row += 1
        tags = [int(node_lines[row + i]) for i in range(n_in_block)]
        row += n_in_block
        for i, tag in enumerate(tags):
            x, y, z = (float(t) for t in node_lines[row + i].split()[:3])
            nodes[tag] = (x, y, z)
        row += n_in_block

    tets, tris = [], []
    elem_lines = [l for l in _section(text, "Elements").splitlines() if l.strip()]
    n_blocks = int(elem_lines[0].split()[0])
    row = 1
    for _ in range(n_blocks):
        ent_dim, ent_tag, elem_type, n_in_block = (int(t) for t in elem_lines[row].split())
        phys = entity_phys.get((ent_dim, ent_tag), 0)
        row += 1
        for i in range(n_in_block):
            toks = elem_lines[row + i].split()
            conn = [int(t) for t in toks[1:]]
            if elem_type == 4:  # Tet4
                tets.append((phys, conn))
            elif elem_type == 2:  # Tri3
                tris.append((phys, conn))
        row += n_in_block

    return nodes, tets, tris, group_names, entity_phys


# ---------------------------------------------------------------------------
# Quality checks.
# ---------------------------------------------------------------------------


def _sub(a, b):
    return (a[0] - b[0], a[1] - b[1], a[2] - b[2])


def _cross(a, b):
    return (
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    )


def _dot(a, b):
    return a[0] * b[0] + a[1] * b[1] + a[2] * b[2]


def _norm(a):
    return math.sqrt(_dot(a, a))


def signed_volume(v0, v1, v2, v3) -> float:
    return _dot(_sub(v1, v0), _cross(_sub(v2, v0), _sub(v3, v0))) / 6.0


def min_dihedral_deg(v) -> float:
    """Minimum dihedral angle (degrees) of tet with vertices v[0..3]."""
    faces = [(1, 2, 3), (0, 2, 3), (0, 1, 3), (0, 1, 2)]
    normals = []
    for a, b, c in faces:
        n = _cross(_sub(v[b], v[a]), _sub(v[c], v[a]))
        normals.append(n)
    worst = 180.0
    for k in range(4):
        for l in range(k + 1, 4):
            cosang = -_dot(normals[k], normals[l]) / (
                _norm(normals[k]) * _norm(normals[l])
            )
            cosang = max(-1.0, min(1.0, cosang))
            ang = math.degrees(math.acos(cosang))
            worst = min(worst, ang)
    return worst


def quality_report(nodes, tets, tris, group_names) -> dict:
    inverted = 0
    min_dih = 180.0
    vol_total = 0.0
    edges = set()
    tet_phys_counts: dict = {}
    for phys, conn in tets:
        v = [nodes[t] for t in conn]
        sv = signed_volume(*v)
        vol_total += sv
        if sv <= 0.0:
            inverted += 1
        min_dih = min(min_dih, min_dihedral_deg(v))
        for i in range(4):
            for j in range(i + 1, 4):
                a, b = conn[i], conn[j]
                edges.add((a, b) if a < b else (b, a))
        tet_phys_counts[phys] = tet_phys_counts.get(phys, 0) + 1

    tri_phys_counts: dict = {}
    for phys, _ in tris:
        tri_phys_counts[phys] = tri_phys_counts.get(phys, 0) + 1

    return {
        "n_nodes": len(nodes),
        "n_tets": len(tets),
        "n_tris": len(tris),
        "n_edges": len(edges),
        "inverted_tets": inverted,
        "min_dihedral_deg": min_dih,
        "total_volume": vol_total,
        "tet_phys_counts": tet_phys_counts,
        "tri_phys_counts": tri_phys_counts,
        "group_names": group_names,
    }


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

    print("\n--- mesh quality report ---")
    print(f"nodes:            {report['n_nodes']}")
    print(f"tets:             {report['n_tets']}")
    print(f"tagged triangles: {report['n_tris']}")
    print(f"unique edges:     {report['n_edges']}  (budget {MAX_EDGES})")
    print(f"inverted tets:    {report['inverted_tets']}")
    print(f"min dihedral:     {report['min_dihedral_deg']:.2f} deg  (gate {MIN_DIHEDRAL_DEG})")
    print(f"total volume:     {report['total_volume']:.6g} mm^3")
    for dim, tag, name in EXPECTED_GROUPS:
        counts = report["tet_phys_counts"] if dim == 3 else report["tri_phys_counts"]
        kind = "tets" if dim == 3 else "tris"
        print(f"  ({dim},{tag:>2}) {name:<16} {counts.get(tag, 0):>7} {kind}")

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
        f"sha256:         {sha}",
        f"generated:      {datetime.now(timezone.utc).isoformat(timespec='seconds')}",
        f"gmsh version:   {gmsh_version}",
        f"script:         reference/gmsh/patch_antenna.geo",
        f"wrapper:        reference/gmsh/generate_patch_fixture.py",
        f"parameters:     {yaml_path.name}",
        "",
        "parameter values:",
    ]
    for section in ("patch", "domain", "mesh", "materials"):
        if section in params:
            prov.append(f"  {section}:")
            for k, v in params[section].items():
                prov.append(f"    {k}: {v}")
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
