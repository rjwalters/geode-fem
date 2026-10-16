#!/usr/bin/env python3
"""Build the tagged gmsh 60-degree sector mesh family of the ICEM initial design.

    uv run --with numpy==2.5.4 --with gmsh==4.15.2 \\
        python -I benchmarks/motor_reference/geometry/build_sector_mesh.py \\
        --levels 0 1 2 3 --out /path/to/meshes

Inputs are the hash-pinned GeoPDEs exports RotorOpt0.txt / StatorOpt0.txt
(fetched into ../_upstream/ by ../extract_reference.py --fetch). Both files
are hash-checked against ../sources.json before parsing; they are read as
data only (see geopdes_nurbs.py, written independently for GEODE).

Geometry is built in gmsh's OpenCASCADE kernel with every patch side as an
exact rational B-spline (gmsh.model.occ.addBSpline with the exported degree,
knots and weights), straight degree-1 sides as lines, and the coupling
circle r_c as an exact circular arc. The two air-gap band patch groups
(rotor patches 13-17, stator patches 1-3 of every slot) are each merged into
one surface so that the coupling arc carries exactly N uniform segments
(transfinite) with no interior breakpoints. Rotor and stator are disjoint
in the mesh: each side of the coupling circle has its own N+1 nodes, at
identical positions at rotor angle 0 (sliding-band convention, #1161).

Each level writes ``icem_initial_sector_L{level}.msh`` (MSH 4.1 ASCII) and a
``..._report.json`` with areas, quality and family statistics. See README.md
for the tag table and conventions.
"""

import argparse
import hashlib
import json
import math
import sys
from pathlib import Path

import numpy as np

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))  # sibling module only; -I keeps cwd off sys.path

import geopdes_nurbs as gn  # noqa: E402

REF = HERE.parent
UPSTREAM = REF / "_upstream"
ROTOR_EXPORT = "results/plots/final/RotorOpt0.txt"
STATOR_EXPORT = "results/plots/final/StatorOpt0.txt"

# Radii of the initial design (conventions.json, meters).
R_I, R_ROTOR, R_C, R_BORE, R_O = 0.016, 0.044, 0.0445, 0.045, 0.0675
SECTOR = math.pi / 3
THETA_LO, THETA_HI = math.pi / 3, 2 * math.pi / 3

# ---------------------------------------------------------------- tag table
# 2-D physical groups (triangles).
TAG_ROTOR_IRON, TAG_MAGNET, TAG_ROTOR_AIR = 1, 2, 3
TAG_GAP_INNER, TAG_GAP_OUTER = 4, 5
TAG_STATOR_IRON, TAG_SLOT_AIR = 6, 7
TAG_WINDING_BASE = 10  # winding group g (1..12) -> tag 10 + g
# 1-D physical groups (line elements).
TAG_DIRICHLET_INNER, TAG_DIRICHLET_OUTER = 101, 102
TAG_ROTOR_EDGE_LO, TAG_ROTOR_EDGE_HI = 111, 112
TAG_STATOR_EDGE_LO, TAG_STATOR_EDGE_HI = 113, 114
TAG_COUPLING_ROTOR, TAG_COUPLING_STATOR = 121, 122

# Patch -> material, one-based, from the upstream generators' default
# ("nonlinear") patch lists (geometries/PSMS/PMSM_rotor1.m, PMSM_stator1.m).
# Note: rotor patches 2 and 5 are commented "air slit" upstream but are iron
# in the nonlinear type used by the optimization.
ROTOR_REGIONS = {
    "rotor_iron": (TAG_ROTOR_IRON, [1, 2, 3, 4, 5, 6, 9, 10, 11]),
    "magnet": (TAG_MAGNET, [8]),
    "rotor_air": (TAG_ROTOR_AIR, [7, 12]),
    "airgap_inner": (TAG_GAP_INNER, [13, 14, 15, 16, 17]),
}
STATOR_SLOT_PATCHES = 19
STATOR_SLOTS = 6
STATOR_BASE = {
    "airgap_outer": (TAG_GAP_OUTER, [1, 2, 3]),
    "stator_iron": (TAG_STATOR_IRON, [4, 6, 15, 16, 17, 18, 19]),
    "slot_air": (TAG_SLOT_AIR, [5]),
}
STATOR_WINDING_BASE = list(range(7, 15))
MERGED = {"airgap_inner", "airgap_outer"}


