"""Compare two study JSON records value by value, skipping wall times (issue #1183)."""
import json
import re
import sys

SKIP = re.compile(r"(seconds|_s$|wall|time|elapsed|threads|host|date|git)", re.I)


def walk(a, b, path, out):
    if isinstance(a, dict) and isinstance(b, dict):
        for k in sorted(set(a) | set(b)):
            if SKIP.search(k):
                out["skipped"].add(k)
                continue
            if k not in a or k not in b:
                out["missing"].append(f"{path}/{k}")
                continue
            walk(a[k], b[k], f"{path}/{k}", out)
    elif isinstance(a, list) and isinstance(b, list):
        if len(a) != len(b):
            out["missing"].append(f"{path} len {len(a)} vs {len(b)}")
            return
        for i, (x, y) in enumerate(zip(a, b)):
            walk(x, y, f"{path}[{i}]", out)
    else:
        out["n"] += 1
        if a != b:
            out["diff"].append((path, a, b))


for old, new in zip(sys.argv[1::2], sys.argv[2::2]):
    out = {"n": 0, "diff": [], "missing": [], "skipped": set()}
    walk(json.load(open(old)), json.load(open(new)), "", out)
    print(f"{old} vs {new}")
    print(f"  compared {out['n']} values; {len(out['diff'])} differ; "
          f"{len(out['missing'])} structural mismatches; skipped keys {sorted(out['skipped'])}")
    for p, x, y in out["diff"][:20]:
        print(f"  DIFF {p}: {x!r} vs {y!r}")
    for m in out["missing"][:20]:
        print(f"  MISSING {m}")
