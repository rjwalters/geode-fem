#!/usr/bin/env python3
"""Design C characterization and the finite-difference-gradient control (issue #1148).

Reads the committed raw records under `benchmarks/patch_antenna_conformal/designc/`
and the references they are compared with (`v6/`, `recentre/`, `copper/`), and
writes `designc/designc_summary.toml`, which `tests/patch_conformal_designc.rs`
pins. Design C is the #1134 headline `recentre/rbG_L1_copper_38_sm57_w16`.

* C1: Palace's port-free eigenmode (`c1/*/postpro/eig.csv`, `domain-E.csv`) of
  the initial patch and of design C on their L1 meshes at p = 2, with the port
  gap open (no feed) and, for comparison, loaded by the 50 ohm port. The patch
  mode is the mode with the largest substrate electric participation. Its
  frequency is set against the matched |S11| dip of the driven Palace sweep on
  the same mesh at the same order (v6 X1 and #1134).
* C2: power balance, efficiency and gain of design C at P1 on its L1 mesh and
  on the 0.2 mm-rim mesh (`c2/*_radiation.toml`), against design A (v6 X3).
* C3: Palace p = 2 on design C's 0.2 mm-rim mesh (`c3/palace_summary.toml`,
  written by `palace_ingest.py`), against Palace p = 2 / p = 3 on L1 and geode
  P1 on the same 0.2 mm-rim mesh (#1134).
* C4: the forward-difference-gradient L-BFGS on the X4 problem
  (`c4/L1_copper_38_fdgrad.toml`) against the adjoint run
  (`copper/L1_copper_38.toml`) and the X4 sep-CMA-ES seeds, solves counted by
  the X4 rule.

`[[input]]` records the sha256 of every file read, so the test can tell a
stale summary from a fresh one.

Usage: python3 designc_summary.py [--write]
"""

from __future__ import annotations

import csv
import hashlib
import math
import os
import sys
import tomllib

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.normpath(os.path.join(HERE, ".."))
REPO = os.path.normpath(os.path.join(ROOT, "..", ".."))
DC = os.path.join(ROOT, "designc")
V6 = os.path.join(ROOT, "v6")
RC = os.path.join(ROOT, "recentre")
COPPER = os.path.join(ROOT, "copper")
BAND = (2.4, 2.4835)
LEVELS = [-5.0, -8.0, -9.0, -9.5, -10.0]

INPUTS: list[str] = []


def track(path: str) -> str:
    if path not in INPUTS:
        INPUTS.append(path)
    return path


def load(path: str) -> dict:
    with open(track(path), "rb") as f:
        return tomllib.load(f)


def read_csv(path: str) -> list[dict]:
    with open(track(path)) as f:
        rows = list(csv.reader(f))
    head = [h.strip() for h in rows[0]]
    return [
        {h: float(v) for h, v in zip(head, r)} for r in rows[1:] if r and r[0].strip()
    ]


def sha256(path: str) -> str:
    with open(path, "rb") as f:
        return hashlib.sha256(f.read()).hexdigest()


# ── C1 ──────────────────────────────────────────────────────────────────

EIG_RUNS = [
    # (run, design, port)
    ("eig_init_p2_open", "initial", "open"),
    ("eig_sm57_p2_open", "sm57", "open"),
    ("eig_init_p2_lumped", "initial", "lumped"),
    ("eig_sm57_p2_lumped", "sm57", "lumped"),
]


def eig_run(run: str) -> dict | None:
    d = os.path.join(DC, "c1", run, "postpro")
    if not os.path.exists(os.path.join(d, "eig.csv")):
        return None
    eig = read_csv(os.path.join(d, "eig.csv"))
    dom = read_csv(os.path.join(d, "domain-E.csv"))
    modes = []
    for e, w in zip(eig, dom):
        assert int(e["m"]) == int(w["m"])
        modes.append(
            {
                "f_re_ghz": e["Re{f} (GHz)"],
                "f_im_ghz": e["Im{f} (GHz)"],
                "q": e["Q"],
                "p_elec_substrate": w["p_elec[1]"],
                "error_bkwd": e["Error (Bkwd.)"],
            }
        )
    k = max(range(len(modes)), key=lambda i: modes[i]["p_elec_substrate"])
    others = [m["p_elec_substrate"] for i, m in enumerate(modes) if i != k]
    pm = modes[k]
    return {
        "n_modes": len(modes),
        "patch_mode": k + 1,
        "f_re_ghz": pm["f_re_ghz"],
        "f_im_ghz": pm["f_im_ghz"],
        "q": pm["q"],
        "p_elec_substrate": pm["p_elec_substrate"],
        "error_bkwd": pm["error_bkwd"],
        "next_largest_p_elec_substrate": max(others) if others else 0.0,
        "other_modes_max_q": max((m["q"] for i, m in enumerate(modes) if i != k), default=0.0),
    }


