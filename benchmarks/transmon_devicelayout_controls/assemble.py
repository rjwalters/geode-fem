"""Assemble benchmarks/transmon_devicelayout_controls/results.toml (issue #1123).

    python3 -I benchmarks/transmon_devicelayout_controls/assemble.py

Run from the repository root. Every number is re-derived from raw inputs:

- the Palace cells of reference/fixtures/transmon_palace/results_controls_1123/
  (eig.csv, port-EPR.csv, palace_run.log, time.txt per cell; the adaptive
  run's iterationN/ subdirectories);
- the committed cells and artifacts of #1079, #1092 and #1111 that the
  mesh-family and cap sections read (paths in the output).

The trajectories of the two gradient-free controls are re-derived from the
Palace values with the driver's own step arithmetic (imported from
driver.py), and each design is checked against the cell its mesh came from
(the generator report next to the cell). No Burn backend and no new geode
solve enters any number here; the one geode-derived input is #1111's
transmission-line delta (a constant from its one-time calibration). So the
artifact carries hand-provenance keys (MANIFEST bucket E), not the f64 trio.
"""

import importlib.util
import json
import math
import os
import re
import subprocess
import sys

ROOT = os.getcwd()
CELLS = "reference/fixtures/transmon_palace/results_controls_1123"
X1092 = "reference/fixtures/transmon_palace/results_crosscheck_1092"
L1079 = "reference/fixtures/transmon_palace/results_loop_1079"
C1078 = "reference/fixtures/transmon_palace/results_coupling_1078"
OUT = "benchmarks/transmon_devicelayout_controls/results.toml"

_spec = importlib.util.spec_from_file_location(
    "controls_driver", os.path.join(ROOT, "benchmarks/transmon_devicelayout_controls/driver.py")
)
drv = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(drv)


# --------------------------------------------------------------- raw reads
def read(path):
    with open(os.path.join(ROOT, path)) as fh:
        return fh.read()


def modes(cell_dir):
    """(f_q, f_r, p_q): the two lowest modes with junction-port |p| >= 1e-6."""
    eig = read(f"{cell_dir}/eig.csv")
    epr = read(f"{cell_dir}/port-EPR.csv")
    f = [float(l.split(",")[1]) for l in eig.splitlines()[1:] if l.strip()]
    p = [abs(float(l.split(",")[-1])) for l in epr.splitlines()[1:] if l.strip()]
    kept = [(fi, pi) for fi, pi in zip(f, p) if pi >= 1e-6]
    return kept[0][0], kept[1][0], kept[0][1]


def nd_all(cell_dir):
    """Every 'ND (p = k): N' count in the run log, in order (one per solve)."""
    return [int(m) for m in re.findall(r"ND \(p = \d\): (\d+)", read(f"{cell_dir}/palace_run.log"))]


def wall_s(cell_dir):
    t = read(f"{cell_dir}/time.txt")
    m = re.search(r"Elapsed \(wall clock\) time \(h:mm:ss or m:ss\): ([\d:.]+)", t)
    parts = [float(x) for x in m.group(1).split(":")]
    s = 0.0
    for x in parts:
        s = 60 * s + x
    return s


def toml_value(text, section, key):
    """Minimal reader: the value of `key` inside `[section]` of a TOML text."""
    sec = None
    for line in text.splitlines():
        s = line.strip()
        if s.startswith("["):
            sec = s.strip("[]").strip()
            continue
        if sec == section and "=" in s:
            k, v = (x.strip() for x in s.split("=", 1))
            if k == key:
                v = v.split("#", 1)[0].strip() if not v.startswith('"') else v
                return json.loads(v.replace("true", "true").replace("false", "false"))
    raise KeyError(f"{section}.{key}")


def report_design(cell_dir):
    """(capacitor_length, total_length) from the generator report of a design."""
    t = read(f"{cell_dir}/generator_report.txt")
    c = float(re.search(r"cap_length:\s*([\d.]+) um", t).group(1))
    L = float(re.search(r"total_length:\s*([\d.]+) um", t).group(1))
    return c, L


# ------------------------------------------------------------- the rule
def richardson3(f):
    r = (f[0] - f[1]) / (f[1] - f[2])
    assert r > 1.0, f
    p = math.log2(r)
    return p, f[2] + (f[2] - f[1]) / (2.0**p - 1.0)


def richardson2(f, p):
    return f[1] + (f[1] - f[0]) / (2.0**p - 1.0)


def part1_rule(p1, p2):
    """The #1106/#1111 rule: estimate = mean of the order-1 (h/2, h/4) and
    order-2 (h, h/2) series extrapolated in f at order 1; band = [min, max]
    over those, the same series in lambda ~ f^2 at order 1, and the same
    series in f at the order-1 ladder's observed order."""
    p_obs, rich1 = richardson3(p1)
    o1 = richardson2(p1[1:], 1.0)
    o2 = richardson2(p2, 1.0)
    l1 = math.sqrt(2 * p1[2] ** 2 - p1[1] ** 2)
    l2 = math.sqrt(2 * p2[1] ** 2 - p2[0] ** 2)
    rich2 = richardson2(p2, p_obs)
    est = 0.5 * (o1 + o2)
    vals = [o1, o2, l1, l2, rich1, rich2]
    return {
        "observed_order": p_obs,
        "p1_order1": o1,
        "p2_order1": o2,
        "p1_lambda": l1,
        "p2_lambda": l2,
        "p1_observed": rich1,
        "p2_observed": rich2,
        "estimate": est,
        "band": [min(vals), max(vals)],
    }


def shared_limit(a1, a2, b1, b2):
    """Common limit and order of two two-level series (a: order-1 h/2, h/4;
    b: order-2 h, h/2), both f = f_inf - C h^p with the same p: exactly
    determined by the four levels."""
    d1, d2 = a2 - a1, b2 - b1
    r = (d1 - d2) / (b2 - a2)  # 2^p - 1
    return math.log2(1 + r), b2 + d2 / r


# ------------------------------------------------------------- formatting
def num(x, prec):
    return f"{x:.{prec}f}"


def arr(v, prec):
    return "[" + ", ".join(num(x, prec) for x in v) + "]"


def sci(x):
    return f"{x:.3e}"


def strs(v):
    return "[" + ", ".join(json.dumps(s) for s in v) + "]"


def pct(x, ref):
    return 100.0 * (x / ref - 1.0)


def git_head():
    r = subprocess.run(["git", "rev-parse", "--short=8", "HEAD"], capture_output=True, text=True, cwd=ROOT)
    return r.stdout.strip()


# ------------------------------------------------------------- sections
def trajectory(prefix, kind, cells=CELLS, delta=None):
    """Re-run a control's step arithmetic on the committed Palace values.

    `delta` is the scaling law's line-model end correction (#1111's by
    default). Returns the list of (cell, theta, F, err, role) in run order."""
    delta = drv.DELTA_UM if delta is None else delta
    # Run 1 enters at the 6-decimal values the driver (and #1111) used; they
    # must be the committed cell's to rounding.
    raw = modes(f"{X1092}/dl_initial")
    assert abs(raw[0] - drv.DL_INITIAL[0]) < 5e-7 and abs(raw[1] - drv.DL_INITIAL[1]) < 5e-7
    runs = [("dl_initial", drv.INITIAL, list(drv.DL_INITIAL), "initial")]
    if kind == "scaling":
        theta, F = drv.INITIAL, runs[0][2]
        k = 1
        while drv.dl_error(F) >= drv.CRITERION:
            theta = drv.newton_step(theta, F, drv.scaling_jacobian(theta, F, delta))
            cell = f"dl_{prefix}{k}"
            assert os.path.isdir(os.path.join(ROOT, cells, cell)), f"missing cell {cell}"
            F = list(modes(f"{cells}/{cell}")[:2])
            runs.append((cell, theta, F, "step"))
            k += 1
        return runs
    # Broyden.
    x0 = drv.to_x(drv.INITIAL)
    F0 = runs[0][2]
    J = [[0.0, 0.0], [0.0, 0.0]]
    for j in range(2):
        xp = list(x0)
        xp[j] += drv.PROBE
        th = drv.to_theta(xp)
        cell = f"dl_bp{j + 1}"
        Fp = list(modes(f"{CELLS}/{cell}")[:2])
        xq = drv.to_x(th)
        for i in range(2):
            J[i][j] = (Fp[i] - F0[i]) / (xq[j] - x0[j])
        runs.append((cell, th, Fp, f"probe x{j + 1}"))
    theta, F = drv.INITIAL, F0
    k = 1
    while drv.dl_error(F) >= drv.CRITERION:
        new = drv.newton_step(theta, F, J)
        cell = f"dl_br{k}"
        assert os.path.isdir(os.path.join(ROOT, CELLS, cell)), f"missing cell {cell}"
        Fn = list(modes(f"{CELLS}/{cell}")[:2])
        x, xn = drv.to_x(theta), drv.to_x(new)
        dx = [xn[0] - x[0], xn[1] - x[1]]
        dF = [Fn[0] - F[0], Fn[1] - F[1]]
        Jdx = [J[0][0] * dx[0] + J[0][1] * dx[1], J[1][0] * dx[0] + J[1][1] * dx[1]]
        nn = dx[0] ** 2 + dx[1] ** 2
        J = [[J[i][j] + (dF[i] - Jdx[i]) * dx[j] / nn for j in range(2)] for i in range(2)]
        theta, F = new, Fn
        runs.append((cell, theta, F, "step"))
        k += 1
    return runs


def write_control(t, name, runs, description, jacobian, cells=CELLS):
    for cell, theta, _, _ in runs[1:]:
        c, L = report_design(f"{cells}/{cell}")
        assert abs(c - theta[0]) < 5e-4 and abs(L - theta[1]) < 5e-4, (cell, c, L, theta)
    errs = [drv.dl_error(F) for _, _, F, _ in runs]
    n_crit = next(i + 1 for i, e in enumerate(errs) if e < drv.CRITERION)
    first_step = next(i for i, r in enumerate(runs) if r[3] == "step")
    final = runs[n_crit - 1]
    t.append(f"\n[controls.{name}]")
    t.append(f"description = {json.dumps(description)}")
    t.append(f"jacobian = {json.dumps(jacobian)}")
    t.append(f"cells = {strs([r[0] for r in runs])}")
    t.append(f"roles = {strs([r[3] for r in runs])}")
    t.append(f"c_um = {arr([r[1][0] for r in runs], 3)}")
    t.append(f"L_um = {arr([r[1][1] for r in runs], 3)}")
    t.append(f"f_q_ghz = {arr([r[2][0] for r in runs], 6)}")
    t.append(f"f_r_ghz = {arr([r[2][1] for r in runs], 6)}")
    t.append("err = [" + ", ".join(sci(e) for e in errs) + "]  # DeviceLayout's sum((f - t)^2 / t^2) / 2")
    t.append(f"palace_runs_at_criterion = {n_crit}  # the initial run counts as run 1, as #1111 counts")
    t.append(f"first_step_err = {sci(errs[first_step])}  # the first run placed by a Newton/secant step")
    t.append(f"final_um = {arr(final[1], 3)}")
    t.append(f"final_ghz = {arr(final[2], 6)}")
    t.append(
        f"final_minus_published_um = {arr([final[1][0] - drv.FINAL[0], final[1][1] - drv.FINAL[1]], 3)}"
    )
    return n_crit, errs[first_step], final


