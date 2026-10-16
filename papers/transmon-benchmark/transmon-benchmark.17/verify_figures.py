#!/usr/bin/env python3
"""Verify the v17 figures (v16's bytes; only the captions changed in main.tex): inputs, the numbers they show, data coordinates, layout, and regeneration.

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
   bounded scalar and tensor brackets and the gap, with no design target
   drawn (Figure 4), the re-optimization's labels and the controls (Figure 5),
   the initial-qubit checks (Figure 3). Extrapolations are labelled
   "extrapolated", never "converged".
3b. Data coordinates (check_coordinates), which text and hash checks cannot
   see: each bounded bracket in Figure 4 is drawn at the tetrahedron count of
   the mesh that sets its lower bound; Figure 5(a)'s mapped band is centred on
   the 3.0 GHz target, Figure 5(b) plots PRIMA's ten runs in our environment
   and the Palace-only Newton, with err capped at 1, and Figure 5(c, d) carries
   step 2's second refinement family; Figure 3(b) draws no adaptive resonator fit.
4. main.tex includes each figure exactly once, in printed order, and the
   captions carry the wording the figures depend on.

v14 changes (against the v13 reviews): Figure 5(b) plots the Palace-delta
Newton under the one counting rule (its run 2 is Broyden's L probe, the run that
sets its delta), labels the published-PRIMA star as the err of the published
frequencies and widens the gap between the (a, b) x-labels and the (c, d)
titles; Figure 3 draws family b with larger unfilled markers so that a level
coinciding with family a stays visible. Figures 2 and 4 replace "certified" with "bounded" (operator ruling); Figure 1 is v9's bytes.

v15 changes (against the v14 reviews): the island capacitance is labelled C_isl in
Figures 2 and 4 (C_Sigma = C_isl + C_J is Koch's total, Fable M1); Figure 2 gives
each refinement family its own h->0 slot, on both geometries (the final design's
family b is new, #1135), moves the static route to its own slot and draws the
DeviceLayout-discretization diamond just right of h (Fable S6, Opus N-4).
Figures 1, 3 and 5 are v14's bytes.

v16 changes (against the v15 reviews): Figure 2's family-a estimate label ("a: 4.444 GHz (+7.3%)") moves from beside
the slot, where the order-2 series ran through it in all four panels (Opus MF-2), into the slot column low in each
panel; check_fig2_coordinates asserts that no label box touches a plotted line, marker or band. Figure 2 is 0.3 in
shorter and Figure 1 0.45 in shorter (panel (a) has equal aspect and was width-limited, so the height it gave up was
empty; panel (c) is shorter); no font size changes. Figures 3 and 5 are v15's (= v14's) bytes.
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
    assert manifest['files'] == {}, 'v13 has no snapshot inputs'
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
    v14 = HERE.parent / 'transmon-benchmark.14' / 'figures'
    v15 = HERE.parent / 'transmon-benchmark.15' / 'figures'
    v16 = HERE.parent / 'transmon-benchmark.16' / 'figures'
    for name in NAMES:   # v17: every figure and script is v16's, byte for byte
        assert sha(FIG / f'{name}.pdf') == sha(v16 / f'{name}.pdf'), name
    for script in SRC.glob('*.py'):
        assert script.read_bytes() == (v16 / 'src' / script.name).read_bytes(), script.name
    for name in ['fig3-initial-qubit', 'fig5-reoptimization']:
        assert sha(FIG / f'{name}.pdf') == sha(v15 / f'{name}.pdf') == sha(v14 / f'{name}.pdf'), name
    assert sha(FIG / 'fig4-convergence.pdf') == sha(v15 / 'fig4-convergence.pdf')
    for name in ['fig1-geometry', 'fig2-crosscheck']:
        assert sha(FIG / f'{name}.pdf') != sha(v15 / f'{name}.pdf'), name
    assert sha(v15 / 'fig1-geometry.pdf') == sha(v9 / 'fig1-geometry.pdf')
    for name in ['fig3_initial_qubit.py', 'fig4_convergence.py', 'fig5_reoptimization.py']:
        assert (SRC / name).read_bytes() == (v15 / 'src' / name).read_bytes(), name
    # Figure 1's script differs from v9's only in the figure height and the axes rectangles.
    diff = [(a, b) for a, b in zip((v9 / 'src' / 'fig1_geometry.py').read_text().splitlines(),
                                   (SRC / 'fig1_geometry.py').read_text().splitlines()) if a != b]
    assert all(('figsize' in a or 'add_axes' in a) for a, _ in diff[:4]), diff
    # Operator ruling (#1129): no "certified" anywhere in the v14 figures or their scripts.
    for script in SRC.glob('*.py'):
        assert 'certif' not in script.read_text().lower(), script.name
    for name in ['geometry_mesh.py', 'figure_style.py']:
        assert (SRC / name).read_bytes() == (v9 / 'src' / name).read_bytes(), name
    for name in NAMES:
        info = subprocess.check_output(['pdfinfo', str(FIG / f'{name}.pdf')], text=True)
        w = float(re.search(r'Page size:\s+([\d.]+) x', info).group(1))
        assert w <= MAX_WIDTH_PT + 0.01, (name, w)


def check_content():
    every = {name: text(name) for name in NAMES}
    for name, t in every.items():
        assert 'ertified' not in t, name
        for stale in ['anchor', '93.95', '93.14', '89.9', 'spurious', 'Same-mesh', 'within 1%', 'passed',
                      'onverged', 'conv.', 'geode-gradient']:
            assert stale not in t, (name, stale)
    cc = toml('benchmarks/transmon_devicelayout_crosscheck/results.toml')
    t2 = every['fig2-crosscheck']
    for geom in ['initial', 'final']:
        conv = cc['convergence'][geom]
        for mode in ['q', 'r']:
            label = f"a: {conv[f'f_{mode}_ghz']:.3f} GHz ({conv[f'f_{mode}_over_published_pct']:+.1f}% vs published)"
            assert label in t2, label
    for label in ['published (DeviceLayout)', 'Palace, order 1', 'Palace, order 2', 'geode-fem, order 1 (same meshes)',
                  "Palace at DeviceLayout's discretization", 'extrapolation band (not a bound)', 'extrapolated estimate',
                  'static route (bounded Cisl + circuit)', 'circuit-model cap from the lower bound', 'h→0, a', 'static']:
        assert label in t2, label
    assert t2.count('h→0, a') == 4 and 'Σ' not in t2 and 'second refinement family' not in t2
    for name in ['fig2-crosscheck', 'fig4-convergence']:
        assert 'Σ' not in every[name], name
    t3 = every['fig4-convergence']
    sb = toml('benchmarks/transmon_diffopt/c_sigma_bracket.toml')['bracket_x0']
    tens = toml('benchmarks/transmon_diffopt/c_sigma_bracket_tensor.toml')
    tb, gap = tens['bracket_x0_tensor'], tens['gap_x0']
    for b in [sb, tb]:
        assert f"{b['lower_ff']:.2f}–{b['upper_ff']:.2f} fF" in t3
    rel = gap['tensor_minus_scalar_relative']
    # A bound, so rounded outward: against the floored/ceiled fF gap and the scalar bracket too.
    lo = min(rel[0], gap['tensor_minus_scalar_lower_ff'] / gap['scalar_upper_ff'])
    hi = max(rel[1], gap['tensor_minus_scalar_upper_ff'] / gap['scalar_lower_ff'])
    shown = (math.floor(1000 * lo) / 10, math.ceil(1000 * hi) / 10)
    assert shown == (2.5, 3.1), shown
    assert f"gap {shown[0]:.1f}–{shown[1]:.1f}%" in t3
    assert 'gap 2.5–3.1%' in t3 and 'P2, sapphire tensor' in t3 and 'bounded, tensor' in t3 and 'two-sided bounds' in t3
    assert 'C isl , unmodified device (fF)' in t3 and 'of X' not in t3
    assert 'Every marker is an upper bound' not in t3   # the caption says it; the figure no longer repeats it
    tq = every['fig3-initial-qubit']
    for label in ['family a (gmsh refinement)', 'family b (Palace refinement)', 'Palace adaptive refinement',
                  'circuit-model cap', 'extrapolation band (not a bound)', 'estimates', 'Nédélec unknowns',
                  'a b fit estimates', 'a b family a, b estimates']:
        assert label in tq, label
    assert tq.count('fit') == 1, 'the adaptive fit is drawn for the qubit only'
    t4 = every['fig5-reoptimization']
    assert 'PRIMA, published (DeviceLayout environment)' not in t4
    for label in ['PRIMA, published (its environment, published err)', 'PRIMA, our environment', 'Newton, shape derivative',
                  'Newton, scaling law', 'Newton, scaling law, Palace-only δ', 'Broyden',
                  'Palace evaluations to the criterion', 'Extrapolated qubit', 'Extrapolated resonator',
                  'shape-derivative steps, extrapolated frequencies', 'scaling-law step, extrapolated frequencies',
                  'step-2 qubit band mapped to c (estimate)', 'estimate band (not a bound)', 'criterion',
                  'extrapolated estimate', 'target (3.0, 4.0 GHz)', '(620, 5000)', 'first legs, from the initial design',
                  'Step 2, family b']:
        assert label in t4, label
    # The extrapolated-target steps carry Table 5's names, in (a) and as the (c, d) tick labels.
    assert t4.count('Step 1') == 3 and t4.count('Step 2') == 5, (t4.count('Step 1'), t4.count('Step 2'))
    for stale in ['cv1', 'cv2', 'Cost to the criterion', 'Converged qubit', '104']:
        assert stale not in t4, stale


def check_coordinates():
    """Figure 4's bounded bars sit at the true tetrahedron counts (rebuilt from the script, not the PDF)."""
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
    check_fig5_coordinates(load, plt)
    check_fig2_coordinates(load, plt)
    check_fig3_coordinates(load, plt)


