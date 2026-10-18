"""Frozen amendment-8 L0 directional checks; complete setup evidence, never main data."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from evaluator import Evaluator, to_bits
from make_starts import verdict
from run import thread_env

STEPS = (1e-3, 1e-4, 1e-5)
REL_TOL = 1e-5  # Existing motor_experiment2_harness CI criterion at h=1e-4.


def relative(actual, expected):
    return abs(actual - expected) / abs(expected) if expected else None


def plan(space):
    lo, hi, x0 = space["lower"], space["upper"], space["x0"]
    direction = [(((k + 1.) * .618_033_988_75) % 1. * 2. - 1.) * (hi[k] - lo[k])
                 for k in range(len(x0))]
    interior = [v + .25 * (.5 * (l + h) - v) for v, l, h in zip(x0, lo, hi)]
    return [("theta0", list(x0), direction), ("interior", interior, direction)]


def assess(rows):
    selected = next(row for row in rows if row["h"] == 1e-4)
    return all(selected[key] is not None and selected[key] < REL_TOL
               for key in ("rel_dF", "rel_dmean_full"))


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--binary", type=Path, required=True)
    ap.add_argument("--space", choices=("a", "b"), required=True)
    ap.add_argument("--newton", choices=("default", "tight"), required=True)
    ap.add_argument("--out", type=Path, required=True)
    a = ap.parse_args()
    if a.out.exists():
        raise SystemExit("refusing to overwrite an existing gradient check")
    root = HERE.parents[2]
    mesh = root / "benchmarks/motor_reference/geometry/meshes/icem_initial_sector_L0.msh"
    binary = a.binary.resolve()
    sources = [HERE / name for name in ("l0_gradient_check.py", "make_starts.py", "evaluator.py",
                                      "run.py", "resource_usage.py")]
    sources += list((root / "crates/geode-core/tests/common").glob("icem*.rs"))
    sources += [root / "crates/geode-core/examples/icem_harness_eval.rs",
                root / "crates/geode-core/tests/common/motor_resources.rs"]
    record = {"issue": 1202, "label": "setup_gradient_check", "space": a.space,
              "newton": a.newton, "steps": list(STEPS), "status": "running",
              "criterion": {"h": 1e-4, "relative_error_below": REL_TOL,
                            "source": "existing motor_experiment2_harness CI criterion",
                            "observables": ["F", "full_current_mean"],
                            "no_best_step_selection": True},
              "points": [], "mesh_sha256": sha(mesh), "binary_sha256": sha(binary),
              "source_sha256": {str(p.relative_to(root)): sha(p) for p in sources},
              "base_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip(),
              "tree_clean": not subprocess.check_output(["git", "status", "--porcelain"], cwd=root),
              "python": sys.version, "counted_in_main_run_budgets": False}
    a.out.parent.mkdir(parents=True, exist_ok=True)
    def save():
        a.out.write_text(json.dumps(record, indent=2, allow_nan=False) + "\n")
    save()
    started = time.monotonic()
    try:
        for point_index in (0, 1):
            item = {"point": ("theta0", "interior")[point_index], "status": "running",
                    "requests": [], "directional": []}
            record["points"].append(item)
            ev = None
            t0 = time.monotonic()
            try:
                command = [str(binary), "--space", a.space, "--train-mesh", str(mesh),
                           "--val-mesh", str(mesh), "--train-level", "0", "--val-level", "0",
                           "--newton", a.newton, "--mesh-out", str(a.out.with_suffix(f".{point_index}.meshes"))]
                item["command"] = command
                ev = Evaluator(command, env=thread_env(), timeout=120,
                               stderr_path=a.out.with_suffix(".stderr.log"))
                item["ready"] = ev.ready
                name, x, direction = plan(ev.ready["space"])[point_index]
                item.update(point=name, x=x, x_bits=to_bits(x), direction=direction)
                assert ev.ready["train_msh_sha256"] == record["mesh_sha256"]
                def request(op, xs, **kwargs):
                    req = {"op": op, "x_bits": to_bits(xs), **kwargs}
                    row = {"request": req, "status": "running"}
                    item["requests"].append(row)
                    save()
                    try:
                        reply = ev.request(op, timeout=180, x_bits=req["x_bits"], **kwargs)
                        row.update(status="replied", reply=reply)
                        save()
                        if not reply.get("ok") or reply.get("x_bits") != req["x_bits"]:
                            raise ValueError(f"{op}: failed or wrong design reply")
                        return reply
                    except BaseException as error:
                        row.update(status="failed", error=f"{type(error).__name__}: {error}")
                        save()
                        raise
                stencils = [(h, [v + h * d for v, d in zip(x, direction)],
                             [v - h * d for v, d in zip(x, direction)]) for h in STEPS]
                for xs in [x, *[p for _, plus, minus in stencils for p in (plus, minus)]]:
                    checked = request("check", xs)
                    if verdict(checked) is not None:
                        raise ValueError(f"inadmissible or non-moved stencil: {verdict(checked)}")
                def train(xs, gradient=False):
                    reply = request("train", xs, gradient=gradient,
                                    purpose="iterate" if gradient else "line_search")
                    if reply.get("status") != "ok" or reply["generation"] != 0 or reply["event"] is not None:
                        raise ValueError("stencil did not remain on frozen generation0")
                    return reply
                base = train(x, True)
                g, dm = base["dF"], base["dmean_full"]
                if len(g) != len(x) or len(dm) != len(x):
                    raise ValueError("gradient dimension differs from frozen space")
                ad_f = sum(v * d for v, d in zip(g, direction))
                ad_m = sum(v * d for v, d in zip(dm, direction))
                for h, plus, minus in stencils:
                    rp, rm = train(plus), train(minus)
                    fd_f = (rp["F"] - rm["F"]) / (2 * h)
                    fd_m = (rp["mean_full"] - rm["mean_full"]) / (2 * h)
                    item["directional"].append({"h": h, "adjoint_dF": ad_f, "fd_dF": fd_f,
                        "rel_dF": relative(fd_f, ad_f), "adjoint_dmean_full": ad_m,
                        "fd_dmean_full": fd_m, "rel_dmean_full": relative(fd_m, ad_m)})
                    save()
                item["criterion_passed"] = assess(item["directional"])
                item["status"] = "PASS" if item["criterion_passed"] else "criterion_failed"
            except Exception as error:
                item.update(status="failed", error=f"{type(error).__name__}: {error}")
                if getattr(error, "evaluator_resources", None):
                    item["startup_failure_resources"] = error.evaluator_resources
            finally:
                if ev is not None:
                    ev.close()
                    item["resources"] = ev.resources()
                item["wall_seconds"] = time.monotonic() - t0
                save()
        if not all(sha(root / name) == digest for name, digest in record["source_sha256"].items()):
            raise ValueError("source changed during gradient check")
        record["status"] = "PASS" if all(p["status"] == "PASS" for p in record["points"]) else "failed"
    except BaseException as error:
        record.update(status="interrupted_or_failed", error=f"{type(error).__name__}: {error}")
        raise
    finally:
        record["wall_seconds"] = time.monotonic() - started
        save()
    print(json.dumps({"status": record["status"], "space": a.space, "newton": a.newton,
                      "points": [{"point": p["point"], "status": p["status"],
                                  "directional": p["directional"]} for p in record["points"]]}))
    return 0 if record["status"] == "PASS" else 1


if __name__ == "__main__":
    sys.exit(main())
