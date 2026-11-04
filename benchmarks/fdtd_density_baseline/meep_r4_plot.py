#!/usr/bin/env python3
"""Render the R1/R2/R4 matched bent-patch spectra (#1088); no spatial convergence claim."""
import argparse
import hashlib
import json
from pathlib import Path

import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
from matplotlib.patches import Patch
import numpy as np

HERE = Path(__file__).resolve().parent


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--r2', type=Path, default=HERE / 'meep_runs/aws_pilot/initial_R2.json')
    parser.add_argument('--r4', type=Path, default=HERE / 'meep_runs/aws_r4/initial_R4.json')
    parser.add_argument('--output', type=Path, default=HERE / 'meep_runs/aws_r4/spectra.png')
    args = parser.parse_args()
    plt.rcParams.update({'font.family': 'DejaVu Sans', 'font.size': 10,
                         'axes.spines.top': False, 'axes.spines.right': False,
                         'pdf.fonttype': 42, 'ps.fonttype': 42})
    fig, axes = plt.subplots(1, 3, figsize=(12.5, 3.9), layout='constrained')
    paths = [HERE / 'meep_runs/initial_R1.json', args.r2, args.r4]
    provenance = {}
    stopped = []
    for path, color in zip(paths, ['#94a3b8', '#0072b2', '#d55e00']):
        run = json.loads(path.read_text())
        f = np.array(run['frequency_ghz'])
        gamma = np.array([complex(*z) for z in run['snapshots'][-1]['s11']])
        z = np.array([complex(*z) for z in run['snapshots'][-1]['impedance_ohm']])
        label = f"R{run['resolution_cells_per_mm']:g} ({1/run['resolution_cells_per_mm']:g} mm cells)"
        axes[0].plot(f, 20*np.log10(np.abs(gamma)), color=color, label=label, lw=2)
        axes[1].plot(f, z.real, color=color, label=label, lw=2)
        axes[2].plot(f, z.imag, color=color, label=label, lw=2)
        provenance[path.name] = hashlib.sha256(path.read_bytes()).hexdigest()
        stopped.append(run['temporal_tolerance_met'])
    for ax in axes:
        ax.set_xlabel('Frequency (GHz)')
        ax.set_xlim(2.2, 2.7)
        ax.grid(alpha=.18)
        ax.axvspan(2.4, 2.4835, color='#e69f00', alpha=.12, zorder=0)
    axes[0].set_ylabel(r'$20\log_{10}|S_{11}|$ (dB)')
    handles, labels = axes[0].get_legend_handles_labels()
    handles.append(Patch(facecolor='#e69f00', alpha=.12))
    labels.append('2.4–2.4835 GHz band')
    axes[0].legend(handles, labels, frameon=False, loc='best', fontsize=9)
    axes[0].set_title('(a) Reflection magnitude', loc='left')
    axes[1].set_ylabel(r'Re($Z$) (Ω)')
    axes[1].set_title('(b) Port resistance', loc='left')
    axes[2].set_ylabel(r'Im($Z$), Meep convention (Ω)')
    axes[2].axhline(0, color='#333333', lw=.8, ls='--')
    axes[2].set_title('(c) Port reactance', loc='left')
    status = 'Temporal stopping met' if all(stopped) else 'Temporal stopping NOT met in all runs'
    fig.suptitle('Initial bent patch: FDTD refinement R1/R2/R4\n'
                 + status + '; spatial convergence unestablished', fontsize=11)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    fig.savefig(args.output, dpi=200)
    fig.savefig(args.output.with_suffix('.pdf'))
    plt.close(fig)
    provenance['generator_sha256'] = hashlib.sha256(Path(__file__).read_bytes()).hexdigest()
    provenance['matplotlib_version'] = matplotlib.__version__
    args.output.with_suffix('.provenance.json').write_text(json.dumps(provenance, indent=2)+'\n')


if __name__ == '__main__':
    main()
