#!/usr/bin/env python3
"""Assemble the 3-way head-to-head table (#651) from COMMITTED artifacts only.

Reads (never re-runs) :
  freeform  : ../conformal_results.toml (73-DOF smoke fixture) and the
              2.4 GHz nested-DOF results ../nested_dof/L*_combined_64*.toml
  parametric: ../nested_dof/L*_{gap_1,gap_4,gap_16,gap_48,outline_16}.toml,
              plus L2_gap_48_warm.toml
  FDTD-dens.: ../../fdtd_density_baseline/{staircasing_results,meep_runtime_scaling}.json
  transfer  : ../nested_dof/verify/*.toml

Writes head_to_head.json and refreshes the table block in README.md.
`--check` rewrites nothing and exits 1 if either is stale.
Stdlib only.
"""
import json
import sys
import tomllib
from pathlib import Path

HERE = Path(__file__).resolve().parent
BENCH = HERE.parent
FDTD = BENCH.parent / "fdtd_density_baseline"
NOT_MEASURED = "not measured"
NOT_RUN = "not run — operator-gated"
BEGIN, END = "<!-- BEGIN GENERATED TABLE -->", "<!-- END GENERATED TABLE -->"

# Method classification follows the #651 rescope: gap_*/outline_* = parametric,
# combined_64 = freeform. NB gap_16/gap_48 are spatially varying B-spline fields,
# not "a few scalars" (nested_dof/README.md); dof_class makes that visible.
PARAMETRIC = {"gap_1", "gap_4", "gap_16", "gap_48", "gap_48_warm", "outline_16"}
FREEFORM = {"combined_64", "combined_64_cold", "combined_64_warm"}


def toml(path):
    with open(path, "rb") as f:
        return tomllib.load(f)


def jload(path):
    with open(path) as f:
        return json.load(f)


def rel(p):
    return str(Path(p).resolve().relative_to(BENCH.parent.parent))


def cost(meta, opt_or_verify):
    wall = opt_or_verify.get("total_wall_s")
    rss = opt_or_verify.get("first_gradient_eval_peak_rss_gib",
                            opt_or_verify.get("sweep_peak_rss_gib"))
    return {
        "wall_s": wall if wall is not None else NOT_MEASURED,
        "peak_rss_gib": rss if rss is not None else NOT_MEASURED,
        "host": meta.get("host", NOT_MEASURED),
    }


def optimized_rows():
    rows = []
    for p in sorted((BENCH / "nested_dof").glob("L*.toml")):
        t = toml(p)
        m, o, b = t["meta"], t["optimization"], t["basis"]
        label = m["label"]
        if label in PARAMETRIC:
            method = "parametric"
        elif label in FREEFORM:
            method = "freeform"
        else:
            raise SystemExit(f"unclassified label {label!r} in {p.name}")
        rows.append({
            "method": method,
            "label": label,
            "dof_class": ("spatially-varying field" if b["n_dofs"] > 4
                          else "few scalars"),
            "source": rel(p),
            "level": m["level"],
            "element_order": "p1",
            "upml_sigma_0": m["upml_sigma_0"],
            "start": t.get("start", {}).get("kind", "cold"),
            "n_dofs": b["n_dofs"],
            "worst_s11_db_initial": o["worst_s11_db_initial"],
            "worst_s11_db": o["verify_worst_s11_db"],
            "reached_target_minus10": o["reached_target"],
            "terminal_condition": o["terminal_condition"],
            "mean_substrate_thickness_note": "non-uniform thickness ablation "
                "(radial DOFs lift the patch); not a 1.6 mm board",
            "conditional_on_mesh_level": True,
            "cost": cost(m, o),
        })
    return rows


