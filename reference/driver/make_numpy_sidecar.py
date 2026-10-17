"""Synthesize a full-matrix assembly sidecar from the NumPy references.

Smoke-test companion to ``eigensolve_from_sidecar.py`` (issue #186).

In CI the full sidecars are produced by the live TF-Java / ONNX assembly
runs, so the full-matrix eigensolve path of the driver is only ever
exercised against freshly generated artifacts. The checked-in
``reference/fixtures/sphere_pec/*_sidecar.json`` fixtures are deliberately
SUMMARY sidecars (scalar invariants only — embedding the 3300x3300 dense
matrices would cost ~90 MB of JSON each), and the other problem families
check in no sidecar at all.

This script closes the local-smoke gap: it runs the in-tree NumPy
reference assembly for any of the four problem families and emits a
sidecar with the same ``outputs`` schema the JVM / ONNX dumps produce,
so the driver's full-matrix eigensolve path can be smoke-tested without
a Java or ONNX toolchain (the pattern the issue-174 recovery used to
validate the sphere-mie mode).

Usage
=====
    # Cube cavity (scalar Helmholtz, P1):
    python3 reference/driver/make_numpy_sidecar.py \\
        --problem cube-cavity --n 4 --out /tmp/reduced_kM.json

    # Sphere PEC (full 774-node mesh; ~430 MB JSON, CI-sized):
    python3 reference/driver/make_numpy_sidecar.py \\
        --problem sphere-pec --out /tmp/reduced_kM_sphere_pec.json

    # Sphere PML (small 48-node mesh by default):
    python3 reference/driver/make_numpy_sidecar.py \\
        --problem sphere-pml --out /tmp/reduced_kM_sphere_pml.json

    # Sphere Mie (small 48-node mesh, tensor-epsilon UPML):
    python3 reference/driver/make_numpy_sidecar.py \\
        --problem sphere-mie --out /tmp/reduced_kM_sphere_mie.json

See ``reference/driver/README.md`` for the per-mode smoke invocations
that consume these sidecars.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

import numpy as np

_HERE = Path(__file__).resolve().parent
_REPO_ROOT = _HERE.parent.parent
# Repo root on sys.path: `reference.*` resolves as PEP 420 namespace
# packages regardless of cwd (issue #187).
_REPO_ROOT_STR = str(Path(__file__).resolve().parents[2])
if _REPO_ROOT_STR not in sys.path:
    sys.path.insert(0, _REPO_ROOT_STR)

PROBLEMS = ("cube-cavity", "sphere-pec", "sphere-pml", "sphere-mie")

# Imaginary parts of the stiffness matrix above this bound indicate the
# assembly no longer matches the (real K, complex M) sidecar schema.
_K_IMAG_ABS_MAX = 1e-12


def _f64_field(arr: np.ndarray, description: str) -> dict:
    arr = np.ascontiguousarray(arr, dtype=np.float64)
    return {
        "shape": list(arr.shape),
        "dtype": "f64",
        "description": description,
        "data": arr.ravel().tolist(),
    }


def _scalar_input(value: float, dtype: str, description: str) -> dict:
    data = [int(value)] if dtype == "i64" else [float(value)]
    return {"shape": [1], "dtype": dtype, "description": description, "data": data}


def _count_outputs(n_nodes: int, n_tets: int, n_edges: int, n_int: int) -> dict:
    return {
        "n_nodes": _f64_field(np.array([float(n_nodes)]), "Number of mesh nodes."),
        "n_tets": _f64_field(np.array([float(n_tets)]), "Number of tetrahedra."),
        "n_edges": _f64_field(np.array([float(n_edges)]), "Total global edge count."),
        "n_interior_edges": _f64_field(
            np.array([float(n_int)]), "Interior edge count after PEC elimination."
        ),
    }


def _real_part_checked(mat: np.ndarray, name: str) -> np.ndarray:
    """Take Re(mat), asserting Im(mat) is at f64 roundoff (schema guard)."""
    if np.iscomplexobj(mat):
        im_max = float(np.max(np.abs(mat.imag))) if mat.size else 0.0
        if im_max > _K_IMAG_ABS_MAX:
            raise ValueError(
                f"{name} has non-negligible imaginary part (max |Im| = "
                f"{im_max:.3e}); the sidecar schema expects a real-valued "
                "stiffness."
            )
        return np.ascontiguousarray(mat.real)
    return mat


# ---------------------------------------------------------------------------
# Per-problem synthesis
# ---------------------------------------------------------------------------


def _make_cube_cavity(args) -> dict:
    from reference.numpy.cube_cavity_minimal import assemble_global_p1, restrict_to_interior
    from reference.numpy.mesh import cube_interior_mask, cube_tet_mesh

    nodes, tets = cube_tet_mesh(args.n, args.side)
    k_csr, m_csr = assemble_global_p1(nodes, tets)
    mask = cube_interior_mask(nodes, args.side)
    k_int, m_int = restrict_to_interior(k_csr, m_csr, mask)
    k_int = k_int.toarray()
    m_int = m_int.toarray()
    n_int = k_int.shape[0]
    print(f"[make-sidecar] cube-cavity: n={args.n}, side={args.side}, n_int={n_int}")

    return {
        "schema_version": "1",
        "fixture_id": f"cube_cavity/n{args.n}_numpy_sidecar",
        "description": (
            "Full-matrix cube-cavity sidecar synthesized from the NumPy "
            "reference assembly (reference/numpy/cube_cavity_minimal.py) "
            "for local driver smoke (issue #186). Mirrors the schema of "
            "the TF-Java / ONNX reduced_kM.json CI dumps."
        ),
        "units": "dimensionless",
        "inputs": {
            "n": _scalar_input(args.n, "i64", "Cells per side."),
            "side": _scalar_input(args.side, "f64", "Cube side."),
        },
        "outputs": {
            "k_int": _f64_field(k_int, "Interior stiffness matrix (dense f64)."),
            "m_int": _f64_field(m_int, "Interior mass matrix (dense f64)."),
        },
        "provenance": {
            "source": "reference/numpy/cube_cavity_minimal.py via "
                      "reference/driver/make_numpy_sidecar.py",
            "issue": "#186",
        },
    }


def _make_sphere_pec(args) -> dict:
    from reference.numpy.sphere_pec import (
        apply_dirichlet,
        assemble_global_nedelec,
        build_edges,
        build_epsilon_r,
        read_sphere_fixture,
        sphere_pec_interior_edges,
    )

    fixture = read_sphere_fixture(args.mesh)
    epsilon_r = build_epsilon_r(fixture.tet_physical_tags, n_inside=args.n_index)
    edges, tet_edge_idx, tet_edge_sign = build_edges(fixture.tets)
    interior_mask, _ = sphere_pec_interior_edges(
        fixture.nodes, edges, r_outer=args.r_buffer
    )
    K, M = assemble_global_nedelec(
        fixture.nodes, fixture.tets, edges, tet_edge_idx, tet_edge_sign, epsilon_r
    )
    k_int, m_int = apply_dirichlet(K, M, interior_mask)
    k_int = k_int.toarray()
    m_int = m_int.toarray()
    n_int = k_int.shape[0]
    print(
        f"[make-sidecar] sphere-pec: mesh={args.mesh}, "
        f"n_nodes={fixture.n_nodes}, n_tets={fixture.n_tets}, n_int={n_int}"
    )

    sidecar = {
        "schema_version": "1",
        "fixture_id": f"sphere_pec/n{fixture.n_nodes}_pec_numpy_sidecar",
        "description": (
            "Full-matrix sphere-PEC sidecar synthesized from the NumPy "
            "reference assembly (reference/numpy/sphere_pec.py) for local "
            "driver smoke (issue #186). Mirrors the schema of the "
            "reduced_kM_sphere_pec.json CI dumps."
        ),
        "units": "dimensionless mesh coordinates",
        "inputs": {
            "n_index": _scalar_input(
                args.n_index, "f64", "Refractive index inside the sphere."
            ),
            "r_buffer": _scalar_input(args.r_buffer, "f64", "Outer PEC wall radius."),
        },
        "outputs": {
            **_count_outputs(fixture.n_nodes, fixture.n_tets, len(edges), n_int),
            "k_int": _f64_field(k_int, "Interior curl-curl stiffness (dense f64)."),
            "m_int": _f64_field(m_int, "Interior epsilon-weighted mass (dense f64)."),
        },
        "provenance": {
            "source": "reference/numpy/sphere_pec.py via "
                      "reference/driver/make_numpy_sidecar.py",
            "issue": "#186",
        },
    }
    return sidecar


def _make_sphere_complex(args, problem: str) -> dict:
    """Shared synthesis for sphere-pml (scalar complex epsilon) and
    sphere-mie (anisotropic UPML diagonal tensor epsilon)."""
    from reference.numpy.sphere_pec import (
        apply_dirichlet,
        build_edges,
        read_sphere_fixture,
        sphere_pec_interior_edges,
    )

    fixture = read_sphere_fixture(args.mesh)
    edges, tet_edge_idx, tet_edge_sign = build_edges(fixture.tets)
    interior_mask, _ = sphere_pec_interior_edges(
        fixture.nodes, edges, r_outer=args.r_buffer
    )

    if problem == "sphere-pml":
        from reference.numpy.sphere_pml import (
            assemble_global_nedelec_complex,
            build_complex_epsilon_r_pml,
            tet_centroid_radii,
        )

        centroid_radii = tet_centroid_radii(fixture.nodes, fixture.tets)
        eps = build_complex_epsilon_r_pml(
            fixture.tet_physical_tags,
            centroid_radii,
            n_inside=args.n_index,
            sigma_0=args.sigma0,
        )
        K, M = assemble_global_nedelec_complex(
            fixture.nodes, fixture.tets, edges, tet_edge_idx, tet_edge_sign, eps
        )
        extra_inputs = {}
        epsilon_desc = "scalar-isotropic complex-epsilon PML"
    else:
        from reference.numpy.sphere_mie import (
            assemble_global_nedelec_anisotropic,
            build_anisotropic_pml_tensor_diag,
            tet_centroids,
        )

        centroids = tet_centroids(fixture.nodes, fixture.tets)
        eps = build_anisotropic_pml_tensor_diag(
            fixture.tet_physical_tags,
            centroids,
            n_inside=args.n_index,
            sigma_0=args.sigma0,
            k0_ref=args.k0_ref,
        )
        K, M = assemble_global_nedelec_anisotropic(
            fixture.nodes, fixture.tets, edges, tet_edge_idx, tet_edge_sign, eps
        )
        extra_inputs = {
            "k0_ref": _scalar_input(
                args.k0_ref, "f64", "Reference wavenumber in the UPML stretch."
            ),
        }
        epsilon_desc = "anisotropic UPML diagonal tensor epsilon"

    k_int, m_int = apply_dirichlet(K, M, interior_mask)
    k_int = np.asarray(k_int.todense())
    m_int = np.asarray(m_int.todense())
    n_int = k_int.shape[0]
    k_int_real = _real_part_checked(k_int, "K_int")
    print(
        f"[make-sidecar] {problem}: mesh={args.mesh}, "
        f"n_nodes={fixture.n_nodes}, n_tets={fixture.n_tets}, n_int={n_int}"
    )

    tag = "pml" if problem == "sphere-pml" else "aniso_upml_mie"
    return {
        "schema_version": "1",
        "fixture_id": f"{problem.replace('-', '_')}/n{fixture.n_nodes}_{tag}_numpy_sidecar",
        "description": (
            f"Full-matrix {problem} sidecar ({epsilon_desc}) synthesized "
            "from the NumPy reference assembly for local driver smoke "
            "(issue #186). Mirrors the (k_int, m_re_int, m_im_int) schema "
            "of the TF-Java CI dumps (Re/Im split because TF-Java 1.0.0 "
            "has no native c128 typed value)."
        ),
        "units": "dimensionless mesh coordinates",
        "inputs": {
            "sigma_0": _scalar_input(
                args.sigma0, "f64", "PML absorption strength at r=R_BUFFER."
            ),
            **extra_inputs,
            "n_index": _scalar_input(
                args.n_index, "f64", "Refractive index in the dielectric."
            ),
            "r_buffer": _scalar_input(args.r_buffer, "f64", "Outer PEC wall radius."),
        },
        "outputs": {
            **_count_outputs(fixture.n_nodes, fixture.n_tets, len(edges), n_int),
            "k_int": _f64_field(
                k_int_real, "Interior curl-curl stiffness (real-valued, dense f64)."
            ),
            "m_re_int": _f64_field(m_int.real, "Re(M_int) (dense f64)."),
            "m_im_int": _f64_field(m_int.imag, "Im(M_int) (dense f64)."),
        },
        "provenance": {
            "source": (
                f"reference/numpy/{problem.replace('-', '_')}.py via "
                "reference/driver/make_numpy_sidecar.py"
            ),
            "issue": "#186",
        },
    }


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------


def main():
    parser = argparse.ArgumentParser(
        description=(
            "Synthesize a full-matrix assembly sidecar from the in-tree "
            "NumPy references for eigensolve_from_sidecar.py smoke runs."
        )
    )
    parser.add_argument(
        "--problem", required=True, choices=list(PROBLEMS), metavar="PROBLEM",
        help="Problem family. One of: " + ", ".join(PROBLEMS),
    )
    parser.add_argument("--out", required=True, help="Output sidecar JSON path.")
    parser.add_argument(
        "--mesh", default=None,
        help=(
            "Mesh path (sphere problems). Defaults: sphere-pec -> "
            "reference/fixtures/sphere_pec/sphere.msh; sphere-pml / "
            "sphere-mie -> reference/fixtures/sphere_pml_small/sphere.msh "
            "(small mesh; pass the full sphere_pml mesh for a CI-sized run)."
        ),
    )
    parser.add_argument("--n", type=int, default=4, help="[cube-cavity] Cells per side.")
    parser.add_argument("--side", type=float, default=1.0, help="[cube-cavity] Cube side.")
    parser.add_argument("--n-index", type=float, default=1.5,
                        help="[sphere-*] Dielectric refractive index.")
    parser.add_argument("--r-buffer", type=float, default=2.0,
                        help="[sphere-*] Outer PEC wall radius.")
    parser.add_argument("--sigma0", type=float, default=5.0,
                        help="[sphere-pml/-mie] PML absorption strength.")
    parser.add_argument("--k0-ref", type=float, default=2.0,
                        help="[sphere-mie] UPML reference wavenumber.")
    args = parser.parse_args()

    if args.mesh is None:
        if args.problem == "sphere-pec":
            args.mesh = str(_REPO_ROOT / "reference/fixtures/sphere_pec/sphere.msh")
        elif args.problem in ("sphere-pml", "sphere-mie"):
            args.mesh = str(_REPO_ROOT / "reference/fixtures/sphere_pml_small/sphere.msh")

    if args.problem == "cube-cavity":
        sidecar = _make_cube_cavity(args)
    elif args.problem == "sphere-pec":
        sidecar = _make_sphere_pec(args)
    else:
        sidecar = _make_sphere_complex(args, args.problem)

    out_path = Path(args.out)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    with open(out_path, "w") as f:
        json.dump(sidecar, f, separators=(",", ":"))
        f.write("\n")
    size_mb = out_path.stat().st_size / (1024 * 1024)
    print(f"[make-sidecar] Wrote {out_path} ({size_mb:.1f} MB)")


if __name__ == "__main__":
    main()
