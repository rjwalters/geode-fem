"""Frozen source/runtime/input bindings required before linked recovery dispatch.

Process absence and retry eligibility are separate checks. These functions never
launch an optimizer, remeasure a calibration, or infer permission from a signal.
"""
import hashlib
import json
from pathlib import Path
import shutil
import subprocess

from run import thread_env
import runtime_fingerprint

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
SOURCE_PATHS = ("Cargo.toml", "Cargo.lock", "rust-toolchain.toml", ".cargo", "crates",
                "benchmarks/motor_optimization", "benchmarks/motor_reference",
                "benchmarks/motor_shape_gradient")
NUMERICAL_ENV = ("OMP_NUM_THREADS", "OPENBLAS_NUM_THREADS", "MKL_NUM_THREADS",
                 "VECLIB_MAXIMUM_THREADS", "BLIS_NUM_THREADS", "OMP_DYNAMIC",
                 "OMP_SCHEDULE", "MKL_DYNAMIC", "MKL_CBWR", "PYTHONHASHSEED",
                 "RAYON_NUM_THREADS", "GEODE_NUM_THREADS", "OPENBLAS_CORETYPE",
                 "GOTO_NUM_THREADS", "BLIS_ARCH_TYPE",
                 "LD_LIBRARY_PATH", "LD_PRELOAD", "DYLD_LIBRARY_PATH", "DYLD_INSERT_LIBRARIES")


def command(argv, cwd, *, env=None):
    result = subprocess.run(argv, cwd=cwd, env=env, capture_output=True, text=True, timeout=120)
    if result.returncode != 0:
        # Probe output can contain local paths but is never a credential dump.
        raise RuntimeError(f"binding probe failed ({result.returncode}): {argv[0]}")
    return result.stdout


def sources(repo=ROOT):
    repo = Path(repo).resolve()
    names = command(["git", "ls-files", "-z", "--", *SOURCE_PATHS], repo).split("\0")
    files = {name: runtime_fingerprint.sha(repo / name) for name in names if name}
    if not files:
        raise ValueError("no tracked experiment sources")
    status = command(["git", "status", "--porcelain", "-z", "--untracked-files=all", "--", *SOURCE_PATHS], repo)
    dirty, untracked = False, []
    for entry in filter(None, status.split("\0")):
        if not entry.startswith("?? "):
            dirty = True
            continue
        name = entry[3:]
        path = Path(name)
        # New output JSON, journals and meshes in the registered runs/ location
        # are evidence, not new executable source. Track all existing files and
        # additionally detect untracked code/configuration that can be loaded.
        if (path.suffix in (".py", ".rs", ".toml", ".sh", ".geo", ".cfg", ".ini")
                or path.name in ("Cargo.lock", "requirements.lock", "rust-toolchain")):
            untracked.append(name)
            files[name] = runtime_fingerprint.sha(repo / name)
    return {"repo": str(repo), "commit": command(["git", "rev-parse", "HEAD"], repo).strip(),
            "tree_clean": not dirty and not untracked, "files": files,
            "untracked_sources": sorted(untracked)}


def runtime(repo=ROOT):
    env = thread_env()
    uv = shutil.which("uv", path=env.get("PATH"))
    if uv is None:
        raise ValueError("uv runtime is unavailable")
    # Match the Rust mesher's pinned packages, interpreter request, isolation and
    # working directory. Offline prevents a network fetch during verification;
    # a missing warm environment is a refusal. Initial setup can prepare it with
    # the evaluator's exact uv command before this check.
    remesh = json.loads(command([uv, "run", "-q", "--offline", "--with", "numpy==2.5.4",
        "--with", "gmsh==4.15.2", "python", "-I", str(HERE / "runtime_fingerprint.py"),
        "numpy", "gmsh"], Path.cwd(), env=env))
    numerical = {key: env.get(key) for key in NUMERICAL_ENV}
    # UV configuration can change resolution; hash its values without disclosing
    # tokens or paths stored in environment variables.
    uv_env = {key: value for key, value in env.items() if key.startswith("UV_")}
    harness = runtime_fingerprint.capture(("numpy", "scipy"))
    for observed, required in ((harness, {"numpy": "2.5.4", "scipy": "1.18.1"}),
                               (remesh, {"numpy": "2.5.4", "gmsh": "4.15.2"})):
        if observed.get("isolated") is not True or any(
                observed["distributions"][name]["version"] != version for name, version in required.items()):
            raise ValueError("runtime does not match the registered isolated package versions")
    return {"harness": harness, "remesher": remesh,
            "uv": {"path": str(Path(uv).resolve()), "sha256": runtime_fingerprint.sha(uv)},
            "numerical_env": numerical,
            "uv_env_sha256": hashlib.sha256(json.dumps(uv_env, sort_keys=True).encode()).hexdigest(),
            "working_directory": str(Path.cwd().resolve())}


