"""Recheck numeric supplement tables/options against pinned Git objects.

This verifies transcription and provenance, not FEM results, motion quality,
protocol adoption, or whether an unmerged implementation is ready to run.
Run from any directory with Python 3 and Git; no numerical packages needed.
"""
import ast
import hashlib
import json
from pathlib import Path
import re
import subprocess

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[2]


def number(node, constants):
    if isinstance(node, ast.Constant) and type(node.value) in (int, float, bool):
        return node.value
    if isinstance(node, ast.Name):
        return constants[node.id]
    if isinstance(node, ast.BinOp) and isinstance(node.op, ast.Pow):
        return number(node.left, constants) ** number(node.right, constants)
    raise ValueError(f"unsupported numeric syntax: {ast.dump(node)}")


def main():
    data = json.loads((HERE / "main-study-settings-supplement.json").read_text())
    sources = {}
    for path, provenance in data["source_files"].items():
        raw = subprocess.check_output(["git", "show", provenance["commit"] + ":" + path], cwd=REPO)
        assert hashlib.sha256(raw).hexdigest() == provenance["sha256"], path
        sources[path] = raw.decode()
    ref = json.loads(sources["benchmarks/motor_reference/reference_data.json"])
    reference_bounds = {r["name"]: r for r in ref["optimization_parameter_bounds"]}
    tex = (HERE / "main-study-settings-supplement.tex").read_text()
    for p in data["scalar_parameters"]:
        row = reference_bounds[p["name"]]
        for key in ("minimum", "initial", "maximum"):
            assert p[key] == row[key], (p["name"], key)
        table = next(line for line in tex.splitlines() if line.startswith(p["name"] + " & "))
        fields = table.rstrip(" \\").split(" & ")
        assert [float(x) for x in fields[-3:]] == [p[k] * 1000 for k in ("minimum", "initial", "maximum")]
    cp = sources["crates/geode-core/src/mesh/icem_controlpoints.rs"]
    block = cp.split("pub const OFFSET_BOUNDS:")[1].split("= [", 1)[1].split("];", 1)[0]
    bounds = [tuple(map(float, p)) for p in re.findall(r"\(([^,]+),\s*([^)]+)\)", block)]
    assert len(bounds) == len(data["control_point_groups"]) == 10
    assert sum(p["control_points"] for p in data["control_point_groups"]) == 55
    for p, (lo, hi) in zip(data["control_point_groups"], bounds):
        assert (p["minimum"], p["initial"], p["maximum"]) == (lo, 0, hi)
        table = next(line for line in tex.splitlines() if line.startswith(p["name"] + " & "))
        fields = table.rstrip(" \\").split(" & ")
        assert int(fields[1]) == p["control_points"]
        assert [float(x) for x in fields[-2:]] == [lo * 1000, hi * 1000]
    driver = ast.parse(sources["benchmarks/motor_optimization/harness/driver.py"])
    constants = {}
    for n in driver.body:
        if isinstance(n, ast.Assign) and len(n.targets) == 1 and isinstance(n.targets[0], ast.Name):
            if n.targets[0].id in ("COBYQA_RADIUS0", "SURROGATE"):
                constants[n.targets[0].id] = ast.literal_eval(n.value)
    assert constants["SURROGATE"] == data["registered_settings"]["C"]
    options = {}
    for fn in ("run_slsqp", "run_cobyqa", "run_surrogate"):
        f = next(n for n in driver.body if isinstance(n, ast.FunctionDef) and n.name == fn)
        calls = [n for n in ast.walk(f) if isinstance(n, ast.Call)
                 and isinstance(n.func, ast.Name) and n.func.id == "minimize"]
        assert len(calls) == 1
        d = next(k.value for k in calls[0].keywords if k.arg == "options")
        options[fn] = {ast.literal_eval(k): number(v, constants) for k, v in zip(d.keys, d.values)}
    expected = data["observed_provisional_options"]
    assert options["run_slsqp"] == {"ftol": data["registered_settings"]["SLSQP_ftol"],
                                    "maxiter": expected["SLSQP_maxiter"]}
    assert options["run_cobyqa"] == {"initial_tr_radius": data["registered_settings"]["COBYQA_initial_radius"],
                                     "final_tr_radius": data["registered_settings"]["COBYQA_final_radius"],
                                     "maxfev": expected["COBYQA_maxfev"],
                                     "maxiter": expected["COBYQA_maxiter"], "scale": expected["COBYQA_scale"]}
    assert options["run_surrogate"] == {"maxiter": expected["C_subproblem_maxiter"],
                                        "ftol": expected["C_subproblem_ftol"]}
    gate = ast.parse(sources["benchmarks/motor_optimization/harness/anchor_preflight.py"])
    assignments = {n.targets[0].id: ast.literal_eval(n.value) for n in gate.body
                   if isinstance(n, ast.Assign) and len(n.targets) == 1
                   and isinstance(n.targets[0], ast.Name)
                   and n.targets[0].id in ("TOLERANCE", "RULE")}
    rule = data["cross_platform_anchor"]
    assert assignments["RULE"] == rule["url"]
    for key in ("relative_F0", "relative_full_current_mean", "absolute_each_torque_Nm"):
        assert rule[key] == assignments["TOLERANCE"] == 1e-9
    snapshot = json.loads((HERE / rule["protocol_snapshot"]).read_text())
    assert snapshot["html_url"] == rule["url"]
    assert hashlib.sha256(snapshot["body"].encode()).hexdigest() == snapshot["body_sha256"]
    print(json.dumps({"status": "PASS", "scope": "source hashes, scalar/group numeric tables, optimizer options, anchor tolerance and protocol snapshot",
                      "source_files": len(sources), "scalar_rows": 8, "group_rows": 10,
                      "limitations": "No runtime, group-topology, protocol or publication verdict"}, indent=2))


if __name__ == "__main__":
    main()