def smoke_row():
    t = toml(BENCH / "conformal_results.toml")
    m, o = t["meta"], t["optimization"]
    return {
        "method": "freeform",
        "label": "smoke_73dof",
        "dof_class": "spatially-varying field",
        "source": rel(BENCH / "conformal_results.toml"),
        "level": "curved_smoke",
        "element_order": "p1",
        "upml_sigma_0": m["sigma_0"],
        "n_dofs": m["n_freeform_dofs"],
        "worst_s11_db_initial": o["worst_s11_db_initial"],
        "worst_s11_db": o["worst_s11_db_final"],
        "reached_target_minus10": o["reached_target"],
        "terminal_condition": o["terminal_condition"],
        "conditional_on_mesh_level": True,
        "comparable_to_2p4ghz_rows": False,
        "problem_note": "16 x 12 mm smoke patch, band omega 0.3-0.4 rad/mm "
                        "(~14.3-19.1 GHz): a different problem from the 2.4 GHz "
                        "patch_conformal_L* rows; do not rank against them",
        "cost": {"wall_s": NOT_MEASURED, "peak_rss_gib": NOT_MEASURED,
                 "host": NOT_MEASURED},
    }


def transfer_rows():
    rows = []
    for p in sorted((BENCH / "nested_dof" / "verify").glob("*.toml")):
        t = toml(p)
        m, d, v = t["meta"], t["design"], t["verify"]
        rows.append({
            "design": d["name"],
            "source": rel(p),
            "evaluated_on_level": m["level"],
            "element_order": m["element_order"],
            "upml_sigma_0": m["upml_sigma_0"],
            "n_dofs": d["n_radial_dofs"] + d["n_outline_dofs"],
            "worst_s11_db": v["verify_worst_s11_db"],
            "cost": {"wall_s": v.get("total_wall_s", NOT_MEASURED),
                     "peak_rss_gib": v.get("sweep_peak_rss_gib", NOT_MEASURED),
                     "host": m.get("host", NOT_MEASURED)},
        })
    return rows


def gaps(opt):
    """parametric - freeform worst-of-band per own level (dB; >0 = freeform better)."""
    out = []
    for level in ("L0", "L1", "L2"):
        ff = {r["label"]: r for r in opt if r["level"] == level and r["method"] == "freeform"}
        pa = [r for r in opt if r["level"] == level and r["method"] == "parametric"]
        best_pa = min(pa, key=lambda r: r["worst_s11_db"])
        reaching = [r for r in pa if r["worst_s11_db"] <= -10]
        fewest = min(reaching, key=lambda r: r["n_dofs"]) if reaching else None
        for name, f in sorted(ff.items()):
            out.append({
                "level": level,
                "freeform": name,
                "freeform_worst_s11_db": f["worst_s11_db"],
                "best_parametric": best_pa["label"],
                "best_parametric_n_dofs": best_pa["n_dofs"],
                "best_parametric_worst_s11_db": best_pa["worst_s11_db"],
                "gap_db_parametric_minus_freeform": round(
                    best_pa["worst_s11_db"] - f["worst_s11_db"], 6),
                "fewest_dof_parametric_reaching_minus10":
                    ({"label": fewest["label"], "n_dofs": fewest["n_dofs"],
                      "worst_s11_db": fewest["worst_s11_db"]} if fewest else None),
                "start_mismatch_note": ("warm-started rows are not like-for-like "
                                        "with cold rows" if f["start"] == "warm"
                                        or best_pa["start"] == "warm" else None),
            })
    return out


