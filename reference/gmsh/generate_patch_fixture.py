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
  4. optionally (YAML ``bend:`` section, issue #1084) wrap the flat mesh
     into a **curved conformal** patch: the exact node map of
     ``PatchFixture::bent_conformal`` in ``crates/geode-core/src/mesh/
     patch.rs`` (cylinder of radius ``bend_radius`` about an axis parallel
     to y through ``(x = 0, z = h/2 - R)``, windowed to the identity at the
     UPML inner wall). The bent mesh is written to ``<output>`` and the flat
     mesh it came from (same topology, node for node) to
     ``<output stem>.flat.msh``,
  5. write a ``<output>.provenance.txt`` next to the mesh: sha256 (of both
     meshes when bent), quality report, edge/node/tet counts, substrate
     layers, per-region element sizes and elements per wavelength
     (``band:`` section) in air (lambda_0) and in the substrate (lambda_d).

Usage:
  python3 reference/gmsh/generate_patch_fixture.py [--max-edges N] \
      reference/gmsh/patch_2g4_benchmark.yaml \
      crates/geode-core/tests/fixtures/patch_2g4.msh

``--max-edges`` overrides the default sparse-LU edge budget
(``MAX_EDGES``) for deliberately large refinement levels that are
regenerated rather than committed.

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
    "mesh": [
        "lc_patch",
        "lc_sub",
        "lc_port",
        "lc_far",
        "dist_far",
        "lc_air",
        "lc_pml",
        "sub_zpad",
        "sub_box_thick",
    ],
}

# Speed of light in mm/s (lengths are in mm; matches
# geode_core::constants::C_MM_PER_S).
C_MM_PER_S = 299_792_458.0e3

# Minimum per-tet signed-volume ratio (bent / flat) accepted for a bent
# mesh — the same floor the Rust bend test asserts
# (curved_conformal_bend_is_valid_and_actually_curved).
MIN_BEND_VOL_RATIO = 0.2


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
# Curved conformal bend (issue #1084) — mirrors PatchFixture::bent_conformal.
# ---------------------------------------------------------------------------


def substrate_box(nodes, tets):
    """AABB over the nodes of the substrate (tag 1) tets."""
    lo = [math.inf] * 3
    hi = [-math.inf] * 3
    for phys, conn in tets:
        if phys != 1:
            continue
        for tag in conn:
            p = nodes[tag]
            for k in range(3):
                lo[k] = min(lo[k], p[k])
                hi[k] = max(hi[k], p[k])
    return lo, hi


def air_box(nodes, pml_thick):
    """Domain AABB shrunk by the UPML thickness (PatchFixture::air_box)."""
    lo = [math.inf] * 3
    hi = [-math.inf] * 3
    for p in nodes.values():
        for k in range(3):
            lo[k] = min(lo[k], p[k])
            hi[k] = max(hi[k], p[k])
    return [v + pml_thick for v in lo], [v - pml_thick for v in hi]


def _smoothstep(t):
    t = min(1.0, max(0.0, t))
    return t * t * (3.0 - 2.0 * t)


def _ramp(v, s_lo, s_hi, a_lo, a_hi):
    if v > s_hi:
        return _smoothstep((a_hi - v) / max(a_hi - s_hi, 1e-12))
    if v < s_lo:
        return _smoothstep((v - a_lo) / max(s_lo - a_lo, 1e-12))
    return 1.0


def bend_nodes(nodes, tets, bend_radius, pml_thick):
    """Cylindrical conformal bend, node for node identical to
    ``PatchFixture::bent_conformal(bend_radius, pml_thick)`` in Rust.

    Returns ``(bent_nodes, z0)`` where ``z0`` is the substrate mid-plane (the
    bend axis sits at ``(x = 0, z = z0 - bend_radius)``)."""
    sub_lo, sub_hi = substrate_box(nodes, tets)
    a_lo, a_hi = air_box(nodes, pml_thick)
    z0 = 0.5 * (sub_lo[2] + sub_hi[2])
    r_bend = max(bend_radius, 1e-6)
    bent = {}
    for tag, (x, y, z) in nodes.items():
        wx = _ramp(x, sub_lo[0], sub_hi[0], a_lo[0], a_hi[0])
        wy = _ramp(y, sub_lo[1], sub_hi[1], a_lo[1], a_hi[1])
        wz = _ramp(z, sub_lo[2], sub_hi[2], a_lo[2], a_hi[2])
        w = wx * wy * wz
        if w == 0.0:
            bent[tag] = (x, y, z)
            continue
        phi = x / r_bend
        r = r_bend + (z - z0)
        bx = r * math.sin(phi)
        bz = z0 + r * math.cos(phi) - r_bend
        bent[tag] = (x + w * (bx - x), y, z + w * (bz - z))
    return bent, z0