def load(name, script):
    import importlib.util
    spec = importlib.util.spec_from_file_location(name, SRC / script)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    captured = []
    mod.save = lambda fig, name: captured.append(fig)
    mod.main()
    return captured[0]


def check_fig5_coordinates(load, plt):
    fig = load('fig5', 'fig5_reoptimization.py')
    ax, bx, cq, cr = fig.axes[:4]
    opt = toml('benchmarks/transmon_devicelayout_optimize/results.toml')
    ctl = toml('benchmarks/transmon_devicelayout_controls/results.toml')
    cv2 = opt['conv']['cv2']
    log = [json.loads(l) for l in (REPO / 'benchmarks/transmon_devicelayout_optimize/opt_log.jsonl').read_text()
           .splitlines() if l.strip()]
    s2 = [e for e in log if e['kind'] == 'conv_step' and e['step'] == 2][0]
    slope = s2['S'][0] * s2['dfdc_h'][0]
    lo, hi = cv2['ladder']['f_q_band_ghz']
    want = [cv2['to_um'][0] + (lo - 3.0) / -slope, cv2['to_um'][0] + (hi - 3.0) / -slope]
    band = [l for l in ax.get_lines() if l.get_label().startswith('step-2 qubit band')][0]
    assert list(band.get_xdata()) == want and list(band.get_ydata()) == [cv2['to_um'][1]] * 2, band.get_xdata()
    assert round(want[0]) == 1423 and round(want[1]) == 1690
    series = {tuple(l.get_ydata()) for l in bx.get_lines()}
    c = ctl['controls']
    for name in ['prima_this_build', 'scaling', 'broyden']:
        assert tuple(c[name]['err']) in series, name
    # One counting rule: the Palace-delta Newton's run 2 is Broyden's L probe (Broyden run 3, dl_bp2).
    sp, br = c['scaling_palace'], c['broyden']
    assert br['c_um'][2] == 620.0 and br['L_um'][2] > br['L_um'][0]
    want_sp = (sp['err'][0], br['err'][2]) + tuple(sp['err'][1:])
    assert want_sp in series and tuple(sp['err']) not in series, series
    line = [l for l in bx.get_lines() if tuple(l.get_ydata()) == want_sp][0]
    assert list(line.get_xdata()) == [1, 2, 3, 4]
    # The published-PRIMA star: 9 runs, err of the published frequencies.
    opt_nd = opt['newton_devicelayout']
    pub = [l for l in bx.get_lines() if list(l.get_xdata()) == [opt_nd['prima_palace_runs']]][0]
    f, t = opt_nd['published_final_f_ghz'], opt['meta']['targets_ghz']
    assert abs(pub.get_ydata()[0] - sum(((a - b) / b) ** 2 for a, b in zip(f, t)) / 2) < 1e-15
    # Room between the top row's x-labels and the bottom row's titles (0.155 of the figure height).
    assert ax.get_position().y0 - (cq.get_position().y0 + cq.get_position().height) >= 0.15
    assert len(c['prima_this_build']['err']) == 10
    assert bx.get_ylim()[1] == 1, bx.get_ylim()
    fb = ctl['mesh_family']['cv2']
    for axx, mode in [(cq, 'q'), (cr, 'r')]:
        labels = [t.get_text() for t in axx.get_xticklabels()]
        k = labels.index('Step 2,\nfamily b')
        ests = [l for l in axx.get_lines() if list(l.get_xdata()) == [k]]
        assert any(list(l.get_ydata()) == [fb[f'family_b_f_{mode}_ghz']] for l in ests), mode
    plt.close('all')