def main():
    t = []
    opt = read("benchmarks/transmon_devicelayout_optimize/results.toml")
    xr = read("benchmarks/transmon_devicelayout_crosscheck/results.toml")
    loop = read("benchmarks/transmon_loop_inductance/results.toml")
    prima = toml_value(opt, "newton_devicelayout", "prima_palace_runs")
    geode_runs = toml_value(opt, "newton_devicelayout", "palace_runs_at_criterion")
    geode_err = toml_value(opt, "newton_devicelayout", "err")

    t.append("# Generated by `python3 -I benchmarks/transmon_devicelayout_controls/assemble.py` (issue #1123).")
    t.append("# Do NOT edit by hand. Pinned by crates/geode-core/tests/transmon_devicelayout_controls.rs.")
    t.append("# No Burn backend and no new geode solve: every frequency is a Palace run. The one geode-derived input is")
    t.append("# delta_um, #1111's one-time transmission-line calibration (a finite difference of two geode order-1 solves),")
    t.append("# which the scaling-law and geode-gradient Newtons share in their L column (MANIFEST bucket E).")
    t.append("\n[meta]")
    t.append("issue = 1123")
    t.append('measured_date = "2026-10-14"')
    t.append(f'generated_at_commit = "{git_head()}"')
    t.append('generator = "benchmarks/transmon_devicelayout_controls/assemble.py"')
    t.append('palace_commit = "fba6a5b9d17ef09db2907e9c4c6f92d123d131c0"  # v0.17.0-99, reference/palace/docker/Dockerfile, image palace:cpu-1043')
    t.append(f'cells = "{CELLS}"')
    t.append(f'cells_1129 = "{C1129}"  # issue #1129: [controls.scaling_palace], [controls.prima_this_build], [mesh_family.cv2]')
    t.append(f'cells_1135 = "{C1135}"  # issue #1135: [mesh_family.final], [controls.prima_rhobeg0p1], [controls.prima_rhobeg0p4], [controls.prima_rhobeg]')
    t.append(f'cells_1145 = "{C1145}"  # issue #1145 T7: [amr.final]')
    t.append(
        'hosts = "AWS us-east-1, Ubuntu 24.04 arm64: r8g.4xlarge dl1123 (16 vCPU, 123 GiB; the DeviceLayout-discretization '
        'runs and fam_u1_p2), r8g.4xlarge lad1123 (the conv-step ladder) and r8g.8xlarge amr1123 (32 vCPU, 247 GiB; the '
        'adaptive run, fam_u1_p1 and fam_u2_p1)"'
    )
    t.append(
        'hosts_1129 = "AWS us-east-2, Ubuntu 24.04 arm64: r8g.8xlarge w1129 (32 vCPU, 247 GiB), 2026-10-15; the T1/T2 '
        'DeviceLayout-discretization runs on 16 ranks and the T3 cells on the other 16, concurrently; Palace image '
        'palace:cpu-1043 loaded from the #1123 build"'
    )
    t.append(
        'hosts_1135 = "AWS us-east-1, Ubuntu 24.04 arm64: r8g.8xlarge w1135 (32 vCPU, 247 GiB), 2026-10-15; the T4 cells '
        'on 16 ranks, the two T5 PRIMA runs on 8 ranks each, concurrently; Palace image palace:cpu-1043 loaded from the '
        '#1123 build"'
    )

    # ---------------------------------------------------- 1. controls
    t.append("\n[controls]")
    t.append(
        'objective = "Palace at DeviceLayout\'s own discretization (its mesh_order 2 mesh, solver_order 2, its configfile '
        'verbatim), the objective of #1111 and of DeviceLayout\'s PRIMA run"'
    )
    t.append(f"targets_ghz = {arr(drv.TARGETS, 1)}")
    t.append(f"criterion = {sci(drv.CRITERION)}  # run_optimization reltol^2")
    t.append(f"delta_um = {num(drv.DELTA_UM, 3)}  # #1111's transmission-line end correction, reused unchanged")
    t.append(f"broyden_probe_dx = {num(drv.PROBE, 2)}  # forward probe in x1 and in x2")
    t.append(
        'run_counting = "the committed part-1 run at the initial geometry (results_crosscheck_1092/dl_initial) is run 1 '
        'of every method, as #1111 counts it; every later Palace run adds one; a method meets the criterion at the '
        'first run whose err < 1e-4"'
    )
    sc = trajectory("sc", "scaling")
    br = trajectory("br", "broyden")
    n_sc, e_sc, fin_sc = write_control(
        t,
        "scaling",
        sc,
        "Newton in DeviceLayout's variables x1 = sqrt(620/c), x2 = 5000/L whose c column is a scaling law instead of "
        "geode's shape derivative; the L column is the one #1111's geode-gradient Newton uses",
        "c column: dF_q/dx1 = F_q/x1 (f_q ~ 1/sqrt(C_Sigma) ~ x1), no derivative; L column: dF_r/dx2 = -F_r/(L + delta) "
        "dL/dx2, #1111's transmission-line model shared with the geode-gradient Newton, with delta = 215.954 um from "
        "#1111's one-time calibration (one geode finite difference of two order-1 solves, counted in neither method's "
        "runs); off-diagonals 0; re-evaluated at every point from the Palace values",
    )
    n_br, e_br, fin_br = write_control(
        t,
        "broyden",
        br,
        "secant method: one forward finite-difference probe per variable from the initial design gives the full "
        "2x2 Jacobian for the first step; Broyden's good rank-one update after every later run",
        "J0 from the two probes (each a Palace run at DeviceLayout's discretization); "
        "J += (dF - J dx) dx^T / (dx^T dx) after each step; no geode input of any kind",
    )
    rerun = modes(f"{CELLS}/dl_initial_rerun")
    orig = modes(f"{X1092}/dl_initial")
    t.append("\n[controls.build_check]")
    t.append(
        'description = "run 1 of every method is the committed part-1 cell dl_initial (#1092 worker, 32 ranks); '
        'dl_initial_rerun repeats it on this issue\'s DeviceLayout-discretization worker and Palace build (same '
        'commit and Dockerfile, rebuilt; 16 ranks) from a regenerated, byte-identical mesh; the two agree to 6 '
        'decimals (relative differences below 1e-7), not bit for bit"'
    )
    t.append(f"rerun_minus_original_rel = [{sci(rerun[0] / orig[0] - 1)}, {sci(rerun[1] / orig[1] - 1)}]")
    t.append(f"original_ghz = {arr(orig[:2], 6)}")
    t.append(f"rerun_ghz = {arr(rerun[:2], 6)}")
    t.append(f"rerun_minus_original_pct = {arr([pct(rerun[0], orig[0]), pct(rerun[1], orig[1])], 6)}")
    t.append("\n[controls.comparison]")
    t.append('methods = ["PRIMA (DeviceLayout, published)", "Newton, geode-gradient Jacobian (#1111)", "Newton, scaling-law c column (shared line-model L column)", "Broyden from two probes"]')
    t.append(f"palace_runs_at_criterion = [{prima}, {geode_runs}, {n_sc}, {n_br}]")
    t.append(
        f"first_step_err = [nan, {sci(geode_err[1])}, {sci(e_sc)}, {sci(e_br)}]  # PRIMA's trajectory is unpublished"
    )
    t.append(f"geode_gradient_extra_solves = {geode_runs}  # one geode order-1 solve with gradient per Newton point (#1111)")
    t.append(
        "scaling_beats_or_ties_geode_gradient = " + ("true" if n_sc <= geode_runs else "false")
    )

    # delta: the scaling law's first step under other values of the line model's end correction.
    bp2 = modes(f"{CELLS}/dl_bp2")
    th_bp2 = report_design(f"{CELLS}/dl_bp2")
    f0r = drv.DL_INITIAL[1]
    d_palace = (bp2[1] * th_bp2[1] - f0r * drv.INITIAL[1]) / (f0r - bp2[1])
    firsts = []
    for d in (0.0, drv.DELTA_UM, d_palace):
        xx = drv.to_x(drv.INITIAL)
        jr2 = -drv.DL_INITIAL[1] / (drv.INITIAL[1] + d) * (-5000.0 / xx[1] ** 2)
        J = [[drv.DL_INITIAL[0] / xx[0], 0.0], [0.0, jr2]]
        firsts.append(drv.newton_step(drv.INITIAL, list(drv.DL_INITIAL), J))
    t.append("\n[controls.delta_sensitivity]")
    t.append(
        'description = "the L column\'s end correction delta is the only geode-derived input of the scaling-law control '
        '(#1111\'s calibration). The Jacobian is diagonal, so delta moves only the L component of each step, never the '
        'c component the comparison with the shape derivative is about. Shown: the first step under delta = 0, '
        '#1111\'s 215.954 um, and the Palace-only value from the Broyden probe dl_bp2 by the same f_r (L + delta) = '
        'const model; not run"'
    )
    t.append(f"delta_palace_dl_bp2_um = {num(d_palace, 3)}")
    t.append("deltas_um = [0.0, " + num(drv.DELTA_UM, 3) + ", " + num(d_palace, 3) + "]")
    t.append(f"first_step_c_um = {arr([f[0] for f in firsts], 3)}")
    t.append(f"first_step_L_um = {arr([f[1] for f in firsts], 3)}")

    # ---------------------------------------------------- 1a. #1129 controls
    n_sp, e_sp, n_pr = controls_1129(t, d_palace, prima, geode_runs, geode_err, (n_sc, e_sc), (n_br, e_br))

    # ---------------------------------------------------- 1b. conv step
    J = drv.scaling_jacobian(drv.FINAL, drv.CONV_FINAL)
    to = drv.newton_step(drv.FINAL, drv.CONV_FINAL, J)
    lad = "sccv1"
    p1q, p1r, p2q, p2r = [], [], [], []
    for cell, (q, r) in (
        (f"{lad}_h_p1", (p1q, p1r)),
        (f"{lad}_r1_p1", (p1q, p1r)),
        (f"{lad}_r2_p1", (p1q, p1r)),
        (f"{lad}_h_p2", (p2q, p2r)),
        (f"{lad}_r1_p2", (p2q, p2r)),
    ):
        fq, fr, _ = modes(f"{CELLS}/{cell}")
        q.append(fq)
        r.append(fr)
    c_um, L_um = report_design(f"{CELLS}/{lad}_h_p1")
    assert abs(c_um - to[0]) < 5e-4 and abs(L_um - to[1]) < 5e-4
    rq, rr = part1_rule(p1q, p2q), part1_rule(p1r, p2r)
    cv1_from = toml_value(opt, "conv.cv1", "to_um")
    cv2_to = toml_value(opt, "conv.cv2", "to_um")
    cv1_q = toml_value(opt, "conv.cv1.ladder", "f_q_ghz")
    cv1_r = toml_value(opt, "conv.cv1.ladder", "f_r_ghz")
    est = [rq["estimate"], rr["estimate"]]
    t.append("\n[conv_step.scaling]")
    t.append(
        'description = "one scaling-law Newton step (the [controls.scaling] Jacobian) from DeviceLayout\'s published final '
        'design toward the targets on the converged frequencies (the part-1 estimate there), evaluated by the Palace half '
        'of #1111\'s ladder (order 1 at h, h/2, h/4; order 2 at h, h/2; PEC box) under the part-1 rule"'
    )
    t.append(f"from_um = {arr(drv.FINAL, 3)}")
    t.append(f"from_converged_ghz = {arr(drv.CONV_FINAL, 4)}")
    t.append(f"to_um = {arr(to, 3)}")
    t.append(f"to_minus_published_final_um = {arr([to[0] - drv.FINAL[0], to[1] - drv.FINAL[1]], 3)}")
    t.append(f"geode_gradient_first_step_um = {arr(cv1_from, 3)}  # #1111 cv1")
    t.append(f"geode_gradient_second_step_um = {arr(cv2_to, 3)}  # #1111 cv2")
    t.append(f"to_minus_cv2_um = {arr([to[0] - cv2_to[0], to[1] - cv2_to[1]], 3)}")
    t.append(f"p1_f_q_ghz = {arr(p1q, 6)}  # h, h/2, h/4")
    t.append(f"p1_f_r_ghz = {arr(p1r, 6)}")
    t.append(f"p2_f_q_ghz = {arr(p2q, 6)}  # h, h/2")
    t.append(f"p2_f_r_ghz = {arr(p2r, 6)}")
    t.append(f"f_q_ghz = {num(rq['estimate'], 4)}")
    t.append(f"f_q_band_ghz = {arr(rq['band'], 4)}")
    t.append(f"f_r_ghz = {num(rr['estimate'], 4)}")
    t.append(f"f_r_band_ghz = {arr(rr['band'], 4)}")
    t.append(f"over_target_pct = {arr([pct(est[0], 3.0), pct(est[1], 4.0)], 3)}")
    t.append(f"geode_gradient_first_step_over_target_pct = {arr([pct(cv1_q, 3.0), pct(cv1_r, 4.0)], 3)}  # #1111 cv1 ladder")
    t.append(f"err = {sci(drv.dl_error(est))}  # DeviceLayout's measure on the converged pair")
    t.append(
        "targets_in_band = ["
        + ", ".join(
            "true" if b[0] <= tv <= b[1] else "false" for b, tv in ((rq["band"], 3.0), (rr["band"], 4.0))
        )
        + "]"
    )
    sl_q = shared_limit(p1q[1], p1q[2], p2q[0], p2q[1])
    t.append(f"shared_limit_f_q_ghz = {num(sl_q[1], 4)}  # order {num(sl_q[0], 3)}")

    # ---------------------------------------------------- 2. adaptive refinement
    amr = f"{CELLS}/amr_initial_p2"
    iters = sorted(int(d[len("iteration"):]) for d in os.listdir(os.path.join(ROOT, amr)) if d.startswith("iteration"))
    assert iters == list(range(1, len(iters) + 1)), iters
    dirs = [f"{amr}/iteration{i:02d}" for i in iters] + [amr]
    ind = []
    aq, ar, an = [], [], []
    for d in dirs:
        fq, fr, _ = modes(d)
        aq.append(fq)
        ar.append(fr)
        e = read(f"{d}/error-indicators.csv").splitlines()[1]
        ind.append(float(e.split(",")[0]))
    an = nd_all(amr)
    assert len(an) == len(dirs), (len(an), len(dirs))
    conv = toml_value(xr, "convergence.initial", "f_q_ghz")
    band = toml_value(xr, "convergence.initial", "f_q_band_ghz")
    cap = toml_value(xr, "static_cross_check.initial", "circuit_cap_f_q_ghz")
    convr = toml_value(xr, "convergence.initial", "f_r_ghz")
    bandr = toml_value(xr, "convergence.initial", "f_r_band_ghz")
    p1 = toml_value(xr, "convergence.initial", "p1_f_q_ghz")
    p2 = toml_value(xr, "convergence.initial", "p2_f_q_ghz")
    shared_init = shared_limit(p1[1], p1[2], p2[0], p2[1])
    last = aq[-1]
    t.append("\n[amr.initial]")
    t.append(
        'description = "Palace adaptive mesh refinement on the INITIAL geometry (620 / 5000 um): the DeviceLayout '
        'order-1 mesh h (initial_o1_v22.msh, the ladder\'s h), solver order 2, PEC box, sapphire tensor, the same '
        'AMS/GMRES eigen setup as the ladder\'s order-2 cells; Model.Refinement: MaxIts 12, MaxSize 1e7, Tol 1e-6 '
        '(never reached), UpdateFraction 0.7 (Doerfler), nonconformal (Palace defaults otherwise); Eigenmode N = 2, '
        'so the error indicator sums the qubit and resonator modes"'
    )
    t.append(f"iterations = {len(dirs)}  # solves, the unrefined h mesh first")
    t.append(f"nd = [{', '.join(str(n) for n in an)}]")
    t.append(f"f_q_ghz = {arr(aq, 6)}")
    t.append(f"f_r_ghz = {arr(ar, 6)}")
    t.append(f"error_indicator_norm = [{', '.join(sci(x) for x in ind)}]")
    t.append(f"last_change_f_q_pct = {num(pct(aq[-1], aq[-2]), 4)}  # last solve over the one before")
    t.append(f"last_change_f_r_pct = {num(pct(ar[-1], ar[-2]), 4)}")
    t.append(f"estimate_f_q_ghz = {num(conv, 4)}  # part-1 extrapolation of the uniform ladders")
    t.append(f"estimate_f_q_band_ghz = {arr(band, 4)}")
    t.append(f"shared_limit_f_q_ghz = {num(shared_init[1], 4)}  # order {num(shared_init[0], 3)}")
    t.append(f"circuit_cap_f_q_ghz = {num(cap, 4)}")
    t.append(f"final_minus_estimate_pct = {num(pct(last, conv), 3)}")
    t.append(f"final_minus_cap_pct = {num(pct(last, cap), 3)}")
    t.append(f"final_minus_shared_limit_pct = {num(pct(last, shared_init[1]), 3)}")
    t.append(f"final_in_estimate_band = {'true' if band[0] <= last <= band[1] else 'false'}")
    t.append(f"final_below_cap = {'true' if last <= cap else 'false'}")
    t.append(f"estimate_f_r_ghz = {num(convr, 4)}")
    t.append(f"estimate_f_r_band_ghz = {arr(bandr, 4)}")
    t.append(f"final_r_minus_estimate_pct = {num(pct(ar[-1], convr), 3)}")
    t.append(f"final_r_in_estimate_band = {'true' if bandr[0] <= ar[-1] <= bandr[1] else 'false'}")
    w = list(aq[-3:])
    w[2] *= 1.001
    amr_sens = dof_extrapolation(an[-3:], w)[1] - dof_extrapolation(an[-3:], aq[-3:])[1]
    t.append(f"dof_extrapolation_f_q_sensitivity_ghz_per_0p1pct_finest = {num(amr_sens, 4)}  # exactly determined like the shared limit: an estimate, not a bound")
    for lab, seq in (("q", aq), ("r", ar)):
        alpha, lim = dof_extrapolation(an[-3:], seq[-3:])
        t.append(f"dof_extrapolation_alpha_{lab} = {num(alpha, 4)}  # f = f_inf - C N^-alpha through the last three solves")
        t.append(f"dof_extrapolation_f_{lab}_ghz = {num(lim, 4)}")
    t.append(f"wall_s = {num(wall_s(amr), 1)}")
    t.append(f"peak_host_used_gib = {num(max_mem_gib(f'{CELLS}/logs/mem_amr1123.log', 'amr_initial_p2'), 1)}  # mem.log sampler, while this cell ran")

    # ---------------------------------------------------- 3. mesh family
    fam = {}
    for name in ("fam_u1_p1", "fam_u2_p1", "fam_u1_p2"):
        fam[name] = modes(f"{CELLS}/{name}")
    gq = toml_value(loop, "convergence", "global_f_q_ghz")  # fixture: h, Palace u1, gmsh h/2, p2
    gr = toml_value(loop, "convergence", "global_f_r_ghz")
    gnd = toml_value(loop, "convergence", "global_nd")
    # Re-read the fixture cells themselves.
    fx = {
        "h": modes(f"{L1079}/h_L14p86"),
        "palace_u1": modes(f"{L1079}/u1_L14p86"),
        "gmsh_h2": modes(f"{C1078}/h2_L14p86"),
        "p2": modes(f"{L1079}/p2_L14p86"),
    }
    for (k, v), g in zip(fx.items(), gq):
        assert abs(v[0] - g) < 5e-7, (k, v, g)
    dl_h = modes(f"{X1092}/initial_h_p1")
    dl_r1 = modes(f"{X1092}/initial_r1_p1")
    dl_r2 = modes(f"{X1092}/initial_r2_p1")
    dl_p2h = modes(f"{X1092}/initial_h_p2")
    dl_p2r1 = modes(f"{X1092}/initial_r1_p2")
    h2 = {
        "dl_gmsh": dl_r1[0],
        "dl_palace": fam["fam_u1_p1"][0],
        "fixture_gmsh": fx["gmsh_h2"][0],
        "fixture_palace": fx["palace_u1"][0],
    }
    rule_a_q = part1_rule([dl_h[0], dl_r1[0], dl_r2[0]], [dl_p2h[0], dl_p2r1[0]])
    rule_a_r = part1_rule([dl_h[1], dl_r1[1], dl_r2[1]], [dl_p2h[1], dl_p2r1[1]])
    rule_b_q = part1_rule([dl_h[0], fam["fam_u1_p1"][0], fam["fam_u2_p1"][0]], [dl_p2h[0], fam["fam_u1_p2"][0]])
    rule_b_r = part1_rule([dl_h[1], fam["fam_u1_p1"][1], fam["fam_u2_p1"][1]], [dl_p2h[1], fam["fam_u1_p2"][1]])
    assert abs(rule_a_q["estimate"] - conv) < 5e-5, (rule_a_q["estimate"], conv)
    spread_q = pct(rule_b_q["estimate"], rule_a_q["estimate"])
    spread_r = pct(rule_b_r["estimate"], rule_a_r["estimate"])
    union_q = [min(rule_a_q["band"][0], rule_b_q["band"][0]), max(rule_a_q["band"][1], rule_b_q["band"][1])]
    union_r = [min(rule_a_r["band"][0], rule_b_r["band"][0]), max(rule_a_r["band"][1], rule_b_r["band"][1])]
    t.append("\n[mesh_family.initial]")
    t.append(
        'description = "the initial geometry\'s h/2 qubit level from two h meshes (the DeviceLayout mesh of #1092 and '
        'the earlier generator\'s fixture transmon_smoke.msh) times two uniform refinements (gmsh -refine, which splits '
        'every tet into 8; Palace\'s UniformLevels 1, which first bisects the h elements at under-resolved interior '
        'boundaries, the metal sheet, and then refines), all Palace order 1; and the part-1 rule applied to two '
        'refinement families of the DeviceLayout mesh"'
    )
    t.append('h2_variants = ["DeviceLayout mesh, gmsh refinement (#1092 ladder)", "DeviceLayout mesh, Palace refinement", "fixture, gmsh refinement (#1078)", "fixture, Palace refinement (#1079)"]')
    t.append(f"h2_cells = {strs([f'{X1092}/initial_r1_p1', f'{CELLS}/fam_u1_p1', f'{C1078}/h2_L14p86', f'{L1079}/u1_L14p86'])}")
    t.append(f"h2_f_q_ghz = {arr(list(h2.values()), 6)}")
    t.append(f"h2_nd = [{nd_all(f'{X1092}/initial_r1_p1')[-1]}, {nd_all(f'{CELLS}/fam_u1_p1')[-1]}, {gnd[2]}, {gnd[1]}]")
    t.append(f"h_f_q_ghz = {arr([dl_h[0], fx['h'][0]], 6)}  # DeviceLayout mesh, fixture")
    t.append(f"fixture_over_dl_h_pct = {num(pct(fx['h'][0], dl_h[0]), 4)}")
    t.append(f"published_spread_pct = {num(pct(h2['fixture_palace'], h2['dl_gmsh']), 3)}  # the ~2 % of the #1111 geometry caveat: fixture/Palace over DL/gmsh")
    t.append(f"refinement_scheme_pct = {arr([pct(h2['dl_palace'], h2['dl_gmsh']), pct(h2['fixture_palace'], h2['fixture_gmsh'])], 3)}  # Palace over gmsh, on each h mesh")
    t.append(f"mesh_family_pct = {arr([pct(h2['fixture_gmsh'], h2['dl_gmsh']), pct(h2['fixture_palace'], h2['dl_palace'])], 3)}  # fixture over DL, under each refinement")
    t.append(f"family_a_levels_f_q_ghz = {arr([dl_h[0], dl_r1[0], dl_r2[0], dl_p2h[0], dl_p2r1[0]], 6)}  # gmsh refinement: p1 h, h/2, h/4; p2 h, h/2 (the #1092 ladder)")
    t.append(f"family_b_levels_f_q_ghz = {arr([dl_h[0], fam['fam_u1_p1'][0], fam['fam_u2_p1'][0], dl_p2h[0], fam['fam_u1_p2'][0]], 6)}  # Palace refinement of the same h mesh")
    t.append(f"family_b_levels_f_r_ghz = {arr([dl_h[1], fam['fam_u1_p1'][1], fam['fam_u2_p1'][1], dl_p2h[1], fam['fam_u1_p2'][1]], 6)}")
    t.append(f"family_b_nd = [{nd_all(f'{X1092}/initial_h_p1')[-1]}, {nd_all(f'{CELLS}/fam_u1_p1')[-1]}, {nd_all(f'{CELLS}/fam_u2_p1')[-1]}, {nd_all(f'{X1092}/initial_h_p2')[-1]}, {nd_all(f'{CELLS}/fam_u1_p2')[-1]}]")
    t.append(f"family_a_f_q_ghz = {num(rule_a_q['estimate'], 4)}")
    t.append(f"family_a_f_q_band_ghz = {arr(rule_a_q['band'], 4)}")
    t.append(f"family_b_f_q_ghz = {num(rule_b_q['estimate'], 4)}")
    t.append(f"family_b_f_q_band_ghz = {arr(rule_b_q['band'], 4)}")
    t.append(f"family_b_observed_order_q = {num(rule_b_q['observed_order'], 4)}")
    t.append(f"family_a_f_r_ghz = {num(rule_a_r['estimate'], 4)}")
    t.append(f"family_a_f_r_band_ghz = {arr(rule_a_r['band'], 4)}")
    t.append(f"family_b_f_r_ghz = {num(rule_b_r['estimate'], 4)}")
    t.append(f"family_b_f_r_band_ghz = {arr(rule_b_r['band'], 4)}")
    t.append(f"estimate_spread_pct = {arr([spread_q, spread_r], 3)}  # family b over family a, qubit and resonator")
    t.append(f"union_f_q_band_ghz = {arr(union_q, 4)}")
    t.append(f"union_f_r_band_ghz = {arr(union_r, 4)}")
    rng = [min(rule_a_q["estimate"], rule_b_q["estimate"]), max(rule_a_q["estimate"], rule_b_q["estimate"])]
    amr_x = dof_extrapolation(an[-3:], aq[-3:])[1]
    t.append(f"estimate_range_f_q_ghz = {arr(rng, 4)}  # the two families' estimates")
    t.append(
        "family_b_band_inside_a = "
        + ("true" if rule_a_q["band"][0] <= rule_b_q["band"][0] and rule_b_q["band"][1] <= rule_a_q["band"][1] else "false")
    )
    t.append(f"amr_dof_extrapolation_f_q_ghz = {num(amr_x, 4)}")
    t.append(f"amr_dof_extrapolation_in_range = {'true' if rng[0] <= amr_x <= rng[1] else 'false'}")
    t.append(
        "all_estimates_below_cap = "
        + ("true" if max(rule_a_q["estimate"], rule_b_q["estimate"], last) <= cap else "false")
        + "  # family a, family b and the adaptive run's finest computed level; the DOF extrapolation is left out (its margin is inside its conditioning)"
    )
    t.append(f"amr_final_f_q_ghz = {num(last, 4)}")
    t.append(f"amr_final_in_union_band = {'true' if union_q[0] <= last <= union_q[1] else 'false'}")
    t.append(PROPOSAL)

    mesh_family_cv2(t, opt)
    fam_final = mesh_family_final(t, xr)
    prima_rhobeg(t, n_pr)
    amr_final(t, fam_final)

    # ---------------------------------------------------- 4. the cap
    lj, cj, csig = 14.860e-9, 5.5e-15, toml_value(xr, "static_cross_check.initial", "c_sigma_bracket_ff")
    cap_re = 1.0 / (2 * math.pi * math.sqrt(lj * (csig[0] * 1e-15 + cj))) / 1e9
    assert abs(cap_re - cap) < 1e-4, (cap_re, cap)
    t.append("\n[cap.initial]")
    t.append('tier = "model-dependent: a rigorous consequence of the certified C_Sigma bracket only under the lumped two-node circuit model; not a certified bound on the full-wave eigenfrequency"')
    t.append(f"f_q_ghz = {num(cap_re, 4)}  # 1/(2 pi sqrt(L_J (C_Sigma_lower + C_J)))")
    t.append("l_j_nh = 14.860  # the junction lumped element of DeviceLayout's configfile (LumpedPort L = 1.486e-8)")
    t.append("c_j_ff = 5.5  # the same element's C = 5.5e-15")
    t.append(f"c_sigma_bracket_ff = {arr(csig, 2)}  # benchmarks/transmon_diffopt/c_sigma_bracket_tensor.toml [bracket_x0_tensor]; the lower end enters")
    t.append('c_sigma_definition = "electrostatic island capacitance with the feedline floating and neutral (C_ii - C_if^2/C_ff), the resonator at ground through its short, sapphire lab tensor, on the meshed polygonal geometry of transmon_smoke.msh"')
    t.append("l_s_nh_used = 0.0  # the series loop inductance band [0.113, 0.125] nH (DEVICELAYOUT_LOOP_INDUCTANCE_BAND_H, #1079) only lowers the qubit")
    t.append("k_used = 0.0  # the coupling band [0.0227, 0.0310] (DEVICELAYOUT_COUPLING_K_BAND, #1078) only lowers the qubit (the resonator is above it)")
    t.append(f"dressed_f_q_band_ghz = {arr(toml_value(xr, 'static_cross_check.initial', 'dressed_f_q_band_ghz'), 4)}  # with the L_s and k bands: the static-route estimate")
    t.append(CAP_ASSUMPTIONS)
    # Shared limits for every committed series, and their conditioning.
    series = {
        "initial_q": (p1[1], p1[2], p2[0], p2[1]),
        "initial_r": tuple(toml_value(xr, "convergence.initial", "p1_f_r_ghz")[1:]) + tuple(toml_value(xr, "convergence.initial", "p2_f_r_ghz")),
        "final_q": tuple(toml_value(xr, "convergence.final", "p1_f_q_ghz")[1:]) + tuple(toml_value(xr, "convergence.final", "p2_f_q_ghz")),
        "final_r": tuple(toml_value(xr, "convergence.final", "p1_f_r_ghz")[1:]) + tuple(toml_value(xr, "convergence.final", "p2_f_r_ghz")),
        "cv2_q": tuple(toml_value(opt, "conv.cv2.ladder", "p1_f_q_ghz")[1:]) + tuple(toml_value(opt, "conv.cv2.ladder", "p2_f_q_ghz")),
        "cv2_r": tuple(toml_value(opt, "conv.cv2.ladder", "p1_f_r_ghz")[1:]) + tuple(toml_value(opt, "conv.cv2.ladder", "p2_f_r_ghz")),
        "sccv1_q": (p1q[1], p1q[2], p2q[0], p2q[1]),
        "sccv1_r": (p1r[1], p1r[2], p2r[0], p2r[1]),
        "family_b_initial_q": (fam["fam_u1_p1"][0], fam["fam_u2_p1"][0], dl_p2h[0], fam["fam_u1_p2"][0]),
    }
    t.append("\n[shared_limit]")
    t.append('construction = "order-1 (h/2, h/4) and order-2 (h, h/2) series fitted as f_inf - C_k h^p with one common f_inf and p: four levels, four unknowns, exactly determined (the #1111 cv2 diagnostic)"')
    t.append(f"series = {strs(list(series))}")
    sls = [shared_limit(*v) for v in series.values()]
    t.append(f"order = {arr([s[0] for s in sls], 4)}")
    t.append(f"f_ghz = {arr([s[1] for s in sls], 4)}")
    # Sensitivity of the initial-qubit shared limit to a +0.1 % change of each level.
    base = series["initial_q"]
    sens = []
    for i in range(4):
        w = list(base)
        w[i] *= 1.001
        sens.append(shared_limit(*w)[1] - shared_init[1])
    t.append(f"initial_q_sensitivity_ghz_per_0p1pct = {arr(sens, 4)}  # d f_inf for +0.1 % on p1 h/2, p1 h/4, p2 h, p2 h/2")
    t.append(f"initial_q_excess_over_cap_ghz = {num(shared_init[1] - cap, 4)}")
    need = (shared_init[1] - cap) / abs(sens[3]) * 0.1
    t.append(f"initial_q_excess_as_p2_h2_change_pct = {num(need, 4)}  # linear estimate from the sensitivity")
    lo, hi = 0.0, 1.0  # exact: the relative decrease of p2 h/2 that brings the shared limit down to the cap
    for _ in range(200):
        mid = 0.5 * (lo + hi)
        w = list(base)
        w[3] *= 1 - mid / 100
        if shared_limit(*w)[1] > cap:
            lo = mid
        else:
            hi = mid
    t.append(f"initial_q_excess_as_p2_h2_change_pct_exact = {num(0.5 * (lo + hi), 4)}")
    t.append(WHY_ABOVE_CAP)

    with open(os.path.join(ROOT, OUT), "w") as fh:
        fh.write("\n".join(t) + "\n")
    print(OUT)


