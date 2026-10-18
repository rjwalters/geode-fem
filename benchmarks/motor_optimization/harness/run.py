"""One experiment-2 optimizer run (issues #1195, #1201): a fresh evaluator
process, a fresh ledger, one method from one start in one space, and the
trace.

    pyrun.sh run.py --session SESSION.json --method slsqp|cobyqa|surrogate
                    --space a|b --start theta0|lhs1|...|lhs4 --repeat 1|2
                    --out TRACE.json [--position K]

SESSION.json is written by session.py (the unit, the per-resolution
training-model records with their solve-count weights and theta0 training
constants, the evaluator command, the meshes and the anchor). A method's
training model is its spec's resolution (`driver.METHOD_SPECS`), else the
session's primary model. The ledger starts before the evaluator process is
spawned, so the process start-up is charged to the run.

Wall time is recorded in two parts: `in_run_wall_seconds` (the run's
ledger, the budgeted quantity) and `post_run_wall_seconds` (C's post-run
diagnostic, outside the budget; the #1200 Judge's finding 3).
"""
import json
import os
import sys
import time
import traceback
from pathlib import Path

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

from acceptance import Anchor  # noqa: E402
from driver import (DIAGNOSTIC_FRACTIONS, METHOD_LABELS, METHOD_SPECS, METHODS,  # noqa: E402
                    Problem, Run, c_current_predictions, execute)
from evaluator import Aborted, Evaluator, EvaluatorFailure, to_bits  # noqa: E402
from ledger import Ledger  # noqa: E402
from linalg_info import fingerprint as linalg_fingerprint  # noqa: E402
import resource_usage  # noqa: E402


def phase_resources(before, ev, error, label):
    evidence = (ev.resources() if ev is not None and hasattr(ev, "resources")
                else getattr(error, "evaluator_resources", None))
    receipt = (evidence or {}).get("child_receipt") or {}
    measured = resource_usage.interval(
        before, resource_usage.snapshot(), label=label,
        children_complete=receipt.get("cpu_complete") is True,
        incomplete_reason="evaluator clean exit and descendant accounting not established")
    measured["evaluator"] = evidence or {
        "available": False, "reason": "no evaluator lifecycle measurements"}
    return measured


def model_of(session, method=None):
    """The training-model record of `method` (its spec's resolution, else
    the session's primary model). A session without `models` (older
    records) has one model: its top-level fields."""
    res = METHOD_SPECS[method]["resolution"] if method else None
    models = session.get("models")
    if not models:
        if res is not None:
            raise ValueError(f"{method} requires an explicit {res} training model")
        return {"train_mesh": session["evaluator"].get("train_mesh"),
                "train_level": session["evaluator"].get("train_level"),
                "theta0_train": session.get("theta0_train"), "weights": session.get("weights")}
    model = models[res or session["primary_model"]]
    if res is not None and model["train_level"] != int(res[1:]):
        raise ValueError(f"{method} requires training level {res}")
    return model


def evaluator_argv(session, space, mesh_out, method=None):
    s = session["evaluator"]
    m = model_of(session, method)
    argv = [s["binary"], "--space", space, "--train-mesh", m["train_mesh"],
            "--val-mesh", s["val_mesh"], "--anchor", s["anchor"],
            "--train-level", str(m["train_level"]), "--val-level", str(s["val_level"]),
            "--mesh-out", str(mesh_out)]
    return argv + list(s.get("extra_args", []))


def thread_env():
    env = dict(os.environ)
    env["RAYON_NUM_THREADS"] = "1"
    env["GEODE_NUM_THREADS"] = "1"
    return env


def load_start(session, start, space):
    starts = json.loads(Path(session["starts"]).read_text())
    return starts["starts"][start][space]


def rel(p, a):
    return abs(p - a) / abs(a) if a else None


