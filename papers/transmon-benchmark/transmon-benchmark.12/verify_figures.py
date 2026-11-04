#!/usr/bin/env python3
"""Verify the v12 figures: inputs, the numbers they show, data coordinates, layout, and regeneration.

Python >= 3.11 with NumPy and Matplotlib; Poppler's pdftotext and pdfinfo.
Run from any working directory. This checks author-side transcription and
rendering, not the physics; no field solve is run.

1. Every repository input in figures/src/data/provenance.json has its recorded
   SHA-256, every artifact path a script names is in the manifest, and the data
   directory holds no snapshot.
2. The four figure scripts run; each one's save() enforces the layout rules of
   figure_style.py (width, fonts >= 8 pt at print size, no text collisions).
   The regenerated PDFs must be byte-identical to the committed ones, and
   Figure 1 (PDF and scripts) must be byte-identical to v9's.
3. Figure-specific content, from the PDFs' extracted text and the committed
   artifacts: the cross-check estimates and their labels (Figure 2), the
   certified scalar and tensor brackets and the gap, with no design target
   drawn (Figure 4), the re-optimization's labels, the controls and the PRIMA
   end point (Figure 5), the initial-qubit checks (Figure 3). Estimates are
   labelled as not bounds wherever they appear.
3b. Data coordinates (check_coordinates): each certified bracket in Figure 4
   is drawn at the tetrahedron count of the mesh that sets its lower bound;
   text and hash checks cannot see a misplaced coordinate.
4. main.tex includes each figure exactly once, in printed order, and the
   captions carry the wording the figures depend on.
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
NAMES = ['fig1-geometry', 'fig2-crosscheck', 'fig3-initial-qubit', 'fig4-convergence', 'fig5-reoptimization']
LABELS = ['geometry', 'crosscheck', 'initialq', 'convergence', 'reopt']
MAX_WIDTH_PT = 6.3 * 72


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def toml(rel):
    return tomllib.loads((REPO / rel).read_text())


def text(name):
    raw = subprocess.check_output(['pdftotext', '-raw', str(FIG / f'{name}.pdf'), '-'], text=True)
    return re.sub(r'\s+', ' ', raw)


def check_inputs():
    manifest = json.loads((DATA / 'provenance.json').read_text())
    assert manifest['files'] == {}, 'v12 has no snapshot inputs'
    for name, expected in manifest['repository_inputs'].items():
        assert sha(REPO / name) == expected, f'input changed since the figures were drawn: {name}'
    assert sorted(p.name for p in DATA.iterdir()) == ['provenance.json']
    sources = '\n'.join(p.read_text() for p in SRC.glob('*.py'))
    named = set(re.findall(r"'((?:benchmarks|crates)/[^']+\.(?:toml|json|jsonl|msh))'", sources))
    named |= {a + b for a, b in re.findall(r"'((?:benchmarks|crates)/[^']+/)'\s*\n?\s*'([^']+\.(?:toml|json|jsonl))'", sources)}
    for rel in named:
        assert rel in manifest['repository_inputs'], rel
    assert 'benchmarks/transmon_devicelayout_optimize/opt_log.jsonl' in manifest['repository_inputs']


def check_regeneration():
    pdfs = sorted(FIG.glob('*.pdf'))
    assert [p.stem for p in pdfs] == sorted(NAMES), [p.stem for p in pdfs]
    scripts = sorted(SRC.glob('fig[1-9]_*.py'))
    assert [s.name.split('_')[0] for s in scripts] == ['fig1', 'fig2', 'fig3', 'fig4', 'fig5']
    before = {p.name: sha(p) for p in pdfs}
    for script in scripts:
        subprocess.run([sys.executable, str(script)], check=True, stdout=subprocess.DEVNULL)
    after = {p.name: sha(p) for p in sorted(FIG.glob('*.pdf'))}
    assert before == after, 'regenerated figure bytes differ from the committed PDFs'
    v9 = HERE.parent / 'transmon-benchmark.9' / 'figures'
    assert sha(FIG / 'fig1-geometry.pdf') == sha(v9 / 'fig1-geometry.pdf')
    for name in ['fig1_geometry.py', 'geometry_mesh.py', 'figure_style.py']:
        assert (SRC / name).read_bytes() == (v9 / 'src' / name).read_bytes(), name
    for name in NAMES:
        info = subprocess.check_output(['pdfinfo', str(FIG / f'{name}.pdf')], text=True)
        w = float(re.search(r'Page size:\s+([\d.]+) x', info).group(1))
        assert w <= MAX_WIDTH_PT + 0.01, (name, w)


def check_content():
    every = {name: text(name) for name in NAMES}
    for name, t in every.items():
        for stale in ['anchor', '93.95', '93.14', '89.9', 'spurious', 'Same-mesh', 'within 1%', 'passed']:
            assert stale not in t, (name, stale)
    cc = toml('benchmarks/transmon_devicelayout_crosscheck/results.toml')
    t2 = every['fig2-crosscheck']
    for geom in ['initial', 'final']:
        conv = cc['convergence'][geom]
        for mode in ['q', 'r']:
            label = f"{conv[f'f_{mode}_ghz']:.3f} GHz ({conv[f'f_{mode}_over_published_pct']:+.1f}% vs published)"
            assert label in t2, label
    for label in ['published (DeviceLayout)', 'Palace, order 1', 'Palace, order 2', 'geode-fem, order 1 (same meshes)',
                  "Palace at DeviceLayout's discretization", 'converged estimate band (not a bound)',
                  'static route (certified', 'circuit-model cap from the certified bound', 'estimate, second refinement family']:
        assert label in t2, label
    t3 = every['fig4-convergence']
    sb = toml('benchmarks/transmon_diffopt/c_sigma_bracket.toml')['bracket_x0']
    tens = toml('benchmarks/transmon_diffopt/c_sigma_bracket_tensor.toml')
    tb, gap = tens['bracket_x0_tensor'], tens['gap_x0']
    for b in [sb, tb]:
        assert f"{b['lower_ff']:.2f}–{b['upper_ff']:.2f} fF" in t3
    rel = gap['tensor_minus_scalar_relative']
    # Certified, so rounded outward: against the floored/ceiled fF gap and the scalar bracket too.
    lo = min(rel[0], gap['tensor_minus_scalar_lower_ff'] / gap['scalar_upper_ff'])
    hi = max(rel[1], gap['tensor_minus_scalar_upper_ff'] / gap['scalar_lower_ff'])
    shown = (math.floor(1000 * lo) / 10, math.ceil(1000 * hi) / 10)
    assert shown == (2.5, 3.1), shown
    assert f"gap {shown[0]:.1f}–{shown[1]:.1f}%" in t3
    assert 'gap 2.5–3.1%' in t3 and 'P2, sapphire tensor' in t3 and 'certified, tensor' in t3
    assert 'Every marker is an upper bound' not in t3   # the caption says it; the figure no longer repeats it
    tq = every['fig3-initial-qubit']
    for label in ['family a (gmsh refinement)', 'family b (Palace refinement)', 'Palace adaptive refinement',
                  'circuit-model cap', 'extrapolation band (not a bound)', 'estimates', 'Nédélec unknowns']:
        assert label in tq, label
    t4 = every['fig5-reoptimization']
    for label in ['published final (PRIMA)', "geode-gradient Newton, DeviceLayout's discr.",
                  'geode-gradient Newton, converged frequencies', 'Newton, scaling law', 'Broyden', 'PRIMA (published)',
                  'Palace evaluations to the criterion', 'Extrapolated qubit', 'scaling-law step, converged frequencies',
                  'qubit band mapped to', '(estimate)', 'estimate band (not a bound)', 'criterion',
                  'target (3.0, 4.0 GHz)', '(620, 5000)', 'first leg, from the initial design',
                  'step-2 qubit band mapped to']:
        assert label in t4, label
    # The converged-target steps carry Table 5's names, in (a) and as the (c, d) tick labels.
    assert t4.count('Step 1') == 3 and t4.count('Step 2') == 3, (t4.count('Step 1'), t4.count('Step 2'))
    for stale in ['cv1', 'cv2', 'Cost to the criterion', 'Converged qubit']:
        assert stale not in t4, stale


def check_coordinates():
    """Figure 4's certified bars sit at the true tetrahedron counts (rebuilt from the script, not the PDF)."""
    import importlib.util
    import matplotlib
    matplotlib.use('Agg')
    import matplotlib.pyplot as plt
    sys.path.insert(0, str(SRC))
    spec = importlib.util.spec_from_file_location('fig4', SRC / 'fig4_convergence.py')
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    captured = []
    mod.save = lambda fig, name: captured.append(fig)
    mod.main()
    ax = captured[0].axes[0]
    scal = toml('benchmarks/transmon_diffopt/c_sigma_bracket.toml')
    tens = toml('benchmarks/transmon_diffopt/c_sigma_bracket_tensor.toml')
    want = {}
    for b, rows in [(scal['bracket_x0'], scal['adaptive_x0']), (tens['bracket_x0_tensor'], tens['adaptive_x0_tensor'])]:
        n = {r['mesh']: r['n_tets'] for r in rows}[b['lower_from'].rsplit(' P2', 1)[0]]
        want[(b['lower_ff'], b['upper_ff'])] = n
    assert sorted(want.values()) == [3289543, 3388197], want
    found = {}
    for line in ax.get_lines():
        x, y = list(line.get_xdata()), list(line.get_ydata())
        if len(x) == 2 and x[0] == x[1] and (y[0], y[1]) in want:
            found[(y[0], y[1])] = x[0]
    assert found == want, (found, want)
    plt.close('all')


