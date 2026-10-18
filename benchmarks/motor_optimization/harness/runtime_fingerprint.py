"""Hash the installed numerical runtime without recording environment secrets."""
import hashlib
import importlib.metadata
import json
from pathlib import Path
import sys


def sha(path):
    h = hashlib.sha256()
    with Path(path).open("rb") as stream:
        for block in iter(lambda: stream.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()


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
    if "gmsh" in packages:
        import gmsh
        library = Path(gmsh.lib._name).resolve()
        result["gmsh_library"] = {"path": str(library), "sha256": sha(library)}
    return result


if __name__ == "__main__":
    print(json.dumps(capture(sys.argv[1:]), sort_keys=True))