def post_run_diagnostic(run, make_evaluator, unit, clock=time.monotonic):
    """Method C's held-out-current diagnostic (#1185 amendment 6 §3): after
    the run has stopped, a fresh evaluator process evaluates the final
    incumbent at current fractions 0.25 and 0.75 on the training and the
    validation model, against C's predictions there
    (`driver.c_current_predictions`). Its cost is recorded here only: it is
    never charged to the run's ledger and cannot change the run's budget,
    path or attainment times."""
    pred = c_current_predictions(run)
    out = {"counted_in_run_budget": False, "fractions": list(DIAGNOSTIC_FRACTIONS),
           "prediction": pred}
    if pred is None:
        return dict(out, status="no incumbent")
    t0 = clock()
    resources_before = resource_usage.snapshot()
    ev = None
    failure = None
    r = None
    try:
        ev = make_evaluator()
        r = ev.request("diagnose", timeout=max(10 * unit, 60.0), x_bits=to_bits(pred["x"]),
                       fractions=list(DIAGNOSTIC_FRACTIONS))
    except Exception as e:  # a diagnostic failure cannot discard the stopped run
        failure = e
        out.update(status="failed", error=str(e) or "timeout")
    finally:
        if ev is not None:
            ev.close()
        out["resources"] = phase_resources(resources_before, ev, failure, "post_run_diagnostic")
        out["seconds"] = clock() - t0
    if r is None:
        return out
    out.update(status="ok" if r.get("ok") else "failed", error=r.get("error"),
               charges=r.get("charges", []), seconds=clock() - t0)
    if not r.get("ok"):
        return out
    rows = []
    for k, p in enumerate(pred["predictions"]):
        t_s, t_m = r["training"]["stds"][k], r["training"]["means"][k]
        v_s, v_m = r["validation"]["stds"][k], r["validation"]["means"][k]
        rows.append(dict(p, std_train=t_s, mean_train=t_m, std_val=v_s, mean_val=v_m,
                         rel_std_train=rel(p["std_pred"], t_s),
                         rel_mean_train=rel(p["mean_pred"], t_m),
                         rel_std_val=rel(p["std_pred"], v_s),
                         rel_mean_val=rel(p["mean_pred"], v_m)))
    out["rows"] = rows
    out["validation_mesh"] = r["validation"]["mesh"]
    return out


def physical_post_run_diagnostic(run, make_evaluator, unit, clock=time.monotonic):
    """D/E nonlinear solves, isolated from the stopped optimizer and its ledger."""
    import physical_diagnostic
    selected = physical_diagnostic.select(run)
    out = {"kind": "nonlinear_physical_model", "counted_in_run_budget": False,
           "fractions": list(DIAGNOSTIC_FRACTIONS), "selection": selected,
           "units": {"torque": "N m", "seconds": "s"}}
    if selected is None:
        return dict(out, status="unavailable", reason="no buildable training incumbent")
    t0, resources_before = clock(), resource_usage.snapshot()
    ev = failure = None
    try:
        ev = make_evaluator()
        if ev.ready.get("validation_msh_sha256") != run.anchor.mesh_sha256:
            raise ValueError("diagnostic validator differs from the registered anchor mesh")
        bits = to_bits(selected["x"])
        reply = ev.request("diagnose", timeout=max(10 * unit, 60.0), x_bits=bits,
                           fractions=list(DIAGNOSTIC_FRACTIONS))
        out["reply"] = reply  # Preserve failures and malformed diagnostic data too.
        out["charges"] = reply.get("charges", [])
        if not reply.get("ok"):
            raise ValueError(reply.get("error") or "diagnostic evaluator failed")
        if reply.get("x_bits") != bits:
            raise ValueError("diagnostic returned a different design point")
        out["rows"] = physical_diagnostic.rows(reply)
        out["status"] = "ok"
    except Exception as e:
        failure = e
        out.update(status="failed", error=str(e) or type(e).__name__)
    finally:
        if ev is not None:
            ev.close()
        out["resources"] = phase_resources(resources_before, ev, failure, "post_run_diagnostic")
        out["seconds"] = clock() - t0
    return out