C1129 = "reference/fixtures/transmon_palace/results_controls_1129"
O1092 = "reference/fixtures/transmon_palace/results_optimize_1092"


def prima_runs(cells=None, prefix="pr", log="logs/prima_log.jsonl"):
    """The #1129 PRIMA run on this build (or, with cells/prefix/log, one of
    the #1135 trust-radius re-runs), from its log, each run checked
    against its cell: the design is DeviceLayout's transform of the logged x
    (bit for bit), the mesh was generated at that design (generator report),
    and the logged frequencies are the cell's."""
    cells = cells or C1129
    runs = []
    with open(os.path.join(ROOT, cells, log)) as fh:
        for line in fh:
            if line.strip():
                runs.append(json.loads(line))
    for i, r in enumerate(runs):
        assert r["run"] == i + 1, r
        x1, x2 = (float(v) for v in r["x"])
        c, L = float(r["c_um"]), float(r["L_um"])
        assert c == (1 / (x1 * x1)) * 620.0 and L == (1 / x2) * 5000.0, r
        assert 0.6 <= x1 <= 1.4 and 0.6 <= x2 <= 1.4, r
        if r["cell"] == "dl_initial":
            assert (x1, x2) == (1.0, 1.0)
            cell_dir = f"{X1092}/dl_initial"
        else:
            assert r["cell"] == f"dl_{prefix}{i + 1}", r
            cell_dir = f"{cells}/{r['cell']}"
            assert report_design(cell_dir) == (c, L), (r, report_design(cell_dir))
        f = modes(cell_dir)[:2]
        assert list(f) == r["f_ghz"], (r, f)
        assert abs(drv.dl_error(f) - r["err"]) <= 1e-15 * max(1.0, r["err"]), r
        r["dir"] = cell_dir
        r["xf"] = (x1, x2)
        r["theta"] = (c, L)
        r["F"] = list(f)
    return runs


