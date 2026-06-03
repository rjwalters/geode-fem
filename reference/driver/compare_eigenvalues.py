"""Cross-IR eigenvalue comparison for the cube-cavity reference (Epic #88 / #93).

Given a TF-Java eigenresult JSON (produced by `eigensolve_from_tfjava.py`)
and the in-tree JAX baseline JSON (`reference/fixtures/cube_cavity/jax_baseline.json`),
plus an optional NumPy result computed in-process from
`reference/numpy/cube_cavity_minimal.py`, this script writes a
cross-IR agreement table and asserts the TF-Java eigenvalues agree
with the JAX/NumPy baselines on the lowest 5 modes within a relative
tolerance (default 1e-5, per the Epic #88 framing comment on
cross-language f64 reproducibility).

When invoked from the TF-Java CI job (`tfjava-cube-cavity.yml`) the
`--burn` argument is not supplied — that gate is intentionally three-way
(TF-Java vs JAX vs NumPy). Burn agreement against the same JAX baseline
is exercised separately by the `arpack` workflow and by the per-push
`cube_cavity_jax_reference` cargo test. Passing `--burn` here is still
supported for ad-hoc/local audits where all four rows are convenient.

Exit code:
    0 — agreement within tolerance.
    1 — disagreement (or missing input).

This script is intentionally framework-light (only numpy + stdlib) so it
runs inside the TF-Java CI job without dragging JAX into the JVM-side
container. The Burn row is read from a fixture-shaped JSON if provided,
or omitted with a footnote pointing at where Burn agreement is checked.

Usage
=====
    python3 reference/driver/compare_eigenvalues.py \
        --tfjava path/to/eigenresult_tfjava.json \
        --jax    reference/fixtures/cube_cavity/jax_baseline.json \
        [--numpy path/to/numpy_baseline.json] \
        [--burn  path/to/burn_baseline.json] \
        [--rtol 1e-5] \
        [--out path/to/agreement_table.md]

Note: `--burn` is optional and is NOT wired in by `tfjava-cube-cavity.yml`.
The CI gate is three-way (TF-Java vs JAX vs NumPy); Burn agreement against
the same JAX baseline is exercised by the `arpack` workflow and the
`cube_cavity_jax_reference` cargo test.
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


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--tfjava", required=True, type=Path,
                        help="Path to the TF-Java eigenresult JSON (from eigensolve_from_tfjava.py).")
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
    args = parser.parse_args()

    if not args.tfjava.exists():
        print(f"TF-Java eigenresult not found: {args.tfjava}", file=sys.stderr)
        return 1
    if not args.jax.exists():
        print(f"JAX baseline not found: {args.jax}", file=sys.stderr)
        return 1

    tfjava = _load_eigenvalues(args.tfjava)
    jax_e = _load_eigenvalues(args.jax)
    numpy_e = _load_eigenvalues(args.numpy) if args.numpy and args.numpy.exists() else None
    burn_e = _load_eigenvalues(args.burn) if args.burn and args.burn.exists() else None

    k = min(args.k, tfjava.size, jax_e.size)
    tfjava_k = tfjava[:k]
    jax_k = jax_e[:k]

    # --- Compute agreement ---
    rel_tfjava_vs_jax = np.abs(tfjava_k - jax_k) / np.maximum(np.abs(jax_k), 1e-30)
    max_rel_tj_jax = float(np.max(rel_tfjava_vs_jax))

    rel_tfjava_vs_numpy = None
    max_rel_tj_np = None
    if numpy_e is not None:
        numpy_k = numpy_e[:k]
        rel_tfjava_vs_numpy = np.abs(tfjava_k - numpy_k) / np.maximum(np.abs(numpy_k), 1e-30)
        max_rel_tj_np = float(np.max(rel_tfjava_vs_numpy))

    rel_tfjava_vs_burn = None
    max_rel_tj_burn = None
    if burn_e is not None:
        burn_k = burn_e[:k]
        rel_tfjava_vs_burn = np.abs(tfjava_k - burn_k) / np.maximum(np.abs(burn_k), 1e-30)
        max_rel_tj_burn = float(np.max(rel_tfjava_vs_burn))

    # --- Render Markdown table ---
    # Title reflects the actual gate scope: cross-IR agreement among the
    # backends that were actually provided. When `--burn` is not supplied
    # (the default in the TF-Java CI workflow), the Burn row is omitted
    # entirely and replaced by an explicit footnote pointing at where
    # Burn agreement is checked. See issue #111.
    if burn_e is not None:
        title = "## Cube-cavity cross-IR eigenvalue agreement (TF-Java vs JAX vs NumPy vs Burn)"
    else:
        title = "## Cube-cavity cross-IR eigenvalue agreement (TF-Java vs JAX vs NumPy)"

    backend_rows = [
        _format_row("NumPy", numpy_e, k),
        _format_row("JAX",   jax_e, k),
        _format_row("TF-Java", tfjava, k),
    ]
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
        "### Relative drift vs JAX (XLA-vs-XLA)",
        "",
        f"- max |TF-Java − JAX| / |JAX| over {k} modes: **{max_rel_tj_jax:.3e}**",
    ]
    if max_rel_tj_np is not None:
        lines.append(f"- max |TF-Java − NumPy| / |NumPy| over {k} modes: **{max_rel_tj_np:.3e}**")
    if max_rel_tj_burn is not None:
        lines.append(f"- max |TF-Java − Burn|  / |Burn|  over {k} modes: **{max_rel_tj_burn:.3e}**")
    else:
        lines.extend([
            "",
            "> **Note:** the Burn row is not included in this gate. Burn agreement",
            "> against the same JAX baseline is exercised by `cargo test --features arpack`",
            "> (see `.github/workflows/arpack.yml`) and by the default-CI",
            "> `cube_cavity_jax_reference` cargo test. See issue #111 for the",
            "> decoupling rationale.",
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
    if max_rel_tj_jax > args.rtol:
        failures.append(f"TF-Java vs JAX: {max_rel_tj_jax:.3e} > {args.rtol:g}")
    if max_rel_tj_np is not None and max_rel_tj_np > args.rtol:
        failures.append(f"TF-Java vs NumPy: {max_rel_tj_np:.3e} > {args.rtol:g}")
    if max_rel_tj_burn is not None and max_rel_tj_burn > args.rtol:
        failures.append(f"TF-Java vs Burn: {max_rel_tj_burn:.3e} > {args.rtol:g}")

    if failures:
        print("AGREEMENT FAILURE (cross-XLA-vs-XLA drift is highly informative — see #88):",
              file=sys.stderr)
        for f in failures:
            print(f"  - {f}", file=sys.stderr)
        return 1

    print(f"OK: TF-Java agrees with all available baselines to within {args.rtol:g} relative.",
          file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