def require(cond, msg):
    if not cond:
        raise SystemExit(f"FAIL: {msg}")


def verify_sources():
    manifest = json.loads((REF / "sources.json").read_text())
    out = {}
    for entry in manifest["files"]:
        if entry["path"] in (ROTOR_EXPORT, STATOR_EXPORT):
            raw = (UPSTREAM / entry["path"]).read_bytes()
            digest = hashlib.sha256(raw).hexdigest()
            require(len(raw) == entry["bytes"] and digest == entry["sha256"],
                    f"hash mismatch for {entry['path']}; rerun extract_reference.py --fetch")
            out[entry["path"]] = digest
    require(len(out) == 2, "exports missing from sources.json")
    return manifest["commit"], out


def winding_groups():
    data = json.loads((REF / "reference_data.json").read_text())
    return data["winding_groups_one_based_stator_patch_ids"]


def stator_regions():
    regions = {}
    for name, (tag, base) in STATOR_BASE.items():
        regions[name] = (tag, [b + STATOR_SLOT_PATCHES * k
                               for k in range(STATOR_SLOTS) for b in base])
    groups = winding_groups()
    require(len(groups) == 12, "expected 12 winding groups")
    expected = sorted(b + STATOR_SLOT_PATCHES * k
                      for k in range(STATOR_SLOTS) for b in STATOR_WINDING_BASE)
    got = sorted(p for g in groups for p in g["patches"])
    require(got == expected, "winding groups do not partition the winding patches")
    for g, grp in enumerate(groups, start=1):
        name = f"winding_{g:02d}_{grp['phase']}{'+' if grp['sign'] > 0 else '-'}"
        regions[name] = (TAG_WINDING_BASE + g, list(grp["patches"]))
    return regions, groups


# ------------------------------------------------------------ exact areas
def patch_area_quadrature(patch, nq=10):
    """Independent area check: Gauss tensor quadrature of |det J| per knot span."""
    x, wq = gn.gauss_legendre(nq)
    (pu, pv), (ku, kv) = patch.degree, patch.knots
    total = 0.0
    hw = patch.cp * patch.w[..., None]
    span_u = gn.knot_spans(gn.NurbsCurve(pu, ku, patch.cp[0], patch.w[0]))
    span_v = gn.knot_spans(gn.NurbsCurve(pv, kv, patch.cp[:, 0], patch.w[:, 0]))
    for ua, ub in span_u:
        us = 0.5 * (ub - ua) * x + 0.5 * (ua + ub)
        for va, vb in span_v:
            vs = 0.5 * (vb - va) * x + 0.5 * (va + vb)
            jac = 0.25 * (ub - ua) * (vb - va)
            for i, u in enumerate(us):
                su = gn._find_span(ku, pu, u)
                Nu, dNu = gn._basis_and_deriv(ku, pu, su, u)
                for j, v in enumerate(vs):
                    sv = gn._find_span(kv, pv, v)
                    Nv, dNv = gn._basis_and_deriv(kv, pv, sv, v)
                    blk = hw[sv - pv:sv + 1, su - pu:su + 1]
                    wb = patch.w[sv - pv:sv + 1, su - pu:su + 1]
                    A = np.einsum("j,i,jik->k", Nv, Nu, blk)
                    W = Nv @ wb @ Nu
                    Au = np.einsum("j,i,jik->k", Nv, dNu, blk)
                    Wu = Nv @ wb @ dNu
                    Av = np.einsum("j,i,jik->k", dNv, Nu, blk)
                    Wv = dNv @ wb @ Nu
                    P = A / W
                    Pu = (Au - P * Wu) / W
                    Pv = (Av - P * Wv) / W
                    total += wq[i] * wq[j] * jac * (Pu[0] * Pv[1] - Pu[1] * Pv[0])
    return total


def region_exact_areas(mp, regions):
    green = {p.index: abs(gn.patch_area(p, nq=16)) for p in mp.patches}
    green_lo = {p.index: abs(gn.patch_area(p, nq=8)) for p in mp.patches}
    quad = {p.index: abs(patch_area_quadrature(p)) for p in mp.patches}
    out = {}
    for name, (tag, ids) in regions.items():
        a = sum(green[i] for i in ids)
        out[name] = {
            "tag": tag,
            "patches": ids,
            "area_green_m2": a,
            "green_nq8_vs_nq16_rel": abs(sum(green_lo[i] for i in ids) - a) / a,
            "tensor_quadrature_vs_green_rel": abs(sum(quad[i] for i in ids) - a) / a,
        }
    return out


