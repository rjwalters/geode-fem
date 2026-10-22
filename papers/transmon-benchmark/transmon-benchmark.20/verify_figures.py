#!/usr/bin/env python3
"""Verify the v20 figures: inputs, the numbers they show, data coordinates, colours, layout, and regeneration.

v20 (coauthor clarity revision) gives the figures one colour scheme, with no data changed. Figure 1: each label has
the colour of the object it names; the junction has its own colour in (a) and (b), not the resonator's orange; the
island is blue in (a), (b) and (d); the ground is one grey in (a), (c) and (d); the design-parameter annotations (c,
L, the stretched strip, the translation by delta c) share one annotation colour. Figure 2: the key of the r = 1 pair
extrapolations shows both markers, blue (p = 1) and green (p = 2), as drawn. Figure 5: a method keeps its colour in
every panel (the shape-derivative steps against the extrapolated frequencies are blue, like the shape-derivative
Newton), and the step-2 band mapped to c is the grey of the estimate bands in (c, d). Figures 3 and 4 are v19's bytes.

v19 (reader-clarity revision) redraws all five figures: Figure 1 gains the design parameters c and L, the box wall,
the stretched strip and translated region of the capacitor_length motion (velocity bands read from
benchmarks/transmon_devicelayout_optimize/runs/fd_initial.toml) and a panel (d) with the two-node circuit; Figure 2
names element order as p, draws the two r = 1 pair extrapolations whose mean is the estimate, runs the cap across
panel (a) and keys the h->0 slots; Figure 3 labels the levels h, h/2, h/4, the adaptive passes 1-5, the last-pass
change and the 1e7 size limit, and letters the estimate panels; Figure 4 adds panel (b), the adaptive upper and lower
bound sequences, and names the fixture levels h_F; Figure 5 shades and names the two objectives in (a), calls out the
published and re-optimized capacitor lengths, and counts "Palace runs". The regenerated PDFs must be byte-identical
to the committed ones. The v18 notes follow.


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
    v19 = HERE.parent / 'transmon-benchmark.19' / 'figures'
    for name in NAMES:   # v20: Figures 1, 2 and 5 are recoloured; 3 and 4 are v19's bytes and scripts
        same = name in ('fig3-initial-qubit', 'fig4-convergence')
        assert (sha(FIG / f'{name}.pdf') == sha(v19 / f'{name}.pdf')) == same, name
    for script in ['fig3_initial_qubit.py', 'fig4_convergence.py']:
        assert (SRC / script).read_bytes() == (v19 / 'src' / script).read_bytes(), script
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
    for label in ['published (DeviceLayout)', 'Palace, p=1', 'Palace, p=2', 'geode-fem, p=1 (same meshes)',
                  'r=1 extrapolation of each pair (estimate = mean)', 'a = gmsh split of h', 'b = Palace refinement of h',
                  'static = bounded Cisl + circuit',
                  "Palace at DeviceLayout's discretization", 'extrapolation band (not a bound)', 'extrapolated estimate',
                  'static route (bounded Cisl + circuit)', 'circuit-model cap from the lower bound', 'h→0, a', 'static']:
        assert label in t2, label
    assert t2.count('h→0, a') == 4 and 'Σ' not in t2 and 'second refinement family' not in t2
    assert 'order' not in t2, 'element order is p, the rate is r (v19)'
    t1 = every['fig1-geometry']
    for label in ['(a) Conductors, design parameters and box', 'length L (total_length)', 'Box wall', 'stretched strip',
                  'Junction', 'junction, 1 × 1 µm, 4 triangles', 'island lead', 'Readout resonator', 'Ground',
                  'translated by Δc', '(c) Cross-section (schematic)', '(d) Two-node circuit', 'CΣ Ls LJ', 'resonator k']:
        assert label in t1, label
    for name in ['fig2-crosscheck', 'fig4-convergence']:
        assert 'Σ' not in every[name], name
    for name in NAMES:
        assert 'order' not in every[name], name
    t3 = every['fig4-convergence']
    sb = toml('benchmarks/transmon_diffopt/c_sigma_bracket.toml')['bracket_x0']
    tens = toml('benchmarks/transmon_diffopt/c_sigma_bracket_tensor.toml')
    tb, gap = tens['bracket_x0_tensor'], tens['gap_x0']
    for b in [sb, tb]:
        assert f"{b['lower_ff']:.2f}–{b['upper_ff']:.2f} fF" in t3
    for label in ['hF hF/2 hF/4', '(a) Uniform refinement: upper bounds only', '(b) Adaptive, two-sided',
                  '(b) upper bound per pass (P2)', '(b) lower bound per pass']:
        assert label in t3, label
    rel = gap['tensor_minus_scalar_relative']
    # A bound, so rounded outward: against the floored/ceiled fF gap and the scalar bracket too.
    lo = min(rel[0], gap['tensor_minus_scalar_lower_ff'] / gap['scalar_upper_ff'])
    hi = max(rel[1], gap['tensor_minus_scalar_upper_ff'] / gap['scalar_lower_ff'])
    shown = (math.floor(1000 * lo) / 10, math.ceil(1000 * hi) / 10)
    assert shown == (2.5, 3.1), shown
    assert f"gap {shown[0]:.1f}–{shown[1]:.1f}%" in t3
    assert 'gap 2.5–3.1%' in t3 and 'P2, sapphire tensor' in t3 and 'bounded, tensor' in t3 and 'two-sided' in t3
    assert 'C isl , unmodified device (fF)' in t3 and 'of X' not in t3
    assert 'Every marker is an upper bound' not in t3   # the caption says it; the figure no longer repeats it
    tq = every['fig3-initial-qubit']
    amr = toml('benchmarks/transmon_devicelayout_controls/results.toml')['amr']['initial']
    for label in ['family a (gmsh refinement)', 'family b (Palace refinement)', 'Palace adaptive refinement',
                  'circuit-model cap', 'extrapolation band (not a bound)', 'estimates', 'Nédélec unknowns',
                  'a b fit (b) estimates', 'a b family a, b (d) estimates', 'h h/2 h/4 1 2 3 4 5',
                  f"pass 5: {amr['last_change_f_q_pct']:+.2f}% (still rising)", 'size limit, 107']:
        assert label in tq, label
    assert tq.count('fit') == 1, 'the adaptive fit is drawn for the qubit only'
    t4 = every['fig5-reoptimization']
    assert 'PRIMA, published (DeviceLayout environment)' not in t4
    for label in ['PRIMA, published (its environment, published err)', 'PRIMA, our environment', 'Newton, shape derivative',
                  'Newton, scaling law', 'Newton, scaling law, Palace-only δ', 'Broyden',
                  'Palace runs to the criterion', 'Extrapolated qubit', 'Extrapolated resonator',
                  'shape-derivative steps, extrapolated frequencies', 'scaling-law step, extrapolated frequencies',
                  'step-2 qubit band mapped to c (estimate)', 'estimate band (not a bound)', 'criterion',
                  'extrapolated estimate', 'target (3.0, 4.0 GHz)', 'Step 2, family b',
                  "(a) D: against DeviceLayout's discretization", '(a) E: against the extrapolated frequencies',
                  'c: 1218 (published) → 1446 μm (+19%)']:
        assert label in t4, label
    # The extrapolated-target steps carry Table 5's names, in (a) and as the (c, d) tick labels.
    assert t4.count('Step 1') == 3 and t4.count('Step 2') == 5, (t4.count('Step 1'), t4.count('Step 2'))
    for stale in ['cv1', 'cv2', 'Cost to the criterion', 'Converged qubit', '104', 'Palace evaluations']:
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
    check_v19_coordinates(load, plt)
    check_v20_colours(load, plt)


def check_v20_colours(load, plt):
    """v20: one colour per object. Figure 1's labels take their object's colour, the junction has one colour of its
    own in (a) and (b), and the design-parameter annotations share one colour that no object uses; Figure 2 keys the
    pair extrapolations with both markers; Figure 5 keeps each method's colour across panels and draws the mapped
    band in the estimate-band grey."""
    from matplotlib.colors import to_hex
    from matplotlib.collections import PolyCollection
    import importlib.util
    if str(SRC) not in sys.path:
        sys.path.insert(0, str(SRC))
    spec = importlib.util.spec_from_file_location('fig1src', SRC / 'fig1_geometry.py')
    f1 = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(f1)
    style = {k: to_hex(getattr(f1, k)).lower() for k in ['BLUE', 'TEAL', 'ORANGE', 'GROUND', 'JUNCTION', 'PARAM']}
    assert len(set(style.values())) == 6, style   # six distinct colours: five objects and the annotation colour
    fig = load('fig1', 'fig1_geometry.py')
    a, b, c, d = fig.axes[:4]
    hexc = lambda col: to_hex(col).lower()
    texts = {t.get_text(): t for ax in (a, b, c, d) for t in ax.texts}
    want = {'Feedline': 'TEAL', 'Readout resonator': 'ORANGE', 'Qubit island': 'BLUE', 'Junction': 'JUNCTION',
            'Ground': 'GROUND', 'Box wall': None, 'length $L$\n(total_length)': 'PARAM', '$c$': 'PARAM',
            'stretched strip': 'PARAM', 'translated\nby $\\Delta c$': 'PARAM',
            'junction,\n1 × 1 µm,\n4 triangles': 'JUNCTION', 'island\nlead': 'BLUE', 'island': 'BLUE',
            'resonator': 'ORANGE', '$C_\\Sigma$': 'BLUE', '$L_s$': 'BLUE', '$L_J$': 'JUNCTION'}
    for label, key in want.items():
        assert label in texts, label
        got = hexc(texts[label].get_color())
        assert got == (style[key] if key else '#000000'), (label, got)
    # The box wall is drawn in the colour of its label (black), in (a) and (c).
    for ax in (a, c):
        walls = [r for r in ax.patches if getattr(r, 'get_linestyle', lambda: '')() == '--']
        assert walls and all(hexc(r.get_edgecolor()) == '#000000' for r in walls)
    # The junction sheet is filled with the junction colour in (a) and (b); never the resonator's orange.
    for ax in (a, b):
        fills = {hexc(pc.get_facecolor()[0]) for pc in ax.collections if isinstance(pc, PolyCollection)}
        assert style['JUNCTION'] in fills and style['ORANGE'] not in (fills if ax is b else set()), fills
    # (d): L_J is the junction, so its coil is the junction colour; the rest of the island branch (C_Sigma, L_s,
    # the wires) stays blue. Coils are Arc patches: the L_s coil spans y 0.53-0.70, the L_J coil y 0.25-0.47.
    from matplotlib.patches import Arc
    arcs = [p for p in d.patches if isinstance(p, Arc) and abs(p.center[0] - .30) < 1e-9]
    lj = {hexc(p.get_edgecolor()) for p in arcs if p.center[1] < .5}
    ls = {hexc(p.get_edgecolor()) for p in arcs if p.center[1] > .5}
    assert lj == {style['JUNCTION']} and ls == {style['BLUE']}, (lj, ls)
    # The c dimension and the delta-c arrow are in the annotation colour.
    arrows = [t for t in a.texts if t.get_text() == '' and t.arrow_patch is not None]
    assert len(arrows) == 2 and all(hexc(t.arrow_patch.get_edgecolor()) == style['PARAM'] for t in arrows)
    plt.close('all')
    fig = load('fig2', 'fig2_crosscheck.py')
    from matplotlib.offsetbox import HPacker, TextArea
    from matplotlib.lines import Line2D
    from figure_style import BLUE, GREEN
    keys = []
    for hp in fig.legends[0].findobj(HPacker):
        ch = hp.get_children()
        if len(ch) == 2 and isinstance(ch[1], TextArea) and ch[1].get_text().startswith('$r = 1$ extrapolation'):
            keys.append([(hexc(l.get_color()), l.get_marker()) for l in ch[0].findobj(Line2D)
                         if l.get_marker() not in (None, 'None', '')])
    assert keys == [[(hexc(BLUE), 'o'), (hexc(GREEN), 's')]], keys   # the p = 1 and p = 2 markers, as drawn
    plt.close('all')
    fig = load('fig5', 'fig5_reoptimization.py')
    ax, bx, cq, cr = fig.axes[:4]
    lines = {l.get_label(): l for l in ax.get_lines()}
    assert hexc(lines['shape-derivative steps, extrapolated frequencies'].get_color()) == \
        hexc(lines['Newton, shape derivative'].get_color()) == hexc(BLUE)
    assert hexc(lines['scaling-law step, extrapolated frequencies'].get_color()) == \
        hexc(lines['Newton, scaling law'].get_color())
    band = lines['step-2 qubit band mapped to $c$ (estimate)']
    band_fill = {hexc(coll.get_facecolor()[0]) for coll in cq.collections}
    assert band_fill == {hexc(band.get_color())}, (band_fill, band.get_color())
    steps = [t for t in ax.texts if t.get_text() in ('Step 1', 'Step 2')]
    assert len(steps) == 2 and all(hexc(t.get_color()) == hexc(BLUE) for t in steps)
    plt.close('all')


def check_v19_coordinates(load, plt):
    """v19: Figure 1's c dimension and strip come from the mesh and the velocity bands; Figure 2's two pair
    extrapolations average to the estimate; Figure 4(b) draws every adaptive pass, upper and best lower."""
    run = toml('benchmarks/transmon_devicelayout_optimize/runs/fd_initial.toml')['run']
    y_min, y0, y1, yt, y_max = run['velocity_bands_um']
    fig = load('fig1', 'fig1_geometry.py')
    a = fig.axes[0]
    from matplotlib.patches import Rectangle
    strips = [r for r in a.patches if isinstance(r, Rectangle) and r.get_y() == y0 and abs(r.get_height() - (y1 - y0)) < 1e-9]
    assert len(strips) == 1, 'the stretched strip is drawn at the velocity bands'
    box = [r for r in a.patches if isinstance(r, Rectangle) and r.get_y() == y_min and r.get_linestyle() == '--']
    assert len(box) == 1 and abs(box[0].get_height() - (y_max - y_min)) < 1e-9, 'the box wall is the mesh box'
    arrows = [t for t in a.texts if getattr(t, 'xy', None) is not None and t.get_text() == '']
    assert any(t.xy[1] == y0 and abs(t.xyann[1] - (y0 + run['capacitor_length_um'])) < 1e-9 for t in arrows), \
        'the c dimension runs from the island bottom to 12 + c'
    plt.close('all')
    cc = toml('benchmarks/transmon_devicelayout_crosscheck/results.toml')
    fig = load('fig2', 'fig2_crosscheck.py')
    for k, (geom, mode) in enumerate([('initial', 'q'), ('initial', 'r'), ('final', 'q'), ('final', 'r')]):
        ax = fig.axes[k]
        p1 = cc['palace'][geom][f'p1_f_{mode}_ghz']
        p2 = cc['palace'][geom][f'p2_f_{mode}_ghz']
        e1, e2 = 2 * p1[2] - p1[1], 2 * p2[1] - p2[0]
        ys = {l.get_ydata()[-1] for l in ax.get_lines() if len(l.get_xdata()) in (1, 2)}
        assert e1 in ys and e2 in ys, (geom, mode)
        assert abs((e1 + e2) / 2 - cc['convergence'][geom][f'f_{mode}_ghz']) < 6e-5, (geom, mode)
    cap = [l for l in fig.axes[0].get_lines() if l.get_label() == 'circuit-model cap from the lower bound'][0]
    assert cap.get_xdata()[0] <= fig.axes[0].get_xlim()[0], 'the cap runs across the whole panel'
    plt.close('all')
    fig = load('fig4', 'fig4_convergence.py')
    bx = fig.axes[1]
    scal = toml('benchmarks/transmon_diffopt/c_sigma_bracket.toml')['adaptive_x0']
    tens = toml('benchmarks/transmon_diffopt/c_sigma_bracket_tensor.toml')['adaptive_x0_tensor']
    series = {tuple(l.get_ydata()) for l in bx.get_lines()}
    for rows in (scal, tens):
        assert tuple(r['upper_ff'] for r in rows) in series
        assert tuple(max(v for k, v in r.items() if k.startswith('lower_eq_k')) for r in rows) in series
    plt.close('all')


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
                    "refinement (b)", 'the static route of Section~\\ref{sec:static} (natural box; the grounded bound lowers its low end by about $0.3$\\,MHz)',
                    'the open diamond, just right of $h$, is Palace at DeviceLayout\'s own discretization',
                    'the circuit-model cap, which trims the untrimmed bands drawn',
                    'the estimate is their mean',
                    'Mesh convergence of the island capacitance $\\Cisl$',
                    'every marker is a conforming upper bound on the exact capacitance of the meshed polygonal geometry',
                    'two-sided bounds from adapted meshes', 'The adaptive passes behind those bars',
                    'natural box; the grounded box changes only the tensor upper bound, to $79.77$\\,fF',
                    'mapped linearly to capacitor\\_length (an estimate)',
                    "the published PRIMA point is at its 9 runs and the err of its published frequencies, "
                    "$1.151\\times10^{-5}$ (DeviceLayout's environment)",
                    "probes counted (the Palace-$\\delta$ Newton's run 2 is the $L$ probe it shares with Broyden)",
                    'Extrapolated estimates and bands against the targets, with step~2 in both refinement families',
                    'This counts Palace runs, not total cost',
                    'drawn at the tetrahedron count of the mesh that sets its lower bound',
                    "the adaptive run's ill-conditioned fit (the resonator fit is unreliable)",
                    'passes numbered in (a)', "the adaptive run's size limit, $10^7$ unknowns",
                    'the dashed outline is the box wall', '$c$ is the island length',
                    'The two-node circuit of Section~\\ref{sec:junction}',
                    'with the scalar trace-averaged permittivity (blue) and the sapphire tensor (orange)',
                    'Each label has the colour of the object it names; the design parameters are annotated in green',
                    'the junction (purple, as in (b) and as $L_J$ in (d)) joins the island to ground']:
        assert wording in norm, wording
    assert 'converged estimate' not in norm and 'conv.' not in norm


def main():
    check_inputs()
    check_regeneration()
    check_content()
    check_coordinates()
    check_manuscript()
    print('PASS (v20: Figures 1, 2 and 5 recoloured to one scheme, Figures 3 and 4 v19\'s bytes; Figure 1 labels in '
          'their objects\' colours, the junction in its own colour in (a) and (b), the design parameters in one '
          'annotation colour; Figure 2 pair key; Figure 5 method colours and band grey). v19: input hashes, byte-identical regeneration; Figure 1 c, L, box, strip and '
          'circuit; Figure 2 pair extrapolations averaging to the estimate and the full-width cap; Figure 3 levels, passes '
          'and size limit; Figure 4(b) adaptive sequences; Figure 5 regions and Palace runs; no "order" in any figure; '
          'Figure 2 labels clear of every series; widths, figure content '
          'against the artifacts, the bounded bars at their true mesh sizes, Figure 5 band, controls and second family, '
          'Figure 3 without the resonator fit, inclusion order and caption wording.')


if __name__ == '__main__':
    main()
