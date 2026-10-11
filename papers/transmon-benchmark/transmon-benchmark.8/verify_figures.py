#!/usr/bin/env python3
"""Verify the v8 figures: inputs, the numbers they show, layout, and regeneration.

Python >= 3.11 with NumPy and Matplotlib; Poppler's pdftotext and pdfinfo.
Run from any working directory. This checks author-side transcription and
rendering, not the physics; no field solve is run.

1. Every input in figures/src/data/provenance.json has its recorded SHA-256.
2. The optimized-outline snapshot reproduces the committed design.json extent.
3. The figure scripts all run; each one's save() enforces the layout rules of
   figure_style.py (width, fonts >= 8 pt at print size, no text collisions).
   The regenerated PDFs must be byte-identical to the committed ones.
4. Figure-specific content, from the PDFs' extracted text and from the
   committed artifacts: the qubit mode and its precision in Figure 2, the
   withdrawn labels ("Same-mesh", "junction_lc", "six physical modes") absent,
   the seven-round held-out series, bounds, bands and the by-construction
   label in Figure 6, and no single-round or "1 %" verdict anywhere.
5. main.tex includes each figure exactly once, in printed order.
"""
from pathlib import Path
import hashlib
import json
import math
import re
import subprocess
import sys
import tomllib

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[2]
FIG = HERE / 'figures'
SRC = FIG / 'src'
DATA = SRC / 'data'
NAMES = ['fig1-geometry', 'fig2-agreement', 'fig3-lscaling', 'fig4-participation', 'fig5-convergence',
         'fig6-refined-optimization', 'fig7-cpu-wallclock', 'fig8-legacy-diffopt']
MAX_WIDTH_PT = 6.3 * 72


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def toml(rel):
    return tomllib.loads((REPO / rel).read_text())


def text(name):
    raw = subprocess.check_output(['pdftotext', '-raw', str(FIG / f'{name}.pdf'), '-'], text=True)
    return re.sub(r'\s+', ' ', raw)


def ceil_to(x, digits):
    scale = 10 ** digits
    return math.ceil(x * scale - 1e-9) / scale


def check_inputs():
    manifest = json.loads((DATA / 'provenance.json').read_text())
    for name, expected in manifest['files'].items():
        assert sha(DATA / name) == expected, name
    for name, expected in manifest['repository_inputs'].items():
        assert sha(REPO / name) == expected, f'input changed since the figures were drawn: {name}'
    assert sorted(p.name for p in DATA.iterdir()) == ['optimized_island.json', 'provenance.json']
    # The outline snapshot reproduces the committed design exactly.
    snap = json.loads((DATA / 'optimized_island.json').read_text())
    assert snap['mesh_sha256'] == manifest['optimized_island']['exported_mesh_sha256']
    assert len(snap['triangles']) == 139
    pts = [p for tri in snap['triangles'] for p in tri]
    design = json.loads((REPO / 'benchmarks/transmon_diffopt/refined_optimization/p2_cycles10/design.json').read_text())
    (x0, y0, _), (x1, y1, _) = design['island_bbox_um']
    assert (min(p[0] for p in pts), min(p[1] for p in pts)) == (x0, y0)
    assert (max(p[0] for p in pts), max(p[1] for p in pts)) == (x1, y1)
    assert math.isclose(x1 - x0, design['pad_width_um'], abs_tol=1e-12)
    assert math.isclose(y1 - y0, design['island_length_um'], abs_tol=1e-12)
    summary = toml('benchmarks/transmon_diffopt/refined_optimization/p2_cycles10/summary.toml')
    assert f"{design['island_length_um']:.6f}" == f"{summary['design']['island_length_um']:.6f}"
    assert f"{design['pad_width_um']:.6f}" == f"{summary['design']['pad_width_um']:.6f}"


def check_regeneration():
    pdfs = sorted(FIG.glob('*.pdf'))
    assert [p.stem for p in pdfs] == sorted(NAMES), [p.stem for p in pdfs]
    scripts = sorted(SRC.glob('fig[1-8]_*.py'))
    assert len(scripts) == 8
    before = {p.name: sha(p) for p in pdfs}
    for script in scripts:
        subprocess.run([sys.executable, str(script)], check=True, stdout=subprocess.DEVNULL)
    after = {p.name: sha(p) for p in sorted(FIG.glob('*.pdf'))}
    assert before == after, 'regenerated figure bytes differ from the committed PDFs'
    for name in NAMES:
        info = subprocess.check_output(['pdfinfo', str(FIG / f'{name}.pdf')], text=True)
        w = float(re.search(r'Page size:\s+([\d.]+) x', info).group(1))
        assert w <= MAX_WIDTH_PT + 0.01, (name, w)
        assert text(name).strip(), name


