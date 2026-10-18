"""Amendment-8 main queue, used by session.py (#1202).

The historical pilot order stays in driver.py. This queue records intended
dispatch order; a concurrent launcher must separately record actual starts.
"""

LETTERS = tuple("ABCDE")
STARTS = ("theta0", "lhs1", "lhs2", "lhs3", "lhs4")
SPACES = ("a", "b")


def method_order(start_index, repeat):
    """Rotate ABCDE by the start index; reverse that rotation for repeat 2."""
    if type(start_index) is not int or not 0 <= start_index < len(STARTS):
        raise ValueError("start index must be an integer in 0..4")
    if type(repeat) is not int or repeat not in (1, 2):
        raise ValueError("repeat must be 1 or 2")
    order = LETTERS[start_index:] + LETTERS[:start_index]
    return order if repeat == 1 else order[::-1]


def build_jobs(method_keys):
    """Build the 100 registered logical jobs from letter -> harness method.

    The caller supplies the merged method registry. Refuse missing, extra or
    aliased methods rather than silently scheduling a partial experiment.
    No processes are launched and no output paths are touched here.
    """
    if set(method_keys) != set(LETTERS):
        raise ValueError("the main study requires exactly methods A through E")
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
                                 "order_index": order_index,
                                 "name": f"{method}_{space}_{start}_r{repeat}"})
    return jobs