def controls_1129(t, d_palace, prima, geode_runs, geode_err, sc, br):
    """T2 (the scaling-law Newton with the Palace-only delta) and T1 (PRIMA on
    this build) of issue #1129, and the six-method comparison."""
    assert abs(drv.palace_delta(ROOT) - d_palace) < 1e-9
    sp = trajectory("sp", "scaling", cells=C1129, delta=d_palace)
    n_sp, e_sp, fin_sp = write_control(
        t,
        "scaling_palace",
        sp,
        "issue #1129 T2: the [controls.scaling] Newton with the L column's end correction taken from Palace runs "
        "only (delta_palace_dl_bp2_um of [controls.delta_sensitivity]); no geode input of any kind",
        "c column: dF_q/dx1 = F_q/x1; L column: dF_r/dx2 = -F_r/(L + delta) dL/dx2 with delta = 563.879 um from the "
        "Palace runs dl_initial and dl_bp2 by f_r (L + delta) = const; off-diagonals 0; re-evaluated at every point "
        "from the Palace values",
        cells=C1129,
    )
    t.append(f"delta_um = {num(d_palace, 3)}")
    t.append(f"first_step_um = {arr(sp[1][1], 3)}  # [controls.delta_sensitivity] first_step_*_um[2]")

    pr = prima_runs()
    with open(os.path.join(ROOT, C1129, "logs/prima_summary.json")) as fh:
        summ = json.load(fh)
    errs = [r["err"] for r in pr]
    below = [i + 1 for i, e in enumerate(errs) if e < drv.CRITERION]
    n_pr = below[0]
    assert n_pr == len(pr) == summ["nf"], (n_pr, len(pr), summ["nf"])
    assert summ["status"] == "FTARGET_ACHIEVED", summ
    best = min(range(len(pr)), key=lambda i: errs[i])
    fin = pr[n_pr - 1]
    pub = drv.FINAL
    t.append("\n[controls.prima_this_build]")
    t.append(
        'description = "issue #1129 T1: DeviceLayout\'s run_optimization re-run with this repository\'s Palace build: '
        "the same PRIMA call (prima(errfunc, [1.0, 1.0]; ftarget = reltol^2 = 1e-4, xl = [0.6, 0.6], xu = [1.4, 1.4], "
        "rhobeg = 0.2), which with bounds only is BOBYQA, PRIMA.jl 0.2.4 / PRIMA_jll 0.7.1 from the "
        "reference/julia/devicelayout environment) on the same objective (sum((f - t)^2 / t^2) / 2 with "
        "cap_length = 620 um / x1^2, total_length = 5000 um / x2, exact), every point a Palace run at DeviceLayout's "
        "discretization like the Newton controls; the initial point is the committed dl_initial (run 1)\""
    )
    t.append('driver = "benchmarks/transmon_devicelayout_controls/prima_driver.jl"')
    t.append(f"cells = {strs([r['cell'] for r in pr])}")
    t.append(f"x1 = [{', '.join(repr(r['xf'][0]) for r in pr)}]")
    t.append(f"x2 = [{', '.join(repr(r['xf'][1]) for r in pr)}]")
    t.append(f"c_um = {arr([r['theta'][0] for r in pr], 3)}")
    t.append(f"L_um = {arr([r['theta'][1] for r in pr], 3)}")
    t.append(f"f_q_ghz = {arr([r['F'][0] for r in pr], 6)}")
    t.append(f"f_r_ghz = {arr([r['F'][1] for r in pr], 6)}")
    t.append("err = [" + ", ".join(sci(e) for e in errs) + "]  # DeviceLayout's sum((f - t)^2 / t^2) / 2")
    t.append(f"palace_runs_at_criterion = {n_pr}  # PRIMA's info.nf; the initial run counts as run 1, as #1111 counts")
    t.append(f"prima_nf = {summ['nf']}")
    t.append(f"prima_status = {json.dumps(summ['status'])}")
    t.append(f"published_palace_runs = {prima}  # DeviceLayout's build (blog / example)")
    t.append("first_model_step_run = 6  # runs 1-5 are BOBYQA's initial interpolation set x0, x0 +- rhobeg e_i")
    t.append(f"first_model_step_err = {sci(errs[5]) if len(errs) > 5 else 'nan'}")
    t.append(f"final_um = {arr(fin['theta'], 3)}")
    t.append(f"final_ghz = {arr(fin['F'], 6)}")
    t.append(f"final_minus_published_um = {arr([fin['theta'][0] - pub[0], fin['theta'][1] - pub[1]], 3)}")
    t.append(f"best_err = {sci(errs[best])}")
    # The published final design on this build: the #1092 part-1 cell dl_final.
    pf = modes(f"{X1092}/dl_final")[:2]
    t.append(f"published_final_this_build_ghz = {arr(pf, 6)}  # results_crosscheck_1092/dl_final")
    t.append(f"published_final_this_build_err = {sci(drv.dl_error(pf))}  # already below the criterion on this build")

    n_sc, e_sc = sc
    n_br, e_br = br
    t.append("\n[controls.comparison_1129]")
    t.append(
        'methods = ["PRIMA (DeviceLayout, published)", "PRIMA (DeviceLayout run_optimization, this build)", '
        '"Newton, geode-gradient Jacobian (#1111)", "Newton, scaling-law c column, #1111 delta", '
        '"Newton, scaling-law c column, Palace-only delta", "Broyden from two probes"]'
    )
    t.append(f"palace_runs_at_criterion = [{prima}, {n_pr}, {geode_runs}, {n_sc}, {n_sp}, {n_br}]")
    t.append(
        f"first_step_err = [nan, nan, {sci(geode_err[1])}, {sci(e_sc)}, {sci(e_sp)}, {sci(e_br)}]  "
        "# PRIMA builds a model from five runs first, so it has no comparable first step"
    )
    t.append("uses_geode_input = [false, false, true, true, false, false]  # #1111's shape gradients, or its geode-calibrated delta")
    t.append(f"prima_this_build_minus_published_runs = {n_pr - prima}")
    return n_sp, e_sp, n_pr