def run_one(session, method, space, start, repeat, out, position=None, clock=time.monotonic,
            evaluator_factory=None, diagnostic=True):
    """Run and write the trace; return it."""
    if method not in METHODS:
        raise ValueError(method)
    resources_before = resource_usage.snapshot()
    out = Path(out)
    out.parent.mkdir(parents=True, exist_ok=True)
    anchor = Anchor.from_file(session["evaluator"]["anchor"])
    budget_fe = session["budget_fe"]
    model = model_of(session, method)
    ledger = Ledger(session["unit_seconds"], budget_fe, weights=model["weights"], clock=clock)
    started = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
    in_run_start = time.time()
    load_before = os.getloadavg()
    trace = {"issue": 1201, "label": session["label"], "method": method,
             "method_label": METHOD_LABELS[method], "method_spec": METHOD_SPECS[method],
             "training_model": model,
             "space": space,
             "start": start, "repeat": repeat, "order_position": position,
             "session": session.get("session_id"), "budget_fe": budget_fe,
             "unit_seconds": session["unit_seconds"], "started_utc": started,
             "load_before": load_before, "linear_algebra": linalg_fingerprint()}
    ev = None
    run = None
    failure = None
    mesh_out = out.with_name(out.stem + ".meshes")
    try:
        if evaluator_factory is None:
            argv = evaluator_argv(session, space, mesh_out, method)
            ev = Evaluator(argv, env=thread_env(), stderr_path=out.with_suffix(".stderr.log"),
                           timeout=ledger.remaining_seconds())
        else:
            ev = evaluator_factory(space, ledger)
        ledger.sync("process start-up")
        problem = Problem(ev.ready["space"], model["theta0_train"])
        x_start = load_start(session, start, space)
        trace["x_start"] = x_start
        run = Run(ev, ledger, anchor, problem, method, clock=clock)
        reason, message, error = execute(run, problem.u_of(x_start))
        if not ledger.exhausted():
            # Up to the stop; a run stopped by the budget is charged the
            # budget exactly, and nothing after it.
            ledger.sync("driver end")
        trace.update(stop_reason=reason, message=message, error=error, **run.result())
    except Aborted as e:
        failure = e
        ledger.charge_to_budget("overhead", "start-up aborted at the budget")
        trace.update(stop_reason="budget", message="budget spent during start-up",
                     error=None, ledger=ledger.to_json())
    except Exception as e:  # noqa: BLE001 - every outcome is recorded
        failure = e
        trace.update(stop_reason="error", message=type(e).__name__, error=str(e),
                     traceback=traceback.format_exc(), ledger=ledger.to_json())
    finally:
        if ev is not None:
            ev.close()
        trace["resources"] = phase_resources(resources_before, ev, failure, "optimizer_run")
    # The in-run phase ends here (wall clock, for the session's host monitor,
    # amendment 7 §6): the post-run diagnostic below is outside it.
    trace["in_run_start_time"] = in_run_start
    trace["in_run_end_time"] = time.time()
    # After the stop and the ledger snapshot above: C's post-run diagnostic,
    # in its own process and its own ledger section.
    if diagnostic and method in ("surrogate", "cobyqa_l0", "slsqp_l0") and run is not None:
        if evaluator_factory is None:
            def make():
                return Evaluator(evaluator_argv(session, space, mesh_out / "diagnostic", method),
                                 env=thread_env(), stderr_path=out.with_suffix(".stderr.log"),
                                 timeout=max(10 * session["unit_seconds"], 60.0))
        else:
            def make():
                return evaluator_factory(space, None)
        diagnostic_fn = (post_run_diagnostic if method == "surrogate"
                         else physical_post_run_diagnostic)
        trace["post_run_diagnostic"] = diagnostic_fn(run, make, session["unit_seconds"],
                                                     clock=clock)
    trace["post_run_end_time"] = time.time()
    trace["in_run_wall_seconds"] = trace["ledger"]["total_seconds"]
    trace["post_run_wall_seconds"] = (trace.get("post_run_diagnostic") or {}).get("seconds", 0.0)
    trace["load_after"] = os.getloadavg()
    trace["finished_utc"] = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
    out.write_text(json.dumps(trace, indent=1, default=float) + "\n")
    return trace


def main(argv=None):
    import argparse
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--session", required=True, type=Path)
    ap.add_argument("--method", required=True, choices=METHODS)
    ap.add_argument("--space", required=True, choices=["a", "b"])
    ap.add_argument("--start", required=True,
                    choices=["theta0", "lhs1", "lhs2", "lhs3", "lhs4", "drill"])
    ap.add_argument("--repeat", required=True, type=int)
    ap.add_argument("--position", type=int, default=None)
    ap.add_argument("--out", required=True, type=Path)
    a = ap.parse_args(argv)
    session = json.loads(a.session.read_text())
    t = run_one(session, a.method, a.space, a.start, a.repeat, a.out, a.position)
    att = {k: v["attained"] for k, v in t.get("attainment", {}).items()}
    print(f"{a.out.name}: {t['stop_reason']} total {t['ledger']['total_fe']:.3f} FE "
          f"attained {att} {t.get('error') or ''}")
    return 0 if t["stop_reason"] != "error" else 1


if __name__ == "__main__":
    sys.exit(main())
