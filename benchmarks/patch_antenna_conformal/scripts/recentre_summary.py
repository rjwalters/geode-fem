#!/usr/bin/env python3
"""Re-centred copper design (issue #1134): every model's verdict in one TOML.

Reads the committed artifacts under `benchmarks/patch_antenna_conformal/recentre/`
(the two space-mapping descents of `examples/patch_conformal_nested_dof.rs`,
their verifications on L1 at P1 and P2, on the 0.2 mm-rim mesh and on the
combined 0.1 mm-rim / 0.3 mm-slab mesh, the Palace summary written by
`palace_ingest.py`, and the moved-mesh rim sizes of `rim_edge_stats.py`) plus
the #1126 X1 references under `v6/`, and writes `recentre/recentre_summary.toml`,
which `tests/patch_conformal_recentre.rs` pins.

For each design and model it reports, on the same criteria for every model:

* `worst_in_band_db`: the worst |S11| over the 9 in-band points (2.400 to
  2.4835 GHz in 10.4375 MHz steps). geode solves them; Palace interpolates
  its 1 MHz grid at them (linear in dB).
* the -10 dB band around the dip, crossings interpolated linearly in dB
  between the computed points (geode: 5 MHz grids, on the combined mesh only
  the points either side of each edge; Palace: 1 MHz), its width, its ISM
  coverage, and the frequency margins at the two band edges
  (`margin_lo_mhz` = 2.400 GHz minus the lower crossing, `margin_hi_mhz` =
  the upper crossing minus 2.4835 GHz; positive means covered).

The `[refinement]` table compares the lower-edge margin on the most accurate
model with the X1 estimate of how far the converged answer still lies above
Palace p = 3 (the initial patch's rim-fit limit and order-1 Richardson value
against its Palace p = 3 dip).

Usage: python3 recentre_summary.py [--write]
"""

from __future__ import annotations

import math
import os
import sys
import tomllib

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import copper_g2  # noqa: E402  (bandwidth helper)

ROOT = os.path.normpath(os.path.join(HERE, ".."))
RC = os.path.join(ROOT, "recentre")
V6 = os.path.join(ROOT, "v6")
BAND = (2.4, 2.4835)
DESIGNS = [
    # (tag, role)
    ("sm54", "candidate: band shift 53.6 MHz (44.94 + 8.7, centred for Palace p = 3 by the v5 design's P2 -> p3 shift)"),
    ("sm57", "headline: band shift 57.0 MHz (3.4 MHz further down, biased against the upward drift that refinement still shows)"),
]
# (model key, description, artifact suffixes merged into one point set)
MODELS = [
    ("L1_p1", "P1 on rbG_L1 (the optimization mesh)", ["_on_rbG_L1_dense"]),
    ("L1_p2", "P2 on rbG_L1 (v5's finest model)", ["_on_rbG_L1_p2"]),
    (
        "rim020_p1",
        "P1 on rbGrim020_L1 (0.2 mm rim), default air morph (min volume ratio 0.17)",
        ["_on_rbGrim020_L1_dense"],
    ),
    (
        "rim020_cyl_p1",
        "P1 on rbGrim020_L1 (0.2 mm rim), on-cylinder air morph",
        ["_on_rbGrim020_L1_cyl_dense"],
    ),
    (
        "comb_cyl_p1",
        "P1 on rbGcomb_L1 (0.1 mm rim, 0.3 mm slab: geode's finest mesh), on-cylinder air morph",
        ["_on_rbGcomb_L1_cyl_dense", "_on_rbGcomb_L1_cyl_edge_hi"],
    ),
    (
        "comb_default_rejected",
        "REJECTED: P1 on rbGcomb_L1 with the default air morph, which inverts tets above the "
        "patch rim (min volume ratio < 0); recorded for disclosure, its S11 is not used",
        ["_on_rbGcomb_L1_default_morph_inverted"],
    ),
]


def load(p):
    with open(p, "rb") as f:
        return tomllib.load(f)


def points(paths):
    """Sorted (f GHz, |S11| dB) over the verify and dense points of the files."""
    pts = {}
    for p in paths:
        d = load(p)
        for sec in ("s11_verify", "s11_dense"):
            for q in d.get(sec, {}).get("point", []):
                f = round(q["f_ghz"], 6)
                db = 20.0 * math.log10(math.hypot(q["s11_re"], q["s11_im"]))
                if f in pts:
                    assert abs(pts[f] - db) < 1e-9, f"{p}: inconsistent repeat at {f}"
                pts[f] = db
    return sorted(pts.items())