def check_fig2_coordinates(load, plt):
    """Each geometry's two families sit in their own slots, with family b's estimate and band from the artifact."""
    fig = load('fig2', 'fig2_crosscheck.py')
    fam = toml('benchmarks/transmon_devicelayout_controls/results.toml')['mesh_family']
    cc = toml('benchmarks/transmon_devicelayout_crosscheck/results.toml')
    for k, (geom, mode) in enumerate([('initial', 'q'), ('initial', 'r'), ('final', 'q'), ('final', 'r')]):
        ax = fig.axes[k]
        ticks = [t.get_text() for t in ax.get_xticklabels()]
        assert ticks[3:5] == ['$h\\to0$, a', 'b'], ticks
        xb = ax.get_xticks()[4]
        ys = {round(l.get_ydata()[0], 6) for l in ax.get_lines() if list(l.get_xdata()) == [xb]}
        assert round(fam[geom][f'family_b_f_{mode}_ghz'], 6) in ys, (geom, mode, ys)
        xa = ax.get_xticks()[3]
        ya = {round(l.get_ydata()[0], 6) for l in ax.get_lines() if list(l.get_xdata()) == [xa]}
        assert round(cc['convergence'][geom][f'f_{mode}_ghz'], 6) in ya, (geom, mode)
        diamond = [l for l in ax.get_lines() if l.get_label() == "Palace at DeviceLayout's discretization"]
        assert diamond and 0 < diamond[0].get_xdata()[0] < .5
    assert [t.get_text() for t in fig.axes[0].get_xticklabels()][-1] == 'static'
    # v16 Opus MF-2: no estimate label touches a plotted line, marker or band (v15's labels sat on the order-2 series).
    import numpy as np
    fig.canvas.draw()
    r = fig.canvas.get_renderer()
    for ax in fig.axes[:4]:
        labels = [t for t in ax.texts if t.get_text().startswith('a: ')]
        assert len(labels) == 1, [t.get_text() for t in ax.texts]
        box = labels[0].get_window_extent(r).expanded(1.05, 1.1)
        assert abs(ax.transData.transform((ax.get_xticks()[3], 0))[0] - (box.x0 + box.x1) / 2) < 1, 'label off its slot'
        for line in ax.get_lines():
            # Each line's own transform (data for the series, blended for axhline), so every series is tested.
            xy = line.get_transform().transform(np.column_stack([line.get_xdata(), line.get_ydata()]).astype(float))
            pts = [xy[0]] if len(xy) == 1 else [xy[i] + t * (xy[i + 1] - xy[i]) for i in range(len(xy) - 1)
                                                 for t in np.linspace(0, 1, 400)]
            pad = line.get_markersize() if line.get_marker() not in (None, 'None', '') else 0
            for x, y in pts:
                assert not (box.x0 - pad <= x <= box.x1 + pad and box.y0 - pad <= y <= box.y1 + pad), \
                    (line.get_label(), labels[0].get_text())
        for coll in ax.collections:
            for path in coll.get_paths():
                ext = path.get_extents(coll.get_transform())
                assert not box.overlaps(ext), labels[0].get_text()
    plt.close('all')