def fdtd_block():
    st = jload(FDTD / "staircasing_results.json")
    me = jload(FDTD / "meep_runtime_scaling.json")
    patch = st["arcs"]["patch"]
    return {
        "method": "FDTD-density",
        "n_dofs": "not applicable (no density run performed)",
        "worst_s11_db": NOT_RUN,
        "mesh_level": "n/a",
        "upml_sigma_0": "n/a",
        "s11_note": "No FDTD-density |S11| exists in any committed artifact. The "
                    "definitive 3-D Meep-adjoint run is OPERATOR-ONLY "
                    "(fdtd_density_baseline/README.md).",
        "geometric_staircasing_patch_arc": {
            "source": rel(FDTD / "staircasing_results.json"),
            "scope": st["scope"],
            "rows": [{k: r[k] for k in ("resolution_cells_per_mm", "cell_size_mm",
                                        "boundary_pos_max_mm", "boundary_pos_rms_mm",
                                        "perimeter_rel_error")}
                     for r in patch["resolutions"]],
            "fem_unstructured_facet_error": [
                {"level": l["level"], "facet_max_radial_error_mm":
                    l["patch"]["facet_max_radial_error_mm"],
                 "facet_rms_radial_error_mm": l["patch"]["facet_rms_radial_error_mm"]}
                for l in st["fem_surface_ladder"]],
            "caveat": st["metric_notes"]["comparison"],
        },
        "meep_cost": {
            "source": rel(FDTD / "meep_runtime_scaling.json"),
            "caveat": me["description"],
            "host": me["provenance"]["host"],
            "measured_perstep": [{k: r[k] for k in ("resolution_px_per_mm", "cells",
                                                    "s_per_step", "peak_rss_gb")}
                                 for r in me["measured_perstep"]],
            "single_forward_solve_wallclock_vs_R": me["scaling_law"][
                "single_forward_solve_wallclock_vs_R"],
        },
    }


def build():
    opt = optimized_rows()
    return {
        "_schema": "geode-head-to-head/1",
        "_generated_by": "benchmarks/patch_antenna_conformal/head_to_head/build_head_to_head.py",
        "_scope": "Assembled from committed artifacts only; no new solver run. "
                  "All 2.4 GHz numbers are conditional on the mesh level "
                  "(nested_dof/README.md) and use UPML sigma_0 = 1.",
        "optimized_2p4ghz": opt,
        "smoke_fixture_freeform": smoke_row(),
        "gaps": gaps(opt),
        "cross_level_transfer": transfer_rows(),
        "fdtd_density": fdtd_block(),
    }


def f(x, nd=2):
    return f"{x:.{nd}f}" if isinstance(x, (int, float)) else str(x)


def hours(wall):
    return f"{wall / 3600:.1f} h" if isinstance(wall, (int, float)) else str(wall)