def check_manuscript():
    tex = (HERE / 'main.tex').read_text()
    norm = re.sub(r'\s+', ' ', tex)
    order = [m.group(1) for m in re.finditer(r'\\anvilfig\{(fig\d-[a-z-]+)\.pdf\}', tex)]
    assert order == NAMES, order
    labels = re.findall(r'\\label\{fig:([a-z-]+)\}', tex)
    assert labels == LABELS, labels
    for wording in ['extrapolation band, which is not a bound', 'certified capacitance bracket read through the',
                    'Every marker is a conforming upper bound on the converged value of the committed polygonal geometry',
                    'certified two-sided brackets from adapted meshes', 'magnified in the inset',
                    'mapped linearly to capacitor\\_length (an estimate)', 'since its trajectory is not published',
                    'Extrapolated estimates and bands', 'This counts Palace evaluations, not total cost',
                    'drawn at the tetrahedron count of the mesh that sets its lower bound',
                    'the adaptive run\'s three-point fit, an ill-conditioned estimate']:
        assert wording in norm, wording


def main():
    check_inputs()
    check_regeneration()
    check_content()
    check_coordinates()
    check_manuscript()
    print('PASS: input hashes, byte-identical regeneration of five figures (Figure 1 as in v9), widths, figure content '
          'against the artifacts, the certified bars at their true mesh sizes, inclusion order and caption wording.')


if __name__ == '__main__':
    main()
