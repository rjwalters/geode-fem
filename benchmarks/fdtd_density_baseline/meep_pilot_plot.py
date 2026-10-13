#!/usr/bin/env python3
"""Render coarse initial-antenna spectra; no spatial convergence claim."""
import argparse
import hashlib
import json
from pathlib import Path

import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
import numpy as np

HERE = Path(__file__).resolve().parent


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--results', type=Path, default=HERE / 'meep_runs/aws_pilot')
    parser.add_argument('--output', type=Path, default=HERE / 'meep_runs/aws_pilot/spectra.png')
    args = parser.parse_args()
    plt.rcParams.update({'font.family': 'DejaVu Sans', 'font.size': 10,
                         'axes.spines.top': False, 'axes.spines.right': False,
                         'pdf.fonttype': 42, 'ps.fonttype': 42})
    fig, axes = plt.subplots(1, 2, figsize=(9.0, 3.8), layout='constrained')
    paths = [HERE / 'meep_runs/initial_R1.json', args.results / 'initial_R2.json']
    provenance = {}
    for path, color in zip(paths, ['#64748b', '#0072b2']):
        run = json.loads(path.read_text())
        f = np.array(run['frequency_ghz'])
        gamma = np.array([complex(*z) for z in run['snapshots'][-1]['s11']])
        z = np.array([complex(*z) for z in run['snapshots'][-1]['impedance_ohm']])
        label = f"R{run['resolution_cells_per_mm']:g} ({1/run['resolution_cells_per_mm']:g} mm cells)"
        axes[0].plot(f, 20*np.log10(np.abs(gamma)), color=color, label=label, lw=2)
        axes[1].plot(f, z.imag, color=color, label=label, lw=2)
        provenance[path.name] = hashlib.sha256(path.read_bytes()).hexdigest()
    for ax in axes:
        ax.set_xlabel('Frequency (GHz)')
        ax.set_xlim(2.2, 2.7)
        ax.grid(alpha=.18)
        ax.axvspan(2.4, 2.4835, color='#e69f00', alpha=.12, zorder=0)
    axes[0].set_ylabel(r'$20\log_{10}|S_{11}|$ (dB)')
    axes[0].legend(frameon=False, loc='best')
    axes[0].set_title('(a) Reflection magnitude', loc='left')
    axes[1].set_ylabel(r'Im($Z$), Meep convention (Ω)')
    axes[1].axhline(0, color='#333333', lw=.8, ls='--')
    axes[1].set_title('(b) Port reactance', loc='left')
    fig.suptitle('Initial bent patch: coarse FDTD refinement\n'
                 'Spatial convergence unestablished · shading: 2.4–2.4835 GHz', fontsize=11)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    fig.savefig(args.output, dpi=200)
    fig.savefig(args.output.with_suffix('.pdf'))
    plt.close(fig)
    provenance['generator_sha256'] = hashlib.sha256(Path(__file__).read_bytes()).hexdigest()
    provenance['matplotlib_version'] = matplotlib.__version__
    args.output.with_suffix('.provenance.json').write_text(json.dumps(provenance, indent=2)+'\n')


if __name__ == '__main__':
    main()
