"""Pilot analysis of an experiment-2 session (issue #1198, of #1185).

    pyrun.sh summarize_pilot.py DIR

DIR is a session directory written by session.py (the pilot:
benchmarks/motor_optimization/runs/pilot). This script first runs
summarize_session.py (the per-run table and the method C diagnostic rows),
then writes DIR/SUMMARY.md with these sections:

* the session's labels and provenance (host_idle, loads, N, unit);
* attainment by target: first-attainment FE (wall time / solve count) for
  T50, T90 and T99, or "censored", per method, space, start and repeat, and
  attainment fractions;
* the budget profile: each method's FE split by ledger category;
* feasibility: targets attained by every method, by none, and whether the
  60 FE budget is binding; the failed runs and their causes; each run's host
  load, in-run validation cost and solve-count / wall-time FE ratio;
* quality-versus-cost curves: DIR/curves.csv (the data) and
  DIR/quality_vs_cost.svg (the figure, written without a plotting library
  so that the locked environment is unchanged);
* method C's held-out-current diagnostic, with its interpolation rule named
  (#1185 amendment 6 addendum).

If DIR/findings.txt exists (hand-written observations made after
inspecting the results), it is included after the session section, so the
summary can be regenerated without losing it.

It makes no speedup claim and computes no speedup ratio: the pilot is a
feasibility and budget check on a host that is not idle (#1185
amendments 2 and 3).
"""
import csv
import json
import math
import statistics
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import summarize_session  # noqa: E402

TARGETS = ("T50", "T90", "T99")
METHODS = ("slsqp", "cobyqa", "surrogate")
LETTER = {"slsqp": "A", "cobyqa": "B", "surrogate": "C"}
SPACES = ("a", "b")
STARTS = ("theta0", "lhs1", "lhs2")
CATEGORIES = ("setup_calibration", "forward", "adjoint", "line_search", "mesh_motion",
              "remesh", "validation", "surrogate_update", "overhead")
COLOURS = {"slsqp": "#1f77b4", "cobyqa": "#d62728", "surrogate": "#2ca02c"}
DASH = {"theta0": "", "lhs1": "6,3", "lhs2": "2,2"}


def load(d):
    m = json.loads((d / "manifest.json").read_text())
    runs = []
    for job in m["jobs"]:
        p = d / job["trace"]
        t = json.loads(p.read_text()) if p.exists() else None
        runs.append((job, t))
    return m, runs


def curve(t):
    """The run's best-so-far feasible validated F_val + δF (the acceptance
    quantity compared with each target) over the cumulative wall-time and
    solve-count FE of its credited validations."""
    pts, best = [], None
    for v in t.get("validations", []):
        if not v.get("credited"):
            continue
        if v.get("mean_ok") and (best is None or v["F_upper"] < best):
            best = v["F_upper"]
        pts.append((v["wall_fe"], v["solve_fe"], v["F_val"], v["F_upper"], v["mean_ok"], best))
    return pts


def fe_cell(a):
    return f"{a['wall_fe']:.2f} / {a['solve_fe']:.2f}" if a.get("attained") else "censored"