# ------------------------------------------------------------ topology
class Part:
    """One multipatch (rotor or stator) turned into OCC points/curves/surfaces."""

    def __init__(self, occ, name, mp, regions, coupling_ids, side_tol=1e-12):
        self.occ, self.name, self.mp, self.regions = occ, name, mp, regions
        self.coupling = {ps for bid, sides in mp.boundaries if bid in coupling_ids
                         for ps in sides}
        self.points = []  # (tag, xy)
        self.point_merge_max = 0.0
        self.side_tol = side_tol
        self.curve_of = {}  # (patch, side) -> (curve tag, v_start, v_end) along side param
        self.curve_ends = {}  # curve tag -> (v_start, v_end)
        self.shared_cp_dev = 0.0
        partner = {}
        for _, a, b, _flag in mp.interfaces:
            partner[a] = b
            partner[b] = a
        self.partner = partner
        self.surfaces = {}  # region name -> [surface tags]
        self.coupling_arc = None

    def point(self, xy):
        for tag, q in self.points:
            d = math.hypot(xy[0] - q[0], xy[1] - q[1])
            if d < 1e-9:
                self.point_merge_max = max(self.point_merge_max, d)
                return tag
        tag = self.occ.addPoint(float(xy[0]), float(xy[1]), 0.0)
        self.points.append((tag, (float(xy[0]), float(xy[1]))))
        return tag

    def side(self, key):
        """gmsh curve for patch side ``key`` (created once per shared side)."""
        if key in self.curve_of:
            return self.curve_of[key]
        c = gn.side_curve(self.mp.patches[key[0] - 1], key[1])
        P, _ = gn.eval_curve(c, [c.knots[0], c.knots[-1]])
        v0, v1 = self.point(P[0]), self.point(P[1])
        other = self.partner.get(key)
        if other is not None and other in self.curve_of:
            tag, a, b = self.curve_of[other]
            oc = gn.side_curve(self.mp.patches[other[0] - 1], other[1])
            same = (a, b) == (v0, v1)
            require(same or (a, b) == (v1, v0), f"{self.name} {key}/{other}: endpoints differ")
            cmp = oc if same else oc.reversed()
            if len(cmp.w) == len(c.w):
                dev = float(np.max(np.abs(cmp.cp - c.cp)))
            else:
                dev = math.inf
            ts = np.linspace(c.knots[0], c.knots[-1], 41)
            Pc, _ = gn.eval_curve(c, ts)
            Po, _ = gn.eval_curve(cmp, np.linspace(cmp.knots[0], cmp.knots[-1], 41))
            dev = min(dev, float(np.max(np.hypot(*(Pc - Po).T))))
            self.shared_cp_dev = max(self.shared_cp_dev, dev)
            require(dev < self.side_tol, f"{self.name} {key}/{other}: shared side mismatch {dev}")
            self.curve_of[key] = (tag, v0, v1)
            return self.curve_of[key]
        if c.degree == 1 and len(c.w) == 2:
            tag = self.occ.addLine(v0, v1)
        else:
            inner = [self.occ.addPoint(float(x), float(y), 0.0) for x, y in c.cp[1:-1]]
            knots, mults = np.unique(c.knots, return_counts=True)
            tag = self.occ.addBSpline([v0] + inner + [v1], degree=c.degree,
                                      weights=[float(w) for w in c.w],
                                      knots=[float(k) for k in knots],
                                      multiplicities=[int(m) for m in mults])
            self.occ.remove([(0, t) for t in inner])
        self.curve_of[key] = (tag, v0, v1)
        self.curve_ends[tag] = (v0, v1)
        return self.curve_of[key]

    def loop(self, edges):
        """Chain undirected (curve, va, vb) edges into one closed signed loop."""
        edges = list(edges)
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
                raise SystemExit(f"FAIL: {self.name}: open boundary loop")
        require(cur == start, f"{self.name}: loop does not close")
        return self.occ.addCurveLoop(signed)

    def build(self):
        for name, (_tag, ids) in self.regions.items():
            if name in MERGED:
                members = set(ids)
                edges, arc_ends = [], []
                for pid in ids:
                    for s in range(1, 5):
                        key = (pid, s)
                        other = self.partner.get(key)
                        if other is not None and other[0] in members:
                            continue  # internal to the merged band
                        if key in self.coupling:
                            c = gn.side_curve(self.mp.patches[pid - 1], s)
                            P, _ = gn.eval_curve(c, [c.knots[0], c.knots[-1]])
                            arc_ends.extend(P)
                            continue
                        edges.append(self.side(key))
                ang = [math.atan2(p[1], p[0]) for p in arc_ends]
                lo = arc_ends[int(np.argmin(ang))]
                hi = arc_ends[int(np.argmax(ang))]
                v_lo, v_hi = self.point(lo), self.point(hi)
                centre = self.occ.addPoint(0.0, 0.0, 0.0)
                arc = self.occ.addCircleArc(v_lo, centre, v_hi)
                self.occ.remove([(0, centre)])
                self.coupling_arc = arc
                self.curve_ends[arc] = (v_lo, v_hi)
                edges.append((arc, v_lo, v_hi))
                self.surfaces[name] = [self.occ.addPlaneSurface([self.loop(edges)])]
            else:
                tags = []
                for pid in ids:
                    edges = [self.side((pid, s)) for s in (3, 2, 4, 1)]
                    tags.append(self.occ.addPlaneSurface([self.loop(edges)]))
                self.surfaces[name] = tags

    def boundary_curves(self, ids):
        out = []
        for bid, sides in self.mp.boundaries:
            if bid in ids:
                for key in sides:
                    out.append(self.curve_of[key][0])
        return out


