"""Recover the design points of the pilot's failed requests (issue #1201,
#1185 amendment 7 §1; the method of the #1200 Judge's replay).

The pilot traces (`../runs/pilot/`, #1198) do not record the x of the
request that failed. This script recovers it by re-driving the **pilot's
own harness** (extracted with `git archive` at the pilot's commit, from the
pilot manifest) on the committed traces:

- COBYQA and C (`replay` mode): every training and validation reply is
  replayed from the trace, and the run is driven until it asks for the
  first request beyond the trace. Every replayed x must match the trace
  bit for bit, in order. Needs nothing but git and the locked environment
  **on the pilot's linear-algebra stack**: COBYQA's and C's paths depend on
  BLAS/LAPACK at the bit level, and the pilot ran on macOS arm64 with the
  lock's wheels (Accelerate). On that stack `--check` re-derives these
  points bit for bit (`pilot_replay_host.json` records the stack it was
  checked on); on another (the Linux wheels' OpenBLAS) the replays diverge
  after a few steps, and only the stack-independent checks apply (the
  recovered designs' construction verdicts and the error strings,
  `test_safeguards.PilotReplayTests`).
- SLSQP (`real` mode): its path needs the gradients, which the trace does
  not record, so training goes to the pilot's evaluator binary (the
  manifest's SHA-256 is checked) on the L1 and L2 θ0 meshes; validations
  are replayed. The failing request's error must equal the trace's, and a
  fresh evaluator probed at the recovered x must return it again.

    pyrun.sh recover_pilot_failures.py [--binary PILOT_EVALUATOR] [--out F]
    pyrun.sh recover_pilot_failures.py --check [--record-host]
        # replay mode vs the record; --record-host writes the stack it
        # passed on (pilot_replay_host.json)

The record (`pilot_failures.json`) has, per failed run, the recovered x and
its bit patterns, the mode, the match counts and the trace's error. Repeats
are recovered separately; the distinct designs are listed once each.
"""
import argparse
import hashlib
import io
import json
import subprocess
import sys
import tarfile
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
PILOT = HERE.parent / "runs/pilot"
OUT = HERE / "pilot_failures.json"
HOST = HERE / "pilot_replay_host.json"
MESHES = ROOT / "benchmarks/motor_reference/geometry/meshes"


def pilot_commit():
    return json.loads((PILOT / "manifest.json").read_text())["provenance"]["commit"]


def extract_harness(commit, dest):
    """The pilot's harness directory at `commit`, under `dest`."""
    tar = subprocess.run(["git", "archive", commit, "benchmarks/motor_optimization/harness"],
                         cwd=ROOT, capture_output=True, check=True).stdout
    with tarfile.open(fileobj=io.BytesIO(tar)) as t:
        t.extractall(dest, filter="data")
    return Path(dest) / "benchmarks/motor_optimization/harness"


def failed_runs():
    out = []
    for p in sorted(PILOT.glob("*_r[12].json")):
        t = json.loads(p.read_text())
        if t.get("stop_reason") == "failed":
            out.append(p.stem)
    return out