def c1() -> dict:
    out: dict = {}
    for run, design, port in EIG_RUNS:
        r = eig_run(run)
        if r is not None:
            out[run] = {"design": design, "port": port, "order": 2} | r
    x1 = load(os.path.join(V6, "x1", "palace_summary.toml"))["run"]
    rc = load(os.path.join(RC, "palace_summary.toml"))["run"]
    v6 = load(os.path.join(V6, "v6_summary.toml"))
    dips = {
        "initial": x1["init_L1_p2"]["dip_f_ghz"],
        "sm57": rc["sm57_p2"]["dip_f_ghz"],
    }
    out["initial_dip_p2_ghz"] = dips["initial"]
    out["sm57_dip_p2_ghz"] = dips["sm57"]
    out["sm57_band_centre_p2_ghz"] = 0.5 * (rc["sm57_p2"]["band_lo_ghz"] + rc["sm57_p2"]["band_hi_ghz"])
    out["textbook_tl_initial_ghz"] = v6["x1"]["textbook_tl_initial_ghz"]
    for design in ("initial", "sm57"):
        tag = "init" if design == "initial" else "sm57"
        r = out.get(f"eig_{tag}_p2_open")
        if r is None:
            continue
        # Positive: the matched dip lies above the port-free cavity resonance.
        out[f"{design}_dip_minus_eig_open_mhz"] = 1e3 * (dips[design] - r["f_re_ghz"])
        rl = out.get(f"eig_{tag}_p2_lumped")
        if rl is not None:
            out[f"{design}_eig_lumped_minus_open_mhz"] = 1e3 * (rl["f_re_ghz"] - r["f_re_ghz"])
    r = out.get("eig_init_p2_open")
    if r is not None:
        out["textbook_minus_initial_eig_open_mhz"] = 1e3 * (
            out["textbook_tl_initial_ghz"] - r["f_re_ghz"]
        )
        out["textbook_minus_initial_dip_p2_mhz"] = 1e3 * (
            out["textbook_tl_initial_ghz"] - dips["initial"]
        )
    return out


# ── C2 ──────────────────────────────────────────────────────────────────

RAD_KEYS = (
    "f_ghz",
    "s11_db",
    "eta_rad",
    "diel_fraction",
    "balance_residual",
    "directivity_broadside_dbi",
    "front_to_back_db",
    "gain_broadside_dbi",
    "realized_gain_broadside_dbi",
)


def rad(path: str) -> list[dict]:
    return [{k: p[k] for k in RAD_KEYS} for p in load(path)["radiation"]["point"]]


def span(rows: list[dict], key: str) -> list[float]:
    v = [r[key] for r in rows]
    return [min(v), max(v)]


