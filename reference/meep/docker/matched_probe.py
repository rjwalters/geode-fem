#!/usr/bin/env python3
"""Bounded full-domain allocation/stepping probe; produces no S11 accuracy claim."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import math
from pathlib import Path
import platform
import resource
import time

import meep as mp
import numpy as np
from mpi4py import MPI
from matched_antenna import antenna_setup, C_MM_NS


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--config', type=Path, required=True)
    parser.add_argument('--resolution', type=float, required=True)
    parser.add_argument('--steps', type=int, default=100)
    parser.add_argument('--max-estimated-gib', type=float, default=48)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    if args.steps < 1 or not math.isfinite(args.resolution) or args.resolution <= 0:
        parser.error('positive steps and finite positive resolution required')
    if not math.isfinite(args.max_estimated_gib) or args.max_estimated_gib <= 0:
        parser.error('finite positive memory cap required')
    config = json.loads(args.config.read_text())
    frequencies = np.linspace(2.2, 2.7, 51) / C_MM_NS
    setup = antenna_setup(config, args.resolution, frequencies)
    cells = math.prod(math.ceil(s * args.resolution) for s in setup['cell'])
    if cells * 400 / 2**30 > args.max_estimated_gib:
        raise RuntimeError('Planning memory cap exceeded; no allocation attempted')
    source = mp.GaussianSource(frequency=float(np.mean(frequencies)), fwidth=.012, cutoff=5)
    sim = mp.Simulation(cell_size=setup['cell'], resolution=args.resolution,
        geometry=setup['geometry'], boundary_layers=setup['boundary'],
        eps_averaging=False, force_complex_fields=True,
        sources=[mp.Source(source, mp.Ez, center=setup['port'], size=setup['source_size'])])
    # Include the production DFT monitor in the allocation and per-step cost.
    sim.add_dft_fields([mp.Ez], frequencies, center=setup['port'],
                       size=setup['monitor_size'], yee_grid=False)
    comm = MPI.COMM_WORLD
    started = datetime.now(timezone.utc).isoformat()
    comm.Barrier()
    start = time.perf_counter()
    sim.init_sim()
    init = comm.allreduce(time.perf_counter() - start, op=MPI.MAX)
    comm.Barrier()
    start = time.perf_counter()
    sim.run(until=args.steps * sim.fields.dt)
    stepping = comm.allreduce(time.perf_counter() - start, op=MPI.MAX)
    rss = comm.allreduce(resource.getrusage(resource.RUSAGE_SELF).ru_maxrss * 1024, op=MPI.SUM)
    result = dict(schema='geode-meep-probe/1', started_utc=started,
        meep_version=mp.__version__, architecture=platform.machine(),
        mpi_ranks=mp.count_processors(), resolution_cells_per_mm=args.resolution,
        estimated_cells=cells, initialization_seconds=init,
        requested_steps=args.steps, actual_steps=sim.meep_time()/sim.fields.dt,
        stepping_seconds=stepping, time_step=sim.fields.dt,
        sum_rank_peak_rss_bytes=rss, configuration=config,
        source_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        antenna_source_sha256=hashlib.sha256(Path(__file__).with_name('matched_antenna.py').read_bytes()).hexdigest(),
        limitations=['Short startup stepping sample, not steady-state or full-run timing.',
                     'Sum of rank peak RSS is not simultaneous aggregate RSS.',
                     'No S11, temporal convergence, spatial convergence or cost-to-accuracy result.'])
    if mp.am_master():
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(result, indent=2, allow_nan=False) + '\n')
        print('PROBE_RESULT', args.output, init, stepping, rss)


if __name__ == '__main__':
    main()
