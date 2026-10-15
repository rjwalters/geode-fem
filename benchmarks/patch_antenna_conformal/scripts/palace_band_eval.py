#!/usr/bin/env python3
"""Palace band evaluation of one moved conformal-patch mesh (issue #1130).

The external forward solver of `patch_conformal_nested_dof --palace-eval`:
geode writes the design's moved mesh (MSH 2.2, the fixture's physical tags),
calls

    palace_band_eval.py MESH OUTDIR F1,F2,...

and reads one `f re im` line of S11 per frequency (GHz) from stdout. This
script writes the Palace config for exactly those frequencies (the
`palace_patch_config.py` model of #1126 X1: FR-4, PEC patch and ground, the
50 ohm lumped port, the UPML shell as air with a first-order absorbing outer
wall), runs Palace in the vendored docker image, and converts
`port-S.csv` (|S11| in dB, arg in degrees) to complex S11.

There is no adaptive (PROM) sweep: every frequency is one Palace driven
solve on the mesh, the same unit geode's `--gradient-free` counts as one
solve. MESH and OUTDIR must lie under one directory, mounted as /work.

Environment:
  PALACE_NP      MPI ranks (default 16)
  PALACE_IMAGE   docker image (default palace:cpu)
  PALACE_ORDER   Nedelec order (default 1, geode's P1)
  PALACE_DOCKER  docker command (default "sudo docker")

Each call appends one line (wall time, Palace's own solve time, rank count)
to WORKDIR/evals.log and keeps Palace's log as OUTDIR/palace.log.
"""

from __future__ import annotations

import csv
import json
import math
import os
import re
import shlex
import subprocess
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from palace_patch_config import config as patch_config  # noqa: E402


class _Args:
    pass


def palace_config(mesh_c: str, postpro_c: str, freqs: list[float], order: int) -> dict:
    a = _Args()
    a.mesh = mesh_c
    a.postpro = postpro_c
    a.order = order
    a.levels = 0
    a.fmin = min(freqs)
    a.fmax = max(freqs)
    a.fstep = 1.0
    a.adaptive_tol = 0.0
    a.tol = 1e-8
    cfg = patch_config(a)
    # Exactly the requested points, each one direct driven solve.
    cfg["Solver"]["Driven"] = {
        "Samples": [{"Type": "Point", "Freq": freqs}],
        "SaveStep": 0,
    }
    return cfg


def read_s11(path: Path) -> list[tuple[float, complex]]:
    rows = []
    with path.open() as fh:
        reader = csv.reader(fh)
        header = [h.strip() for h in next(reader)]
        i_db = next(i for i, h in enumerate(header) if "S[1][1]" in h and "dB" in h)
        i_arg = next(i for i, h in enumerate(header) if "S[1][1]" in h and "arg" in h)
        for r in reader:
            if not r or not r[0].strip():
                continue
            f = float(r[0])
            mag = 10.0 ** (float(r[i_db]) / 20.0)
            ph = math.radians(float(r[i_arg]))
            rows.append((f, complex(mag * math.cos(ph), mag * math.sin(ph))))
    rows.sort(key=lambda t: t[0])
    return rows


def main() -> int:
    if len(sys.argv) != 4:
        print(__doc__, file=sys.stderr)
        return 2
    mesh = Path(sys.argv[1]).resolve()
    outdir = Path(sys.argv[2]).resolve()
    freqs = [float(v) for v in sys.argv[3].split(",")]
    work = mesh.parent
    try:
        out_rel = outdir.relative_to(work)
    except ValueError:
        print(f"OUTDIR {outdir} must lie under the mesh directory {work}", file=sys.stderr)
        return 2
    outdir.mkdir(parents=True, exist_ok=True)
    np_ = int(os.environ.get("PALACE_NP", "16"))
    image = os.environ.get("PALACE_IMAGE", "palace:cpu")
    order = int(os.environ.get("PALACE_ORDER", "1"))
    docker = shlex.split(os.environ.get("PALACE_DOCKER", "sudo docker"))
    cfg = palace_config(f"/work/{mesh.name}", f"/work/{out_rel}/postpro", freqs, order)
    (outdir / "config.json").write_text(json.dumps(cfg, indent=2) + "\n")
    cmd = docker + [
        "run", "--rm", "--ipc=host", "-v", f"{work}:/work", image,
        "-np", str(np_), f"/work/{out_rel}/config.json",
    ]
    t0 = time.monotonic()
    with (outdir / "palace.log").open("w") as log:
        rc = subprocess.run(cmd, stdout=log, stderr=subprocess.STDOUT).returncode
    wall = time.monotonic() - t0
    text = (outdir / "palace.log").read_text(errors="replace")
    if rc != 0:
        print(f"palace exited {rc}; see {outdir / 'palace.log'}", file=sys.stderr)
        print(text[-3000:], file=sys.stderr)
        return 1
    rows = read_s11(outdir / "postpro" / "port-S.csv")
    if len(rows) != len(freqs):
        print(f"port-S.csv has {len(rows)} rows for {len(freqs)} frequencies", file=sys.stderr)
        return 1
    m = re.findall(r"Elapsed time:?\s*([0-9.eE+-]+)\s*s", text)
    solve_s = m[-1] if m else "nan"
    with (work / "evals.log").open("a") as fh:
        fh.write(
            f"{out_rel} wall_s={wall:.2f} palace_elapsed_s={solve_s} np={np_} "
            f"order={order} n_freq={len(freqs)}\n"
        )
    for f, s in rows:
        print(f"{f:.9f} {s.real:.15e} {s.imag:.15e}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
