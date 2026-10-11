"""Self-contained print style for the v8 figures, with layout checks.

Every figure is drawn at most WIDTH inches wide and included in the
manuscript at 0.98 of the 6.5-inch text width (\\anvilfig default), so it is
never shrunk: a font drawn at MIN_PT points is at least MIN_PT points on the
printed page. save() refuses to write a figure that breaks any of these:

- the figure is wider than WIDTH;
- any visible text is smaller than MIN_PT;
- any visible text leaves the canvas;
- a legend overlaps any text outside itself;
- an annotation (ax.text / ax.annotate / fig.text) overlaps another
  annotation, a tick label, an axis label or a title.

The checks are geometric (text bounding boxes); they do not replace looking
at the rendered page.
"""
from pathlib import Path
import json
import tomllib
import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
from matplotlib.text import Text

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[4]
TEXT_WIDTH_IN = 6.5            # letter paper, 1-inch margins (anvil-paper.cls)
INCLUDE_FRACTION = 0.98        # \anvilfig default width in main.tex
WIDTH = 6.3                    # every figure is drawn at most this wide
MIN_PT = 8.0                   # smallest font allowed at drawing size

BLUE = "#23679A"          # physical qubit / primary optimized design
TEAL = "#16847B"          # independent check / feedline / cavity modes
GRAY = "#72777D"          # port-sheet artifact / initial geometry / historical
ORANGE = "#B65D16"        # junction / anchor or target
GREEN = "#5E8A2E"         # resolution-corrected design
plt.rcParams.update({
    "font.family": "DejaVu Sans", "font.size": 9,
    "axes.labelsize": 9, "axes.titlesize": 9,
    "xtick.labelsize": 8.5, "ytick.labelsize": 8.5, "legend.fontsize": 8.5,
    "axes.spines.top": False, "axes.spines.right": False,
    "axes.edgecolor": "#9CA3AA", "axes.linewidth": .7,
    "pdf.fonttype": 42, "savefig.facecolor": "white",
})


def eigen_data():
    return tomllib.loads((REPO / "benchmarks/transmon_eigen/results.toml").read_text())


def toml(rel):
    return tomllib.loads((REPO / rel).read_text())


def recorded(name):
    return json.loads((HERE / "data" / name).read_text())


def _visible(t):
    return t.get_visible() and t.get_text().strip() and t.get_alpha() != 0


def _overlap(a, b, pad=0.5):
    return (a.x0 + pad < b.x1 and b.x0 + pad < a.x1 and
            a.y0 + pad < b.y1 and b.y0 + pad < a.y1)


def check_layout(fig, name):
    """Raise AssertionError on any violation listed in the module docstring."""
    w, _ = fig.get_size_inches()
    assert w <= WIDTH + 1e-9, f"{name}: {w:.2f} in wide > {WIDTH}"
    fig.canvas.draw()
    r = fig.canvas.get_renderer()
    canvas = fig.bbox
    # Tick labels matplotlib will not draw (ticks outside the view) are skipped.
    drawn, undrawn = [], set()
    for ax in fig.axes:
        for axis in (ax.xaxis, ax.yaxis):
            shown = {id(t) for t in axis._update_ticks()}
            for tick in axis.get_major_ticks() + axis.get_minor_ticks():
                labels = (tick.label1, tick.label2)
                if id(tick) in shown:
                    drawn += [lb for lb in labels if _visible(lb)]
                else:
                    undrawn |= {id(lb) for lb in labels}
    texts = [t for t in fig.findobj(Text) if _visible(t) and id(t) not in undrawn]
    for t in texts:
        assert t.get_fontsize() >= MIN_PT - 1e-9, (name, t.get_text(), t.get_fontsize())
        bb = t.get_window_extent(r)
        assert (bb.x0 >= canvas.x0 - 1 and bb.x1 <= canvas.x1 + 1 and
                bb.y0 >= canvas.y0 - 1 and bb.y1 <= canvas.y1 + 1), (name, "off canvas", t.get_text())
    legends = [ax.get_legend() for ax in fig.axes if ax.get_legend()] + list(fig.legends)
    legend_texts = {id(t) for lg in legends for t in lg.get_texts() + [lg.get_title()]}
    fixed, free = list(drawn), []
    for ax in fig.axes:
        fixed += [t for t in (ax.xaxis.label, ax.yaxis.label, ax.title,
                              ax._left_title, ax._right_title) if _visible(t)]
        free += [t for t in ax.texts if _visible(t)]
    free += [t for t in fig.texts if _visible(t)]
    box = {id(t): t.get_window_extent(r) for t in fixed + free}
    for lg in legends:
        lb = lg.get_window_extent(r)
        for t in fixed + free:
            if id(t) not in legend_texts:
                assert not _overlap(lb, box[id(t)]), (name, "legend overlaps", t.get_text())
    for i, a in enumerate(free):
        for b in free[i + 1:] + fixed:
            assert not _overlap(box[id(a)], box[id(b)]), (name, "overlap", a.get_text(), b.get_text())
    final_min = MIN_PT * INCLUDE_FRACTION * TEXT_WIDTH_IN / w
    return final_min


def save(fig, name):
    final_min = check_layout(fig, name)
    path = HERE.parent / name
    fig.savefig(path, metadata={"CreationDate": None, "ModDate": None})
    plt.close(fig)
    print(f"{path}  (smallest font at print size >= {final_min:.2f} pt)")