def sha256(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def replay(old, name, real_binary=None):
    """Re-drive the pilot harness (module `old`) on trace `name`; return the
    recovery record."""
    import importlib
    sys.path.insert(0, str(old))
    try:
        for m in ("run", "driver", "evaluator", "acceptance", "ledger", "surrogate"):
            sys.modules.pop(m, None)
        runmod = importlib.import_module("run")
        evmod = importlib.import_module("evaluator")
        acc = importlib.import_module("acceptance")
    finally:
        sys.path.remove(str(old))
    T = json.loads((PILOT / f"{name}.json").read_text())
    session = json.loads((PILOT / "session.json").read_text())
    session["starts"] = str(old / "starts.json")
    session["evaluator"] = dict(session["evaluator"], anchor=str(HERE.parent / "anchor.json"),
                                train_mesh=str(MESHES / "icem_initial_sector_L1.msh"),
                                val_mesh=str(MESHES / "icem_initial_sector_L2.msh"))
    if real_binary is not None:
        session["evaluator"]["binary"] = str(real_binary)
    starts = json.loads((old / "starts.json").read_text())
    sp, A = T["space"], T["anchor"]
    trains = [e for e in T["events"] if e["kind"] == "train"]
    vals = T["validations"]
    log = {"name": name, "method": T["method"], "space": sp, "start": T["start"],
           "repeat": T["repeat"], "mode": "real" if real_binary else "replay",
           "train_matches": 0, "train_bitwise": 0, "val_matches": 0, "divergence": None,
           "x": None, "op": None, "error": None, "trace_error": T["error"]}

    class Fake:
        def __init__(self, real=None):
            self.real, self.ti, self.vi = real, 0, 0
            if real is not None:
                self.ready = real.ready
            else:
                s = starts["spaces"][sp]
                self.ready = {"op": "ready", "ok": True,
                              "space": {k: s[k] for k in ("free", "lower", "upper", "x0")},
                              "validation_msh_sha256": A["msh_sha256"],
                              "anchor": {"F0": A["F0"],
                                         "mean0_full_current": A["mean0_full_current"],
                                         "level": A["level"], "msh_sha256": A["msh_sha256"]}}

        def _end(self, op, x, err):
            log["x"], log["op"], log["error"] = x, op, err
            return {"ok": False, "error": err, "charges": []}

        def request(self, op, timeout=None, **f):
            x = evmod.from_bits(f["x_bits"]) if "x_bits" in f else None
            if op == "train":
                if self.ti >= len(trains):
                    if self.real is not None:
                        r = self.real.request(op, timeout=None, **f)
                        return self._end(op, x, r.get("error") if not r.get("ok")
                                         else "beyond the trace and succeeded")
                    return self._end(op, x, "replay: request beyond the recorded trace")
                e = trains[self.ti]
                if e["x"] != x:
                    log["divergence"] = f"train {self.ti}: x differs"
                    return self._end(op, x, "divergence")
                self.ti += 1
                log["train_matches"] += 1
                if self.real is not None:
                    r = self.real.request(op, timeout=None, **f)
                    if not r.get("ok"):
                        log["divergence"] = f"train {self.ti - 1} failed: {r.get('error')}"
                        return r
                    log["train_bitwise"] += int(r["F"] == e["F"]
                                                and r["mean_full"] == e["mean_full"])
                    r["charges"] = []
                    return r
                return {"ok": True, "op": "train", "x_bits": f["x_bits"], "F": e["F"],
                        "mean_full": e["mean_full"], "stds": e["stds"], "means": None,
                        "fractions": [0.0, 0.5, 1.0], "dF": None, "dmean_full": None,
                        "generation": e["generation"], "event": None,
                        "quality": e.get("quality"),
                        "newton_iterations": e.get("newton_iterations"), "charges": []}
            if op == "validate":
                if self.vi >= len(vals):
                    return self._end(op, x, "replay: request beyond the recorded trace")
                v = vals[self.vi]
                if v["x"] != x:
                    log["divergence"] = f"validate {self.vi}: x differs"
                    return self._end(op, x, "divergence")
                self.vi += 1
                log["val_matches"] += 1
                fl = v["attained"]
                return {"ok": True, "op": "validate", "x_bits": f["x_bits"],
                        "F_val": v["F_val"], "mean_full": v["mean_full"], "stds": v["stds"],
                        "fractions": v["fractions"],
                        "acceptance": {"F_val": v["F_val"], "mean_full": v["mean_full"],
                                       "mean_ok": v["mean_ok"],
                                       "targets": [{"name": n, "attained": fl[n],
                                                    "T": fr * A["F0"]}
                                                   for n, fr in acc.TARGETS]},
                        "validator_msh_sha256": A["msh_sha256"],
                        "anchor_msh_sha256": A["msh_sha256"], "mesh": v["mesh"],
                        "msh_sha256": v["msh_sha256"], "charges": []}
            if self.real is not None:
                return self.real.request(op, timeout=60, **f)
            return {"ok": True, "charges": []}

        def close(self):
            if self.real is not None:
                self.real.close()

    with tempfile.TemporaryDirectory() as tmp:
        def factory(space, ledger):
            real = None
            if real_binary is not None:
                argv = runmod.evaluator_argv(session, space, Path(tmp) / "meshes")
                real = evmod.Evaluator(argv, env=runmod.thread_env())
            return Fake(real)

        t = runmod.run_one(session, T["method"], sp, T["start"], T["repeat"],
                           Path(tmp) / "trace.json", position=None, clock=lambda: 0.0,
                           evaluator_factory=factory, diagnostic=False)
        log["replay_stop"] = t["stop_reason"]
        log["replay_error"] = t.get("error")
        log["replay_traceback"] = t.get("traceback")
        if real_binary is not None and log["x"] is not None:
            # A fresh evaluator at the recovered x returns the trace's error.
            ev = evmod.Evaluator(runmod.evaluator_argv(session, sp, Path(tmp) / "probe"),
                                 env=runmod.thread_env())
            try:
                r = ev.request("train", timeout=None, x_bits=evmod.to_bits(log["x"]),
                               gradient=False, purpose="iterate", note="probe")
            finally:
                ev.close()
            log["probe_error"] = None if r.get("ok") else "train: " + str(r.get("error"))
    log["recorded_trains"], log["recorded_vals"] = len(trains), len(vals)
    log["x_bits"] = evmod.to_bits(log["x"]) if log["x"] is not None else None
    return log


def check_record(rec, log):
    """A replay-mode recovery must match the committed record bit for bit."""
    keys = ("x_bits", "train_matches", "val_matches", "divergence", "recorded_trains",
            "recorded_vals")
    return {k: (rec.get(k), log.get(k)) for k in keys if rec.get(k) != log.get(k)}


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--binary", type=Path, default=None,
                    help="the pilot's evaluator (SHA-256 checked against the manifest)")
    ap.add_argument("--out", type=Path, default=OUT)
    ap.add_argument("--check", action="store_true",
                    help="re-derive the replay-mode points and compare with --out")
    ap.add_argument("--record-host", action="store_true",
                    help="with --check: write the linear-algebra stack it passed on")
    a = ap.parse_args(argv)
    commit = pilot_commit()
    want = json.loads((PILOT / "manifest.json").read_text())["provenance"]["evaluator"]["sha256"]
    with tempfile.TemporaryDirectory() as d:
        old = extract_harness(commit, d)
        if a.check:
            record = json.loads(a.out.read_text())
            bad = {}
            n = 0
            for rec in record["runs"]:
                if rec["mode"] != "replay":
                    continue
                n += 1
                diff = check_record(rec, replay(old, rec["name"]))
                if diff:
                    bad[rec["name"]] = diff
            print(f"replay-mode recoveries checked: {n}, mismatches: {bad or 'none'}")
            ok = not bad and n > 0
            if ok and a.record_host:
                import importlib.util
                spec = importlib.util.spec_from_file_location("linalg_info",
                                                              HERE / "linalg_info.py")
                li = importlib.util.module_from_spec(spec)
                spec.loader.exec_module(li)
                fingerprint = li.fingerprint
                manifest = json.loads((PILOT / "manifest.json").read_text())
                HOST.write_text(json.dumps({
                    "issue": 1201, "checked": n, "mismatches": "none",
                    "linear_algebra": fingerprint(),
                    "pilot_host": {"platform": manifest["host_before"]["platform"],
                                   "cpu_model": manifest["host_before"]["cpu_model"],
                                   "packages": manifest["provenance"]["packages"]},
                    "note": "the replay-mode recoveries re-derive bit for bit under this "
                            "linear-algebra stack (the pilot host's: the same platform and "
                            "lock); the bit-for-bit test runs only where the stack matches"},
                    indent=1) + "\n")
            return 0 if ok else 1
        if a.binary is not None and sha256(a.binary) != want:
            raise SystemExit(f"{a.binary}: SHA-256 is not the pilot evaluator's {want}")
        runs = []
        for name in failed_runs():
            method = name.split("_")[0]
            real = a.binary if method == "slsqp" else None
            if method == "slsqp" and real is None:
                raise SystemExit("SLSQP runs need --binary (the pilot evaluator)")
            log = replay(old, name, real)
            print(json.dumps({k: log[k] for k in ("name", "mode", "train_matches",
                                                  "val_matches", "divergence", "error")}),
                  flush=True)
            runs.append(log)
    designs = {}
    for r in runs:
        key = tuple(r["x_bits"])
        d = designs.setdefault(key, {"space": r["space"], "x": r["x"], "x_bits": r["x_bits"],
                                     "trace_error": r["trace_error"], "runs": []})
        d["runs"].append(r["name"])
    kind = (lambda e: "slot_flank" if "slot flank degenerates" in e
            else "fresh_mesh_policy" if "fresh mesh fails the policy" in e else "other")
    record = {"issue": 1201, "pilot_commit": commit, "pilot_evaluator_sha256": want,
              "method": "re-drive the pilot harness (git archive at the pilot commit) on the "
                        "committed traces; COBYQA and C replay every reply, SLSQP trains on "
                        "the pilot evaluator; each recovered x is the first request beyond "
                        "the trace",
              "runs": runs,
              "designs": [dict(d, kind=kind(d["trace_error"])) for d in designs.values()]}
    a.out.write_text(json.dumps(record, indent=1) + "\n")
    print(f"{len(runs)} failed runs, {len(designs)} distinct designs -> {a.out}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
