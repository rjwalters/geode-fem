"""Post-run nonlinear physical-model diagnostic rules (amendment 8)."""
import math
import statistics


def select(run):
    """Best credited common-feasible F, earliest validation on a tie.

    If none exists, use the last optimizer-reported, successfully trained
    incumbent (including the initial point), without a feasibility claim.
    Never substitute the last line-search trial for that incumbent.
    """
    feasible = [v for v in run.validations if v.get("credited") and v.get("status") == "ok"
                and v.get("admissible") and v.get("mean_ok")]
    if feasible:
        v = min(feasible, key=lambda v: (v["F_val"], v["index"]))
        return {"x": list(v["x"]), "source": "best_common_validated_feasible",
                "validation": v["index"], "common_feasibility_established": True}
    inc = run.training_incumbent
    if inc is None:
        return None
    return {"x": list(inc["x"]), "source": "last_buildable_training_incumbent",
            "validation": None, "common_feasibility_established": False}


def rows(reply, fractions=(0.25, 0.75)):
    """Verify waveform statistics and keep raw fields in the caller's record."""
    if reply["fractions"] != list(fractions):
        raise ValueError("diagnostic returned different current fractions")
    for name in ("training", "validation"):
        m = reply[name]
        mesh_hash = m["reference_msh_sha256" if name == "training" else "msh_sha256"]
        if (not isinstance(mesh_hash, str) or len(mesh_hash) != 64
                or any(c not in "0123456789abcdef" for c in mesh_hash)):
            raise ValueError(f"{name}: missing or invalid mesh provenance")
        if m["fractions"] != list(fractions) or m["angles_deg"] != list(range(20)):
            raise ValueError(f"{name}: diagnostic must use the registered twenty angles")
        if any(len(m[k]) != len(fractions) for k in ("means", "stds", "torques")):
            raise ValueError(f"{name}: incomplete current waveforms")
        for ts, mean, std in zip(m["torques"], m["means"], m["stds"]):
            if len(ts) != 20 or not all(math.isfinite(t) for t in [*ts, mean, std]):
                raise ValueError(f"{name}: incomplete or nonfinite torque waveform")
            if not (math.isclose(statistics.mean(ts), mean, rel_tol=1e-10, abs_tol=1e-12)
                    and math.isclose(statistics.pstdev(ts), std, rel_tol=1e-10, abs_tol=1e-12)):
                raise ValueError(f"{name}: waveform does not reproduce mean/std")
    return [{"fraction": f, "mean_train": reply["training"]["means"][k],
             "mean_val": reply["validation"]["means"][k],
             "std_train": reply["training"]["stds"][k],
             "std_val": reply["validation"]["stds"][k],
             "mean_train_minus_val": reply["training"]["means"][k] - reply["validation"]["means"][k],
             "std_train_minus_val": reply["training"]["stds"][k] - reply["validation"]["stds"][k]}
            for k, f in enumerate(fractions)]
