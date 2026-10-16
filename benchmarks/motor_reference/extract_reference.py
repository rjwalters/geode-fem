#!/usr/bin/env python3
"""Extract numeric facts from a hash-pinned ICEM snapshot, without running MATLAB.

uv run --with scipy==1.18.1 python benchmarks/motor_reference/extract_reference.py --fetch
uv run --with scipy==1.18.1 python benchmarks/motor_reference/extract_reference.py --check

--fetch retrieves missing upstream files into ignored _upstream/. Existing
files must match their hashes. --check compares the extraction with committed
JSON without rewriting it. This is data preparation, not a FEM reproduction.
"""

import argparse
import csv
import hashlib
import io
import json
from pathlib import Path
import re
import urllib.request

import numpy as np
from scipy.io import loadmat


ROOT = Path(__file__).resolve().parent


def require(condition, message):
    if not condition:
        raise ValueError(message)


def source_text(path):
    return (ROOT / "_upstream" / path).read_text()


def numeric_array(text, name):
    match = re.search(r"obj\." + name + r"\s*=\s*\[([^]]+)\]", text)
    require(match is not None, f"Missing {name} array")
    return [float(x) for x in re.split(r"[,\s]+", match[1].strip()) if x]


def scalar_defaults(text):
    # Only numeric scalar assignments before the input-override block.
    header = text.split("if nargin", 1)[0]
    return {name: float(value) for name, value in re.findall(
        r"^\s*(\w+)\s*=\s*([-+0-9.eE]+)\s*;", header, re.M)}


def prepare_sources(fetch):
    manifest = json.loads((ROOT / "sources.json").read_text())
    prefix = ("https://raw.githubusercontent.com/MichaelWiesheu/ICEMsnapshot/"
              + manifest["commit"] + "/")
    for entry in manifest["files"]:
        path = ROOT / "_upstream" / entry["path"]
        require(entry["url"] == prefix + entry["path"], "Unpinned source URL")
        if not path.exists():
            require(fetch, f"Missing {path}; rerun with --fetch")
            raw = urllib.request.urlopen(entry["url"], timeout=90).read()
            require(hashlib.sha256(raw).hexdigest() == entry["sha256"],
                    f"Download hash mismatch: {entry['path']}")
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(raw)
        raw = path.read_bytes()
        require(len(raw) == entry["bytes"], f"Size mismatch: {entry['path']}")
        require(hashlib.sha256(raw).hexdigest() == entry["sha256"],
                f"Hash mismatch: {entry['path']}")
    return manifest


