"""The {A', F} ablation queue of #1218, used by `session.py --fd-ablation`.

A post-hoc ablation, outside the preregistered #1185 study: method F
(`slsqp_fd`, forward-difference gradients) against a within-session rerun
A' of method A (`slsqp`, adjoint gradients, unchanged). The cohort is the
frozen #1185 one (protocol §3): starts theta0 and lhs1-lhs4, spaces a and b,
repeats 1 and 2, 40 runs. Within each start and space the order of {A', F}
is rotated by the start index as in `main_schedule.method_order` (AF for an
even start index, FA for an odd one) and reversed on repeat 2.

The 100-run main queue (`main_schedule`) is not touched. Like it, this
records the intended dispatch order only; the launcher records the actual
starts.
"""
from main_schedule import SPACES, STARTS

#: The arms in rotation order. A' is method A itself (letter A); only the
#: arm label tells the rerun apart.
LETTERS = ("A", "F")
ARMS = {"A": "A_prime", "F": "F"}


def method_order(start_index, repeat):
    """Rotate AF by the start index; reverse that rotation for repeat 2."""
    if type(start_index) is not int or not 0 <= start_index < len(STARTS):
        raise ValueError("start index must be an integer in 0..4")
    if type(repeat) is not int or repeat not in (1, 2):
        raise ValueError("repeat must be 1 or 2")
    k = start_index % len(LETTERS)
    order = LETTERS[k:] + LETTERS[:k]
    return order if repeat == 1 else order[::-1]


def registered_jobs(method_specs):
    """The 40 jobs from the two method specs of the ablation (A and F).

    Refuse anything but exactly A's adjoint SLSQP and F's forward-difference
    SLSQP, both on the session's primary training model, rather than
    silently scheduling another comparison."""
    if len(method_specs) != len(LETTERS):
        raise ValueError("the ablation requires exactly the A and F method specs")
    keys = {spec["letter"]: method for method, spec in method_specs.items()}
    if set(keys) != set(LETTERS):
        raise ValueError("the ablation requires exactly methods A and F")
    a, f = method_specs[keys["A"]], method_specs[keys["F"]]
    if (a["optimizer"] != "slsqp" or f["optimizer"] != "slsqp"
            or a["resolution"] is not None or f["resolution"] is not None
            or a.get("gradient", "adjoint") != "adjoint"
            or f.get("gradient") != "forward_difference"):
        raise ValueError("A' must be the adjoint SLSQP and F the forward-difference SLSQP, "
                         "both on the primary training model")
    return build_jobs(keys)


def build_jobs(method_keys):
    """Build the 40 logical jobs from letter -> harness method."""
    if set(method_keys) != set(LETTERS):
        raise ValueError("the ablation requires exactly methods A and F")
    values = tuple(method_keys[k] for k in LETTERS)
    if (any(type(v) is not str or not v or not all(c.isalnum() or c == "_" for c in v)
            for v in values) or len(set(values)) != len(values)):
        raise ValueError("method keys must be distinct nonempty identifiers")
    jobs = []
    for repeat in (1, 2):
        for space in SPACES:
            for start_index, start in enumerate(STARTS):
                for order_index, letter in enumerate(method_order(start_index, repeat)):
                    method = method_keys[letter]
                    jobs.append({"position": len(jobs), "repeat": repeat, "space": space,
                                 "start": start, "start_index": start_index,
                                 "method": method, "method_letter": letter,
                                 "arm": ARMS[letter], "order_index": order_index,
                                 "name": f"{method}_{space}_{start}_r{repeat}"})
    return jobs