def mesh_family_cv2(t, opt):
    """T3 of issue #1129: the second refinement family (Palace UniformLevels
    of the same h mesh) on #1111's step-2 design cv2, as [mesh_family.initial]
    did on the initial geometry, and what each family estimate implies for
    the converged design's capacitor length."""
    design = toml_value(opt, "conv.cv2.ladder", "design_um")
    a = {k: modes(f"{O1092}/cv2_{k}") for k in ("h_p1", "r1_p1", "r2_p1", "h_p2", "r1_p2")}
    b = {k: modes(f"{C1129}/famcv2_{k}") for k in ("u1_p1", "u2_p1", "u1_p2")}
    for k, (q, r) in (("p1", ("p1_f_q_ghz", "p1_f_r_ghz")), ("p2", ("p2_f_q_ghz", "p2_f_r_ghz"))):
        levels = [a[f"{lv}_{k}"] for lv in (("h", "r1", "r2") if k == "p1" else ("h", "r1"))]
        for i, lv in enumerate(levels):
            assert abs(lv[0] - toml_value(opt, "conv.cv2.ladder", q)[i]) < 5e-7
            assert abs(lv[1] - toml_value(opt, "conv.cv2.ladder", r)[i]) < 5e-7
    fa = {}
    fb = {}
    for j, lab in ((0, "q"), (1, "r")):
        fa[lab] = part1_rule([a["h_p1"][j], a["r1_p1"][j], a["r2_p1"][j]], [a["h_p2"][j], a["r1_p2"][j]])
        fb[lab] = part1_rule([a["h_p1"][j], b["u1_p1"][j], b["u2_p1"][j]], [a["h_p2"][j], b["u1_p2"][j]])
    assert abs(fa["q"]["estimate"] - toml_value(opt, "conv.cv2.ladder", "f_q_ghz")) < 5e-5
    assert abs(fa["r"]["estimate"] - toml_value(opt, "conv.cv2.ladder", "f_r_ghz")) < 5e-5
    pub_c = drv.FINAL[0]

    def c_for_target(f_est):
        # The scaling law of [controls.scaling]: f_q ~ sqrt(620 / c), so the
        # capacitor length that puts this estimate on 3.0 GHz.
        return design[0] * (f_est / drv.TARGETS[0]) ** 2

    t.append("\n[mesh_family.cv2]")
    t.append(
        'description = "issue #1129 T3: the second refinement family on #1111\'s step-2 design cv2 (1446.078 / '
        "7416.352 um), as [mesh_family.initial] did on the initial geometry: family a is #1111's ladder (gmsh -refine "
        "of the DeviceLayout h mesh: order 1 at h, h/2, h/4, order 2 at h, h/2), family b refines the same h mesh "
        "(cv2_o1_v22.msh, regenerated byte-identically) with Palace's UniformLevels 1, 2 instead (the h cells are "
        "shared); both through the part-1 rule. The capacitor lengths are the scaling law f_q ~ sqrt(620/c) "
        "applied to each estimate (and band edge): the c at which that estimate would sit on 3.0 GHz, with "
        'df_q/dL = 0; an estimate, not a run"'
    )
    t.append(f"design_um = {arr(design, 3)}")
    t.append(f"family_b_cells = {strs([f'{C1129}/famcv2_{k}' for k in ('u1_p1', 'u2_p1', 'u1_p2')])}")
    t.append(f"family_a_levels_f_q_ghz = {arr([a[k][0] for k in ('h_p1', 'r1_p1', 'r2_p1', 'h_p2', 'r1_p2')], 6)}  # p1 h, h/2, h/4; p2 h, h/2")
    t.append(f"family_b_levels_f_q_ghz = {arr([a['h_p1'][0], b['u1_p1'][0], b['u2_p1'][0], a['h_p2'][0], b['u1_p2'][0]], 6)}")
    t.append(f"family_a_levels_f_r_ghz = {arr([a[k][1] for k in ('h_p1', 'r1_p1', 'r2_p1', 'h_p2', 'r1_p2')], 6)}")
    t.append(f"family_b_levels_f_r_ghz = {arr([a['h_p1'][1], b['u1_p1'][1], b['u2_p1'][1], a['h_p2'][1], b['u1_p2'][1]], 6)}")
    t.append(
        f"family_b_nd = [{nd_all(f'{O1092}/cv2_h_p1')[-1]}, {nd_all(f'{C1129}/famcv2_u1_p1')[-1]}, "
        f"{nd_all(f'{C1129}/famcv2_u2_p1')[-1]}, {nd_all(f'{O1092}/cv2_h_p2')[-1]}, {nd_all(f'{C1129}/famcv2_u1_p2')[-1]}]"
    )
    t.append(f"refinement_scheme_h2_pct = {num(pct(b['u1_p1'][0], a['r1_p1'][0]), 3)}  # Palace over gmsh refinement, qubit, order 1 at h/2")
    for lab in ("q", "r"):
        for fam, rr in (("a", fa), ("b", fb)):
            t.append(f"family_{fam}_f_{lab}_ghz = {num(rr[lab]['estimate'], 4)}")
            t.append(f"family_{fam}_f_{lab}_band_ghz = {arr(rr[lab]['band'], 4)}")
            t.append(f"family_{fam}_observed_order_{lab} = {num(rr[lab]['observed_order'], 4)}")
    spread = [pct(fb["q"]["estimate"], fa["q"]["estimate"]), pct(fb["r"]["estimate"], fa["r"]["estimate"])]
    t.append(f"estimate_spread_pct = {arr(spread, 3)}  # family b over family a, qubit and resonator")
    uq = [min(fa["q"]["band"][0], fb["q"]["band"][0]), max(fa["q"]["band"][1], fb["q"]["band"][1])]
    ur = [min(fa["r"]["band"][0], fb["r"]["band"][0]), max(fa["r"]["band"][1], fb["r"]["band"][1])]
    t.append(f"union_f_q_band_ghz = {arr(uq, 4)}")
    t.append(f"union_f_r_band_ghz = {arr(ur, 4)}")
    t.append(
        "family_b_band_inside_a = "
        + ("true" if fa["q"]["band"][0] <= fb["q"]["band"][0] and fb["q"]["band"][1] <= fa["q"]["band"][1] else "false")
    )
    t.append(f"estimate_range_f_q_ghz = {arr(sorted([fa['q']['estimate'], fb['q']['estimate']]), 4)}")
    t.append(
        "targets_in_family_b_band = ["
        + ", ".join("true" if bb[0] <= tv <= bb[1] else "false" for bb, tv in ((fb["q"]["band"], 3.0), (fb["r"]["band"], 4.0)))
        + "]"
    )
    sl = shared_limit(b["u1_p1"][0], b["u2_p1"][0], a["h_p2"][0], b["u1_p2"][0])
    t.append(f"family_b_shared_limit_f_q_ghz = {num(sl[1], 4)}  # order {num(sl[0], 3)}")
    ca, cb = c_for_target(fa["q"]["estimate"]), c_for_target(fb["q"]["estimate"])
    t.append(f"c_on_target_um = {arr([ca, cb], 1)}  # family a, family b: the scaling-law c that puts each estimate on 3.0 GHz")
    t.append(f"c_on_target_over_published_pct = {arr([pct(ca, pub_c), pct(cb, pub_c)], 2)}  # vs DeviceLayout's published 1217.678 um")
    t.append(f"design_over_published_pct = {num(pct(design[0], pub_c), 2)}  # cv2 itself: the +19 % of the v12 text")
    cbands = [sorted([c_for_target(fb["q"]["band"][0]), c_for_target(fb["q"]["band"][1])])]
    cbands.append(sorted([c_for_target(uq[0]), c_for_target(uq[1])]))
    t.append(f"family_b_c_band_um = {arr(cbands[0], 1)}  # the family-b qubit band mapped by the same law")
    t.append(f"union_c_band_um = {arr(cbands[1], 1)}")
    t.append(f"family_b_c_band_over_published_pct = {arr([pct(c, pub_c) for c in cbands[0]], 2)}")
    t.append(f"union_c_band_over_published_pct = {arr([pct(c, pub_c) for c in cbands[1]], 2)}")
    t.append(f"wall_s = {arr([wall_s(f'{C1129}/famcv2_{k}') for k in ('u1_p1', 'u2_p1', 'u1_p2')], 1)}  # u1_p1, u2_p1, u1_p2 on 16 ranks")


