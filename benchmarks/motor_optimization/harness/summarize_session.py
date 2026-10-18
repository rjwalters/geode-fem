"""Summarise one harness session directory into SUMMARY.md (issues #1195,
#1201).

    pyrun.sh summarize_session.py DIR

Reads DIR/manifest.json and the run traces. Per run: the stop reason, the
charged FE (wall time and solve count), the training and validation counts,
remeshes, the best feasible validated F, and the first attainment of each
target (wall-time FE / solve-count FE; "censored" if not attained), the
60-FE checkpoint, and the amendment-7 safeguard record (refusals by kind,
restorations, restarts, mesh checks, repairs after a failed attempt,
inadmissible validations, C's initial-set retries) with the host record
(contamination, deferrals, in-run and post-run wall time apart). A smoke
session is labelled as such on the first line.
"""
import collections
import json
import sys
from pathlib import Path


def fmt(v, spec=".3g"):
    return "—" if v is None else format(v, spec)


def main(argv=None):
    d = Path((argv or sys.argv[1:])[0])
    m = json.loads((d / "manifest.json").read_text())
    lines = [f"# Harness session `{m['session_id']}` (label: **{m['label']}**)", ""]
    if m["label"] == "smoke":
        lines += ["**Smoke run: not data.** Both models at L0, an L0 anchor computed for "
                  "this session, a short budget, and drills that exercise the safeguards "
                  "(injected mesher failures, a boundary start, the gate). It checks the "
                  "harness end to end, nothing else.", ""]
    hb, ha, p = m["host_before"], m["host_after"], m["provenance"]
    cal = m["calibration"]
    lines += [
        f"- Host: {hb['host']}, {hb['cpu_model']}, {hb['cpu_count']} cores; load averages "
        f"before {', '.join(f'{x:.2f}' for x in hb['load'])}, after "
        f"{', '.join(f'{x:.2f}' for x in ha['load'])}; idle: {m['host_idle']}.",
        f"- Processes: {m['procs']} single-threaded; commit {p['commit'][:10]} "
        f"(tree clean: {p['tree_clean']}); {p['rustc']}; scipy {p['packages']['scipy']}, "
        f"numpy {p['packages']['numpy']}; lock {p['requirements_lock_sha256'][:12]}.",
        f"- Unit: {cal['unit_seconds']:.4f} s, the median of {len(cal['validation_seconds'])} "
        f"cold θ0 validations in {cal.get('processes', 1)} simultaneous processes "
        f"(range {min(cal['validation_seconds']):.4f}–{max(cal['validation_seconds']):.4f} s); "
        f"single-process median {cal.get('single_process', {}).get('unit_seconds', float('nan')):.4f} s "
        f"(diagnostic); "
        f"training forward {cal['weights']['train_forward_eval']:.4f} FE, adjoint "
        f"{cal['weights']['adjoint_eval']:.4f} FE per evaluation; budget {m['budget_fe']:g} FE.",
        "",
        "| run | stop | wall FE | solve FE | train | remesh | valid. | best feasible F_val "
        "| T50 | T90 | T99 |",
        "|---|---|---|---|---|---|---|---|---|---|---|",
    ]
    ran = [j for j in m["jobs"] if j.get("status", "ran") == "ran"]
    for job in ran:
        t = json.loads((d / job["trace"]).read_text())
        led = t["ledger"]
        ev = t.get("events", [])
        ntr = sum(1 for e in ev if e["kind"] == "train")
        nrm = sum(1 for e in ev if e["kind"] == "train" and e["remesh"])
        best = t.get("best_validated_feasible")
        att = []
        for name in ("T50", "T90", "T99"):
            a = t.get("attainment", {}).get(name, {})
            att.append(f"{a['wall_fe']:.2f} / {a['solve_fe']:.2f}" if a.get("attained")
                       else "censored")
        lines.append(f"| {job['name']} | {t['stop_reason']} | {led['total_fe']:.3f} | "
                     f"{fmt(led['solve_fe'], '.2f')} | {ntr} | {nrm} | "
                     f"{len(t.get('validations', []))} | "
                     f"{fmt(best and best['F_val'], '.4e')} | {' | '.join(att)} |")
    lines += ["", "Attainment cells are wall-time FE / solve-count FE at the first validation "
              "that attained the target, within the budget. Method C is an empirical RBF "
              "surrogate, not a reduced physical or waveform model (#1185 amendment 6)."]
    # Amendment 7: safeguards and host.
    lines += ["", "## Safeguards (amendment 7) and host", "",
              "| run | refusals | restorations ok/all | restarts | mesh checks ok/all | "
              "repaired after a failed attempt | inadmissible validations | C initial retries "
              "| 60-FE checkpoint | contaminated | in-run s | post-run s |",
              "|---|---|---|---|---|---|---|---|---|---|---|---|"]
    total = collections.Counter()
    for job in ran:
        t = json.loads((d / job["trace"]).read_text())
        ref = collections.Counter(r["kind"] for r in t.get("refusals") or [])
        rest = t.get("restorations") or []
        rok = sum(1 for r in rest if r["restored_x"] is not None)
        mc = [e for e in t.get("events", []) if e["kind"] == "mesh_check"]
        mok = sum(1 for e in mc if e["status"] == "ok")
        rep_ok = sum(1 for e in t.get("events", []) if e["kind"] == "train" and e.get("remesh")
                     and len(e.get("attempts") or []) > 1)
        inad = sum(1 for v in t.get("validations", [])
                   if v.get("status", "ok") == "ok" and not v.get("admissible", True))
        ci = sum(1 for c in t.get("surrogate_initial_set") or [] if len(c["tried"]) > 1)
        cp = (t.get("checkpoint_60") or {}).get("targets", {})
        cps = "/".join(n for n in ("T50", "T90", "T99") if cp.get(n, {}).get("attained")) or "—"
        for k, v in ref.items():
            total[f"refused {k} ({t['method']})"] += v
        total["restorations ok"] += rok
        total["restorations failed"] += len(rest) - rok
        total["restarts"] += len(t.get("restarts") or [])
        total["mesh checks ok"] += mok
        total["repairs after a failed attempt"] += rep_ok
        total["inadmissible validations (no credit)"] += inad
        total["C initial-set retries"] += ci
        lines.append(f"| {job['name']} | {dict(ref) or '—'} | {rok}/{len(rest)} | "
                     f"{len(t.get('restarts') or [])} | {mok}/{len(mc)} | {rep_ok} | {inad} | "
                     f"{ci} | {cps} | {t.get('host_contaminated')} | "
                     f"{t.get('in_run_wall_seconds', 0):.1f} | "
                     f"{t.get('post_run_wall_seconds', 0):.1f} |")
    lines += ["", "Totals: " + "; ".join(f"{k} {v}" for k, v in sorted(total.items())) + "."]
    hr = m.get("host_rule") or {}
    defer = [j for j in m["jobs"] if j.get("status") == "deferred"]
    lines += ["", f"- Host gate: {'on' if hr.get('gate', True) else 'off (smoke on a shared host)'}"
              f"; pre-run limit load <= {hr.get('pre_run_max_load')}, contamination above "
              f"{hr.get('contamination_load')} for more than "
              f"{hr.get('contamination_seconds')} s of the in-run phase.",
              f"- Deferred (not started): {len(defer)}; host_idle: {m['host_idle']}."]
    cal_h = (hr.get("calibration") or {})
    if cal_h:
        lines.append(f"- Calibration: pre-run load {cal_h.get('pre_run_load'):.2f}, "
                     f"contaminated {cal_h.get('host_contaminated')} (longest overload "
                     f"{cal_h.get('longest_overload_s'):.0f} s).")
    for g in m.get("gate_drill") or []:
        lines.append(f"- Gate drill {g['name']}: {g['status']} (deferral {g['deferrals']}, "
                     f"load {g['pre_run_load']:.2f}); never started.")
    diag = []
    for job in ran:
        t = json.loads((d / job["trace"]).read_text())
        pd = t.get("post_run_diagnostic")
        if t.get("aborted_request"):
            lines.append(f"- {job['name']}: budget abort during a "
                         f"{t['aborted_request']['op']} request "
                         f"({t['aborted_request']['category']}).")
        if pd and pd.get("rows"):
            for r in pd["rows"]:
                diag.append(f"| {job['name']} | {r['fraction']} | {r['std_pred']:.4e} | "
                            f"{r['std_train']:.4e} | {r['std_val']:.4e} | "
                            f"{r['mean_pred']:.6f} | {r['mean_train']:.6f} | "
                            f"{r['mean_val']:.6f} |")
        elif pd:
            diag.append(f"| {job['name']} | — | {pd.get('status')} {pd.get('error') or ''} "
                        "| | | | | |")
    if diag:
        lines += ["", "Method C post-run held-out-current diagnostic (amendment 6 §3; not "
                  "counted in the run's budget or attainment): C's prediction at the final "
                  "incumbent against the training and validation models.", "",
                  "| run | fraction | std pred | std train | std val | mean pred | "
                  "mean train | mean val |", "|---|---|---|---|---|---|---|---|", *diag]
    (d / "SUMMARY.md").write_text("\n".join(lines) + "\n")
    print("\n".join(lines))


if __name__ == "__main__":
    main()