def check_fig3_coordinates(load, plt):
    fig = load('fig3', 'fig3_initial_qubit.py')
    amr = toml('benchmarks/transmon_devicelayout_controls/results.toml')['amr']['initial']
    ex_q, ex_r = fig.axes[1], fig.axes[3]
    ys = lambda a: [y for l in a.get_lines() for y in l.get_ydata()]
    assert amr['dof_extrapolation_f_q_ghz'] in ys(ex_q)
    assert amr['dof_extrapolation_f_r_ghz'] not in ys(ex_r), 'the unreliable resonator fit is drawn'
    assert [t.get_text() for t in ex_r.get_xticklabels()] == ['a', 'b']
    # Family b's markers are larger and unfilled, so a coinciding family-a level shows through.
    ax_q = fig.axes[0]
    fb = [l for l in ax_q.get_lines() if 'family b' in l.get_label()]
    fa = [l for l in ax_q.get_lines() if 'family a' in l.get_label()]
    assert len(fb) == 2 and all(l.get_markerfacecolor() == 'none' for l in fb)
    assert min(l.get_markersize() for l in fb) > max(l.get_markersize() for l in fa)
    plt.close('all')


def check_manuscript():
    tex = (HERE / 'main.tex').read_text()
    norm = re.sub(r'\s+', ' ', tex)
    order = [m.group(1) for m in re.finditer(r'\\anvilfig\{(fig\d-[a-z-]+)\.pdf\}', tex)]
    assert order == NAMES, order
    labels = re.findall(r'\\label\{fig:([a-z-]+)\}', tex)
    assert labels == LABELS, labels
    # v17: the captions are shortened to recover space; the wording each figure depends on is kept.
    for wording in ["the estimate with its band for the gmsh refinement family (a, labelled) and for Palace's "
                    "refinement (b)", 'the static route of Section~\\ref{sec:static} (grounded box)',
                    'the open diamond, just right of $h$, is Palace at DeviceLayout\'s own discretization',
                    'the circuit-model cap, which trims the untrimmed bands drawn',
                    'Mesh convergence of the island capacitance $\\Cisl$',
                    'Every marker is a conforming upper bound on the exact capacitance of the meshed polygonal geometry',
                    'two-sided bounds from adapted meshes', 'magnified in the inset', 'All values are for the natural box.',
                    'mapped linearly to capacitor\\_length (an estimate)',
                    "the published PRIMA point is at its 9 runs and the err of its published frequencies, "
                    "$1.151\\times10^{-5}$ (DeviceLayout's environment)",
                    "probes counted (the Palace-$\\delta$ Newton's run 2 is the $L$ probe it shares with Broyden)",
                    'Extrapolated estimates and bands against the targets, with step~2 in both refinement families',
                    'This counts Palace evaluations, not total cost',
                    'drawn at the tetrahedron count of the mesh that sets its lower bound',
                    "the adaptive run's ill-conditioned fit (the resonator fit is unreliable)",
                    'with the scalar trace-averaged permittivity (blue) and the sapphire tensor (orange)']:
        assert wording in norm, wording
    assert 'converged estimate' not in norm and 'conv.' not in norm


def main():
    check_inputs()
    check_regeneration()
    check_content()
    check_coordinates()
    check_manuscript()
    print('PASS (v17, figures identical to v16): input hashes, byte-identical regeneration of five figures (Figures 3 and 5 as in v14 and v15, 4 as in v15; Figure 2 labels clear of every series; Figure 1 and 2 heights), widths, figure content '
          'against the artifacts, the bounded bars at their true mesh sizes, Figure 5 band, controls and second family, '
          'Figure 3 without the resonator fit, inclusion order and caption wording.')


if __name__ == '__main__':
    main()
