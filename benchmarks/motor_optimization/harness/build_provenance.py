"""Witness a private Cargo build and bind its evaluator to exact clean sources.

Run from the worktree root with pyrun.sh build_provenance.py --out NEW_DIRECTORY.
The receipt is local provenance, not a signature or an independent code review.
"""
import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import sys
import time

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import interruption
import owned_child
import recovery_bindings
from session_journal import Journal, digest, freeze, read_events, sync_directory

ROOT = HERE.parents[2]
NAME = "icem_harness_eval"


def target_directory(repo, env):
    """Honor a Loom-owned target; otherwise use this worktree's target/."""
    repo = Path(repo).resolve()
    given = Path(env["CARGO_TARGET_DIR"]).resolve() if env.get("CARGO_TARGET_DIR") else None
    marker = repo / ".loom-cargo-target-dir"
    loom = repo / ".loom/targets"
    if given and (loom in given.parents or (marker.is_file() and
            given == Path(marker.read_text().strip()).resolve())):
        return given
    return repo / "target"


def environment(repo, env):
    tools = {}
    for name in ("cargo", "rustc"):
        path = shutil.which(name, path=env.get("PATH"))
        if path is None:
            raise ValueError(f"{name} is unavailable")
        tools[name] = {"path": path, "sha256": digest(path),
                       "version": recovery_bindings.command([path, "--version", "--verbose"], repo, env=env)}
    paths = set()
    for directory in (repo, *repo.parents):
        paths.update(directory / ".cargo" / name for name in ("config", "config.toml"))
    cargo_home = Path(env.get("CARGO_HOME", str(Path.home() / ".cargo")))
    paths.update(cargo_home / name for name in ("config", "config.toml"))
    selected = {k: v for k, v in env.items() if k.startswith(("CARGO_", "RUST", "CC", "CXX", "AR_", "LD_"))
                or k in ("CC", "CXX", "AR", "LD", "PATH", "SDKROOT", "MACOSX_DEPLOYMENT_TARGET")}
    return {"tools": tools, "config_sha256": {str(p): digest(p) for p in sorted(paths) if p.is_file()},
            "build_environment_sha256": hashlib.sha256(json.dumps(selected, sort_keys=True).encode()).hexdigest(),
            "host": platform.node(), "platform": platform.platform()}


def cargo_artifact(log, target, *, require_exists=True):
    rows = []
    finished = []
    for line in Path(log).read_text().splitlines():
        try:
            row = json.loads(line)
        except json.JSONDecodeError:
            continue  # Cargo's verbose progress and compiler diagnostics are retained.
        if not isinstance(row, dict):
            continue
        if row.get("reason") == "build-finished":
            finished.append(row.get("success"))
        if (row.get("reason") == "compiler-artifact" and row.get("target", {}).get("name") == NAME
                and "example" in row.get("target", {}).get("kind", [])):
            rows.append(row)
    if finished != [True] or len(rows) != 1:
        raise ValueError("Cargo did not report exactly one successful evaluator artifact")
    binary = Path(rows[0].get("executable") or "").resolve()
    if binary != (target / "release/examples" / NAME).resolve() or (require_exists and not binary.is_file()):
        raise ValueError("Cargo evaluator artifact is outside the selected private target")
    return binary, rows[0]