def c2() -> dict:
    out: dict = {}
    shift = load(os.path.join(RC, "rbG_L1_copper_38_sm57_w16.toml"))["meta"]["band_shift_ghz"]
    shifted = [BAND[0] - shift + (BAND[1] - BAND[0]) * i / 4 for i in range(5)]
    for key, name, pick in [
        ("L1_shifted_band", "rbG_L1_copper_38_sm57_w16_on_rbG_L1_radiation.toml", shifted),
        (
            "L1_ism",
            "rbG_L1_copper_38_sm57_w16_on_rbG_L1_radiation.toml",
            [2.38, 2.40, 2.42, 2.44, 2.46, 2.4835, 2.50],
        ),
        (
            "rim020_ism",
            "rbG_L1_copper_38_sm57_w16_on_rbGrim020_L1_cyl_radiation.toml",
            [2.40, 2.42, 2.44, 2.46, 2.4835],
        ),
    ]:
        p = os.path.join(DC, "c2", name)
        if not os.path.exists(p):
            continue
        rows = [r for r in rad(p) if any(abs(r["f_ghz"] - f) < 1e-5 for f in pick)]
        assert len(rows) == len(pick), (key, len(rows))
        out[key] = {
            "n_points": len(rows),
            "f_ghz": span(rows, "f_ghz"),
            "eta_rad": span(rows, "eta_rad"),
            "diel_fraction": span(rows, "diel_fraction"),
            "balance_residual": span(rows, "balance_residual"),
            "directivity_broadside_dbi": span(rows, "directivity_broadside_dbi"),
            "gain_broadside_dbi": span(rows, "gain_broadside_dbi"),
            "realized_gain_broadside_dbi": span(rows, "realized_gain_broadside_dbi"),
            "front_to_back_db": span(rows, "front_to_back_db"),
            "s11_db": span(rows, "s11_db"),
        }
    # Design A (v6 X3) for comparison, as committed.
    x3 = load(os.path.join(V6, "v6_summary.toml"))["x3"]
    for key in ("radiation_final_L1", "radiation_final_L3"):
        rows = x3[key]
        out[f"design_a_{key.removeprefix('radiation_')}"] = {
            "eta_rad": span(rows, "eta_rad"),
            "diel_fraction": span(rows, "diel_fraction"),
            "gain_broadside_dbi": span(rows, "gain_broadside_dbi"),
        }
    # Realized gain on the most accurate match: the 0.2 mm-rim gain less the
    # mismatch loss at Palace p = 3's worst in-band |S11| (#1134).
    if "rim020_ism" in out:
        w = load(os.path.join(RC, "palace_summary.toml"))["run"]["sm57_p3"]["worst_in_band_db"]
        out["palace_p3_worst_in_band_db"] = w
        out["mismatch_loss_at_palace_p3_worst_db"] = -10.0 * math.log10(1.0 - 10.0 ** (w / 10.0))
        out["realized_gain_floor_dbi"] = (
            out["rim020_ism"]["gain_broadside_dbi"][0] - out["mismatch_loss_at_palace_p3_worst_db"]
        )
    if "L1_shifted_band" in out:
        a = out["design_a_final_L1"]["eta_rad"]
        c = out["L1_shifted_band"]["eta_rad"]
        out["eta_c_minus_a_L1"] = 0.5 * (c[0] + c[1]) - 0.5 * (a[0] + a[1])
    return out


# ── C3 ──────────────────────────────────────────────────────────────────


def c3() -> dict:
    p = os.path.join(DC, "c3", "palace_summary.toml")
    if not os.path.exists(p):
        return {}
    r = load(p)["run"]["c3_sm57_rim020_p2"]
    rc = load(os.path.join(RC, "palace_summary.toml"))["run"]
    rs = load(os.path.join(RC, "recentre_summary.toml"))
    geode_rim = rs["model"]["sm57"]["rim020_cyl_p1"]
    out = {
        k: r[k]
        for k in (
            "dip_f_ghz",
            "dip_db",
            "band_lo_ghz",
            "band_hi_ghz",
            "bandwidth_mhz",
            "ism_coverage_frac",
            "worst_in_band_db",
            "nd_order",
            "n_dofs",
            "wall_s",
        )
        if k in r
    }
    out["margin_lo_mhz"] = 1e3 * (BAND[0] - r["band_lo_ghz"])
    out["margin_hi_mhz"] = 1e3 * (r["band_hi_ghz"] - BAND[1])
    lo = r["band_lo_ghz"]
    out["minus_palace_L1_p3_lo_edge_mhz"] = 1e3 * (lo - rc["sm57_p3"]["band_lo_ghz"])
    out["minus_palace_L1_p2_lo_edge_mhz"] = 1e3 * (lo - rc["sm57_p2"]["band_lo_ghz"])
    out["minus_geode_rim020_p1_lo_edge_mhz"] = 1e3 * (lo - geode_rim["bw_lo_ghz"])
    out["minus_palace_L1_p3_dip_mhz"] = 1e3 * (r["dip_f_ghz"] - rc["sm57_p3"]["dip_f_ghz"])
    # The #1134 drift estimates above Palace p = 3 (from the initial patch).
    ref = rs["refinement"]
    out["drift_richardson_mhz"] = ref["drift_richardson_mhz"]
    out["drift_rim_fit_mhz"] = ref["drift_rim_fit_mhz"]
    out["palace_L1_p3_margin_lo_mhz"] = ref["sm57_p3_margin_lo_mhz"]
    # X1's p-step on the initial patch (L1, p = 2 -> 3), for scale.
    x1 = load(os.path.join(V6, "x1", "palace_summary.toml"))["run"]
    out["initial_L1_p2_to_p3_dip_mhz"] = 1e3 * (
        x1["init_L1_p3"]["dip_f_ghz"] - x1["init_L1_p2"]["dip_f_ghz"]
    )
    out["sm57_L1_p2_to_p3_lo_edge_mhz"] = 1e3 * (
        rc["sm57_p3"]["band_lo_ghz"] - rc["sm57_p2"]["band_lo_ghz"]
    )
    return out