def model_row(tag, key, desc, suffixes):
    paths = [os.path.join(RC, f"rbG_L1_copper_38_{tag}_w16{s}.toml") for s in suffixes]
    paths = [p for p in paths if os.path.exists(p)]
    if not paths:
        return None
    first = load(paths[0])
    v = first["verify"]
    row = {
        "description": desc,
        "artifacts": [os.path.relpath(p, ROOT) for p in paths],
        "element_order": first["meta"].get("element_order", "p1"),
        "air_motion": first["design"]["air_motion"],
        "n_edges": first["meta"]["n_edges"],
        "worst_in_band_db": v["verify_worst_s11_db"] if v["verify_n_points"] == 9 else None,
        "min_vol_ratio": min(load(p)["verify"]["min_vol_ratio"] for p in paths),
    }
    pts = points(paths)
    row["n_points"] = len(pts)
    if key.endswith("_rejected"):
        return row
    if v.get("dense_n_points", 0) > 0 or len(paths) > 1:
        b = copper_g2.bandwidth(pts)
        # copper_g2 falls back to the grid end for a missing crossing; report
        # nothing rather than a grid-limited width or margin.
        if b.get("bw_lo_ghz") is None or b.get("bw_hi_ghz") is None:
            for k in ("bw_mhz", "ism_cover_mhz", "ism_cover_pct", "centring_mhz"):
                b[k] = None
        if b.get("bw_lo_ghz") is None:
            b["margin_lo_mhz"] = None
        if b.get("bw_hi_ghz") is None:
            b["margin_hi_mhz"] = None
        for k in (
            "dip_f_ghz",
            "bw_lo_ghz",
            "bw_hi_ghz",
            "bw_mhz",
            "ism_cover_mhz",
            "ism_cover_pct",
            "centring_mhz",
            "margin_lo_mhz",
            "margin_hi_mhz",
        ):
            row[k] = b.get(k)
        # Edge crossings bracketed by computed points on both sides?
        fs = [f for f, _ in pts]
        row["edges_bracketed"] = (
            b.get("bw_lo_ghz") is not None
            and b.get("bw_hi_ghz") is not None
            and fs[0] < b["bw_lo_ghz"]
            and fs[-1] > b["bw_hi_ghz"]
        )
    return row


def design_row(tag, role):
    d = load(os.path.join(RC, f"rbG_L1_copper_38_{tag}_w16.toml"))
    o, c, g = d["optimization"], d["copper"], d["guard"]["final_worst"]
    return {
        "role": role,
        "band_shift_mhz": 1e3 * d["meta"]["band_shift_ghz"],
        "start": d["start"]["design_toml"],
        "n_steps": o["n_steps"],
        "n_driven_solves": o["n_driven_solves"],
        "terminal_condition": o["terminal_condition"],
        "worst_shifted_band_db_initial": o["worst_s11_db_initial"],
        "worst_shifted_band_db_final": o["worst_s11_db_final"],
        "min_vol_ratio_budget": o["min_vol_ratio_budget"],
        "worst_vol_ratio": o["worst_vol_ratio"],
        "guard_site": f"{g['region']}, s = {g['s_mm']:.2f} mm, y = {g['y_mm']:.2f} mm, "
        f"{g['outside_patch_mm']:.2f} mm outside the patch rectangle",
        "probe_w_mm": c["probe_w_mm"],
        "inset_mm": c["inset_mm"],
        "patch_length_mm": c["patch_length_mm"],
        "patch_width_mm": c["patch_width_mm"],
        "patch_area_mm2": c["patch_area_mm2"],
    }


def palace_rows():
    p = os.path.join(RC, "palace_summary.toml")
    if not os.path.exists(p):
        return {}
    out = {}
    for name, r in load(p)["run"].items():
        row = dict(r)
        if "band_lo_ghz" in r and "band_hi_ghz" in r:
            row["margin_lo_mhz"] = 1e3 * (BAND[0] - r["band_lo_ghz"])
            row["margin_hi_mhz"] = 1e3 * (r["band_hi_ghz"] - BAND[1])
        out[name] = row
    return out


