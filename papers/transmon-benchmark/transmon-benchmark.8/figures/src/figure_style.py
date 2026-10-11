"""Self-contained print style; all figures are designed for a 6.4-inch width."""
from pathlib import Path
import json
import tomllib
import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[4]
BLUE = "#23679A"          # physical qubit / optimized design
TEAL = "#16847B"          # independent validation / feedline
GRAY = "#72777D"          # port-sheet artifact / historical result
ORANGE = "#B65D16"        # junction / target
plt.rcParams.update({
    "font.family": "DejaVu Sans", "font.size": 9,
    "axes.labelsize": 10, "axes.titlesize": 10,
    "xtick.labelsize": 9, "ytick.labelsize": 9, "legend.fontsize": 9,
    "axes.spines.top": False, "axes.spines.right": False,
    "axes.edgecolor": "#9CA3AA", "axes.linewidth": .7,
    "pdf.fonttype": 42, "savefig.facecolor": "white",
})

def eigen_data():
    return tomllib.loads((REPO / "benchmarks/transmon_eigen/results.toml").read_text())

def recorded(name):
    return json.loads((HERE / "data" / name).read_text())

def save(fig, name):
    path = HERE.parent / name
    fig.savefig(path, metadata={"CreationDate": None, "ModDate": None})
    plt.close(fig)
    print(path)