def extract(manifest):
    material = source_text("material/softmagnetic/M_27.m")
    b, h = numeric_array(material, "B"), numeric_array(material, "H")
    require(len(b) == len(h) == 47, "Unexpected M27 table length")
    require(b[0] == h[0] == 0, "M27 table must start at origin")
    require(all(y > x for x, y in zip(b, b[1:])), "B must increase")
    require(all(y > x for x, y in zip(h, h[1:])), "H must increase")
    # H(B) is monotone; H/B is NOT. Prevent an incorrect material restriction.
    require(h[2] / b[2] < h[1] / b[1], "Expected low-field reluctivity decrease")

    opt = source_text("results/PMSM_optimization_nonlin.m")
    parameters = re.findall(
        r'addOptimizationParameter\("(\w+)",\s*([^,]+),\s*([^,]+),\s*([^)]+)\)', opt)
    require(len(parameters) == 12, "Expected 12 engineering parameters")
    parameter_records = [dict(name=n, initial=float(v), minimum=float(lo), maximum=float(hi))
                         for n, v, lo, hi in parameters]
    winding = [dict(phase=p, patches=[int(v) for v in re.split(r"[,\s]+", ids)],
                    sign=int(sign)) for p, ids, sign in re.findall(
                        r'addPhase\("([UVW])",\s*\[([^]]+)\],\s*(-?1)\)', opt)]
    require(len(winding) == 12, "Expected 12 winding groups")
    history = loadmat(ROOT / "_upstream/results/plots/final/OptimizationHistory.mat",
                      simplify_cells=True)["OptimizationHistory"]
    csv_values = np.array(list(csv.reader(io.StringIO(source_text("results/TorqueData.csv")))),
                          dtype=float)
    require(csv_values.shape == (21, 7), "Unexpected saved torque CSV shape")
    require(np.array_equal(csv_values[:, 0], np.arange(21)), "CSV angle grid differs")
    require(np.array_equal(csv_values[0, 1:], csv_values[-1, 1:]),
            "CSV last row must duplicate the periodic endpoint")
    designs = {}
    for name, iteration, mat_name, csv_start in [
        ("initial", 0, "InitTorques.mat", 1),
        ("optimized", 120, "OptTorques.mat", 4),
    ]:
        matches = [v for v in history if v["iteration"] == iteration]
        require(len(matches) == 1, f"Ambiguous history iteration {iteration}")
        item = matches[0]
        params = item["Parameters"]
        require(len(params) == len(parameter_records), "History parameter count mismatch")
        # MATLAB string names are opaque to scipy; bind by source order and
        # verify every initial value and bound, not just the number of fields.
        for spec, value in zip(parameter_records, params):
            for expected, actual in [("initial", "InitVal"), ("minimum", "MinVal"),
                                     ("maximum", "MaxVal")]:
                require(float(value[actual]) == spec[expected],
                        f"History/source order mismatch: {spec['name']} {actual}")
        values = {spec["name"]: float(value["OptVal"])
                  for spec, value in zip(parameter_records, params)}
        saved = loadmat(ROOT / "_upstream/results" / mat_name, simplify_cells=True)
        angles = np.asarray(saved["angles"])
        torque = np.asarray(saved["Torques"])
        currents = np.asarray(saved["currents"])
        require(np.array_equal(angles, np.arange(20)), "Saved angles must be 0..19 degrees")
        require(torque.shape == (3, 20), "Saved torque must have 3 currents x 20 angles")
        require(np.allclose(currents, item["Currents"] * values["ScaleR"],
                            rtol=1e-13, atol=1e-13), "Saved current radial scaling mismatch")
        require(np.allclose(currents, [0, currents[-1] / 2, currents[-1]], atol=1e-13),
                "Saved current levels must be zero/half/full")
        # CSV was rounded by MATLAB csvwrite: compare at its printed precision.
        delta = np.max(np.abs(csv_values[:-1, csv_start:csv_start + 3].T - torque))
        require(delta < 5.1e-5, "CSV and MAT torque disagree beyond CSV rounding")
        designs[name] = {
            "history_iteration": iteration,
            "parameters_by_source_registration_order": values,
            "physical_phase_current_amplitudes_A": currents.tolist(),
            "history_unscaled_current_amplitudes_A": item["Currents"].tolist(),
            "mechanical_angles_degrees": angles.tolist(),
            "torque_Nm_for_modeled_sector_by_current_then_angle": torque.tolist(),
            "mean_torque_Nm": torque.mean(axis=1).tolist(),
            "population_std_torque_Nm": torque.std(axis=1).tolist(),
            "csv_max_abs_rounding_difference_Nm": float(delta),
            "history_optimization_angles_degrees": item["Angles"].tolist(),
            "history_optimization_mean_torque_Nm": item["Tmean"].tolist(),
            "history_optimization_std_torque_Nm": item["Tsigma"].tolist(),
            "rotor_geometry_export": f"results/plots/final/RotorOpt{iteration}.txt",
            "stator_geometry_export": f"results/plots/final/StatorOpt{iteration}.txt",
        }
    return {
        "schema_version": 1,
        "status": "extracted_reference_data_not_GEODE_reproduction",
        "source_commit": manifest["commit"],
        "source_manifest_sha256": hashlib.sha256((ROOT / "sources.json").read_bytes()).hexdigest(),
        "material": {"name": "M27 / M330-50A (snapshot table)", "B_T": b, "H_A_per_m": h,
                     "source": "material/softmagnetic/M_27.m",
                     "interpolation": "PCHIP H(B); nu(B)=H(B)/B; not monotone nu(B^2)",
                     "zero_and_tail_policy": "Reference getMu clamps B<=1e-5 T to 1e-5; above 2.3 T, H extrapolates with slope 1/mu0. See conventions.json for tangent caveats."},
        "geometry_function_defaults": {
            "rotor": scalar_defaults(source_text("geometries/PSMS/PMSM_rotor1.m")),
            "stator": scalar_defaults(source_text("geometries/PSMS/PMSM_stator1.m")),
            "warning": "Function defaults differ from optimization initial values for SW3 and Sr1. Use iteration-0 exported geometry for saved torque reproduction; parameter values alone do not encode surface control offsets.",
        },
        "optimization_parameter_bounds": parameter_records,
        "winding_groups_one_based_stator_patch_ids": winding,
        "designs": designs,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fetch", action="store_true")
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    manifest = prepare_sources(args.fetch)
    data = extract(manifest)
    rendered = json.dumps(data, indent=2, allow_nan=False) + "\n"
    output = ROOT / "reference_data.json"
    if args.check:
        require(output.read_text() == rendered, "Reference extraction differs from stored JSON")
        print("PASS: all source hashes, material data, parameter ordering, current scaling, "
              "angle grids and CSV/MAT consistency; stored JSON reproduces exactly.")
    else:
        output.write_text(rendered)
        print(f"Wrote {output}")


if __name__ == "__main__":
    main()
