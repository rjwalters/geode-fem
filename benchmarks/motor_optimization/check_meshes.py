"""Check the rebuilt (gitignored) meshes of #1182 against committed SHA-256s.

The NURBS-multipatch family and the Opt120 export carry their hash in the
committed `*_report.json`; the construction meshes are checked against the
hashes recorded in this directory's MESHES.json (from the run of #1193).
Exit status 1 on any mismatch.
"""
import hashlib
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
GEO = ROOT / "benchmarks/motor_reference/geometry/meshes"
VAL = ROOT / "benchmarks/motor_validation/meshes"


def sha(p):
    return hashlib.sha256(p.read_bytes()).hexdigest()


bad = 0
for d, names in [(GEO, [f"icem_initial_sector_L{L}" for L in (0, 1, 2, 3)]),
                 (VAL, [f"icem_opt120_sector_L{L}" for L in (2, 3)])]:
    for n in names:
        want = json.loads((d / f"{n}_report.json").read_text())["msh_sha256"]
        got = sha(d / f"{n}.msh")
        ok = got == want
        bad += not ok
        print(f"{n}.msh {got} {'ok' if ok else 'MISMATCH ' + want}")
pinned = json.loads((Path(__file__).parent / "MESHES.json").read_text())["construction_sha256"]
for n, want in pinned.items():
    got = sha(VAL / n)
    ok = got == want
    bad += not ok
    print(f"{n} {got} {'ok' if ok else 'MISMATCH ' + want}")
sys.exit(1 if bad else 0)