# ------------------------------------------------------------ mesh build
def rot(theta):
    c, s = math.cos(theta), math.sin(theta)
    return [c, -s, 0, 0, s, c, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1]


def gmsh_green_area(gmsh, surface, n_sub=256, n_q=8):
    """Area of a gmsh surface by Green's theorem on its gmsh (OCC) boundary curves.

    Composite Gauss-Legendre over each curve's parameter range; checks that the
    CAD model handed to the mesher encloses the exact NURBS area. (OCC's own
    ``getMass`` is not used as the check: its integration tolerance is ~1e-4.)
    """
    x, wq = np.polynomial.legendre.leggauss(n_q)
    total = 0.0
    for dim, ctag in gmsh.model.getBoundary([(2, surface)], oriented=True):
        lo, hi = gmsh.model.getParametrizationBounds(1, abs(ctag))
        edges = np.linspace(lo[0], hi[0], n_sub + 1)
        ts = (0.5 * (edges[1:] - edges[:-1])[:, None] * x
              + 0.5 * (edges[1:] + edges[:-1])[:, None]).ravel()
        wt = (0.5 * (edges[1:] - edges[:-1])[:, None] * wq).ravel()
        P = np.array(gmsh.model.getValue(1, abs(ctag), ts)).reshape(-1, 3)
        D = np.array(gmsh.model.getDerivative(1, abs(ctag), ts)).reshape(-1, 3)
        val = np.sum(wt * 0.5 * (P[:, 0] * D[:, 1] - P[:, 1] * D[:, 0]))
        total += val if ctag > 0 else -val
    return abs(total)


def radial_order(gmsh, curves):
    def rmid(t):
        lo, hi = gmsh.model.getParametrizationBounds(1, t)
        x = gmsh.model.getValue(1, t, [0.5 * (lo[0] + hi[0])])
        return math.hypot(x[0], x[1])
    return sorted(curves, key=rmid)