def svg(runs, F0, targets, budget, path):
    W, H, ml, mr, mt, mb, gap = 1000, 460, 70, 20, 40, 60, 60
    pw = (W - ml - mr - gap) / 2
    ph = H - mt - mb
    ys = [p[3] / F0 for _, t in runs if t for p in curve(t) if p[5] is not None]
    lo = min([targets["T99"] / F0 * 0.5] + ys)
    hi = max([1.5] + ys)
    llo, lhi = math.floor(math.log10(lo)), math.ceil(math.log10(hi))

    def X(panel, fe):
        return ml + panel * (pw + gap) + pw * fe / budget

    def Y(r):
        return mt + ph * (lhi - math.log10(r)) / (lhi - llo)

    out = [f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" '
           f'font-family="sans-serif" font-size="12">',
           f'<rect width="{W}" height="{H}" fill="white"/>',
           f'<text x="{W / 2}" y="18" text-anchor="middle" font-size="14">Experiment 2 '
           f'pilot (host not idle): best feasible validated (F_val + dF) / F0 vs wall-time FE'
           f'</text>']
    for k, sp in enumerate(SPACES):
        x0, x1 = X(k, 0), X(k, budget)
        out.append(f'<rect x="{x0}" y="{mt}" width="{pw}" height="{ph}" fill="none" '
                   f'stroke="black"/>')
        out.append(f'<text x="{(x0 + x1) / 2}" y="{mt - 6}" text-anchor="middle">space '
                   f'{sp.upper()}</text>')
        for fe in range(0, int(budget) + 1, 10):
            out.append(f'<line x1="{X(k, fe)}" y1="{mt + ph}" x2="{X(k, fe)}" '
                       f'y2="{mt + ph + 4}" stroke="black"/>')
            out.append(f'<text x="{X(k, fe)}" y="{mt + ph + 17}" text-anchor="middle">{fe}'
                       f'</text>')
        out.append(f'<text x="{(x0 + x1) / 2}" y="{H - 22}" text-anchor="middle">wall-time FE '
                   f'(budget {budget:g})</text>')
        for e in range(llo, lhi + 1):
            out.append(f'<line x1="{x0}" y1="{Y(10 ** e)}" x2="{x1}" y2="{Y(10 ** e)}" '
                       f'stroke="#ddd"/>')
            out.append(f'<text x="{x0 - 5}" y="{Y(10 ** e) + 4}" text-anchor="end">1e{e}'
                       f'</text>')
        for name in TARGETS:
            y = Y(targets[name] / F0)
            out.append(f'<line x1="{x0}" y1="{y}" x2="{x1}" y2="{y}" stroke="#888" '
                       f'stroke-dasharray="8,4"/>')
            out.append(f'<text x="{x1 - 4}" y="{y - 3}" text-anchor="end" fill="#555">'
                       f'{name}</text>')
        for job, t in runs:
            if not t or job["space"] != sp:
                continue
            pts = [p for p in curve(t) if p[5] is not None]
            if not pts:
                continue
            end = t["ledger"]["total_fe"]
            seg = [(X(k, pts[0][0]), Y(pts[0][5] / F0))]
            for a, b in zip(pts, pts[1:] + [None]):
                nx = b[0] if b else end
                seg.append((X(k, nx), Y(a[5] / F0)))
                if b:
                    seg.append((X(k, nx), Y(b[5] / F0)))
            d = " ".join(f"{x:.1f},{y:.1f}" for x, y in seg)
            op = "0.9" if job["repeat"] == 1 else "0.5"
            out.append(f'<polyline points="{d}" fill="none" stroke="{COLOURS[job["method"]]}" '
                       f'stroke-width="1.6" stroke-opacity="{op}" '
                       f'stroke-dasharray="{DASH[job["start"]]}"/>')
    lx = ml + 10
    for i, mth in enumerate(METHODS):
        out.append(f'<line x1="{lx + 170 * i}" y1="{H - 6}" x2="{lx + 170 * i + 25}" '
                   f'y2="{H - 6}" stroke="{COLOURS[mth]}" stroke-width="2"/>')
        out.append(f'<text x="{lx + 170 * i + 30}" y="{H - 2}">{LETTER[mth]} {mth}</text>')
    out.append(f'<text x="{lx + 520}" y="{H - 2}">solid theta0, dashed lhs1, dotted lhs2; '
               f'faint = repeat 2</text>')
    out.append("</svg>")
    path.write_text("\n".join(out) + "\n")