def write_msh_with_nodes(src: Path, dst: Path, nodes) -> None:
    """Copy an MSH 4.1 ASCII file, replacing only the node coordinates."""
    lines = src.read_text().splitlines(keepends=True)
    start = next(i for i, l in enumerate(lines) if l.strip() == "$Nodes")
    end = next(i for i, l in enumerate(lines) if l.strip() == "$EndNodes")
    out = lines[: start + 2]  # "$Nodes" + header
    row = start + 2
    n_blocks = int(lines[start + 1].split()[0])
    for _ in range(n_blocks):
        header = lines[row]
        out.append(header)
        n_in_block = int(header.split()[3])
        row += 1
        tags = [int(lines[row + i]) for i in range(n_in_block)]
        out.extend(lines[row : row + n_in_block])
        row += n_in_block
        for tag in tags:
            x, y, z = nodes[tag]
            out.append(f"{x!r} {y!r} {z!r}\n")
        row += n_in_block
    assert row == end, "node section parse mismatch"
    out.extend(lines[end:])
    dst.write_text("".join(out))


def min_volume_ratio(flat_nodes, bent_nodes, tets) -> float:
    worst = math.inf
    for _, conn in tets:
        v0 = signed_volume(*[flat_nodes[t] for t in conn])
        v1 = signed_volume(*[bent_nodes[t] for t in conn])
        if v0 != 0.0:
            worst = min(worst, v1 / v0)
    return worst


def cylinder_residual(bent_nodes, tris, tag, radius, z_axis) -> float:
    """Max | |(x, z) - axis| - radius | over the nodes of tagged triangles."""
    worst = 0.0
    seen = set()
    for phys, conn in tris:
        if phys != tag:
            continue
        for n in conn:
            if n in seen:
                continue
            seen.add(n)
            x, _, z = bent_nodes[n]
            worst = max(worst, abs(math.hypot(x, z - z_axis) - radius))
    return worst


# ---------------------------------------------------------------------------
# Resolution statistics (issue #1084): per-region element sizes, substrate
# layers, elements per wavelength.
# ---------------------------------------------------------------------------


def region_edge_stats(nodes, tets) -> dict:
    """Per-region (tet physical tag) ``(mean, p95, max)`` tet edge length
    (mm), over the six edges of every tet of the region. gmsh honours size
    fields only approximately, so a few percent of edges run well past the
    target; the 95th percentile is the robust "coarsest typical" size."""
    acc: dict = {}
    for phys, conn in tets:
        lst = acc.setdefault(phys, [])
        for i in range(4):
            for j in range(i + 1, 4):
                lst.append(_norm(_sub(nodes[conn[i]], nodes[conn[j]])))
    out = {}
    for k, lst in acc.items():
        lst.sort()
        out[k] = (sum(lst) / len(lst), lst[int(0.95 * (len(lst) - 1))], lst[-1])
    return out


def patch_edge_stats(nodes, tris, tag=12):
    """Mean / max edge length (mm) of the tagged surface triangles."""
    total, count, worst = 0.0, 0, 0.0
    for phys, conn in tris:
        if phys != tag:
            continue
        for i in range(3):
            d = _norm(_sub(nodes[conn[i]], nodes[conn[(i + 1) % 3]]))
            total += d
            count += 1
            worst = max(worst, d)
    return (total / count, worst) if count else (0.0, 0.0)


