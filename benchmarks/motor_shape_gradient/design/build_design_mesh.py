#!/usr/bin/env python3
"""Fresh gmsh meshes of the ICEM sector at arbitrary design parameters (issue #1177).

    uv run --with numpy==2.5.4 --with gmsh==4.15.2 python -I \\
        benchmarks/motor_shape_gradient/design/build_design_mesh.py \\
        --plan PLAN.json --levels 0 1 2 --out DIR [--designs NAME ...]

The remeshed cross-check of the design gradients needs meshes of the
geometry at ``theta0 +- h e_k``, which no upstream export provides. This
builder meshes the construction of ``geode_core::mesh::icem_parametric``
directly: ``PLAN.json`` (``icem_design_gradient plan``) lists, per design,
every material interface and boundary as an exact line or circular arc.
Each region is a plane surface bounded by a closed loop of those curves;
rotor and stator are disjoint parts that meet on two coincident coupling
arcs, as in ``../../motor_reference/geometry/build_sector_mesh.py``, whose
tags, coupling grid, antiperiodic edge pairing and mesh-size rules are
reused unchanged (imported from that module). Only the CAD differs: the
upstream NURBS multipatch has internal patch sides, this one has only the
interfaces, so the two meshes of theta0 are different meshes of the same
geometry.

Winding group tags (11..22) are assigned per slot and coil side by matching
the committed L0 mesh of the initial design (the group of each slot's inner
and outer coil side), which is read once at start-up.

Each mesh is written as ``icem_remesh_{name}_L{level}.msh`` (MSH 4.1 ASCII),
with a summary line per mesh in ``remesh_meshes.json``.
"""

import argparse
import json
import math
import sys
from pathlib import Path

import numpy as np

HERE = Path(__file__).resolve().parent
GEOMETRY = HERE.parents[1] / "motor_reference" / "geometry"
sys.path.insert(0, str(GEOMETRY))  # build_sector_mesh (and its sibling); -I keeps cwd off

import build_sector_mesh as bsm  # noqa: E402

SECTOR = bsm.SECTOR
SLOTS = 6
STATOR_FRAME0 = 13.0 * math.pi / 36.0
SLOT_PITCH = math.pi / 18.0


def require(cond, msg):
    if not cond:
        raise SystemExit(f"FAIL: {msg}")


class Part:
    """Points and curves of one part (rotor or stator), deduplicated."""

    def __init__(self, occ, name, curves):
        self.occ, self.name, self.curves = occ, name, curves
        self.points = []  # (tag, xy)
        self.cache = {}  # curve name -> (tag, va, vb)

    def point(self, xy):
        for tag, q in self.points:
            if math.hypot(xy[0] - q[0], xy[1] - q[1]) < 1e-9:
                return tag
        tag = self.occ.addPoint(float(xy[0]), float(xy[1]), 0.0)
        self.points.append((tag, (float(xy[0]), float(xy[1]))))
        return tag

    def curve(self, name):
        if name in self.cache:
            return self.cache[name]
        c = self.curves[name]
        va, vb = self.point(c["a"]), self.point(c["b"])
        require(va != vb, f"{name}: degenerate curve")
        if c["kind"] == "line":
            tag = self.occ.addLine(va, vb)
        else:
            centre = self.occ.addPoint(float(c["c"][0]), float(c["c"][1]), 0.0)
            tag = self.occ.addCircleArc(va, centre, vb)
            self.occ.remove([(0, centre)])
        self.cache[name] = (tag, va, vb)
        return self.cache[name]

    def line(self, name, a, b):
        """An extra straight curve between two points (split radial edges)."""
        self.curves[name] = {"kind": "line", "a": a, "b": b}
        return self.curve(name)

    def loop(self, names):
        edges = [self.curve(n) for n in names]
        tag, a, b = edges.pop(0)
        signed, cur, start = [tag], b, a
        while edges:
            for k, (t, va, vb) in enumerate(edges):
                if va == cur or vb == cur:
                    signed.append(t if va == cur else -t)
                    cur = vb if va == cur else va
                    edges.pop(k)
                    break
            else:
                raise SystemExit(f"FAIL: {self.name}: open loop {names}")
        require(cur == start, f"{self.name}: loop does not close {names}")
        return self.occ.addCurveLoop(signed)

    def surface(self, outer, holes=()):
        return self.occ.addPlaneSurface([self.loop(outer)] + [self.loop(h) for h in holes])


