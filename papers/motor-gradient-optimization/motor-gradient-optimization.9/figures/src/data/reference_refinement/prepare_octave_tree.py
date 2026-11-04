#!/usr/bin/env python3
"""Prepare an Octave-runnable copy of the pinned ICEM snapshot (issue #1189).

The upstream snapshot (GPL-3.0-or-later, MichaelWiesheu/ICEMsnapshot at
b145aaf4) is MATLAB code. Octave rejects two MATLAB idioms it uses:

* ``x == "string"`` between char arrays of different length (MATLAB string
  scalars compare as wholes; Octave compares char arrays element-wise), and
* ``fnder`` from the Curve Fitting Toolbox (provided here by ``shims/fnder.m``,
  written for this repository, which calls Octave's ``ppder``).

This script never edits the pristine clone. It verifies the clone's commit
and the manifest hashes in ``../sources.json``, copies the needed
directories into a separate output tree, and applies an explicit list of
textual substitutions, each of which must match an exact expected number of
times. No upstream code is written into the GEODE repository.

Usage (run with ``python3 -I``):

    prepare_octave_tree.py --src /Volumes/Stripe/scratch-icem-ref/icem \
        --out /Volumes/Stripe/scratch-icem-ref/icem-octave
"""

import argparse
import hashlib
import json
import shutil
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
COMMIT = "b145aaf4ab1a6424828bc941eaf172f48fa0d458"
COPY_DIRS = ["classes", "operators", "material", "geometries"]
COPY_FILES = [
    "results/plots/final/RotorOpt0.txt",
    "results/plots/final/StatorOpt0.txt",
    "LICENSE",
]

# (relative path, old text, new text, expected count)
SUBSTITUTIONS = [
    (
        "classes/MotorSimulation.m",
        'obj.Materials(iMat).Material.getType() == "nonlinear"',
        'strcmp(obj.Materials(iMat).Material.getType(), "nonlinear")',
        5,
    ),
    (
        "material/Mat_Magnetic.m",
        'any(type == ["linear", "nonlinear"])',
        'any(strcmp(type, {"linear", "nonlinear"}))',
        1,
    ),
    (
        "material/Mat_Magnetic.m",
        'obj.getType() == "linear"',
        'strcmp(obj.getType(), "linear")',
        2,
    ),
    ("geometries/PSMS/PMSM_rotor1.m", 'Type == "linear"', 'strcmp(Type, "linear")', 1),
    ("geometries/PSMS/PMSM_rotor1.m", 'Type == "nonlinear"', 'strcmp(Type, "nonlinear")', 1),
]


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--src", required=True, type=Path)
    ap.add_argument("--out", required=True, type=Path)
    args = ap.parse_args()
    src, out = args.src.resolve(), args.out.resolve()

    head = subprocess.run(
        ["git", "-C", str(src), "rev-parse", "HEAD"], capture_output=True, text=True, check=True
    ).stdout.strip()
    if head != COMMIT:
        sys.exit(f"clone is at {head}, expected {COMMIT}")
    dirty = subprocess.run(
        ["git", "-C", str(src), "status", "--porcelain"], capture_output=True, text=True, check=True
    ).stdout.strip()
    if dirty:
        sys.exit("pristine clone has local modifications")

    manifest = json.loads((HERE.parent / "sources.json").read_text())
    for entry in manifest["files"]:
        got = sha256(src / entry["path"])
        if got != entry["sha256"]:
            sys.exit(f"hash mismatch: {entry['path']}")
    print(f"verified commit {COMMIT} and {len(manifest['files'])} manifest hashes")

    if out.exists():
        shutil.rmtree(out)
    out.mkdir(parents=True)
    for d in COPY_DIRS:
        shutil.copytree(src / d, out / d)
    for f in COPY_FILES:
        (out / f).parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(src / f, out / f)

    record = {"source_commit": COMMIT, "substitutions": []}
    for rel, old, new, expected in SUBSTITUTIONS:
        p = out / rel
        text = p.read_text()
        n = text.count(old)
        if n != expected:
            sys.exit(f"{rel}: expected {expected} matches of {old!r}, found {n}")
        before = sha256(p)
        p.write_text(text.replace(old, new))
        record["substitutions"].append(
            {"file": rel, "old": old, "new": new, "count": n, "sha256_before": before}
        )
        print(f"{rel}: {n} x {old!r} -> {new!r}")

    for rel in sorted({s[0] for s in SUBSTITUTIONS}):
        record.setdefault("patched_sha256", {})[rel] = sha256(out / rel)
    (out / "PATCH_RECORD.json").write_text(json.dumps(record, indent=1) + "\n")
    print(json.dumps(record["patched_sha256"], indent=1))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