def main(argv=None):
    d = Path((argv or sys.argv[1:])[0])
    summarize_session.main([str(d)])
    session_md = (d / "SUMMARY.md").read_text()
    m, runs = load(d)
    budget = m["budget_fe"]
    anchor = next(t["anchor"] for _, t in runs if t)
    F0, targets = anchor["F0"], anchor["targets"]
    hb, ha, p, cal = m["host_before"], m["host_after"], m["provenance"], m["calibration"]
    walls = [j["wall_s"] for j, _ in runs]
    L = ["# Experiment 2 pilot (issue #1198, of #1185): feasibility and budget only", "",
         f"**Session `{m['session_id']}`, label `{m['label']}`, `host_idle: "
         f"{str(m['host_idle']).lower()}`.** The pilot checks feasibility and the budget only "
         "(#1185 amendments 2 and 3). The targets T50/T90/T99 are fixed and are not revised "
         "here. The host was carrying other agents' work, so **no speedup ratio comes from "
         "this pilot and none is computed**.", "",
         "## Session", "",
         f"- Host {hb['host']}, {hb['cpu_model']}, {hb['cpu_count']} cores. Load averages "
         f"(1, 5, 15 min) before {', '.join(f'{x:.2f}' for x in hb['load'])} "
         f"({hb['utc']}), after {', '.join(f'{x:.2f}' for x in ha['load'])} ({ha['utc']}).",
         f"- N = {m['procs']} single-threaded processes (RAYON_NUM_THREADS = "
         "GEODE_NUM_THREADS = 1, faer Par::Seq) for the 36 runs and for the concurrent unit "
         "calibration.",
         f"- Unit = {cal['unit_seconds']:.4f} s: the median of {len(cal['validation_seconds'])} "
         f"cold θ0 validations in {cal['processes']} simultaneous processes (amendment 4 "
         f"addition 6); range {min(cal['validation_seconds']):.3f}–"
         f"{max(cal['validation_seconds']):.3f} s. Single-process median "
         f"{cal['single_process']['unit_seconds']:.4f} s (diagnostic only). Budget "
         f"{budget:g} FE = {budget * cal['unit_seconds']:.1f} s per run.",
         f"- Solve-count weights: training forward {cal['weights']['train_forward_eval']:.4f} "
         f"FE, adjoint {cal['weights']['adjoint_eval']:.4f} FE per evaluation; validation 1.",
         f"- Commit {p['commit']} (tree clean: {p['tree_clean']}); {p['rustc']}; Python "
         f"{p['python'].split()[0]} (isolated: {p['python_isolated']}); scipy "
         f"{p['packages']['scipy']}, numpy {p['packages']['numpy']}; requirements.lock "
         f"SHA-256 {p['requirements_lock_sha256']}; evaluator SHA-256 "
         f"{p['evaluator']['sha256']}.",
         f"- Meshes: training L{m['meshes']['train']['level']} "
         f"{m['meshes']['train']['sha256']}, validation L{m['meshes']['validation']['level']} "
         f"{m['meshes']['validation']['sha256']}; anchor.json "
         f"{m['meshes']['anchor']['sha256']}. Both meshes were rebuilt and match their "
         "committed records (`logs/check_meshes_pilot.log`).",
         f"- Anchor F0 = {F0:.12e} N² m²; T50 = {targets['T50']:.6e}, T90 = "
         f"{targets['T90']:.6e}, T99 = {targets['T99']:.6e}; mean floor "
         f"{anchor['mean_floor']:.12f} N m.",
         f"- Per-run wall time {min(walls):.0f}–{max(walls):.0f} s; total session wall time "
         "in `session_console.log`.", ""]
    findings = d / "findings.txt"
    if findings.exists():
        L += [findings.read_text().strip(), ""]

    # Attainment.
    L += ["## 1. Attainment by target", "",
          "Cells: wall-time FE / solve-count FE at the first credited validation that "
          "attained the target (F_val + δF ≤ T and the validated mean at or above the floor), "
          "or **censored** (not attained within the budget). No finite speedup is computed "
          "from a censored cell.", "",
          "| method | space | start | rep | stop | best feasible F_val / F0 | T50 | T90 | T99 |",
          "|---|---|---|---|---|---|---|---|---|"]
    order = sorted(runs, key=lambda r: (METHODS.index(r[0]["method"]),
                                         r[0]["space"], STARTS.index(r[0]["start"]),
                                         r[0]["repeat"]))
    for job, t in order:
        if t is None:
            L.append(f"| {LETTER[job['method']]} {job['method']} | {job['space'].upper()} | "
                     f"{job['start']} | {job['repeat']} | no trace (exit {job['exit']}) | — "
                     "| censored | censored | censored |")
            continue
        b = t.get("best_validated_feasible")
        L.append(f"| {LETTER[job['method']]} {job['method']} | {job['space'].upper()} | "
                 f"{job['start']} | {job['repeat']} | {t['stop_reason']} | "
                 f"{summarize_session.fmt(b and b['F_val'] / F0, '.4g')} | "
                 + " | ".join(fe_cell(t["attainment"][n]) for n in TARGETS) + " |")
    L += ["", "Attainment fractions (runs that attained / runs):", "",
          "| method | space | T50 | T90 | T99 |", "|---|---|---|---|---|"]
    frac = {}
    for mth in METHODS:
        for sp in SPACES:
            rs = [t for j, t in runs if j["method"] == mth and j["space"] == sp]
            cells = []
            for n in TARGETS:
                k = sum(1 for t in rs if t and t["attainment"][n]["attained"])
                frac[(mth, sp, n)] = (k, len(rs))
                cells.append(f"{k}/{len(rs)}")
            L.append(f"| {LETTER[mth]} {mth} | {sp.upper()} | " + " | ".join(cells) + " |")

    # Budget profile.
    L += ["", "## 2. Budget profile", "",
          "Mean wall-time FE per run in each ledger category (all runs of the method in "
          "the space; every run's categories sum to its total). `overhead` is the remaining "
          "wall time: process start-up, I/O and the optimizer's own work (amendment 4 §5).",
          "",
          "| method | space | " + " | ".join(CATEGORIES) + " | total | train evals | "
          "remeshes | validations |",
          "|---|---|" + "---|" * (len(CATEGORIES) + 4)]
    for mth in METHODS:
        for sp in SPACES:
            rs = [t for j, t in runs if j["method"] == mth and j["space"] == sp and t]
            if not rs:
                continue
            cat = [statistics.mean(t["ledger"]["totals"][c]["fe"] for t in rs)
                   for c in CATEGORIES]
            tot = statistics.mean(t["ledger"]["total_fe"] for t in rs)
            ntr = statistics.mean(sum(1 for e in t["events"] if e["kind"] == "train")
                                  for t in rs)
            nrm = statistics.mean(sum(1 for e in t["events"]
                                      if e["kind"] == "train" and e.get("remesh"))
                                  for t in rs)
            nva = statistics.mean(len(t["validations"]) for t in rs)
            L.append(f"| {LETTER[mth]} {mth} | {sp.upper()} | "
                     + " | ".join(f"{c:.2f}" for c in cat)
                     + f" | {tot:.2f} | {ntr:.1f} | {nrm:.1f} | {nva:.1f} |")
    L += ["", "The same split as a share of each method's charged FE (both spaces pooled):",
          "", "| method | " + " | ".join(CATEGORIES) + " |",
          "|---|" + "---|" * len(CATEGORIES)]
    for mth in METHODS:
        rs = [t for j, t in runs if j["method"] == mth and t]
        tot = sum(t["ledger"]["total_fe"] for t in rs)
        L.append(f"| {LETTER[mth]} {mth} | " + " | ".join(
            f"{100 * sum(t['ledger']['totals'][c]['fe'] for t in rs) / tot:.1f}%"
            for c in CATEGORIES) + " |")

    # Feasibility.
    stops = {}
    for j, t in runs:
        stops.setdefault((j["method"], j["space"]), []).append(t["stop_reason"] if t
                                                                else "no trace")
    L += ["", "## 3. Feasibility", ""]
    for sp in SPACES:
        every = [n for n in TARGETS if all(frac[(mth, sp, n)][0] > 0 for mth in METHODS)]
        every_all = [n for n in TARGETS
                     if all(frac[(mth, sp, n)][0] == frac[(mth, sp, n)][1] for mth in METHODS)]
        none = [n for n in TARGETS if all(frac[(mth, sp, n)][0] == 0 for mth in METHODS)]
        L.append(f"- **Space {sp.upper()}.** Attained by at least one run of every method: "
                 f"{', '.join(every) or 'none'}. Attained by every run of every method: "
                 f"{', '.join(every_all) or 'none'}. Attained by no run of any method: "
                 f"{', '.join(none) or 'none'}.")
    L += ["", "Stop reasons (runs per cell):", "", "| method | space | stop reasons |",
          "|---|---|---|"]
    for mth in METHODS:
        for sp in SPACES:
            s = stops[(mth, sp)]
            c = {k: s.count(k) for k in sorted(set(s))}
            L.append(f"| {LETTER[mth]} {mth} | {sp.upper()} | "
                     + ", ".join(f"{k} {v}" for k, v in c.items()) + " |")
    nb = sum(1 for _, t in runs if t and t["stop_reason"] == "budget")
    late = []
    for j, t in runs:
        if not t or t["stop_reason"] != "budget":
            continue
        c = [q for q in curve(t) if q[5] is not None]
        if c:
            imp = [q[0] for q, prev in zip(c, [None] + c[:-1])
                   if prev is None or q[5] < prev[5]]
            late.append((j["name"], imp[-1]))
    lastimp = [x for _, x in late]
    L += ["", f"**Is 60 FE binding?** {nb} of {len(runs)} runs stopped on the budget. "
          + (f"Among the budget-stopped runs with a feasible validation, the last improvement "
             f"of the best feasible validated value came at a median of "
             f"{statistics.median(lastimp):.1f} FE (range {min(lastimp):.1f}–"
             f"{max(lastimp):.1f} FE). " if lastimp else "")
          + "A target censored in a budget-stopped run is censored by the budget; a target "
          "censored in a run that stopped on its own (`converged`) or `failed` is censored by "
          "that stop, not by the budget."]

    # Failures (amendment 4 ruling 4: recorded, counted against the method, never re-run).
    fails = [(j, t) for j, t in order if t and t["stop_reason"] == "failed"]
    if fails:
        def kind(e):
            if "slot flank degenerates" in e:
                return "no geometry: slot flank degenerates"
            if "fresh mesh fails the policy" in e:
                return "fresh mesh fails the remesh policy"
            return e[:80]
        L += ["", f"**Failures.** {len(fails)} of {len(runs)} runs ended `failed` (a request "
              "returned an error; amendment 4 ruling 4: recorded, counted against the method, "
              "never re-run).", "", "| run | FE at failure | failing request | cause | "
              "targets attained before |", "|---|---|---|---|---|"]
        for j, t in fails:
            ev = [e for e in t["events"] if e["kind"] == "train"]
            L.append(f"| {j['name']} | {t['ledger']['total_fe']:.2f} | train "
                     f"(training request {len(ev) + 1}) | {kind(t.get('error') or '')} | "
                     + (", ".join(n for n in TARGETS if t["attainment"][n]["attained"])
                        or "none") + " |")

    # Host load and timing.
    L += ["", "**Host load and timing.** Each run's 1-minute load average at its start and "
          "end, the median wall-time FE of its validation evaluations (1 would equal the "
          "unit), and the ratio of its solve-count FE to its wall-time FE.", "",
          "| run | load start | load end | validation FE per evaluation | solve FE / wall FE |",
          "|---|---|---|---|---|"]
    vcost = []
    for j, t in runs:
        if not t:
            continue
        led = t["ledger"]
        v = [e["seconds"] / led["fe_rule"]["unit_seconds"] for e in led["entries"]
             if e["category"] == "validation" and e.get("forward_solves")]
        vcost += v
        L.append(f"| {j['name']} | {t['load_before'][0]:.1f} | {t['load_after'][0]:.1f} | "
                 f"{summarize_session.fmt(v and statistics.median(v), '.3f')} | "
                 f"{led['solve_fe'] / led['total_fe']:.2f} |")
    if vcost:
        L += ["", f"All {len(vcost)} in-run validation evaluations: median "
              f"{statistics.median(vcost):.3f} FE, range {min(vcost):.3f}–{max(vcost):.3f} "
              "FE."]

    # Curves.
    with open(d / "curves.csv", "w", newline="") as f:
        w = csv.writer(f, lineterminator="\n")
        w.writerow(["run", "method", "space", "start", "repeat", "validation", "wall_fe",
                    "solve_fe", "F_val", "F_val_plus_dF", "mean_ok",
                    "best_feasible_F_val_plus_dF", "best_over_F0"])
        for j, t in runs:
            if not t:
                continue
            for i, q in enumerate(curve(t)):
                w.writerow([j["name"], j["method"], j["space"], j["start"], j["repeat"], i,
                            f"{q[0]:.6f}", f"{q[1]:.6f}", repr(q[2]), repr(q[3]), q[4],
                            "" if q[5] is None else repr(q[5]),
                            "" if q[5] is None else repr(q[5] / F0)])
    svg(runs, F0, targets, budget, d / "quality_vs_cost.svg")
    L += ["", "## 4. Quality versus cost", "",
          "![quality versus cost](quality_vs_cost.svg)", "",
          "Each curve is one run: the best feasible validated F_val + δF (the quantity that "
          "is compared with each target), divided by F0, against the cumulative wall-time FE "
          "of its credited validations; it is drawn to the run's final charged FE. The "
          "dashed grey lines are the targets. A run with no feasible validation has no "
          "curve. The data, with the solve-count FE as well, is `curves.csv`."]

    # Method C diagnostic.
    L += ["", "## 5. Method C: held-out-current diagnostic (amendment 6 §3 and addendum)", "",
          "**Rule.** Method C models currents 0, 0.5 and 1 only. Its prediction at the "
          "held-out currents 0.25 and 0.75 is the **linear interpolation in the current "
          "fraction** between C's own per-current RBF std predictions at the two "
          "neighbouring modelled currents (0 and 0.5 for 0.25; 0.5 and 1 for 0.75). The "
          "means at 0 and 0.5, which C does not model, come from cubic RBFs fitted after the "
          "run to the same training data. This measures **C's interpolation in current under "
          "that rule**, not intrinsic current generalization: torque std need not be linear "
          "in current, so a large error here reflects the rule. The diagnostic runs after "
          "the run stops and is not counted in its budget or attainment.", "",
          "| run | fraction | std pred | std train | rel. err. (train) | std val | rel. err. "
          "(val) | mean pred | mean val | rel. err. (val) |",
          "|---|---|---|---|---|---|---|---|---|---|"]
    errs = {0.25: [], 0.75: []}
    for j, t in order:
        if j["method"] != "surrogate" or not t:
            continue
        pd = t.get("post_run_diagnostic") or {}
        if not pd.get("rows"):
            L.append(f"| {j['name']} | — | {pd.get('status')} {pd.get('error') or ''} "
                     "| | | | | | | |")
            continue
        for r in pd["rows"]:
            errs[r["fraction"]].append(r["rel_std_val"])
            L.append(f"| {j['name']} | {r['fraction']} | {r['std_pred']:.4e} | "
                     f"{r['std_train']:.4e} | {r['rel_std_train']:.3f} | {r['std_val']:.4e} | "
                     f"{r['rel_std_val']:.3f} | {r['mean_pred']:.5f} | {r['mean_val']:.5f} | "
                     f"{r['rel_mean_val']:.4f} |")
    if any(errs.values()):
        L.append("")
    for fr, e in errs.items():
        if e:
            L.append(f"- Fraction {fr}: relative std error against the validation model, "
                     f"median {statistics.median(e):.3f}, range {min(e):.3f}–{max(e):.3f} "
                     f"({len(e)} runs).")
    L += ["", "## 6. Per-run table and notes (summarize_session.py)", "",
          session_md.split("\n", 1)[1].strip(), ""]
    (d / "SUMMARY.md").write_text("\n".join(L) + "\n")
    print("\n".join(L))


if __name__ == "__main__":
    main()
