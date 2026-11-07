"""Parsing / formatting helpers shared by the summarize_*.py scripts (#1405).

Pure stdlib. The summarizers regenerate committed evidence (results_*.toml),
so behaviours that differed between the original per-script copies are explicit
parameters here, and each call site passes the value its old copy had.
"""
import math
import re
import tomllib

FRAGMENT_RE = r"# ---- gpu_driven_scaling TOML fragment.*?\n(.*?)# ---- end fragment ----"


def fragment(path):
    """The [[cell]] tables of one leg's stdout ([] if the leg did not finish)."""
    if not path.exists():
        return []
    m = re.search(FRAGMENT_RE, path.read_text(), re.S)
    return tomllib.loads(m.group(1)).get("cell", []) if m else []


def fragment_partial(path):
    """Like fragment(), but a leg killed mid-run (no end marker) is parsed to EOF.

    Only complete [[cell]] tables are kept (a truncated trailing table is dropped).
    Unlike fragment(), the file must exist.
    """
    m = re.search(
        r"# ---- gpu_driven_scaling TOML fragment.*?\n(.*?)(?:# ---- end fragment ----|\Z)",
        path.read_text(),
        re.S,
    )
    if not m:
        return []
    body = m.group(1)
    try:
        return tomllib.loads(body).get("cell", [])
    except tomllib.TOMLDecodeError:
        return tomllib.loads(body[: body.rfind("[[cell]]")]).get("cell", [])


def time_file(path, macos=True):
    """wall / user / sys seconds and max RSS bytes.

    GNU `time -v` always; macOS `time -l` too when macos=True.
    """
    out = {}
    if not path.exists():
        return out
    txt = path.read_text()
    if macos:
        m = re.search(r"([\d.]+) real\s+([\d.]+) user\s+([\d.]+) sys", txt)
        if m:  # RSS in bytes
            out["wall_s"], out["user_s"], out["sys_s"] = (float(g) for g in m.groups())
            out["max_rss_bytes"] = int(re.search(r"(\d+)\s+maximum resident set size", txt).group(1))
            return out
    m = re.search(r"Elapsed \(wall clock\) time.*: ([\d:.]+)", txt)
    if m:  # RSS in kbytes
        parts = [float(p) for p in m.group(1).split(":")]
        out["wall_s"] = sum(p * 60**i for i, p in enumerate(reversed(parts)))
        out["user_s"] = float(re.search(r"User time \(seconds\): ([\d.]+)", txt).group(1))
        out["sys_s"] = float(re.search(r"System time \(seconds\): ([\d.]+)", txt).group(1))
        out["max_rss_bytes"] = 1024 * int(
            re.search(r"Maximum resident set size \(kbytes\): (\d+)", txt).group(1)
        )
    return out


def keyvals(path, skip_comments=False):
    out = {}
    if path.exists():
        for line in path.read_text().splitlines():
            k, sep, v = line.partition("=")
            if sep and " " not in k and not (skip_comments and k.startswith("#")):
                out[k] = v
    return out


def q(s):
    return '"' + str(s).replace("\\", "\\\\").replace('"', '\\"') + '"'


def sci(x, digits=3):
    return "nan" if x != x else f"{x:.{digits}e}"


def fl(vals, digits=3):
    """A TOML array of fixed-point floats."""
    return "[" + ", ".join(f"{v:.{digits}f}" for v in vals) + "]"


def rel_diff(a, b):
    """|a - b| / |b| for [re, im] pairs."""
    return math.hypot(a[0] - b[0], a[1] - b[1]) / math.hypot(b[0], b[1])


def lsq_exponent(edges, its):
    """Least-squares slope of log(iterations) on log(edges)."""
    xs, ys = [math.log(e) for e in edges], [math.log(i) for i in its]
    mx, my = sum(xs) / len(xs), sum(ys) / len(ys)
    return sum((x - mx) * (y - my) for x, y in zip(xs, ys)) / sum((x - mx) ** 2 for x in xs)