def check_content():
    eigen = toml('benchmarks/transmon_eigen/results.toml')
    pal = eigen['oracles']['palace_target2']
    everything = {name: text(name) for name in NAMES}
    for name, t in everything.items():
        for stale in ['Same-mesh', 'same-mesh', 'junction_lc', 'physical modes', 'passes', 'passed',
                      'within 1%', '±1%', '1% working', 'working tolerance', 'Finer mesh']:
            assert stale not in t, (name, stale)

    # Figure 2: the qubit mode is present, at its committed precision.
    t2 = everything['fig2-agreement']
    qubit = 100 * (pal['geode_qubit_f_ghz'] / pal['qubit_f_ghz'] - 1)
    assert f'{qubit:+.5f}%' == '+0.00005%' and '+0.00005%' in t2 and 'Qubit mode' in t2
    assert 'Port-sheet artifact' in t2 and 'Same input mesh' in t2
    for key, mode in eigen['modes'].items():
        signed = 100 * (mode['f_ghz'] / mode['palace_f_ghz'] - 1)
        assert f'{abs(signed):.3f}' == f"{mode['rel_err_pct']:.3f}", key  # matches Table 1
        assert f'{signed:+.3f}%' in t2, key

    # Figure 4: separate observables, both labels.
    t4 = everything['fig4-participation']
    assert '0.99424' in t4 and 'Port-sheet artifact' in t4 and 'Palace' in t4

    # Figure 5: the uniform study's bounds and band come from the artifact.
    t5 = everything['fig5-convergence']
    conv = toml('benchmarks/transmon_diffopt/c_sigma_convergence.toml')
    lo, hi = conv['x0_extrapolation']['estimate_band_ff']
    for value in [f'{lo:.2f}', f'{hi:.2f}', '< 83.02 fF (rigorous)', '< 89.50 fF (rigorous)', 'not a bound']:
        assert value in t5, value

    # Figure 6: the committed seven-round series, bounds, bands, caveat.
    t6 = everything['fig6-refined-optimization']
    base = 'benchmarks/transmon_diffopt/refined_optimization/'
    prim = toml(base + 'p2_cycles10/summary.toml')
    corr = toml(base + 'p2_cycles10_resolution_corrected/summary.toml')
    for run in (prim, corr):
        rounds = sorted(run['held_out'], key=lambda r: r['round'])
        assert [r['round'] for r in rounds] == list(range(8)), 'seven held-out rounds plus round 0'
        c = [r['c_ff'] for r in rounds]
        assert all(b < a for a, b in zip(c, c[1:]))
        fx = run['final_design_extrapolation']
        assert ceil_to(c[-1], 2) == fx['upper_bound_ff']
        assert f"< {fx['upper_bound_ff']:.2f}" in t6
        assert len(fx['slow_rate_fit']) == 2 and run['optimization']['accepted_steps'] == 3
    assert prim['final_design_extrapolation']['upper_bound_ff'] == 89.02
    assert corr['final_design_extrapolation']['upper_bound_ff'] == 90.08
    band = corr['final_design_extrapolation']['estimate']['band_ff']
    assert band[0] < corr['meta']['anchor_ff'] < band[1], 'the corrected band contains the anchor'
    for phrase in ['contains the anchor by construction', 'not bounds', 'upper bound', 'estimate',
                   'slower rates', 'rounds 0–7', f"{corr['meta']['optimizer_target_ff']:.3f}",
                   f"{prim['optimization']['initial_ff']:.3f}",
                   f"length 625.50 → {prim['design']['island_length_um']:.2f} µm",
                   f"pad width 24.00 → {prim['design']['pad_width_um']:.2f} µm"]:
        assert phrase in t6, phrase
    for stale in ['Fixed-design', 'Palace P2', 'held-out local refinement', '89.6628']:
        assert stale not in t6, stale

    # Figure 7 and 8: wording carried over from Table 6 and the final v7 figure.
    t7 = everything['fig7-cpu-wallclock']
    assert 'Six reference modes' in t7 and '†' in t7
    t8 = everything['fig8-legacy-diffopt']
    for phrase in ['Historical', 'anchor shortfall was a mesh artifact', 'a coarse-mesh artifact',
                   'first tet inversion']:
        assert phrase in t8, phrase
    assert 'Limited by mesh validity' not in t8


def check_manuscript():
    tex = (HERE / 'main.tex').read_text()
    used = re.findall(r'\\anvilfig(?:\[[^]]*\])?\{([^}]+)\.pdf\}', tex)
    assert used == NAMES, used  # each exactly once, in printed order
    assert '\\anvilfig[' not in tex, 'every figure uses the 0.98 default width'
    assert '\\newcommand{\\anvilfig}[3][0.98]' in tex
    for label in ['geometry', 'agreement', 'lscaling', 'participation', 'convergence', 'refined', 'cpu', 'legacy']:
        assert tex.count(f'\\label{{fig:{label}}}') == 1 and f'\\ref{{fig:{label}}}' in tex, label
    assert '\\ref{fig:diffopt}' not in tex and 'github.com' not in tex


def main():
    check_inputs()
    check_regeneration()
    check_content()
    check_manuscript()
    print('PASS: input hashes, outline snapshot against design.json, layout rules and byte-identical '
          'regeneration of all eight PDFs, figure content against the committed artifacts '
          '(qubit mode, seven-round series, bounds, bands, by-construction label, no 1% verdict), '
          'and figure inclusion order.')


if __name__ == '__main__':
    main()