C1135 = "reference/fixtures/transmon_palace/results_controls_1135"


def mesh_family_final(t, xr):
    """T4 of issue #1135: the second refinement family (Palace UniformLevels
    of the same h mesh) on DeviceLayout's published final design, as
    [mesh_family.initial] and [mesh_family.cv2] did, through the same part-1
    rule. Family a is the #1092 ladder ([convergence.final] of the
    cross-check), whose estimate is the "+8.7 %" of the v13 text."""
    a = {k: modes(f"{X1092}/final_{k}") for k in ("h_p1", "r1_p1", "r2_p1", "h_p2", "r1_p2")}
    b = {k: modes(f"{C1135}/famfinal_{k}") for k in ("u1_p1", "u2_p1", "u1_p2")}
    for k, (q, r) in (("p1", ("p1_f_q_ghz", "p1_f_r_ghz")), ("p2", ("p2_f_q_ghz", "p2_f_r_ghz"))):
        levels = [a[f"{lv}_{k}"] for lv in (("h", "r1", "r2") if k == "p1" else ("h", "r1"))]
        for i, lv in enumerate(levels):
            assert abs(lv[0] - toml_value(xr, "convergence.final", q)[i]) < 5e-7
            assert abs(lv[1] - toml_value(xr, "convergence.final", r)[i]) < 5e-7
    fa, fb = {}, {}
    for j, lab in ((0, "q"), (1, "r")):
        fa[lab] = part1_rule([a["h_p1"][j], a["r1_p1"][j], a["r2_p1"][j]], [a["h_p2"][j], a["r1_p2"][j]])
        fb[lab] = part1_rule([a["h_p1"][j], b["u1_p1"][j], b["u2_p1"][j]], [a["h_p2"][j], b["u1_p2"][j]])
    assert abs(fa["q"]["estimate"] - toml_value(xr, "convergence.final", "f_q_ghz")) < 5e-5
    assert abs(fa["r"]["estimate"] - toml_value(xr, "convergence.final", "f_r_ghz")) < 5e-5
    pub = (toml_value(xr, "convergence.final", "published_f_q_ghz"), toml_value(xr, "convergence.final", "published_f_r_ghz"))
    dl = modes(f"{X1092}/dl_final")
    t.append("\n[mesh_family.final]")
    t.append(
        'description = "issue #1135 T4: the second refinement family on DeviceLayout\'s published final design '
        "(1217.678 / 7243.955 um), as [mesh_family.initial] and [mesh_family.cv2] did: family a is the #1092 ladder "
        "([convergence.final] of the cross-check: gmsh -refine of the DeviceLayout h mesh, order 1 at h, h/2, h/4, "
        "order 2 at h, h/2), family b refines the same h mesh (final_o1_v22.msh, regenerated byte-identically) with "
        "Palace's UniformLevels 1, 2 instead (the h cells are shared); both through the part-1 rule. Family a's "
        'qubit estimate is the +8.7 % over the published 3.005 GHz of the v13 text"'
    )
    t.append(f"design_um = {arr(drv.FINAL, 3)}")
    t.append(f"family_b_cells = {strs([f'{C1135}/famfinal_{k}' for k in ('u1_p1', 'u2_p1', 'u1_p2')])}")
    t.append(f"family_a_levels_f_q_ghz = {arr([a[k][0] for k in ('h_p1', 'r1_p1', 'r2_p1', 'h_p2', 'r1_p2')], 6)}  # p1 h, h/2, h/4; p2 h, h/2")
    t.append(f"family_b_levels_f_q_ghz = {arr([a['h_p1'][0], b['u1_p1'][0], b['u2_p1'][0], a['h_p2'][0], b['u1_p2'][0]], 6)}")
    t.append(f"family_a_levels_f_r_ghz = {arr([a[k][1] for k in ('h_p1', 'r1_p1', 'r2_p1', 'h_p2', 'r1_p2')], 6)}")
    t.append(f"family_b_levels_f_r_ghz = {arr([a['h_p1'][1], b['u1_p1'][1], b['u2_p1'][1], a['h_p2'][1], b['u1_p2'][1]], 6)}")
    t.append(
        f"family_b_nd = [{nd_all(f'{X1092}/final_h_p1')[-1]}, {nd_all(f'{C1135}/famfinal_u1_p1')[-1]}, "
        f"{nd_all(f'{C1135}/famfinal_u2_p1')[-1]}, {nd_all(f'{X1092}/final_h_p2')[-1]}, {nd_all(f'{C1135}/famfinal_u1_p2')[-1]}]"
    )
    t.append(f"refinement_scheme_h2_pct = {num(pct(b['u1_p1'][0], a['r1_p1'][0]), 3)}  # Palace over gmsh refinement, qubit, order 1 at h/2")
    for lab in ("q", "r"):
        for fam, rr in (("a", fa), ("b", fb)):
            t.append(f"family_{fam}_f_{lab}_ghz = {num(rr[lab]['estimate'], 4)}")
            t.append(f"family_{fam}_f_{lab}_band_ghz = {arr(rr[lab]['band'], 4)}")
            t.append(f"family_{fam}_observed_order_{lab} = {num(rr[lab]['observed_order'], 4)}")
    spread = [pct(fb["q"]["estimate"], fa["q"]["estimate"]), pct(fb["r"]["estimate"], fa["r"]["estimate"])]
    t.append(f"estimate_spread_pct = {arr(spread, 3)}  # family b over family a, qubit and resonator")
    uq = [min(fa["q"]["band"][0], fb["q"]["band"][0]), max(fa["q"]["band"][1], fb["q"]["band"][1])]
    ur = [min(fa["r"]["band"][0], fb["r"]["band"][0]), max(fa["r"]["band"][1], fb["r"]["band"][1])]
    t.append(f"union_f_q_band_ghz = {arr(uq, 4)}")
    t.append(f"union_f_r_band_ghz = {arr(ur, 4)}")
    t.append(
        "family_b_band_inside_a = "
        + ("true" if fa["q"]["band"][0] <= fb["q"]["band"][0] and fb["q"]["band"][1] <= fa["q"]["band"][1] else "false")
    )
    t.append(f"estimate_range_f_q_ghz = {arr(sorted([fa['q']['estimate'], fb['q']['estimate']]), 4)}")
    t.append(f"estimate_range_f_r_ghz = {arr(sorted([fa['r']['estimate'], fb['r']['estimate']]), 4)}")
    t.append(f"published_ghz = {arr(pub, 3)}")
    t.append(
        f"f_q_over_published_pct = {arr([pct(fa['q']['estimate'], pub[0]), pct(fb['q']['estimate'], pub[0])], 3)}"
        "  # family a (the +8.7 % of the v13 text), family b"
    )
    t.append(f"f_r_over_published_pct = {arr([pct(fa['r']['estimate'], pub[1]), pct(fb['r']['estimate'], pub[1])], 3)}")
    t.append(f"family_b_f_q_band_over_published_pct = {arr([pct(v, pub[0]) for v in fb['q']['band']], 3)}")
    t.append(f"union_f_q_band_over_published_pct = {arr([pct(v, pub[0]) for v in uq], 3)}")
    t.append(f"union_f_r_band_over_published_pct = {arr([pct(v, pub[1]) for v in ur], 3)}")
    t.append(
        "published_below_family_b_band = ["
        + ", ".join("true" if pv < bb[0] else "false" for pv, bb in ((pub[0], fb["q"]["band"]), (pub[1], fb["r"]["band"])))
        + "]"
    )
    t.append(f"devicelayout_run_ghz = {arr(dl[:2], 6)}  # results_crosscheck_1092/dl_final, this environment")
    t.append(
        f"devicelayout_run_under_family_b_pct = {arr([pct(dl[0], fb['q']['estimate']), pct(dl[1], fb['r']['estimate'])], 3)}"
    )
    sl = shared_limit(b["u1_p1"][0], b["u2_p1"][0], a["h_p2"][0], b["u1_p2"][0])
    t.append(f"family_b_shared_limit_f_q_ghz = {num(sl[1], 4)}  # order {num(sl[0], 3)}")
    t.append(f"family_b_shared_limit_order_q = {num(sl[0], 4)}")
    t.append(f"wall_s = {arr([wall_s(f'{C1135}/famfinal_{k}') for k in ('u1_p1', 'u2_p1', 'u1_p2')], 1)}  # u1_p1, u2_p1, u1_p2 on 16 ranks")
    t.append(
        f"host_peak_gib = {arr([max_mem_gib(f'{C1135}/logs/mem_fam.log', f'famfinal_{k}') for k in ('u1_p1', 'u2_p1', 'u1_p2')], 1)}"
        "  # used host memory while each cell ran (the T5 runs shared the host)"
    )
    return fa, fb, pub, dl