# ── C4 ──────────────────────────────────────────────────────────────────


def first(curve: list[tuple[int, float]], lv: float):
    best = math.inf
    for n_s, w in curve:
        best = min(best, w)
        if best <= lv:
            return n_s
    return None


def adjoint_curve() -> list[tuple[int, float]]:
    """X4's reconstruction of the adjoint run (v6_summary.py)."""
    adj = load(os.path.join(COPPER, "L1_copper_38.toml"))
    guard: dict[int, int] = {}
    for e in adj["guard"]["event"]:
        guard[e["step"]] = guard.get(e["step"], 0) + 1
    cum, curve = 5, [(5, adj["trajectory"]["step"][0]["worst_s11_db"])]
    for st in adj["trajectory"]["step"][1:]:
        cum += 5 * (1 + st["backtracks"] - guard.get(st["iter"], 0))
        curve.append((cum, st["worst_s11_db"]))
    return curve


def c4() -> dict:
    p = os.path.join(DC, "c4", "L1_copper_38_fdgrad.toml")
    if not os.path.exists(p):
        return {}
    d = load(p)
    opt = d["optimization"]
    n_band = d["meta"]["n_band_points"]
    n_free = len(d["shape"]["design_vector"])
    steps = d["trajectory"]["step"]
    # The run's own per-step count, and the same count rebuilt from the
    # trajectory and the guard log (start: n_band + n_free * n_band; each step:
    # n_band per guard-passing trial + n_free * n_band for its FD gradient).
    curve = [(s["optimization_solves"], s["worst_s11_db"]) for s in steps]
    guard: dict[int, int] = {}
    for e in d["guard"].get("event", []):
        guard[e["step"]] = guard.get(e["step"], 0) + 1
    cum = n_band * (1 + n_free)
    rebuilt = [cum]
    for st in steps[1:]:
        cum += n_band * (1 + st["backtracks"] - guard.get(st["iter"], 0)) + n_band * n_free
        rebuilt.append(cum)
    assert rebuilt == [s["optimization_solves"] for s in steps], (rebuilt, curve)
    adj = adjoint_curve()
    adj_total = adj[-1][0]
    fd_lv = [first(curve, lv) for lv in LEVELS]
    adj_lv = [first(adj, lv) for lv in LEVELS]
    out = {
        "gradient": opt["gradient"],
        "fd_gradient_h": opt["fd_gradient_h"],
        "fd_jobs": opt["fd_jobs"],
        "n_free_dofs": n_free,
        "band_evals_per_gradient": n_free,
        "solves_per_gradient": n_free * n_band,
        "terminal_condition": opt["terminal_condition"],
        "max_solves": opt["max_solves"],
        "censored": opt["terminal_condition"] == "solve_budget",
        "n_steps": opt["n_steps"],
        "optimization_solves": opt["optimization_solves"],
        "fd_band_evals": opt["fd_band_evals"],
        "worst_s11_db_initial": opt["worst_s11_db_initial"],
        "best_worst_db": min(w for _, w in curve),
        "verify_worst_s11_db": opt["verify_worst_s11_db"],
        "fd_vs_adjoint_rel_x0": opt["fd_vs_adjoint_rel_x0"],
        "fd_vs_adjoint_rel_final": opt["fd_vs_adjoint_rel_final"],
        "total_wall_s": opt["total_wall_s"],
        "levels_db": LEVELS,
        "fd_solves_to_level": [v if v is not None else -1 for v in fd_lv],
        "adjoint_solves_to_level": [v if v is not None else -1 for v in adj_lv],
        "fd_over_adjoint_solves": [
            (f / a) if (f is not None and a is not None) else -1.0 for f, a in zip(fd_lv, adj_lv)
        ],
        "adjoint_total_optimization_solves": adj_total,
        "best_worst_db_within_adjoint_budget": min(
            (w for n_s, w in curve if n_s <= adj_total), default=steps[0]["worst_s11_db"]
        ),
    }
    # The same iterates: the FD run's worst-of-band per step against the
    # adjoint run's, over the steps both have.
    adj_steps = load(os.path.join(COPPER, "L1_copper_38.toml"))["trajectory"]["step"]
    n = min(len(steps), len(adj_steps))
    out["steps_compared"] = n - 1
    out["max_abs_worst_db_diff_vs_adjoint_path"] = max(
        abs(a["worst_s11_db"] - b["worst_s11_db"]) for a, b in zip(steps[:n], adj_steps[:n])
    )
    # Wall time per FD gradient (38 band evaluations on fd_jobs threads).
    walls = [s["wall_s"] for s in steps]
    out["mean_wall_s_per_step"] = (walls[-1] - walls[0]) / max(1, len(walls) - 1)
    # The X4 CMA-ES seeds (v6_summary.toml), for the same levels.
    x4 = load(os.path.join(V6, "v6_summary.toml"))["x4"]
    out["cma_solves_to_level"] = [
        [v if v is not None else -1 for v in g["solves_to_level"]] for g in x4["gradient_free"]
    ]
    return out


