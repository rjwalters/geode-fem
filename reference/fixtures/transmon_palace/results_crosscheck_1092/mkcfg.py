"""Generate the issue #1092 Palace configs for the DeviceLayout cross-check.

Usage:
    python3 -I mkcfg.py <base palace_config.json> <out-dir> <cell> [<cell> ...]

Each cell is NAME:MESHFILE:ORDER:TARGET_GHZ:N:LINEAR:NP, for example
    initial_h_p1:initial_o1_v22.msh:1:3.0:3:Default:16
    initial_r1_p2:initial_r1_v22.msh:2:3.0:3:AMS:32

The base is the committed geode-equivalent config
(reference/fixtures/transmon_palace/palace_config.json: PEC on metal and
on the exterior boundary, the junction as an L || C lumped port, the two
50 ohm readout ports omitted, sapphire tensor). Because gmsh assigns the
physical-group tag numbers per mesh, every attribute list is rebuilt from
the names in the mesh file's $PhysicalNames block (MSH 2.2 and 4.1 both
carry it): PEC = metal + exterior_boundary, the lumped port =
lumped_element, materials = substrate and vacuum.

Relative to the base only these keys change:

- Model.Mesh                    -> /work/<MESHFILE>
- Problem.Output                -> /work/postpro/<NAME>
- Solver.Order                  -> ORDER
- Solver.Eigenmode.Target, N    -> TARGET_GHZ, N; Save -> 0
- Solver.Linear.Type            -> LINEAR (and KSPType GMRES for AMS)
- every Attributes list         -> the tag the mesh assigns to that name

NP is recorded in <out-dir>/cells.txt for run.sh and is not part of the
config. The DeviceLayout-faithful configs (absorbing far field, 50 ohm
readout ports, DeviceLayout's own second-order mesh) are written by
reference/julia/generate_transmon_param.jl; `--dlpec <dl_config.json>
<out-name>` derives the same-mesh PEC-box control from one of them: the
Absorbing block is dropped, its attribute joins PEC, the two R ports are
removed and Save is 0.
"""

import json
import sys


def physical_names(path):
    """name -> (dim, tag) from the $PhysicalNames block."""
    out = {}
    with open(path) as fh:
        for line in fh:
            if line.strip() == "$PhysicalNames":
                n = int(next(fh))
                for _ in range(n):
                    dim, tag, name = next(fh).split(maxsplit=2)
                    out[name.strip().strip('"')] = (int(dim), int(tag))
                break
    if not out:
        raise SystemExit(f"{path}: no $PhysicalNames block")
    return out


def dlpec(src, dst):
    c = json.load(open(src))
    b = c["Boundaries"]
    absorbing = b.pop("Absorbing")["Attributes"]
    b["PEC"]["Attributes"] = sorted(b["PEC"]["Attributes"] + absorbing)
    b["LumpedPort"] = [p for p in b["LumpedPort"] if "R" not in p]
    assert len(b["LumpedPort"]) == 1 and "L" in b["LumpedPort"][0]
    b["LumpedPort"][0]["Index"] = 1
    c["Solver"]["Eigenmode"]["Save"] = 0
    c["Problem"]["Output"] = c["Problem"]["Output"].replace("/postpro/dl_", "/postpro/dlpec_")
    with open(dst, "w") as fh:
        json.dump(c, fh, indent=2, sort_keys=True)
        fh.write("\n")
    print(dst)


if sys.argv[1] == "--dlpec":
    dlpec(sys.argv[2], sys.argv[3])
    sys.exit(0)

src, outdir = sys.argv[1], sys.argv[2]
base = json.load(open(src))
cells = []
for spec in sys.argv[3:]:
    tag, meshfile, order, target, n, linear, np_ = spec.split(":")
    names = physical_names(f"{outdir}/{meshfile}")
    c = json.loads(json.dumps(base))
    c["Model"]["Mesh"] = f"/work/{meshfile}"
    c["Problem"]["Output"] = f"/work/postpro/{tag}"
    c["Solver"]["Order"] = int(order)
    c["Solver"]["Eigenmode"]["Target"] = float(target)
    c["Solver"]["Eigenmode"]["N"] = int(n)
    c["Solver"]["Eigenmode"]["Save"] = 0
    c["Solver"]["Linear"]["Type"] = linear
    if linear == "AMS":
        c["Solver"]["Linear"]["KSPType"] = "GMRES"
    c["Boundaries"]["PEC"]["Attributes"] = sorted(
        [names["metal"][1], names["exterior_boundary"][1]]
    )
    assert len(c["Boundaries"]["LumpedPort"]) == 1
    c["Boundaries"]["LumpedPort"][0]["Attributes"] = [names["lumped_element"][1]]
    mats = c["Domains"]["Materials"]
    assert len(mats) == 2
    for m in mats:
        m["Attributes"] = [names["substrate"][1] if "MaterialAxes" in m else names["vacuum"][1]]
    path = f"{outdir}/palace_config_{tag}.json"
    with open(path, "w") as fh:
        json.dump(c, fh, indent=2, sort_keys=True)
        fh.write("\n")
    print(path)
    cells.append(f"{tag} {np_}")
with open(f"{outdir}/cells.txt", "a") as fh:
    fh.write("\n".join(cells) + "\n")
