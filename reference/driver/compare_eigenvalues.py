"""Cross-IR eigenvalue comparison for the cube-cavity reference (Epic #88 / #93 / #115).

Given a primary eigenresult JSON (TF-Java via `eigensolve_from_tfjava.py`,
or Julia via `reference/julia/cube_cavity.jl --out ...`, or both) and
the in-tree JAX baseline JSON (`reference/fixtures/cube_cavity/jax_baseline.json`),
plus an optional NumPy result computed in-process from
`reference/numpy/cube_cavity_minimal.py`, this script writes a
cross-IR agreement table and asserts the primary eigenvalues agree
with the JAX/NumPy/Julia baselines on the lowest 5 modes within a
relative tolerance (default 1e-5, per the Epic #88 framing comment on
cross-language f64 reproducibility).

The CI workflows wire this script with different primary backends:

- `.github/workflows/tfjava-cube-cavity.yml` (PR #107 / #112): TF-Java
  primary, compared against JAX baseline + NumPy n=4 row.
- `.github/workflows/julia-cube-cavity.yml` (issue #115): Julia primary,
  compared against the NumPy n=10 canonical baseline (`baseline.json`)
  + a freshly emitted NumPy n=10 row. The JAX row is omitted from the
  Julia gate when meshes don't match (jax_baseline.json is n=4); see
  the workflow header for the rationale.
- `.github/workflows/onnx-cube-cavity.yml` (issue #123): ONNX primary,
  compared against the NumPy n=10 canonical baseline + a freshly
  emitted NumPy n=10 row. The JAX row is shown for diagnostic context
  but omitted from the rtol gate via `--skip-jax-comparison`, same
  pattern as the Julia gate (jax_baseline.json is pinned at n=4).

Optional `--burn` is supported for ad-hoc/local audits where all four
rows are convenient.

Exit code:
    0 — agreement within tolerance.
    1 — disagreement (or missing input).

This script is intentionally framework-light (only numpy + stdlib) so it
runs inside the TF-Java / Julia CI jobs without dragging JAX into the
JVM-side or Julia-side container.

Usage
=====
    python3 reference/driver/compare_eigenvalues.py \
        [--tfjava path/to/eigenresult_tfjava.json] \
        [--julia  path/to/julia_baseline.json] \
        [--onnx   path/to/eigenresult_onnx.json] \
        --jax    reference/fixtures/cube_cavity/jax_baseline.json \
        [--numpy path/to/numpy_baseline.json] \
        [--burn  path/to/burn_baseline.json] \
        [--rtol 1e-5] \
        [--out path/to/agreement_table.md]

At least one of `--tfjava`, `--julia`, or `--onnx` must be supplied.
The `--jax` flag is required so the comparator always emits the
cross-IR XLA-vs-ARPACK columns (even when one row is omitted due to
mesh mismatch, the header documents the omission explicitly).
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Optional

import numpy as np


def _load_eigenvalues(path: Path) -> np.ndarray:
    """Pull the `outputs.eigenvalues.data` array out of a fixture-schema JSON."""
    with open(path) as f:
        fixture = json.load(f)
    field = fixture["outputs"]["eigenvalues"]
    data = field["data"]
    shape = field["shape"]
    arr = np.asarray(data, dtype=np.float64).ravel()
    expected = int(np.prod(shape))
    if arr.size != expected:
        raise ValueError(
            f"{path}: eigenvalues data length {arr.size} != shape {shape} (= {expected})"
        )
    return arr.reshape(shape)


def _format_row(name: str, eigs: Optional[np.ndarray], k: int) -> str:
    if eigs is None:
        return f"| {name:<8} | " + " | ".join(["—"] * k) + " |"
    return f"| {name:<8} | " + " | ".join(f"{e:.9e}" for e in eigs[:k]) + " |"


def _max_rel(a: np.ndarray, b: np.ndarray) -> float:
    """Max |a - b| / max(|b|, 1e-30) over the full vector."""
    return float(np.max(np.abs(a - b) / np.maximum(np.abs(b), 1e-30)))


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--tfjava", type=Path, default=None,
                        help="Path to the TF-Java eigenresult JSON (from eigensolve_from_tfjava.py).")
    parser.add_argument("--julia", type=Path, default=None,
                        help="Path to the Julia eigenresult JSON (from reference/julia/cube_cavity.jl).")
    parser.add_argument("--onnx", type=Path, default=None,
                        help="Path to the ONNX eigenresult JSON (from eigensolve_from_onnx.py).")
    parser.add_argument("--jax", required=True, type=Path,
                        help="Path to the JAX baseline JSON.")
    parser.add_argument("--numpy", type=Path, default=None,
                        help="Optional NumPy baseline JSON (fixture-schema).")
    parser.add_argument("--burn", type=Path, default=None,
                        help="Optional Burn baseline JSON (fixture-schema).")
    parser.add_argument("--rtol", type=float, default=1e-5,
                        help="Relative tolerance for cross-backend agreement.")
    parser.add_argument("--k", type=int, default=5,
                        help="Number of lowest eigenmodes to compare.")
    parser.add_argument("--out", type=Path, default=None,
                        help="Optional Markdown agreement-table output path.")
    parser.add_argument("--skip-jax-comparison", action="store_true",
                        help=(
                            "Render the JAX row in the table but DO NOT include it "
                            "in the rtol gate. Used by the Julia gate (issue #115) "
                            "when the Julia pipeline runs at a different `n` than "
                            "the JAX baseline (jax_baseline.json is pinned at n=4)."
                        ))
    args = parser.parse_args()

    if args.tfjava is None and args.julia is None and args.onnx is None:
        print("ERROR: at least one of --tfjava, --julia, or --onnx must be supplied.",
              file=sys.stderr)
        return 1

    if not args.jax.exists():
        print(f"JAX baseline not found: {args.jax}", file=sys.stderr)
        return 1

    tfjava = None
    if args.tfjava is not None:
        if not args.tfjava.exists():
            print(f"TF-Java eigenresult not found: {args.tfjava}", file=sys.stderr)
            return 1
        tfjava = _load_eigenvalues(args.tfjava)

    julia = None
    if args.julia is not None:
        if not args.julia.exists():
            print(f"Julia eigenresult not found: {args.julia}", file=sys.stderr)
            return 1
        julia = _load_eigenvalues(args.julia)

    onnx_e = None
    if args.onnx is not None:
        if not args.onnx.exists():
            print(f"ONNX eigenresult not found: {args.onnx}", file=sys.stderr)
            return 1
        onnx_e = _load_eigenvalues(args.onnx)

    jax_e = _load_eigenvalues(args.jax)
    numpy_e = _load_eigenvalues(args.numpy) if args.numpy and args.numpy.exists() else None
    burn_e = _load_eigenvalues(args.burn) if args.burn and args.burn.exists() else None

    # Truncate to k across whatever rows are present.
    present = [v.size for v in (tfjava, julia, onnx_e, jax_e) if v is not None]
    k = min(args.k, *present)
    jax_k = jax_e[:k]
    tfjava_k = tfjava[:k] if tfjava is not None else None
    julia_k = julia[:k] if julia is not None else None
    onnx_k = onnx_e[:k] if onnx_e is not None else None
    numpy_k = numpy_e[:k] if numpy_e is not None else None
    burn_k = burn_e[:k] if burn_e is not None else None

    # --- Compute per-primary drift against every other backend ---
    drift = {}  # (primary_label, other_label) -> max_rel

    def _record(primary_label: str, primary: np.ndarray):
        drift[(primary_label, "JAX")] = _max_rel(primary, jax_k)
        if numpy_k is not None:
            drift[(primary_label, "NumPy")] = _max_rel(primary, numpy_k)
        if burn_k is not None:
            drift[(primary_label, "Burn")] = _max_rel(primary, burn_k)
        if julia_k is not None and primary_label != "Julia":
            drift[(primary_label, "Julia")] = _max_rel(primary, julia_k)
        if tfjava_k is not None and primary_label != "TF-Java":
            drift[(primary_label, "TF-Java")] = _max_rel(primary, tfjava_k)
        if onnx_k is not None and primary_label != "ONNX":
            drift[(primary_label, "ONNX")] = _max_rel(primary, onnx_k)

    if tfjava_k is not None:
        _record("TF-Java", tfjava_k)
    if julia_k is not None:
        _record("Julia", julia_k)
    if onnx_k is not None:
        _record("ONNX", onnx_k)

    # --- Render Markdown table ---
    primary_labels = []
    if tfjava_k is not None:
        primary_labels.append("TF-Java")
    if julia_k is not None:
        primary_labels.append("Julia")
    if onnx_k is not None:
        primary_labels.append("ONNX")

    other_labels = ["JAX"]
    if numpy_k is not None:
        other_labels.append("NumPy")
    if burn_k is not None:
        other_labels.append("Burn")

    title = "## Cube-cavity cross-IR eigenvalue agreement (" + \
        " vs ".join(primary_labels + other_labels) + ")"

    backend_rows = []
    if numpy_e is not None:
        backend_rows.append(_format_row("NumPy", numpy_e, k))
    backend_rows.append(_format_row("JAX", jax_e, k))
    if tfjava is not None:
        backend_rows.append(_format_row("TF-Java", tfjava, k))
    if julia is not None:
        backend_rows.append(_format_row("Julia", julia, k))
    if onnx_e is not None:
        backend_rows.append(_format_row("ONNX", onnx_e, k))
    if burn_e is not None:
        backend_rows.append(_format_row("Burn", burn_e, k))

    lines = [
        title,
        "",
        f"Lowest {k} modes; rtol gate = {args.rtol:g}",
        "",
        "| Backend  | " + " | ".join(f"λ[{i}]" for i in range(k)) + " |",
        "|----------|" + "|".join(["----------------"] * k) + "|",
        *backend_rows,
        "",
        "### Pairwise relative drift",
        "",
    ]
    for primary_label in primary_labels:
        for other in [lab for lab in ("JAX", "NumPy", "Burn", "Julia", "TF-Java", "ONNX")
                      if lab != primary_label and (primary_label, lab) in drift]:
            tag = ""
            if other == "JAX" and args.skip_jax_comparison:
                tag = " *(diagnostic only — excluded from gate; see header)*"
            lines.append(
                f"- max |{primary_label} − {other}| / |{other}| over {k} modes: "
                f"**{drift[(primary_label, other)]:.3e}**{tag}"
            )
    if burn_e is None:
        lines.extend([
            "",
            "> **Note:** the Burn row is not included in this gate. Burn agreement",
            "> against the same JAX baseline is exercised by `cargo test --features arpack`",
            "> (see `.github/workflows/arpack.yml`) and by the default-CI",
            "> `cube_cavity_jax_reference` / `cube_cavity_julia_reference` cargo tests.",
            "> See issue #111 for the decoupling rationale.",
        ])
    if args.skip_jax_comparison:
        lines.extend([
            "",
            "> **Note:** the JAX row is shown for diagnostic context but is",
            "> excluded from the rtol gate because `jax_baseline.json` is pinned",
            "> at n=4 while this gate runs at a different mesh resolution.",
            "> Julia agreement against JAX at matching meshes is verifiable by",
            "> rerunning Julia at `--n 4` locally.",
        ])

    table_md = "\n".join(lines) + "\n"
    print(table_md)
    if args.out is not None:
        args.out.parent.mkdir(parents=True, exist_ok=True)
        with open(args.out, "w") as f:
            f.write(table_md)
        print(f"Wrote {args.out}", file=sys.stderr)

    # --- Gate decision ---
    failures = []
    for (primary_label, other_label), max_rel in drift.items():
        if other_label == "JAX" and args.skip_jax_comparison:
            continue
        if max_rel > args.rtol:
            failures.append(
                f"{primary_label} vs {other_label}: {max_rel:.3e} > {args.rtol:g}"
            )

    if failures:
        print("AGREEMENT FAILURE (cross-IR drift is highly informative — see #88):",
              file=sys.stderr)
        for f in failures:
            print(f"  - {f}", file=sys.stderr)
        return 1

    print(
        f"OK: {', '.join(primary_labels)} agree with all gated baselines to within "
        f"{args.rtol:g} relative.",
        file=sys.stderr,
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
