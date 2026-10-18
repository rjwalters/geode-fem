"""The linear-algebra stack of this interpreter (issue #1201): platform,
numpy and scipy versions and the BLAS/LAPACK they were built against
(`show_config(mode="dicts")`). The optimizers' paths depend on it at the
bit level (COBYQA's and C's RBF/SLSQP linear algebra: Accelerate on the
macOS wheels, OpenBLAS on the Linux ones), so every session and run record
carries it, and a bit-for-bit replay is only asserted under the stack it was
recorded on.
"""
import platform


def _deps(mod):
    try:
        c = mod.show_config(mode="dicts")
    except Exception as e:  # noqa: BLE001 - recorded, not fatal
        return {"error": str(e)}
    bd = c.get("Build Dependencies", {})
    out = {}
    for k in ("blas", "lapack"):
        d = bd.get(k, {})
        out[k] = d.get("name")
        out[f"{k}_version"] = d.get("version")
    return out


def fingerprint():
    """{system, machine, numpy, scipy, numpy_blas, ..., scipy_lapack}."""
    import numpy
    import scipy
    # The macOS product version too: Accelerate ships with the OS and can
    # change between releases ("" elsewhere).
    out = {"system": platform.system(), "machine": platform.machine(),
           "macos_version": platform.mac_ver()[0],
           "numpy": numpy.__version__, "scipy": scipy.__version__}
    for name, mod in (("numpy", numpy), ("scipy", scipy)):
        for k, v in _deps(mod).items():
            out[f"{name}_{k}"] = v
    return out


#: The keys a bit-for-bit replay depends on.
REPLAY_KEYS = ("system", "machine", "macos_version", "numpy", "scipy",
               "numpy_blas", "numpy_blas_version", "numpy_lapack", "numpy_lapack_version",
               "scipy_blas", "scipy_blas_version", "scipy_lapack", "scipy_lapack_version")


def same_stack(a, b):
    """Whether two fingerprints name the same linear-algebra stack."""
    return all(a.get(k) == b.get(k) for k in REPLAY_KEYS)