def slot_frame_x(xy, k):
    a = STATOR_FRAME0 + SLOT_PITCH * k
    return math.cos(a) * xy[0] + math.sin(a) * xy[1]


def winding_tags(curves0):
    """{(slot, side): tag} from the committed L0 mesh of the initial design."""
    import gmsh

    gmsh.initialize(["-noenv"])
    gmsh.option.setNumber("General.Terminal", 0)
    gmsh.open(str(GEOMETRY / "meshes" / "icem_initial_sector_L0.msh"))
    out = {}
    for g in range(1, 13):
        tag = bsm.TAG_WINDING_BASE + g
        cx = []
        for ent in gmsh.model.getEntitiesForPhysicalGroup(2, tag):
            _, _, enodes = gmsh.model.mesh.getElements(2, ent)
            for n in enodes[0].reshape(-1, 3):
                xyz = [gmsh.model.mesh.getNode(int(v))[0][:2] for v in n]
                cx.append(np.mean(xyz, axis=0))
        c = np.mean(cx, axis=0)
        k = round((math.atan2(c[1], c[0]) - STATOR_FRAME0) / SLOT_PITCH)
        div = curves0[f"slot{k}.coil_divider"]
        side = "inner" if slot_frame_x(c, k) < slot_frame_x(div["a"], k) else "outer"
        require((k, side) not in out, f"two groups on slot {k} {side}")
        out[(k, side)] = tag
    gmsh.finalize()
    require(len(out) == 12, "winding groups do not cover the 12 coil sides")
    return out