def refinement(models, palace):
    """Margins on the most accurate model against the X1 remaining-drift estimate."""
    v6 = load(os.path.join(V6, "v6_summary.toml"))
    x1p = load(os.path.join(V6, "x1", "palace_summary.toml"))["run"]
    p3_init = x1p["init_L1_p3"]["dip_f_ghz"]
    out = {
        "initial_patch_palace_p3_dip_ghz": p3_init,
        "drift_rim_fit_mhz": 1e3 * (v6["x1"]["rim_fit"]["f_inf_ghz"] - p3_init),
        "drift_richardson_mhz": 1e3 * (v6["x1"]["rim_fit"]["richardson_order1_rim_pair_ghz"] - p3_init),
        "note": (
            "X1 estimates the initial patch's converged dip 0-14 MHz above Palace p = 3 "
            "(Palace p = 3 itself as the lower end if the monotone trend holds; order-1 "
            "Richardson; the rim fit). A further upward shift d keeps the band covering ISM "
            "while d <= margin_lo_mhz; the upper edge only gains."
        ),
    }
    for tag, _ in DESIGNS:
        p3 = palace.get(f"{tag}_p3")
        if p3 and "margin_lo_mhz" in p3:
            out[f"{tag}_p3_margin_lo_mhz"] = p3["margin_lo_mhz"]
            out[f"{tag}_p3_margin_hi_mhz"] = p3["margin_hi_mhz"]
            out[f"{tag}_p3_covers_richardson_drift"] = p3["margin_lo_mhz"] >= out["drift_richardson_mhz"]
            out[f"{tag}_p3_covers_rim_fit_drift"] = p3["margin_lo_mhz"] >= out["drift_rim_fit_mhz"]
        m = models.get(tag, {})
        if "rim020_cyl_p1" in m and "comb_cyl_p1" in m and m["comb_cyl_p1"].get("bw_lo_ghz"):
            out[f"{tag}_comb_minus_rim020_lo_edge_mhz"] = 1e3 * (
                m["comb_cyl_p1"]["bw_lo_ghz"] - m["rim020_cyl_p1"]["bw_lo_ghz"]
            )
        if "rim020_p1" in m and "rim020_cyl_p1" in m:
            out[f"{tag}_rim020_air_morph_lo_edge_mhz"] = 1e3 * (
                m["rim020_cyl_p1"]["bw_lo_ghz"] - m["rim020_p1"]["bw_lo_ghz"]
            )
            out[f"{tag}_rim020_air_morph_worst_db"] = (
                m["rim020_cyl_p1"]["worst_in_band_db"] - m["rim020_p1"]["worst_in_band_db"]
            )
        if p3 and "comb_cyl_p1" in m and "band_lo_ghz" in p3 and m["comb_cyl_p1"].get("bw_lo_ghz"):
            out[f"{tag}_palace_p3_minus_comb_lo_edge_mhz"] = 1e3 * (
                p3["band_lo_ghz"] - m["comb_cyl_p1"]["bw_lo_ghz"]
            )
        p2 = palace.get(f"{tag}_p2")
        if p3 and p2 and "band_lo_ghz" in p2 and "band_lo_ghz" in p3:
            out[f"{tag}_palace_p3_minus_p2_lo_edge_mhz"] = 1e3 * (p3["band_lo_ghz"] - p2["band_lo_ghz"])
    return out


def fmt(v):
    if isinstance(v, bool):
        return "true" if v else "false"
    if isinstance(v, float):
        return f"{v:.6g}"
    if isinstance(v, int):
        return str(v)
    if isinstance(v, str):
        return '"' + v.replace('"', '\\"') + '"'
    if isinstance(v, list):
        return "[" + ", ".join(fmt(x) for x in v) + "]"
    raise TypeError(type(v))


def table(lines, name, d):
    lines.append(f"[{name}]")
    for k, v in d.items():
        if v is not None:
            lines.append(f"{k} = {fmt(v)}")
    lines.append("")


def build():
    lines = [
        "# Auto-generated by benchmarks/patch_antenna_conformal/scripts/recentre_summary.py",
        "# (issue #1134). Do NOT edit by hand.",
        "",
    ]
    table(
        lines,
        "meta",
        {
            "description": (
                "The fixed-1.6 mm copper design re-centred on the refined forward model (#1134): "
                "two space-mapping descents on L1 with a 1.6 mm probe, verified at P1 and P2 on "
                "L1, on the 0.2 mm-rim mesh, on the combined 0.1 mm-rim / 0.3 mm-slab mesh, and "
                "with Palace at p = 2 and p = 3 on the design's L1 mesh (the #1126 X1 setup)."
            ),
            "band_f_ghz": list(BAND),
            "headline": "sm57",
        },
    )
    models = {}
    for tag, role in DESIGNS:
        table(lines, f"design.{tag}", design_row(tag, role))
        models[tag] = {}
        for key, desc, suffixes in MODELS:
            row = model_row(tag, key, desc, suffixes)
            if row is not None:
                models[tag][key] = row
                table(lines, f"model.{tag}.{key}", row)
    palace = palace_rows()
    for name, row in palace.items():
        table(lines, f"palace.{name}", row)
    rim = os.path.join(RC, "rim_edges.toml")
    if os.path.exists(rim):
        for stem, row in load(rim)["mesh"].items():
            table(lines, f"rim_edges.{stem}", row)
    table(lines, "refinement", refinement(models, palace))
    return "\n".join(lines).rstrip() + "\n"


def main():
    text = build()
    if "--write" in sys.argv:
        out = os.path.join(RC, "recentre_summary.toml")
        with open(out, "w") as f:
            f.write(text)
        print(out)
    else:
        print(text)


if __name__ == "__main__":
    main()