def substrate_layers(nodes, tets, h) -> float:
    """Median over substrate tets of ``h / (tet z-extent)`` on the FLAT mesh:
    1.0 for a slab one tet thick, ~2 for two layers, and so on."""
    ratios = []
    for phys, conn in tets:
        if phys != 1:
            continue
        zs = [nodes[t][2] for t in conn]
        ext = max(zs) - min(zs)
        if ext > 0.0:
            ratios.append(h / ext)
    ratios.sort()
    return ratios[len(ratios) // 2] if ratios else 0.0


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


def check_quality(report: dict, max_edges: int = MAX_EDGES) -> list:
    failures = []
    if report["inverted_tets"]:
        failures.append(f"{report['inverted_tets']} inverted tets (signed volume <= 0)")
    if report["min_dihedral_deg"] < MIN_DIHEDRAL_DEG:
        failures.append(
            f"min dihedral angle {report['min_dihedral_deg']:.2f} deg "
            f"< gate {MIN_DIHEDRAL_DEG} deg"
        )
    if report["n_edges"] > max_edges:
        failures.append(f"{report['n_edges']} edges exceeds budget {max_edges}")
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
    args = list(argv[1:])
    max_edges = MAX_EDGES
    if "--max-edges" in args:
        k = args.index("--max-edges")
        max_edges = int(args[k + 1])
        del args[k : k + 2]
    if len(args) != 2:
        sys.stderr.write(__doc__ or "")
        return 2
    yaml_path = Path(args[0])
    out_path = Path(args[1])

    params = parse_simple_yaml(yaml_path)
    bend = params.get("bend")
    flat_path = out_path.with_name(out_path.stem + ".flat.msh") if bend else out_path
    gmsh_version = run_gmsh(params, flat_path)

    nodes, tets, tris, group_names, _ = parse_msh(flat_path)
    report = quality_report(nodes, tets, tris, group_names)
    h = float(params["patch"]["h"])
    layers = substrate_layers(nodes, tets, h)

    bent_info = None
    if bend:
        radius = float(bend["bend_radius"])
        pml_thick = float(params["domain"]["pml_thick"])
        bent, z0 = bend_nodes(nodes, tets, radius, pml_thick)
        write_msh_with_nodes(flat_path, out_path, bent)
        flat_report = report
        nodes_b, tets_b, tris_b, group_names_b, _ = parse_msh(out_path)
        report = quality_report(nodes_b, tets_b, tris_b, group_names_b)
        z_axis = z0 - radius
        bent_info = {
            "radius": radius,
            "z0": z0,
            "z_axis": z_axis,
            "flat_report": flat_report,
            "min_vol_ratio": min_volume_ratio(nodes, nodes_b, tets),
            "patch_cyl_resid": cylinder_residual(nodes_b, tris_b, 12, radius + (h - z0), z_axis),
            "ground_cyl_resid": cylinder_residual(nodes_b, tris_b, 13, radius - z0, z_axis),
        }
        nodes, tets, tris = nodes_b, tets_b, tris_b

    edge_stats = region_edge_stats(nodes, tets)
    patch_stats = patch_edge_stats(nodes, tris)

    print("\n--- mesh quality report ---")
    print(f"nodes:            {report['n_nodes']}")
    print(f"tets:             {report['n_tets']}")
    print(f"tagged triangles: {report['n_tris']}")
    print(f"unique edges:     {report['n_edges']}  (budget {max_edges})")
    print(f"inverted tets:    {report['inverted_tets']}")
    print(f"min dihedral:     {report['min_dihedral_deg']:.2f} deg  (gate {MIN_DIHEDRAL_DEG})")
    print(f"total volume:     {report['total_volume']:.6g} mm^3")
    print(f"substrate layers: {layers:.2f}  (median h / tet z-extent, flat mesh)")
    for dim, tag, name in EXPECTED_GROUPS:
        counts = report["tet_phys_counts"] if dim == 3 else report["tri_phys_counts"]
        kind = "tets" if dim == 3 else "tris"
        print(f"  ({dim},{tag:>2}) {name:<16} {counts.get(tag, 0):>7} {kind}")
    if bent_info:
        print(f"bend radius:      {bent_info['radius']} mm (axis x = 0, z = {bent_info['z_axis']:.6g} mm)")
        print(f"min vol ratio:    {bent_info['min_vol_ratio']:.4f}  (gate {MIN_BEND_VOL_RATIO})")
        print(f"patch on arc:     max |r - R_patch| = {bent_info['patch_cyl_resid']:.3e} mm")
        print(f"ground on arc:    max |r - R_ground| = {bent_info['ground_cyl_resid']:.3e} mm")

    failures = check_quality(report, max_edges)
    if bent_info:
        if bent_info["min_vol_ratio"] < MIN_BEND_VOL_RATIO:
            failures.append(
                f"bend min volume ratio {bent_info['min_vol_ratio']:.3f} < {MIN_BEND_VOL_RATIO}"
            )
        for key in ("patch_cyl_resid", "ground_cyl_resid"):
            if bent_info[key] > 1e-9:
                failures.append(f"{key} = {bent_info[key]:.3e} mm: metal not on its arc")
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
    ]
    if bent_info:
        flat_sha = hashlib.sha256(flat_path.read_bytes()).hexdigest()
        prov += [
            f"flat fixture:   {flat_path.name}",
            f"flat sha256:    {flat_sha}",
        ]
    prov += [
        f"generated:      {datetime.now(timezone.utc).isoformat(timespec='seconds')}",
        f"gmsh version:   {gmsh_version}",
        f"script:         reference/gmsh/patch_antenna.geo",
        f"wrapper:        reference/gmsh/generate_patch_fixture.py",
        f"parameters:     {yaml_path.name}",
        "",
        "parameter values:",
    ]
    for section in ("patch", "domain", "mesh", "bend", "band", "solver", "materials"):
        if section in params:
            prov.append(f"  {section}:")
            for k, v in params[section].items():
                prov.append(f"    {k}: {v}")
    prov += [
        "",
        "quality report:",
        f"  nodes: {report['n_nodes']}",
        f"  tets: {report['n_tets']}",
        f"  unique edges: {report['n_edges']} (budget {max_edges})",
        f"  inverted tets: {report['inverted_tets']}",
        f"  min dihedral angle: {report['min_dihedral_deg']:.2f} deg",
    ]
    for dim, tag, name in EXPECTED_GROUPS:
        counts = report["tet_phys_counts"] if dim == 3 else report["tri_phys_counts"]
        prov.append(f"  {name} {'tets' if dim == 3 else 'tris'}: {counts.get(tag, 0)}")
    if bent_info:
        fr = bent_info["flat_report"]
        prov += [
            "",
            "bend (PatchFixture::bent_conformal node map):",
            f"  bend_radius_mm: {bent_info['radius']}",
            f"  axis: parallel to y through (x = 0, z = {bent_info['z_axis']!r}) mm",
            f"  substrate mid-plane z0_mm: {bent_info['z0']!r}",
            f"  min tet volume ratio (bent / flat): {bent_info['min_vol_ratio']:.4f}",
            f"  patch max |r - (R + h - z0)| mm: {bent_info['patch_cyl_resid']:.3e}",
            f"  ground max |r - (R - z0)| mm: {bent_info['ground_cyl_resid']:.3e}",
            f"  flat min dihedral angle: {fr['min_dihedral_deg']:.2f} deg",
        ]
    prov += ["", "resolution (lengths in mm):"]
    prov.append(f"  substrate layers (median h / tet z-extent, flat mesh): {layers:.2f}")
    names = {1: "substrate", 2: "air", 3: "upml"}
    for tag in (1, 2, 3):
        if tag in edge_stats:
            mean, p95, mx = edge_stats[tag]
            prov.append(
                f"  {names[tag]} tet edge mean / p95 / max: {mean:.3f} / {p95:.3f} / {mx:.3f}"
            )
    prov.append(f"  patch triangle edge mean / max: {patch_stats[0]:.3f} / {patch_stats[1]:.3f}")
    band = params.get("band")
    eps_r = params.get("materials", {}).get("substrate_eps_r")
    if band and eps_r:
        f_ref = float(band["f_ref_ghz"])
        lam0 = C_MM_PER_S / (f_ref * 1e9)
        lamd = lam0 / math.sqrt(float(eps_r))
        prov += [
            f"  reference frequency: {f_ref} GHz (lambda_0 = {lam0:.3f} mm, "
            f"lambda_d = lambda_0 / sqrt(eps_r) = {lamd:.3f} mm)",
            f"  air elements per lambda_0 (mean / p95 / max edge): "
            f"{lam0 / edge_stats[2][0]:.2f} / {lam0 / edge_stats[2][1]:.2f} / "
            f"{lam0 / edge_stats[2][2]:.2f}",
            f"  upml elements per lambda_0 (mean / p95 / max edge): "
            f"{lam0 / edge_stats[3][0]:.2f} / {lam0 / edge_stats[3][1]:.2f} / "
            f"{lam0 / edge_stats[3][2]:.2f}",
            f"  substrate elements per lambda_d (mean / p95 / max edge): "
            f"{lamd / edge_stats[1][0]:.2f} / {lamd / edge_stats[1][1]:.2f} / "
            f"{lamd / edge_stats[1][2]:.2f}",
            f"  patch elements per lambda_d (mean / max edge): "
            f"{lamd / patch_stats[0]:.2f} / {lamd / patch_stats[1]:.2f}",
            f"  upml elements through the shell (pml_thick / mean edge): "
            f"{float(params['domain']['pml_thick']) / edge_stats[3][0]:.2f}",
        ]
        if "air_margin" in params.get("domain", {}):
            prov.append(
                f"  air margin / lambda_0: {float(params['domain']['air_margin']) / lam0:.3f}"
            )
    prov.append("")
    prov_path.write_text("\n".join(prov))
    print(f"\nOK — wrote {out_path} and {prov_path}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