def build_level(level, n_base, out_dir, conv, provenance, exact):
    import gmsh

    n_seg = n_base * 2 ** level
    ds = R_C * SECTOR / n_seg
    h_max = 6.0 * ds
    gmsh.initialize(["-noenv"])
    gmsh.option.setNumber("General.Terminal", 0)
    gmsh.model.add(f"icem_initial_sector_L{level}")
    occ = gmsh.model.occ
    rotor_mp = gn.read_multipatch(UPSTREAM / ROTOR_EXPORT)
    stator_mp = gn.read_multipatch(UPSTREAM / STATOR_EXPORT)
    bnd = conv["boundaries"]
    s_regions, _ = stator_regions()
    rotor = Part(occ, "rotor", rotor_mp, ROTOR_REGIONS, set(bnd["rotor_coupling"]))
    stator = Part(occ, "stator", stator_mp, s_regions, set(bnd["stator_coupling"]))
    rotor.build()
    stator.build()
    occ.synchronize()

    def phys(dim, tags, tag, name):
        gmsh.model.addPhysicalGroup(dim, tags, tag)
        gmsh.model.setPhysicalName(dim, tag, name)

    for part in (rotor, stator):
        for name, (tag, _ids) in part.regions.items():
            phys(2, part.surfaces[name], tag, name)
    phys(1, rotor.boundary_curves(set(bnd["rotor_dirichlet"])), TAG_DIRICHLET_INNER,
         "dirichlet_inner")
    phys(1, stator.boundary_curves(set(bnd["stator_dirichlet"])), TAG_DIRICHLET_OUTER,
         "dirichlet_outer")
    r_hi, r_lo = bnd["rotor_antiperiodic_pair"]  # [5,2,11] at 120 deg, [3,1,6] at 60 deg
    s_hi, s_lo = bnd["stator_antiperiodic_pair"]  # [27,29,30,31] at 120, [1,5,6,7] at 60
    edges = {
        "rotor_edge_lo": (TAG_ROTOR_EDGE_LO, radial_order(gmsh, rotor.boundary_curves(set(r_lo)))),
        "rotor_edge_hi": (TAG_ROTOR_EDGE_HI, radial_order(gmsh, rotor.boundary_curves(set(r_hi)))),
        "stator_edge_lo": (TAG_STATOR_EDGE_LO,
                           radial_order(gmsh, stator.boundary_curves(set(s_lo)))),
        "stator_edge_hi": (TAG_STATOR_EDGE_HI,
                           radial_order(gmsh, stator.boundary_curves(set(s_hi)))),
    }
    for name, (tag, curves) in edges.items():
        phys(1, curves, tag, name)
    phys(1, [rotor.coupling_arc], TAG_COUPLING_ROTOR, "coupling_rotor")
    phys(1, [stator.coupling_arc], TAG_COUPLING_STATOR, "coupling_stator")

    # Antiperiodic edges: slave (theta = 120 deg) = master (60 deg) rotated +60 deg.
    for lo, hi in (("rotor_edge_lo", "rotor_edge_hi"), ("stator_edge_lo", "stator_edge_hi")):
        gmsh.model.mesh.setPeriodic(1, edges[hi][1], edges[lo][1], rot(SECTOR))
    for arc in (rotor.coupling_arc, stator.coupling_arc):
        gmsh.model.mesh.setTransfiniteCurve(arc, n_seg + 1)

    f_dist = gmsh.model.mesh.field.add("Distance")
    gmsh.model.mesh.field.setNumbers(f_dist, "CurvesList",
                                     [rotor.coupling_arc, stator.coupling_arc])
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
    gmsh.option.setNumber("General.NumThreads", 1)  # deterministic output
    gmsh.option.setNumber("Mesh.Optimize", 1)
    gmsh.option.setNumber("Mesh.MshFileVersion", 4.1)
    gmsh.option.setNumber("Mesh.Binary", 0)
    gmsh.option.setNumber("Mesh.SaveAll", 0)
    gmsh.model.mesh.generate(2)

    report = analyse(gmsh, level, n_seg, ds, h_max, rotor, stator, edges, exact)
    report["provenance"] = provenance
    out_dir.mkdir(parents=True, exist_ok=True)
    msh = out_dir / f"icem_initial_sector_L{level}.msh"
    gmsh.write(str(msh))
    report["msh_file"] = msh.name
    report["msh_bytes"] = msh.stat().st_size
    report["msh_sha256"] = hashlib.sha256(msh.read_bytes()).hexdigest()
    gmsh.finalize()
    (out_dir / f"icem_initial_sector_L{level}_report.json").write_text(
        json.dumps(report, indent=2) + "\n")
    return report


