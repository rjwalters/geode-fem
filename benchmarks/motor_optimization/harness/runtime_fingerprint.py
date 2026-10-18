"""Hash the installed numerical runtime without recording environment secrets."""
import hashlib
import importlib
import importlib.metadata
import json
from pathlib import Path
import platform
import sys


def sha(path):
    h = hashlib.sha256()
    with Path(path).open("rb") as stream:
        for block in iter(lambda: stream.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()


def linear_algebra(packages):
    """Explicit built-against BLAS/LAPACK identity, compatible with #1201.

    The flat fingerprint uses linalg_info.py's replay keys when both NumPy
    and SciPy are present. The mesher records NumPy alone. Full BLAS/LAPACK
    dependency dictionaries retain ABI/configuration differences hidden by
    the short names. Missing/error config is a refusal, not two equal unknowns.
    Accelerate legitimately reports version "unknown"; OS release/version,
    package payload hashes and the numerical environment remain separately bound.
    """
    fingerprint = {"system": platform.system(), "machine": platform.machine(),
                   "macos_version": platform.mac_ver()[0]}
    dependencies = {}
    for name in ("numpy", "scipy"):
        if name not in packages:
            continue
        module = importlib.import_module(name)
        config = module.show_config(mode="dicts")
        build = config.get("Build Dependencies", {})
        fingerprint[name] = module.__version__
        dependencies[name] = {}
        for kind in ("blas", "lapack"):
            dep = build.get(kind)
            if (not isinstance(dep, dict) or dep.get("found") is not True
                    or not isinstance(dep.get("name"), str) or not dep["name"].strip()
                    or not isinstance(dep.get("version"), str) or not dep["version"].strip()):
                raise ValueError(f"missing explicit {name} {kind} configuration")
            fingerprint[name + "_" + kind] = dep["name"]
            fingerprint[name + "_" + kind + "_version"] = dep["version"]
            dependencies[name][kind] = dep
    return {"fingerprint": fingerprint, "build_dependencies": dependencies}


def capture(packages):
    distributions = {}
    for name in packages:
        dist = importlib.metadata.distribution(name)
        if dist.files is None:
            raise ValueError(f"no installed-file inventory for {name}")
        files = {}
        for entry in dist.files:
            if str(entry).endswith((".pyc", ".pyo")) or "__pycache__" in entry.parts:
                continue
            path = Path(dist.locate_file(entry))
            if not path.is_file():
                raise ValueError(f"missing installed runtime file: {name}/{entry}")
            files[str(entry)] = sha(path)
        if not files:
            raise ValueError(f"empty installed runtime for {name}")
        distributions[name] = {"version": dist.version, "files": files}
    result = {"python": sys.version, "isolated": bool(sys.flags.isolated),
              "executable": str(Path(sys.executable).resolve()),
              "executable_sha256": sha(sys.executable), "distributions": distributions}
    if any(name in packages for name in ("numpy", "scipy")):
        result["linear_algebra"] = linear_algebra(packages)
    if "gmsh" in packages:
        import gmsh
        library = Path(gmsh.lib._name).resolve()
        result["gmsh_library"] = {"path": str(library), "sha256": sha(library)}
    return result


if __name__ == "__main__":
    print(json.dumps(capture(sys.argv[1:]), sort_keys=True))