def table(d):
    L = []
    L.append("### Optimized designs, own level (worst of 9 in-band points, dB)\n")
    L.append("| method | design | DOFs | level | order | σ₀ | start | initial | final | −10 dB? | wall | peak RSS (GiB) |")
    L.append("|---|---|---|---|---|---|---|---|---|---|---|---|")
    order = {"parametric": 0, "freeform": 1}
    for r in sorted(d["optimized_2p4ghz"], key=lambda r: (r["level"], order[r["method"]], r["n_dofs"], r["label"])):
        L.append(f"| {r['method']} | {r['label']} | {r['n_dofs']} | {r['level']} | {r['element_order']} | "
                 f"{r['upml_sigma_0']} | {r['start']} | {f(r['worst_s11_db_initial'])} | {f(r['worst_s11_db'])} | "
                 f"{'yes' if r['reached_target_minus10'] else 'no'} | {hours(r['cost']['wall_s'])} | "
                 f"{f(r['cost']['peak_rss_gib'], 1)} |")
    s = d["smoke_fixture_freeform"]
    L.append(f"| freeform | **{s['label']}** (smoke fixture, ~15 GHz, different problem) | {s['n_dofs']} | "
             f"{s['level']} | p1 | {s['upml_sigma_0']} | cold | {f(s['worst_s11_db_initial'])} | "
             f"{f(s['worst_s11_db'])} | yes | {NOT_MEASURED} | {NOT_MEASURED} |")
    L.append("")
    L.append("### Parametric-vs-freeform gap, per level (dB; positive = freeform better)\n")
    L.append("| level | freeform | its worst | best parametric | DOFs | its worst | gap | fewest-DOF parametric reaching −10 dB | note |")
    L.append("|---|---|---|---|---|---|---|---|---|")
    for g in d["gaps"]:
        fw = g["fewest_dof_parametric_reaching_minus10"]
        fw_s = f"{fw['label']} ({fw['n_dofs']} DOFs, {f(fw['worst_s11_db'])} dB)" if fw else "none"
        L.append(f"| {g['level']} | {g['freeform']} | {f(g['freeform_worst_s11_db'])} | {g['best_parametric']} | "
                 f"{g['best_parametric_n_dofs']} | {f(g['best_parametric_worst_s11_db'])} | "
                 f"{f(g['gap_db_parametric_minus_freeform'])} | {fw_s} | {g['start_mismatch_note'] or ''} |")
    L.append("")
    L.append("### Cross-level transfer (design carried to another fixture, worst of 9 in-band points, dB)\n")
    L.append("| design | DOFs | evaluated on | order | σ₀ | worst | wall | peak RSS (GiB) |")
    L.append("|---|---|---|---|---|---|---|---|")
    for r in d["cross_level_transfer"]:
        L.append(f"| {r['design']} | {r['n_dofs']} | {r['evaluated_on_level']} | {r['element_order']} | "
                 f"{r['upml_sigma_0']} | {f(r['worst_s11_db'])} | {hours(r['cost']['wall_s'])} | "
                 f"{f(r['cost']['peak_rss_gib'], 1)} |")
    L.append("")
    fd = d["fdtd_density"]
    L.append("### FDTD-density (no |S11| measured)\n")
    L.append(f"| method | DOFs | level / σ₀ | worst-of-band \\|S₁₁\\| | cost |")
    L.append("|---|---|---|---|---|")
    mc = fd["meep_cost"]["measured_perstep"][-1]
    L.append(f"| FDTD-density | {fd['n_dofs']} | n/a | {fd['worst_s11_db']} | historical Meep timing on an "
             f"unmatched flat scaffold, R={mc['resolution_px_per_mm']} px/mm: {mc['s_per_step']:.2f} s/step, "
             f"{mc['peak_rss_gb']:.1f} GB RSS ({fd['meep_cost']['host']}) |")
    L.append("")
    L.append("Geometric staircasing floor, patch arc (`staircasing_results.json`; geometry only, no EM claim):\n")
    L.append("| FDTD cells/mm | max boundary position error (mm) | perimeter rel. error |")
    L.append("|---|---|---|")
    for r in fd["geometric_staircasing_patch_arc"]["rows"]:
        L.append(f"| {r['resolution_cells_per_mm']} | {r['boundary_pos_max_mm']:.4f} | {r['perimeter_rel_error']:.4f} |")
    L.append("")
    L.append("| GEODE tet mesh | max facet radial error (mm) | RMS (mm) |")
    L.append("|---|---|---|")
    for r in fd["geometric_staircasing_patch_arc"]["fem_unstructured_facet_error"]:
        L.append(f"| {r['level']} | {r['facet_max_radial_error_mm']:.5f} | {r['facet_rms_radial_error_mm']:.5f} |")
    return "\n".join(L) + "\n"


def render_readme(readme_text, tbl):
    a, b = readme_text.index(BEGIN), readme_text.index(END)
    return readme_text[:a + len(BEGIN)] + "\n" + tbl + readme_text[b:]


def main(argv):
    check = "--check" in argv
    d = build()
    js = json.dumps(d, indent=2) + "\n"
    readme = HERE / "README.md"
    new_readme = render_readme(readme.read_text(), table(d))
    jpath = HERE / "head_to_head.json"
    if check:
        stale = []
        if not jpath.exists() or jpath.read_text() != js:
            stale.append(jpath.name)
        if readme.read_text() != new_readme:
            stale.append(readme.name)
        if stale:
            print("STALE: " + ", ".join(stale), file=sys.stderr)
            return 1
        print("head_to_head: up to date")
        return 0
    jpath.write_text(js)
    readme.write_text(new_readme)
    print(f"wrote {jpath.name} and refreshed {readme.name}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