C1145 = "reference/fixtures/transmon_palace/results_controls_1145"


def amr_final(t, fam):
    """T7 of issue #1145: Palace adaptive refinement on DeviceLayout's
    published final design, with the settings of [amr.initial] (#1123): the
    DeviceLayout order-1 h mesh of the final design (final_o1_v22.msh, the
    #1092 ladder's h), solver order 2, PEC box, sapphire tensor, the AMS/GMRES
    eigen setup of the ladder's order-2 cell final_h_p2 (Target 2.0 GHz, as
    that cell), Model.Refinement MaxIts 12, MaxSize 1e7, Tol 1e-6,
    UpdateFraction 0.7, nonconformal; Eigenmode N = 2. The finest level is a
    computed value with no extrapolation; the three-point DOF fit is an
    estimate."""
    fa, fb, pub, dl = fam
    amr = f"{C1145}/amr_final_p2"
    iters = sorted(int(d[len("iteration"):]) for d in os.listdir(os.path.join(ROOT, amr)) if d.startswith("iteration"))
    assert iters == list(range(1, len(iters) + 1)), iters
    dirs = [f"{amr}/iteration{i:02d}" for i in iters] + [amr]
    aq, ar, ind = [], [], []
    for d in dirs:
        fq, fr, _ = modes(d)
        aq.append(fq)
        ar.append(fr)
        e = read(f"{d}/error-indicators.csv").splitlines()[1]
        ind.append(float(e.split(",")[0]))
    an = nd_all(amr)
    assert len(an) == len(dirs), (len(an), len(dirs))
    h = modes(f"{X1092}/final_h_p2")
    assert abs(aq[0] - h[0]) < 1e-5 and abs(ar[0] - h[1]) < 1e-5, (aq[0], h)
    last_q, last_r = aq[-1], ar[-1]
    uq = [min(fa["q"]["band"][0], fb["q"]["band"][0]), max(fa["q"]["band"][1], fb["q"]["band"][1])]
    ur = [min(fa["r"]["band"][0], fb["r"]["band"][0]), max(fa["r"]["band"][1], fb["r"]["band"][1])]
    rq = sorted([fa["q"]["estimate"], fb["q"]["estimate"]])
    rr = sorted([fa["r"]["estimate"], fb["r"]["estimate"]])
    t.append("\n[amr.final]")
    t.append(
        'description = "issue #1145 T7: Palace adaptive mesh refinement on DeviceLayout\'s PUBLISHED FINAL design '
        "(1217.678 / 7243.955 um) with the settings of [amr.initial]: the DeviceLayout order-1 mesh h of the final "
        "design (final_o1_v22.msh, the #1092 ladder's h), solver order 2, PEC box, sapphire tensor, the AMS/GMRES eigen "
        "setup of the ladder's order-2 cell final_h_p2 (Target 2.0 GHz as in that cell); Model.Refinement: MaxIts 12, "
        "MaxSize 1e7, Tol 1e-6, UpdateFraction 0.7 (Doerfler), nonconformal; Eigenmode N = 2, Linear MaxIts 1000. "
        'finest_* is a computed level (no extrapolation); dof_extrapolation_* is an exactly determined fit, an estimate"'
    )
    t.append(f"iterations = {len(dirs)}  # solves, the unrefined h mesh first")
    t.append(f"nd = [{', '.join(str(n) for n in an)}]")
    t.append(f"f_q_ghz = {arr(aq, 6)}")
    t.append(f"f_r_ghz = {arr(ar, 6)}")
    t.append(f"error_indicator_norm = [{', '.join(sci(x) for x in ind)}]")
    t.append(f"step_change_f_q_pct = {arr([pct(aq[i + 1], aq[i]) for i in range(len(aq) - 1)], 4)}  # each solve over the one before")
    t.append(f"step_change_f_r_pct = {arr([pct(ar[i + 1], ar[i]) for i in range(len(ar) - 1)], 4)}")
    t.append(f"finest_f_q_ghz = {num(last_q, 6)}  # the finest computed level: no extrapolation")
    t.append(f"finest_f_r_ghz = {num(last_r, 6)}")
    t.append(f"published_ghz = {arr(pub, 3)}")
    t.append(f"finest_over_published_pct = {arr([pct(last_q, pub[0]), pct(last_r, pub[1])], 3)}")
    t.append(f"devicelayout_run_ghz = {arr(dl[:2], 6)}  # results_crosscheck_1092/dl_final")
    t.append(f"finest_over_devicelayout_run_pct = {arr([pct(last_q, dl[0]), pct(last_r, dl[1])], 3)}")
    t.append(f"family_estimate_range_f_q_ghz = {arr(rq, 4)}  # [mesh_family.final]: families a and b, part-1 rule (estimates)")
    t.append(f"family_estimate_range_f_r_ghz = {arr(rr, 4)}")
    t.append(f"family_union_f_q_band_ghz = {arr(uq, 4)}")
    t.append(f"family_union_f_r_band_ghz = {arr(ur, 4)}")
    t.append(f"finest_minus_family_estimates_f_q_pct = {arr([pct(last_q, v) for v in rq], 3)}")
    t.append(f"finest_minus_family_estimates_f_r_pct = {arr([pct(last_r, v) for v in rr], 3)}")
    t.append(f"finest_in_union_band = [{'true' if uq[0] <= last_q <= uq[1] else 'false'}, {'true' if ur[0] <= last_r <= ur[1] else 'false'}]  # qubit, resonator")
    t.append(f"finest_above_published = [{'true' if last_q > pub[0] else 'false'}, {'true' if last_r > pub[1] else 'false'}]")
    for lab, seq in (("q", aq), ("r", ar)):
        try:
            alpha, lim = dof_extrapolation(an[-3:], seq[-3:])
        except AssertionError:
            t.append(f'dof_extrapolation_f_{lab} = "not defined: the last three solves admit no f_inf - C N^-alpha with 0 < alpha < 4"')
            continue
        note = "  # far below the asymptotic order: not reliable" if alpha < 0.3 else ""
        t.append(f"dof_extrapolation_alpha_{lab} = {num(alpha, 4)}  # f = f_inf - C N^-alpha through the last three solves: an ESTIMATE")
        if note:
            t.append(f"dof_extrapolation_{lab}_reliable = false{note}")
        t.append(f"dof_extrapolation_f_{lab}_ghz = {num(lim, 4)}")
        w = list(seq[-3:])
        w[2] *= 1.001
        try:
            sens = dof_extrapolation(an[-3:], w)[1] - lim
            t.append(f"dof_extrapolation_f_{lab}_sensitivity_ghz_per_0p1pct_finest = {num(sens, 4)}  # exactly determined: ill-conditioned")
        except AssertionError:
            t.append(f'dof_extrapolation_f_{lab}_sensitivity_ghz_per_0p1pct_finest = "not defined"')
    t.append(f"wall_s = {num(wall_s(amr), 1)}  # 24 ranks; the other 8 vCPU ran the #1145 T6 geode solves and gates during the first ~2 h, so not a clean timing")
    t.append(f"peak_host_used_gib = {num(max_mem_gib(f'{C1145}/logs/mem_amr1145.log', 'amr_final_p2'), 1)}  # mem.log sampler, while this cell ran; reached in the last solve, after the T6 runs ended")