def analyse(gmsh, level, n_seg, ds, h_max, rotor, stator, edges, exact):
    mesh = gmsh.model.mesh
    node_tags, coords, _ = mesh.getNodes()
    xyz = dict(zip(node_tags.tolist(), coords.reshape(-1, 3)[:, :2]))
    rep = {"level": level, "coupling_segments_N": n_seg,
           "coupling_spacing_deg": 60.0 / n_seg, "size_min_m": ds, "size_max_m": h_max,
           "gmsh_version": gmsh.option.getString("General.Version"),
           "point_merge_max_m": max(rotor.point_merge_max, stator.point_merge_max),
           "shared_side_max_dev_m": max(rotor.shared_cp_dev, stator.shared_cp_dev)}
    regions, all_tris, tri_region = {}, [], []
    for part in (rotor, stator):
        for name, (tag, _ids) in part.regions.items():
            tris = []
            cad_area = 0.0
            for s in part.surfaces[name]:
                cad_area += gmsh_green_area(gmsh, s)
                _, _, enodes = mesh.getElements(2, s)
                require(len(enodes) == 1, f"{name}: non-triangle elements")
                tris.append(enodes[0].reshape(-1, 3))
            tris = np.vstack(tris)
            P = np.array([[xyz[n] for n in t] for t in tris.tolist()])
            d1, d2 = P[:, 1] - P[:, 0], P[:, 2] - P[:, 0]
            signed = 0.5 * (d1[:, 0] * d2[:, 1] - d1[:, 1] * d2[:, 0])
            a_ex = exact[name]["area_green_m2"]
            regions[name] = {
                "tag": tag, "triangles": len(tris),
                "area_exact_m2": a_ex,
                "area_cad_rel_err": (cad_area - a_ex) / a_ex,
                "area_mesh_m2": float(np.abs(signed).sum()),
                "area_mesh_rel_err": float((np.abs(signed).sum() - a_ex) / a_ex),
            }
            all_tris.append(tris)
            tri_region += [tag] * len(tris)
    tris = np.vstack(all_tris)
    P = np.array([[xyz[n] for n in t] for t in tris.tolist()])
    # interior angles, gamma quality (2*inradius/circumradius normalised to 1)
    a = np.linalg.norm(P[:, 1] - P[:, 2], axis=1)
    b = np.linalg.norm(P[:, 0] - P[:, 2], axis=1)
    c = np.linalg.norm(P[:, 0] - P[:, 1], axis=1)
    ang = np.degrees(np.stack([
        np.arccos(np.clip((b * b + c * c - a * a) / (2 * b * c), -1, 1)),
        np.arccos(np.clip((a * a + c * c - b * b) / (2 * a * c), -1, 1)),
        np.arccos(np.clip((a * a + b * b - c * c) / (2 * a * b), -1, 1))], axis=1))
    s = 0.5 * (a + b + c)
    area = np.sqrt(np.maximum(s * (s - a) * (s - b) * (s - c), 0))
    gamma = 4 * math.sqrt(3) * area / (a * a + b * b + c * c)  # 1 for equilateral
    rep["mesh"] = {
        "nodes": len(set(tris.ravel().tolist())), "triangles": int(len(tris)),
        "min_angle_deg": float(ang.min()), "max_angle_deg": float(ang.max()),
        "min_angle_location_m": P[int(np.argmin(ang.min(axis=1)))].mean(axis=0).tolist(),
        "min_angle_region_tag": int(tri_region[int(np.argmin(ang.min(axis=1)))]),
        "quality_measure": "4*sqrt(3)*area/sum(edge^2), 1 = equilateral",
        "quality_min": float(gamma.min()), "quality_mean": float(gamma.mean()),
        "quality_p01": float(np.quantile(gamma, 0.01)),
        "frac_min_angle_below_20deg": float(np.mean(ang.min(axis=1) < 20)),
        "h_max_edge_m": float(np.max(np.stack([a, b, c]))),
        "h_min_edge_m": float(np.min(np.stack([a, b, c]))),
    }
    # conformity: every edge in <=2 triangles; boundary edges = tagged lines.
    from collections import Counter
    ecount = Counter()
    for t in tris.tolist():
        for i, j in ((0, 1), (1, 2), (2, 0)):
            ecount[tuple(sorted((t[i], t[j])))] += 1
    require(max(ecount.values()) <= 2, "edge in >2 triangles (overlap)")
    boundary_edges = {e for e, k in ecount.items() if k == 1}
    tagged = set()
    line_groups = {}
    for dim, ptag in gmsh.model.getPhysicalGroups(1):
        name = gmsh.model.getPhysicalName(1, ptag)
        segs = []
        for ent in gmsh.model.getEntitiesForPhysicalGroup(1, ptag):
            _, _, en = mesh.getElements(1, ent)
            segs += en[0].reshape(-1, 2).tolist()
        line_groups[name] = (ptag, segs)
        tagged |= {tuple(sorted(e)) for e in segs}
    require(boundary_edges == tagged, "mesh boundary differs from tagged lines (gap)")
    rep["conformity"] = {"edges": len(ecount), "boundary_edges": len(boundary_edges),
                         "boundary_equals_tagged_lines": True}
    # total area
    total_exact = math.pi * (R_O ** 2 - R_I ** 2) / 6
    total_mesh = sum(r["area_mesh_m2"] for r in regions.values())
    total_nurbs = sum(r["area_exact_m2"] for r in regions.values())
    rep["total_area"] = {"annulus_exact_m2": total_exact,
                         "nurbs_sum_rel_err": (total_nurbs - total_exact) / total_exact,
                         "mesh_sum_rel_err": (total_mesh - total_exact) / total_exact}
    rep["regions"] = regions
    # coupling circle nodes
    cp = {}
    for side in ("coupling_rotor", "coupling_stator"):
        nodes = sorted({n for s in line_groups[side][1] for n in s},
                       key=lambda n: math.atan2(xyz[n][1], xyz[n][0]))
        pts = np.array([xyz[n] for n in nodes])
        th = np.arctan2(pts[:, 1], pts[:, 0])
        grid = THETA_LO + SECTOR * np.arange(n_seg + 1) / n_seg
        cp[side] = pts
        rep[side] = {"nodes": len(nodes), "segments": len(line_groups[side][1]),
                     "max_radius_dev_m": float(np.max(np.abs(np.hypot(*pts.T) - R_C))),
                     "max_angle_dev_rad": float(np.max(np.abs(th - grid)))}
    require(rep["coupling_rotor"]["nodes"] == n_seg + 1, "rotor coupling node count")
    require(rep["coupling_stator"]["nodes"] == n_seg + 1, "stator coupling node count")
    rep["coupling_rotor_vs_stator_max_dev_m"] = float(
        np.max(np.hypot(*(cp["coupling_rotor"] - cp["coupling_stator"]).T)))
    # periodic pairs
    R = np.array(rot(SECTOR)).reshape(4, 4)[:2, :2]
    for part, lo, hi in (("rotor", "rotor_edge_lo", "rotor_edge_hi"),
                         ("stator", "stator_edge_lo", "stator_edge_hi")):
        pairs, dev = 0, 0.0
        for slave in edges[hi][1]:
            _m, s_nodes, m_nodes, _aff = mesh.getPeriodicNodes(1, slave, includeHighOrderNodes=False)
            for sn, mn in zip(s_nodes, m_nodes):
                dev = max(dev, float(np.linalg.norm(R @ xyz[mn] - xyz[sn])))
                pairs += 1
        lo_nodes = {n for s in line_groups[lo][1] for n in s}
        hi_nodes = {n for s in line_groups[hi][1] for n in s}
        require(len(lo_nodes) == len(hi_nodes), f"{part}: unequal edge node counts")
        rep[f"{part}_antiperiodic"] = {"edge_nodes_each_side": len(lo_nodes),
                                       "gmsh_periodic_pairs": pairs,
                                       "max_rotation_mismatch_m": dev}
    rep["line_groups"] = {k: {"tag": v[0], "segments": len(v[1])} for k, v in line_groups.items()}
    return rep


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--levels", type=int, nargs="+", default=[0, 1, 2, 3])
    ap.add_argument("--n-base", type=int, default=120,
                    help="coupling segments at level 0 (doubled per level; multiple of 60)")
    ap.add_argument("--out", type=Path, required=True)
    ap.add_argument("--areas-out", type=Path, default=None,
                    help="write the exact NURBS region-area table (JSON) here")
    args = ap.parse_args()
    require(args.n_base % 60 == 0, "n-base must be a multiple of 60 (1 degree conforming)")
    commit, hashes = verify_sources()
    conv = json.loads((REF / "conventions.json").read_text())
    provenance = {"upstream_repository": "https://github.com/MichaelWiesheu/ICEMsnapshot",
                  "upstream_commit": commit, "exports_sha256": hashes,
                  "license": "upstream data GPL-3.0-or-later; see ../UPSTREAM_LICENSE.txt",
                  "generator": "benchmarks/motor_reference/geometry/build_sector_mesh.py"}
    rotor_mp = gn.read_multipatch(UPSTREAM / ROTOR_EXPORT)
    stator_mp = gn.read_multipatch(UPSTREAM / STATOR_EXPORT)
    s_regions, groups = stator_regions()
    for mp, regs, n in ((rotor_mp, ROTOR_REGIONS, 17), (stator_mp, s_regions, 114)):
        ids = sorted(i for _t, ii in regs.values() for i in ii)
        require(ids == list(range(1, n + 1)), "regions do not partition the patches")
    exact = region_exact_areas(rotor_mp, ROTOR_REGIONS)
    exact.update(region_exact_areas(stator_mp, s_regions))
    if args.areas_out:
        conv_check = gn.check_coordinate_convention(UPSTREAM / ROTOR_EXPORT, [(3, 4, R_ROTOR)])
        mag = exact["magnet"]["area_green_m2"]
        params = json.loads((REF / "reference_data.json").read_text())
        p0 = params["designs"]["initial"]["parameters_by_source_registration_order"]
        i_peak = params["designs"]["initial"]["physical_phase_current_amplitudes_A"][-1]
        windings = []
        for g, grp in enumerate(groups, start=1):
            name = f"winding_{g:02d}_{grp['phase']}{'+' if grp['sign'] > 0 else '-'}"
            ar = exact[name]["area_green_m2"]
            windings.append({"group": g, "name": name, "phase": grp["phase"], "sign": grp["sign"],
                             "patches": grp["patches"], "area_m2": ar,
                             "J_per_A_A_per_m2": 12 * grp["sign"] / ar,
                             "J_at_initial_peak_A_per_m2": 12 * grp["sign"] * i_peak / ar})
        total = sum(v["area_green_m2"] for v in exact.values())
        rot_total = sum(abs(gn.patch_area(p, 16)) for p in rotor_mp.patches)
        sta_total = sum(abs(gn.patch_area(p, 16)) for p in stator_mp.patches)
        table = {
            "provenance": provenance,
            "coordinate_convention_check_max_radius_dev_m": conv_check,
            "regions": exact,
            "magnet_vs_MWxMH": {"area_m2": mag, "MW_x_MH_m2": p0["MW"] * p0["MH"],
                                "rel_err": (mag - p0["MW"] * p0["MH"]) / (p0["MW"] * p0["MH"])},
            "inner_band_vs_annulus": {
                "area_m2": exact["airgap_inner"]["area_green_m2"],
                "annulus_m2": math.pi * (R_C ** 2 - R_ROTOR ** 2) / 6},
            "outer_band_area_m2": exact["airgap_outer"]["area_green_m2"],
            "rotor_total_vs_annulus_rel": (rot_total - math.pi * (R_C ** 2 - R_I ** 2) / 6)
            / (math.pi * (R_C ** 2 - R_I ** 2) / 6),
            "stator_total_vs_annulus_rel": (sta_total - math.pi * (R_O ** 2 - R_C ** 2) / 6)
            / (math.pi * (R_O ** 2 - R_C ** 2) / 6),
            "sector_total_vs_annulus_rel": (total - math.pi * (R_O ** 2 - R_I ** 2) / 6)
            / (math.pi * (R_O ** 2 - R_I ** 2) / 6),
            "winding_groups": windings,
            "initial_peak_current_A": i_peak,
        }
        args.areas_out.write_text(json.dumps(table, indent=2) + "\n")
    for level in args.levels:
        rep = build_level(level, args.n_base, args.out, conv, provenance, exact)
        m = rep["mesh"]
        worst = max(abs(r["area_mesh_rel_err"]) for r in rep["regions"].values())
        print(f"L{level} N={rep['coupling_segments_N']} nodes={m['nodes']} tris={m['triangles']} "
              f"minang={m['min_angle_deg']:.2f} qmin={m['quality_min']:.3f} "
              f"worst|dA/A|={worst:.2e} bytes={rep['msh_bytes']}")


if __name__ == "__main__":
    main()