def build(design, level, n_base, out_dir, wtags):
    import gmsh

    curves = {c["name"]: c for c in design["curves"]}
    n_seg = n_base * 2 ** level
    ds = bsm.R_C * SECTOR / n_seg
    h_max = 6.0 * ds
    gmsh.initialize(["-noenv"])
    gmsh.option.setNumber("General.Terminal", 0)
    name = f"icem_remesh_{design['name']}_L{level}"
    gmsh.model.add(name)
    occ = gmsh.model.occ
    rotor = Part(occ, "rotor", dict(curves))
    stator = Part(occ, "stator", dict(curves))

    # ---- rotor: radial edges split at the rotor surface
    surf = curves["rotor.surface"]
    for side, end in (("lo", "a"), ("hi", "b")):
        e = curves[f"rotor.edge_{side}"]
        rotor.line(f"rotor.edge_{side}_iron", e["a"], surf[end])
        rotor.line(f"rotor.edge_{side}_gap", surf[end], e["b"])
    rotor.curves["rotor.coupling"] = dict(curves["coupling"])
    surfaces = {}
    r = "rotor."
    surfaces["magnet"] = [rotor.surface([r + "magnet_inner", r + "magnet_side_hi",
                                         r + "magnet_outer", r + "magnet_side_lo"])]
    surfaces["rotor_air"] = [
        rotor.surface([r + f"pocket_{s}_inner", r + f"magnet_side_{s}",
                       r + f"pocket_{s}_outer", r + f"pocket_{s}_bridge"]) for s in ("lo", "hi")]
    surfaces["airgap_inner"] = [rotor.surface([r + "surface", r + "edge_lo_gap",
                                               r + "coupling", r + "edge_hi_gap"])]
    surfaces["rotor_iron"] = [rotor.surface(
        [r + "shaft", r + "edge_lo_iron", r + "surface", r + "edge_hi_iron"],
        [[r + "magnet_inner", r + "pocket_lo_inner", r + "pocket_lo_bridge",
          r + "pocket_lo_outer", r + "magnet_outer", r + "pocket_hi_outer",
          r + "pocket_hi_bridge", r + "pocket_hi_inner"]])]

    # ---- stator
    stator.curves["stator.coupling"] = dict(curves["coupling"])
    band = ["stator.coupling", "stator.edge_lo_gap", "stator.edge_hi_gap"]
    iron = ["stator.edge_lo_iron", "stator.outer", "stator.edge_hi_iron"]
    surfaces["slot_air"] = []
    windings = {}
    for k in range(SLOTS):
        p = f"slot{k}."
        band += [p + "bore_lo", p + "bore_open", p + "bore_hi"]
        iron += [p + n for n in ("bore_lo", "channel_lo", "tip_lo", "flank_lo_in", "flank_lo_out",
                                 "fillet_lo", "bottom", "fillet_hi", "flank_hi_out", "flank_hi_in",
                                 "tip_hi", "channel_hi", "bore_hi")]
        surfaces["slot_air"].append(stator.surface(
            [p + "bore_open", p + "channel_hi", p + "opening_end", p + "channel_lo"]))
        windings[wtags[(k, "inner")]] = stator.surface(
            [p + "opening_end", p + "tip_hi", p + "flank_hi_in", p + "coil_divider",
             p + "flank_lo_in", p + "tip_lo"])
        windings[wtags[(k, "outer")]] = stator.surface(
            [p + "coil_divider", p + "flank_hi_out", p + "fillet_hi", p + "bottom",
             p + "fillet_lo", p + "flank_lo_out"])
    surfaces["airgap_outer"] = [stator.surface(band)]
    surfaces["stator_iron"] = [stator.surface(iron)]
    occ.synchronize()

    def phys(dim, tags, tag, pname):
        gmsh.model.addPhysicalGroup(dim, tags, tag)
        gmsh.model.setPhysicalName(dim, tag, pname)

    region_tags = {"rotor_iron": bsm.TAG_ROTOR_IRON, "magnet": bsm.TAG_MAGNET,
                   "rotor_air": bsm.TAG_ROTOR_AIR, "airgap_inner": bsm.TAG_GAP_INNER,
                   "airgap_outer": bsm.TAG_GAP_OUTER, "stator_iron": bsm.TAG_STATOR_IRON,
                   "slot_air": bsm.TAG_SLOT_AIR}
    for rname, tag in region_tags.items():
        phys(2, surfaces[rname], tag, rname)
    for tag, s in sorted(windings.items()):
        phys(2, [s], tag, f"winding_{tag - bsm.TAG_WINDING_BASE:02d}")
    c = lambda part, n: part.cache[n][0]  # noqa: E731
    phys(1, [c(rotor, "rotor.shaft")], bsm.TAG_DIRICHLET_INNER, "dirichlet_inner")
    phys(1, [c(stator, "stator.outer")], bsm.TAG_DIRICHLET_OUTER, "dirichlet_outer")
    edges = {
        "rotor_edge_lo": (bsm.TAG_ROTOR_EDGE_LO, [c(rotor, "rotor.edge_lo_iron"),
                                                  c(rotor, "rotor.edge_lo_gap")]),
        "rotor_edge_hi": (bsm.TAG_ROTOR_EDGE_HI, [c(rotor, "rotor.edge_hi_iron"),
                                                  c(rotor, "rotor.edge_hi_gap")]),
        "stator_edge_lo": (bsm.TAG_STATOR_EDGE_LO, [c(stator, "stator.edge_lo_gap"),
                                                    c(stator, "stator.edge_lo_iron")]),
        "stator_edge_hi": (bsm.TAG_STATOR_EDGE_HI, [c(stator, "stator.edge_hi_gap"),
                                                    c(stator, "stator.edge_hi_iron")]),
    }
    for ename, (tag, cs) in edges.items():
        phys(1, cs, tag, ename)
    arcs = (c(rotor, "rotor.coupling"), c(stator, "stator.coupling"))
    phys(1, [arcs[0]], bsm.TAG_COUPLING_ROTOR, "coupling_rotor")
    phys(1, [arcs[1]], bsm.TAG_COUPLING_STATOR, "coupling_stator")
    for lo, hi in (("rotor_edge_lo", "rotor_edge_hi"), ("stator_edge_lo", "stator_edge_hi")):
        gmsh.model.mesh.setPeriodic(1, edges[hi][1], edges[lo][1], bsm.rot(SECTOR))
    for arc in arcs:
        gmsh.model.mesh.setTransfiniteCurve(arc, n_seg + 1)

    # Mesh-size rules of build_sector_mesh.build_level, unchanged.
    f_dist = gmsh.model.mesh.field.add("Distance")
    gmsh.model.mesh.field.setNumbers(f_dist, "CurvesList", list(arcs))
    gmsh.model.mesh.field.setNumber(f_dist, "Sampling", 4 * n_seg)
    f_thr = gmsh.model.mesh.field.add("Threshold")
    gmsh.model.mesh.field.setNumber(f_thr, "InField", f_dist)
    gmsh.model.mesh.field.setNumber(f_thr, "SizeMin", ds)
    gmsh.model.mesh.field.setNumber(f_thr, "SizeMax", h_max)
    gmsh.model.mesh.field.setNumber(f_thr, "DistMin", 1.0e-3)
    gmsh.model.mesh.field.setNumber(f_thr, "DistMax", 1.0e-2)
    gmsh.model.mesh.field.setAsBackgroundMesh(f_thr)
    gmsh.option.setNumber("Mesh.MeshSizeFromPoints", 0)
    gmsh.option.setNumber("Mesh.MeshSizeExtendFromBoundary", 0)
    gmsh.option.setNumber("Mesh.MeshSizeFromCurvature", 8 * 2 ** level)
    gmsh.option.setNumber("Mesh.MeshSizeMax", h_max)
    gmsh.option.setNumber("Mesh.Algorithm", 6)
    gmsh.option.setNumber("Mesh.RandomSeed", 1)
    gmsh.option.setNumber("General.NumThreads", 1)
    gmsh.option.setNumber("Mesh.Optimize", 1)
    gmsh.option.setNumber("Mesh.MshFileVersion", 4.1)
    gmsh.option.setNumber("Mesh.Binary", 0)
    gmsh.option.setNumber("Mesh.SaveAll", 0)
    gmsh.model.mesh.generate(2)

    node_tags, coords, _ = gmsh.model.mesh.getNodes()
    xyz = dict(zip(node_tags.tolist(), coords.reshape(-1, 3)[:, :2]))
    tris = []
    for _dim, tag in gmsh.model.getPhysicalGroups(2):
        for ent in gmsh.model.getEntitiesForPhysicalGroup(2, tag):
            _, _, en = gmsh.model.mesh.getElements(2, ent)
            tris.append(en[0].reshape(-1, 3))
    tris = np.vstack(tris)
    P = np.array([[xyz[n] for n in t] for t in tris.tolist()])
    a = np.linalg.norm(P[:, 1] - P[:, 2], axis=1)
    b = np.linalg.norm(P[:, 0] - P[:, 2], axis=1)
    cc = np.linalg.norm(P[:, 0] - P[:, 1], axis=1)
    ang = np.degrees(np.arccos(np.clip((b * b + cc * cc - a * a) / (2 * b * cc), -1, 1)))
    ang = np.minimum(ang, np.degrees(np.arccos(np.clip((a * a + cc * cc - b * b) / (2 * a * cc), -1, 1))))
    ang = np.minimum(ang, np.degrees(np.arccos(np.clip((a * a + b * b - cc * cc) / (2 * a * b), -1, 1))))
    out_dir.mkdir(parents=True, exist_ok=True)
    path = out_dir / f"{name}.msh"
    gmsh.write(str(path))
    gmsh.finalize()
    return {"design": design["name"], "level": level, "file": path.name,
            "nodes": len(set(tris.ravel().tolist())), "triangles": int(len(tris)),
            "min_angle_deg": float(ang.min())}


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--plan", type=Path, required=True)
    ap.add_argument("--levels", type=int, nargs="+", default=[0])
    ap.add_argument("--n-base", type=int, default=120)
    ap.add_argument("--designs", nargs="*", default=None)
    ap.add_argument("--out", type=Path, required=True)
    ap.add_argument("--summary", type=Path, default=None)
    args = ap.parse_args()
    plan = json.loads(args.plan.read_text())
    designs = plan["designs"]
    if args.designs:
        designs = [d for d in designs if d["name"] in set(args.designs)]
    theta0 = next(d for d in plan["designs"] if d["name"] == "theta0")
    wtags = winding_tags({c["name"]: c for c in theta0["curves"]})
    rows = []
    for level in args.levels:
        for d in designs:
            rep = build(d, level, args.n_base, args.out, wtags)
            rows.append(rep)
            print(f"{rep['file']}: nodes {rep['nodes']} tris {rep['triangles']} "
                  f"min angle {rep['min_angle_deg']:.2f} deg", flush=True)
    if args.summary:
        args.summary.write_text(json.dumps(
            {"issue": 1177, "winding_tags": {f"slot{k}_{s}": t for (k, s), t in wtags.items()},
             "meshes": rows}, indent=2) + "\n")


if __name__ == "__main__":
    main()