# T5 of issue #1135: (rhobeg, cell prefix, log stem) of the trust-radius re-runs.
# "0.4" is the largest rhobeg BOBYQA accepts on DeviceLayout's box,
# (1.4 - 0.6) / 2 = 0.39999999999999997 in binary64: 0.4 itself is one ulp
# above it, and PRIMA resets it to (1.4 - 0.6) / 4, i.e. DeviceLayout's 0.2
# (logs/rhobeg_probe.txt).
RHOBEG_MAX = (1.4 - 0.6) / 2
RHOBEG_RUNS = ((0.1, "pa", "prima_rhobeg0p1"), (RHOBEG_MAX, "pb", "prima_rhobeg0p4"))


def prima_rhobeg(t, n_default):
    """T5 of issue #1135: DeviceLayout's PRIMA call with only rhobeg changed
    (0.1 and 0.4 against its 0.2), every point a Palace run in the same
    environment as [controls.prima_this_build]; the run count to the
    criterion for each."""
    counts, status, finals = [], [], []
    for rb, prefix, stem in RHOBEG_RUNS:
        pr = prima_runs(C1135, prefix, f"logs/{stem}_log.jsonl")
        with open(os.path.join(ROOT, C1135, f"logs/{stem}_summary.json")) as fh:
            summ = json.load(fh)
        assert summ["rhobeg"] == rb and summ["prefix"] == prefix and not summ["replay"], summ
        errs = [r["err"] for r in pr]
        below = [i + 1 for i, e in enumerate(errs) if e < drv.CRITERION]
        assert len(pr) == summ["nf"], (len(pr), summ["nf"])
        reached = summ["status"] == "FTARGET_ACHIEVED"
        assert reached == bool(below), (summ, below)
        n = below[0] if below else None
        if reached:
            assert n == len(pr), (n, len(pr))
        # BOBYQA's initial interpolation set: x0 and x0 +- rhobeg along each axis.
        init = sorted(r["xf"] for r in pr[:5])
        want = sorted([(1.0, 1.0), (1.0 + rb, 1.0), (1.0 - rb, 1.0), (1.0, 1.0 + rb), (1.0, 1.0 - rb)])
        assert all(abs(u - v) < 1e-12 for p, q in zip(init, want) for u, v in zip(p, q)), (init, want)
        fin = pr[-1]
        best = min(range(len(pr)), key=lambda i: errs[i])
        key = stem.replace("prima_", "")
        t.append(f"\n[controls.prima_{key}]")
        t.append(
            f'description = "issue #1135 T5: [controls.prima_this_build] with only PRIMA\'s initial trust radius changed, '
            f"rhobeg = {rb} instead of DeviceLayout's 0.2 (prima_driver.jl --rhobeg {rb} --prefix {prefix}); the same "
            'objective, bounds, ftarget and Palace environment; the initial point is the committed dl_initial (run 1)"'
        )
        t.append(f"rhobeg = {rb!r}")
        t.append(f"cells = {strs([r['cell'] for r in pr])}")
        t.append(f"x1 = [{', '.join(repr(r['xf'][0]) for r in pr)}]")
        t.append(f"x2 = [{', '.join(repr(r['xf'][1]) for r in pr)}]")
        t.append(f"c_um = {arr([r['theta'][0] for r in pr], 3)}")
        t.append(f"L_um = {arr([r['theta'][1] for r in pr], 3)}")
        t.append(f"f_q_ghz = {arr([r['F'][0] for r in pr], 6)}")
        t.append(f"f_r_ghz = {arr([r['F'][1] for r in pr], 6)}")
        t.append("err = [" + ", ".join(sci(e) for e in errs) + "]")
        t.append(f"palace_runs = {len(pr)}  # PRIMA's info.nf")
        t.append(f"reached_criterion = {'true' if reached else 'false'}")
        if reached:
            t.append(f"palace_runs_at_criterion = {n}")
        t.append(f"prima_status = {json.dumps(summ['status'])}")
        t.append(f"final_um = {arr(fin['theta'], 3)}")
        t.append(f"final_ghz = {arr(fin['F'], 6)}")
        t.append(f"best_err = {sci(errs[best])}")
        t.append(f"best_run = {best + 1}")
        counts.append(n if reached else -1)
        status.append(summ["status"])
        finals.append(fin["theta"])
    t.append("\n[controls.prima_rhobeg]")
    t.append(
        'description = "issue #1135 T5: Palace runs to DeviceLayout\'s criterion for its PRIMA call at three initial '
        "trust radii, all in this repository's Palace environment; 0.2 is DeviceLayout's setting "
        '([controls.prima_this_build]); -1 = not reached"'
    )
    rbs = [RHOBEG_RUNS[0][0], 0.2, RHOBEG_RUNS[1][0]]
    t.append(f"rhobeg = [{', '.join(repr(v) for v in rbs)}]  # 0.39999999999999997 = (1.4 - 0.6) / 2, BOBYQA's limit; 0.4 is reset to 0.2")
    t.append(f"palace_runs_at_criterion = [{counts[0]}, {n_default}, {counts[1]}]")
    t.append(f"prima_status = {strs([status[0], 'FTARGET_ACHIEVED', status[1]])}")
    f0 = prima_runs()[-1]["theta"]  # the #1129 rhobeg 0.2 run
    t.append(f"final_c_um = {arr([finals[0][0], f0[0], finals[1][0]], 3)}")
    t.append(f"final_L_um = {arr([finals[0][1], f0[1], finals[1][1]], 3)}")
    reached = [c for c in counts if c > 0] + [n_default]
    t.append(f"run_count_range = [{min(reached)}, {max(reached)}]  # over the rhobeg values that reached the criterion")
    t.append("newton_runs_at_criterion = 3  # [controls.comparison_1129]: every Newton variant")


def dof_extrapolation(n, f):
    """alpha and f_inf of f = f_inf - C N^-alpha through three (N, f) points:
    bisection on alpha in (0, 4) for the ratio of the two differences."""
    target = (f[1] - f[0]) / (f[2] - f[1])

    def ratio(a):
        x = [v ** -a for v in n]
        return (x[0] - x[1]) / (x[1] - x[2])

    lo, hi = 1e-6, 4.0
    assert (ratio(lo) - target) * (ratio(hi) - target) < 0, (ratio(lo), ratio(hi), target)
    for _ in range(200):
        mid = 0.5 * (lo + hi)
        if (ratio(lo) - target) * (ratio(mid) - target) <= 0:
            hi = mid
        else:
            lo = mid
    a = 0.5 * (lo + hi)
    x = [v ** -a for v in n]
    return a, f[2] + (f[2] - f[1]) * x[2] / (x[1] - x[2])


def max_mem_gib(path, cell):
    """Peak 'used_mib' of the host sampler while `cell` ran (orchestrate.log
    gives the window)."""
    orch = read(path.replace("mem_", "orchestrate_"))
    start = re.search(rf"start {cell} np=\d+ (\S+)", orch).group(1)
    end = re.search(rf"end {cell} exit=\d+ (\S+)", orch).group(1)
    peak = 0
    for line in read(path).splitlines():
        ts, rest = line.split(" ", 1)
        if start <= ts <= end:
            peak = max(peak, int(re.search(r"used_mib=(\d+)", rest).group(1)))
    return peak / 1024.0


PROPOSAL = (
    'proposal = "The ~2 % between the two h/2 levels is almost all refinement scheme (refinement_scheme_pct: 1.5-1.8 %), '
    "not mesh family (mesh_family_pct: 0.15-0.43 %); the two h meshes agree to 0.02 % at h. Level spreads overstate "
    "the effect on the converged value: run through the part-1 rule, the two refinement families of the same h mesh "
    "give estimates 0.64 % apart (estimate_spread_pct), and the second family's band lies inside the first's "
    "(family_b_band_inside_a). Proposal for v12: keep the part-1 band (it already covers the second family), and "
    "quote the converged initial qubit as the range of the two family estimates (estimate_range_f_q_ghz) instead of "
    "one point, stating the 0.64 % as the mesh-family/refinement-scheme spread; the adaptive run's DOF extrapolation "
    "(an ill-conditioned estimate) lies inside that range (amr_dof_extrapolation_in_range). The resonator behaves the same way (0.62 %). Widening the "
    'band by the 1.8 % level spread is not recommended: it double-counts the extrapolation."'
)

CAP_ASSUMPTIONS = (
    'assumptions = ["the lumped two-node circuit: one island node, the junction L_J || C_J, the series loop inductance '
    'L_s >= 0, a capacitive coupling to one resonator mode above the qubit; no other mode below the qubit couples to it", '
    '"the electrostatic C_Sigma (feedline floating, resonator grounded) is the island capacitance the 4.4 GHz mode sees: '
    'quasi-static, with the frequency dependence of the island\'s own distributed L and C neglected", '
    '"the bracket is certified on transmon_smoke.msh (the earlier generator\'s mesh of the same initial geometry), not on '
    'the DeviceLayout mesh the eigen ladders and the adaptive run use; the two agree to 0.02 % on Palace at h", '
    '"the certificate is plain f64, not interval arithmetic (c_sigma_bracket_tensor.toml [method].rounding)"]'
)

WHY_ABOVE_CAP = (
    'why_above_cap = "The initial-qubit shared limit (4.4802 GHz) exceeds the model-dependent cap (4.4766 GHz) by '
    "3.6 MHz, 0.08 %. The construction is exactly determined (four levels, four unknowns) and ill-conditioned: "
    "+0.1 % on the order-2 h/2 level alone moves it by about 20 MHz (initial_q_sensitivity_ghz_per_0p1pct), so the "
    "excess equals a 0.019 % change of that one level (initial_q_excess_as_p2_h2_change_pct_exact), far inside the 0.15-1.8 % "
    "mesh and refinement-scheme effects of [mesh_family.initial]. It also assumes one common order for the order-1 "
    "and order-2 series in the pre-asymptotic range, where the order-1 ladder's own observed order is 0.62. The same "
    "construction on the second refinement family (family_b_initial_q, order 1.02, where the ladder is close to its "
    "asymptotic order) gives 4.4657 GHz, below the cap, as do both family estimates and the adaptive run's finest "
    "computed level, 4.4297 GHz ([mesh_family.initial].all_estimates_below_cap). The adaptive run's three-point DOF "
    "extrapolation (4.4552 GHz) is exactly determined too and at least as ill-conditioned (+0.1 % on its finest level "
    "moves it by about 24 MHz), so its 21 MHz margin below the cap is an estimate, not evidence either way. The excess is within the conditioning of the family-a fit and "
    'is not evidence against the cap; the cap is still model-dependent, not certified."'
)

if __name__ == "__main__":
    sys.exit(main())