# ── TOML writer ─────────────────────────────────────────────────────────


def fmt(v) -> str:
    if isinstance(v, bool):
        return "true" if v else "false"
    if isinstance(v, int):
        return str(v)
    if isinstance(v, float):
        return f"{v:.9g}" if math.isfinite(v) else '"nan"'
    if isinstance(v, str):
        return '"' + v.replace("\\", "\\\\").replace('"', '\\"') + '"'
    if isinstance(v, list):
        return "[" + ", ".join(fmt(x) for x in v) + "]"
    raise TypeError(v)


def emit(lines: list[str], name: str, d: dict) -> None:
    scal = {k: v for k, v in d.items() if not isinstance(v, dict)}
    sub = {k: v for k, v in d.items() if isinstance(v, dict)}
    lines.append(f"[{name}]")
    for k, v in scal.items():
        lines.append(f"{k} = {fmt(v)}")
    lines.append("")
    for k, v in sub.items():
        emit(lines, f"{name}.{k}", v)


def build() -> str:
    INPUTS.clear()
    sections = {"c1": c1(), "c2": c2(), "c3": c3(), "c4": c4()}
    lines = [
        "# Auto-generated by benchmarks/patch_antenna_conformal/scripts/designc_summary.py",
        "# (issue #1148). Do NOT edit by hand; regenerate with --write.",
        "",
        "[meta]",
        'description = "Design C (recentre/rbG_L1_copper_38_sm57_w16) characterization and the '
        "finite-difference-gradient control, issue #1148: C1 port-free eigenmode, C2 power "
        "balance and gain, C3 Palace p = 2 on the 0.2 mm-rim mesh, C4 forward-difference L-BFGS "
        'on the X4 problem. Every value is derived from the raw records listed in [[input]]."',
        'headline_design = "rbG_L1_copper_38_sm57_w16"',
        "",
    ]
    for name, d in sections.items():
        if d:
            emit(lines, name, d)
    for p in INPUTS:
        lines.append("[[input]]")
        lines.append(f"path = {fmt(os.path.relpath(p, REPO))}")
        lines.append(f"sha256 = {fmt(sha256(p))}")
        lines.append("")
    return "\n".join(lines)


def main() -> None:
    text = build()
    if "--write" in sys.argv:
        out = os.path.join(DC, "designc_summary.toml")
        with open(out, "w") as f:
            f.write(text)
        print(out)
    else:
        print(text)


if __name__ == "__main__":
    main()