def inputs(session):
    evaluator = session["evaluator"]
    paths = {"binary": evaluator["binary"], "validation_mesh": evaluator["val_mesh"],
             "anchor": evaluator["anchor"], "starts": session["starts"]}
    if evaluator.get("build_receipt"):
        paths["build_receipt"] = evaluator["build_receipt"]
    paths.update({f"training_{name}": model["train_mesh"] for name, model in session["models"].items()})
    return {name: {"path": str(Path(path).resolve()), "sha256": runtime_fingerprint.sha(path)}
            for name, path in paths.items()}


def capture(session, *, repo=ROOT, runtime_probe=runtime, source_probe=sources):
    procs, unit = session["procs"], session["unit_seconds"]
    import math
    if type(procs) is not int or procs < 1:
        raise ValueError("invalid original worker count")
    if type(unit) not in (int, float) or not math.isfinite(unit) or unit <= 0:
        raise ValueError("invalid original calibration unit")
    from session import host_record
    host = host_record()
    stable_host = {key: host[key] for key in ("host", "platform", "cpu_model", "cpu_count")}
    return {"schema": 1, "source": source_probe(repo), "runtime": runtime_probe(repo), "host": stable_host,
            "inputs": inputs(session), "session": session, "procs": procs, "unit_seconds": unit}


def verify(expected, session, *, repo=ROOT, runtime_probe=runtime, source_probe=sources):
    if expected.get("schema") != 1:
        raise ValueError("no supported frozen recovery bindings")
    observed = capture(session, repo=repo, runtime_probe=runtime_probe, source_probe=source_probe)
    if session.get("label") in ("main", "fd_ablation", "fd_warm_ablation") and (
            expected["source"].get("tree_clean") is not True
            or observed["source"].get("tree_clean") is not True):
        raise ValueError("main recovery requires unchanged clean sources")
    changed = [key for key in observed if observed[key] != expected.get(key)]
    if changed:
        raise ValueError("recovery bindings changed: " + ", ".join(changed))
    return {"verified": True, "schema": 1, "can_launch": False,
            "scope": "source/runtime/input/settings equality only; process/host/eligibility checks separate"}


def validate_setup(bound, provenance, meshes):
    """Refuse source/binary/primary-input changes while calibration was running."""
    if bound["source"] != provenance["source"]:
        raise ValueError("experiment sources changed during setup")
    if bound["runtime"] != provenance["runtime"]:
        raise ValueError("numerical runtime changed during setup")
    expected = {"binary": provenance["evaluator"]["sha256"],
                "validation_mesh": meshes["validation"]["sha256"],
                "anchor": meshes["anchor"]["sha256"], "starts": meshes["starts"]["sha256"],
                "training_" + bound["session"]["primary_model"]: meshes["train"]["sha256"]}
    if provenance.get("evaluator_build"):
        proof = provenance["evaluator_build"]
        if proof.get("verified") is not True:
            raise ValueError("evaluator build provenance is unverified")
        expected["build_receipt"] = proof["sha256"]
    for name, sha in expected.items():
        if bound["inputs"][name]["sha256"] != sha:
            raise ValueError(f"input changed during setup: {name}")
