"""Small optimizer-domain diagnostic; no motor, FEM, timing or speedup claim.

Run with the pilot's locked Python environment, using -I. The analytic toy
objective is defined outside the constraint so requests can be counted without
inventing an objective. The separate NaN experiment tests a rejection signal,
not a motor value. Output includes every requested point.
"""
import hashlib
import json
from pathlib import Path

import numpy as np
import scipy
from scipy.optimize import NonlinearConstraint, minimize


def probe(method, reject=False):
    calls = []

    def objective(x):
        margin = float(1.0 - x @ x)
        rejected = reject and margin < 0.0
        calls.append({"x": x.tolist(), "margin": margin, "rejected": rejected})
        return np.nan if rejected else -sum(x)

    kwargs = {"jac": lambda x: -np.ones(2)} if method == "SLSQP" else {}
    options = {"maxiter": 300, "initial_tr_radius": 0.1} if reject else {"maxiter": 30}
    result = minimize(
        objective, [0.9, 0.0], method=method, bounds=[(-1.0, 1.0)] * 2,
        constraints=[NonlinearConstraint(lambda x: 1.0 - x @ x, 0.0, np.inf,
                                         jac=lambda x: -2.0 * x)],
        options=options, **kwargs,
    )
    return {
        "method": method, "nan_rejection": reject, "calls": calls,
        "outside_nonlinear_domain": sum(c["margin"] < -1e-8 for c in calls),
        "success": bool(result.success), "message": str(result.message),
        "final_x": result.x.tolist(), "final_objective": float(result.fun),
        "final_margin": float(1.0 - result.x @ result.x),
        "known_optimum": -float(np.sqrt(2.0)),
    }


if __name__ == "__main__":
    assert scipy.__version__ == "1.18.1", "Use the pilot's pinned SciPy version."
    results = [probe("SLSQP"), probe("COBYQA"), probe("COBYQA", reject=True)]
    assert all(r["outside_nonlinear_domain"] > 0 for r in results[:2])
    print(json.dumps({
        "purpose": "Domain-handling contract probe; no physical or timing inference",
        "scipy": scipy.__version__, "numpy": np.__version__,
        "source_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        "results": results,
    }, indent=2, allow_nan=False))