def record(out, *, repo=ROOT, stop=None, source_probe=recovery_bindings.sources,
           environment_probe=environment, runner=owned_child.wait_run):
    repo, out = Path(repo).resolve(), Path(out).resolve()
    if Path.cwd().resolve() != repo:
        raise ValueError("run the build recorder from its worktree root")
    env = dict(os.environ)
    target = target_directory(repo, env)
    env["CARGO_TARGET_DIR"] = str(target)
    target.mkdir(parents=True, exist_ok=True)
    with (target / ".motor-build.lock").open("a+") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        journal = Journal(out, {"role": "evaluator_build", "repo": str(repo)})
        started = time.monotonic()
        try:
            interruption.check(stop)
            source = source_probe(repo)
            if source.get("tree_clean") is not True:
                raise ValueError("recorded evaluator build requires clean experiment sources")
            build_env = environment_probe(repo, env)
            command = [build_env["tools"]["cargo"]["path"], "build", "--locked", "--release",
                       "-p", "geode-core", "--example", NAME, "--target-dir", str(target),
                       "--message-format=json", "--verbose"]
            freeze(out / "inputs.json", {"source": source, "environment": build_env,
                                          "command": command, "target_directory": str(target)})
            with (out / "cargo.log").open("x") as log:
                receipt = runner(command, log=log, env=env, stop=stop, journal=journal)
                log.flush(); os.fsync(log.fileno())
            interruption.check(stop)
            if receipt.get("returncode") != 0 or receipt.get("forced_kill"):
                raise ValueError("Cargo did not exit successfully")
            if source_probe(repo) != source or environment_probe(repo, env) != build_env:
                raise ValueError("sources or build environment changed during compilation")
            binary, artifact = cargo_artifact(out / "cargo.log", target)
            data = binary.read_bytes()
            with (out / NAME).open("xb") as stream:
                stream.write(data); stream.flush(); os.fsync(stream.fileno())
            (out / NAME).chmod(0o555)
            sync_directory(out)
            value = {"schema": 1, "status": "recorded", "source": source,
                     "inputs": {"file": "inputs.json", "sha256": digest(out / "inputs.json")},
                     "log": {"file": "cargo.log", "sha256": digest(out / "cargo.log")},
                     "binary": {"file": NAME, "sha256": digest(out / NAME)},
                     "cargo_artifact": artifact, "launcher_receipt": receipt,
                     "wall_seconds": time.monotonic() - started,
                     "scope": "local witnessed Cargo invocation and immutable binary copy; cached artifacts explicitly retained; no independent review"}
            binding = freeze(out / "build-receipt.json", value)
            journal.emit("build_recorded", receipt=binding)
            return out / "build-receipt.json"
        except BaseException as error:
            journal.emit("build_failed", error=f"{type(error).__name__}: {error}")
            raise
        finally:
            journal.close()


def verify(path, binary, source):
    if path is None:
        raise ValueError("main execution requires --build-receipt from build_provenance.py")
    path = Path(path).resolve()
    root = path.parent
    value = json.loads(path.read_text())
    if (value.get("schema") != 1 or value.get("status") != "recorded"
            or value.get("source") != source or source.get("tree_clean") is not True):
        raise ValueError("build receipt does not match current clean experiment sources")
    for key, name in (("inputs", "inputs.json"), ("log", "cargo.log"), ("binary", NAME)):
        item = value[key]
        if item.get("file") != name or digest(root / name) != item.get("sha256"):
            raise ValueError("build receipt artifact changed: " + key)
    inputs = json.loads((root / "inputs.json").read_text())
    if inputs.get("source") != source:
        raise ValueError("build inputs disagree with the recorded sources")
    reported, artifact = cargo_artifact(root / "cargo.log", Path(inputs["target_directory"]), require_exists=False)
    if artifact != value["cargo_artifact"]:
        raise ValueError("Cargo artifact differs from the retained build log")
    if (value["launcher_receipt"].get("returncode") != 0
            or value["launcher_receipt"].get("forced_kill")):
        raise ValueError("build receipt has no successful Cargo exit")
    if digest(binary) != value["binary"]["sha256"]:
        raise ValueError("selected evaluator differs from the witnessed build")
    events = read_events(root / "events.jsonl")
    bindings = [e["receipt"] for e in events["events"] if e["kind"] == "build_recorded"]
    if events["incomplete_tail_bytes"] or bindings != [{"file": path.name, "sha256": digest(path)}]:
        raise ValueError("build journal does not bind this receipt")
    return {"verified": True, "path": str(path), "sha256": digest(path),
            "binary_sha256": value["binary"]["sha256"], "source_commit": source["commit"]}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", required=True, type=Path)
    args = parser.parse_args()
    with interruption.StopSignals() as signals:
        path = record(args.out, stop=lambda: signals.requested)
    print(path)


if __name__ == "__main__":
    main()
