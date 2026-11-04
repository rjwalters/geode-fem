#!/usr/bin/env python3
"""Flatten a GDS cell into a `geode mesh` layout JSON (schema v1).

Runs under KLayout's batch interpreter (own code; no gds2palace code):

    klayout -b -r scripts/gds2geode-layout.py \
        -rd gds=cell.gds -rd cell=TOP -rd layermap=layers.layermap -rd out=layout.json

Arguments (`-rd key=value`):
  gds       input GDS/OASIS file
  cell      top cell name (default: the single top cell)
  layermap  JSON file, see below
  out       output layout JSON path

Layer map JSON:
  {
    "layers": [{"layer": 8, "datatype": 0, "name": "TopMetal1",
                "z_bottom": 4.0, "thickness": 2.0}, ...],
    "template": {"dielectrics": [...], "margin": 20.0, "mesh": {...},
                 "ports": [...]}           # optional; merged into the output
  }
Coordinates are emitted in microns (`length_unit_m` = 1e-6); z_bottom and
thickness in the layer map are therefore microns too.
Hierarchy and array instances are flattened. Polygons come out as outer ring
plus holes (KLayout `Region.merged()`), so touching shapes on one layer merge.
"""
import json
import sys

try:
    import klayout.db as db
except ImportError:  # KLayout's embedded interpreter
    import pya as db


def _arg(name, default=None):
    # `klayout -rd name=value` defines a global variable of that name.
    return globals().get(name, default)


def _ring(points, dbu):
    return [[round(p.x * dbu, 9), round(p.y * dbu, 9)] for p in points]


def convert(gds, cell_name, layermap, out):
    layout = db.Layout()
    layout.read(gds)
    if cell_name:
        cell = layout.cell(cell_name)
        if cell is None:
            raise SystemExit(f"cell {cell_name!r} not found in {gds}")
    else:
        tops = layout.top_cells()
        if len(tops) != 1:
            raise SystemExit(f"{gds} has {len(tops)} top cells; pass cell=NAME")
        cell = tops[0]
    with open(layermap) as f:
        lm = json.load(f)
    conductors = []
    for ent in lm["layers"]:
        idx = layout.find_layer(ent["layer"], ent.get("datatype", 0))
        if idx is None:
            continue
        region = db.Region(cell.begin_shapes_rec(idx)).merged()
        polys = []
        for i, poly in enumerate(region.each()):
            entry = {
                "name": f"{ent['name']}_{i}",
                "outer": _ring(poly.each_point_hull(), layout.dbu),
            }
            holes = [
                _ring(poly.each_point_hole(h), layout.dbu)
                for h in range(poly.holes())
            ]
            if holes:
                entry["holes"] = holes
            polys.append(entry)
        if not polys:
            continue
        conductors.append(
            {
                "name": ent["name"],
                "z_bottom": ent["z_bottom"],
                "thickness": ent.get("thickness", 0.0),
                "polygons": polys,
            }
        )
    doc = {"schema_version": 1, "length_unit_m": 1e-6}  # coordinates are emitted in microns
    doc["description"] = f"{cell.name} flattened from {gds} by gds2geode-layout.py"
    doc.update(lm.get("template", {}))
    doc["conductors"] = conductors
    with open(out, "w") as f:
        json.dump(doc, f, indent=2)
        f.write("\n")


if __name__ == "__main__" or "gds" in globals():
    gds_path = _arg("gds")
    if not gds_path:
        sys.exit("usage: klayout -b -r gds2geode-layout.py -rd gds=… -rd layermap=… -rd out=…")
    convert(gds_path, _arg("cell"), _arg("layermap"), _arg("out"))
