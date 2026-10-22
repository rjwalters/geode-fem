"""The {A', F_w} queue of #1224, used by `session.py --fd-warm-ablation`.

A post-hoc arm, outside the preregistered #1185 study and added after the
#1185 and #1218 outcomes were known: method F_w (`slsqp_fd_warm`,
forward-difference gradients with warm-started stencil solves) against a
within-session rerun A' of method A (`slsqp`, adjoint gradients,
unchanged). The cohort is #1224 protocol §4: the frozen starts theta0 and
lhs1-lhs4, spaces a and b, **repeat 1 only**, 20 runs. Within each start and
space the order of {A', F_w} is rotated by the start index as in
`fd_ablation_schedule.method_order` on repeat 1 (AW for an even start
index, WA for an odd one).

The 100-run main queue (`main_schedule`) and the #1218 40-run queue
(`fd_ablation_schedule`) are not touched. Like them, this records the
intended dispatch order only; the launcher records the actual starts.
"""
from main_schedule import SPACES, STARTS

#: The arms in rotation order. A' is method A itself (letter A); only the
#: arm label tells the rerun apart.
LETTERS = ("A", "W")
ARMS = {"A": "A_prime", "W": "F_w"}
#: Repeat 1 only (#1224 protocol §4).
REPEATS = (1,)


def method_order(start_index):
    """Rotate AW by the start index (repeat 1 of `fd_ablation_schedule`)."""
    if type(start_index) is not int or not 0 <= start_index < len(STARTS):
        raise ValueError("start index must be an integer in 0..4")
    k = start_index % len(LETTERS)
    return LETTERS[k:] + LETTERS[:k]


def registered_jobs(method_specs):
    """The 20 jobs from the two method specs of the arm (A and W).

    Refuse anything but exactly A's adjoint SLSQP and F_w's forward-
    difference SLSQP with base-started stencils, both on the session's
    primary training model, rather than silently scheduling another
    comparison."""
    if len(method_specs) != len(LETTERS):
        raise ValueError("the F_w arm requires exactly the A and W method specs")
    keys = {spec["letter"]: method for method, spec in method_specs.items()}
    if set(keys) != set(LETTERS):
        raise ValueError("the F_w arm requires exactly methods A and W")
    a, w = method_specs[keys["A"]], method_specs[keys["W"]]
    if (a["optimizer"] != "slsqp" or w["optimizer"] != "slsqp"
            or a["resolution"] is not None or w["resolution"] is not None
            or a.get("gradient", "adjoint") != "adjoint" or "fd_stencil_start" in a
            or w.get("gradient") != "forward_difference"
            or w.get("fd_stencil_start") != "base"):
        raise ValueError("A' must be the adjoint SLSQP and F_w the forward-difference SLSQP "
                         "with base-started stencils, both on the primary training model")
    return build_jobs(keys)


def build_jobs(method_keys):
    """Build the 20 logical jobs from letter -> harness method."""
    if set(method_keys) != set(LETTERS):
        raise ValueError("the F_w arm requires exactly methods A and W")
    values = tuple(method_keys[k] for k in LETTERS)
    if (any(type(v) is not str or not v or not all(c.isalnum() or c == "_" for c in v)
            for v in values) or len(set(values)) != len(values)):
        raise ValueError("method keys must be distinct nonempty identifiers")
    jobs = []
    for repeat in REPEATS:
        for space in SPACES:
            for start_index, start in enumerate(STARTS):
                for order_index, letter in enumerate(method_order(start_index)):
                    method = method_keys[letter]
                    jobs.append({"position": len(jobs), "repeat": repeat, "space": space,
                                 "start": start, "start_index": start_index,
                                 "method": method, "method_letter": letter,
                                 "arm": ARMS[letter], "order_index": order_index,
                                 "name": f"{method}_{space}_{start}_r{repeat}"})
    return jobs
