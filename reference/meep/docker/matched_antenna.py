#!/usr/bin/env python3
"""Matched initial bent-patch FDTD experiment and uniform-RC port oracle.

Length unit: mm; Meep frequency: f_GHz / 299.792458. Fields use exp(-i wt).
A resistive sheet and impressed current implement GEODE's tangential surface
port in the thin-volume limit. Voltage is the width-averaged gap integral.
This is true V/I impedance extraction, not a reflected-Ez objective.

PEC sheets are one-sided shells of thickness 1/resolution outside the
substrate. Finite shell/port thickness is a resolution-dependent approximation
that must converge. This script does not declare its own spatial convergence.
"""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import math
import platform
import resource
import time
from pathlib import Path

import numpy as np
import meep as mp
from mpi4py import MPI

C_MM_NS = 299.792458
ETA0 = 376.730313668
COMM = MPI.COMM_WORLD


def shell(radius_lo, radius_hi, half_angle, length, material, segments=256):
    """Closed ring-sector prism, extruded along y; cylinder centre z=-R."""
    phi = np.linspace(-half_angle, half_angle, segments+1)
    vertices = [mp.Vector3(r*math.sin(p), -length/2, r*math.cos(p)-100.)
                for r, angles in [(radius_hi, phi), (radius_lo, phi[::-1])]
                for p in angles]
    return mp.Prism(vertices=vertices, height=length, axis=mp.Vector3(0,1,0), material=material)


def antenna_setup(config, resolution, frequencies):
    p = config['patch']; d = config['domain']; m = config['materials']
    h, width = p['h'], p['probe_w']; dx = 1/resolution
    assert config['bend_radius_mm'] == 100., 'Current ring-sector helper uses R=100'
    sw, sl = p['patch_w']+2*p['sub_pad'], p['patch_l']+2*p['sub_pad']
    cell = mp.Vector3(sw+2*(d['air_margin']+d['pml_thick']),
                      sl+2*(d['air_margin']+d['pml_thick']),
                      h+2*(d['air_margin']+d['pml_thick']))
    fcen = float(np.mean(frequencies)); eps = m['substrate_eps_r']
    loss = 2*np.pi*fcen*m['substrate_tan_delta']
    fr4 = mp.Medium(epsilon=eps, D_conductivity=loss)
    # Conductivity in natural units: G_s = l/(R*w), R measured in eta0.
    conductivity = h*ETA0/(config['port_resistance_ohm']*width*dx)
    resistor = mp.Medium(epsilon=eps, D_conductivity_diag=mp.Vector3(
        loss, loss+conductivity/eps, loss+conductivity/eps))
    port = mp.Vector3(0, p['patch_l']/2-p['probe_inset'], 0)
    geometry = [
        shell(100-h/2, 100+h/2, sw/200, sl, fr4),
        mp.Block(center=port, size=mp.Vector3(dx,width,h), material=resistor),
        shell(100-h/2-dx, 100-h/2, sw/200, sl, mp.metal),
        shell(100+h/2, 100+h/2+dx, p['patch_w']/200, p['patch_l'], mp.metal),
    ]
    source_size = mp.Vector3(0,width,h)
    monitor_size = mp.Vector3(dx,width,h)
    # Flat-space equivalent Norton source: integrated current = width.
    return dict(cell=cell, geometry=geometry, boundary=[mp.PML(d['pml_thick'])],
                port=port, source_size=source_size, monitor_size=monitor_size,
                voltage_factor=1/(dx*width), current_factor=1/h,
                sigma=conductivity, epsilon=eps, loss=loss,
                reference_z=config['port_resistance_ohm'], shell_thickness_mm=dx)


def rc_setup(resolution, frequencies):
    """Spatially uniform periodic RC oracle, analytic capacitor impedance.

    Full-volume current drives a homogeneous periodic cell. No curl fields:
    J_in = sigma E + epsilon dE/dt, equivalent to a capacitor parallel to
    the source resistor. This validates signs, impedance units and source DFT.
    """
    length=2.; area=4.; eps=4.; resistance=50.
    sigma=ETA0*length/(resistance*area)
    return dict(cell=mp.Vector3(2,2,length), geometry=[], boundary=[],
                default_material=mp.Medium(epsilon=eps,D_conductivity_diag=mp.Vector3(0,0,sigma/eps)),
                port=mp.Vector3(), source_size=mp.Vector3(2,2,length),
                monitor_size=mp.Vector3(2,2,length), voltage_factor=1/area,
                current_factor=1/length, sigma=sigma, epsilon=eps, loss=0.,
                reference_z=resistance, analytic_capacitance_natural=eps*area/length)


def spectrum(sim, monitor, src, setup, frequencies, spatial_current):
    _, _, _, weights=sim.get_array_metadata(dft_cell=monitor)
    volts=[]
    for i in range(len(frequencies)):
        field=sim.get_dft_array(monitor,mp.Ez,i)
        volts.append(np.sum(weights*field)*setup['voltage_factor'])
    voltage=np.array(volts)
    # Meep 1.34 Gaussian transform omits the current/dipole f/fcen factor.
    # Forward-differenced dipole adds sinc(f*dt) exp(-i*pi*f*dt).
    # See src/meep.hpp: src_time::current and the uniform-RC oracle.
    source_transform=np.array([src.fourier_transform(f) * (f/src.frequency) * np.sinc(f*sim.fields.dt) * np.exp(-1j*np.pi*f*sim.fields.dt) for f in frequencies])
    # Maxwell's Ampere equation has -J_impressed on its RHS.
    injected=-spatial_current*source_transform
    load=voltage*ETA0/setup['reference_z']
    current=injected-load
    impedance=ETA0*voltage/current
    reflection=(impedance-setup['reference_z'])/(impedance+setup['reference_z'])
    return voltage, injected, impedance, reflection


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--config',type=Path)
    parser.add_argument('--resolution',type=float,default=1)
    parser.add_argument('--after-source',type=float,default=600)
    parser.add_argument('--check-interval',type=float,default=300)
    parser.add_argument('--rc-oracle',action='store_true')
    parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--max-estimated-gib',type=float,default=48)
    parser.add_argument('--max-checks',type=int,default=1)
    parser.add_argument('--temporal-tolerance',type=float,default=.001)
    args=parser.parse_args()
    for name in ['resolution','check_interval','max_estimated_gib','temporal_tolerance']:
        if not math.isfinite(getattr(args,name)) or getattr(args,name)<=0:
            parser.error(f'--{name.replace(chr(95),chr(45))} must be finite and positive')
    if not math.isfinite(args.after_source) or args.after_source<0 or args.max_checks<1:
        parser.error('--after-source must be finite and nonnegative; --max-checks >= 1')
    if mp.__version__ != '1.34.0':
        raise RuntimeError('Source Fourier correction is validated only for Meep 1.34.0')
    source_hash=hashlib.sha256(Path(__file__).read_bytes()).hexdigest()
    started_utc=datetime.now(timezone.utc).isoformat()
    config=json.loads(args.config.read_text()) if args.config else None
    if not args.rc_oracle and config is None: parser.error('--config required for antenna')
    frequencies=np.linspace(2.2,2.7,51)/C_MM_NS
    setup=rc_setup(args.resolution,frequencies) if args.rc_oracle else antenna_setup(config,args.resolution,frequencies)
    cells=int(np.prod([math.ceil(s*args.resolution) for s in setup['cell']]))
    # Conservative preflight only, not a measurement. Includes monitor overhead allowance.
    estimated_gib=cells*400/2**30
    if estimated_gib>args.max_estimated_gib:
        raise RuntimeError(f'Preflight: {cells} cells, estimated {estimated_gib:.1f} GiB exceeds {args.max_estimated_gib} GiB; no allocation attempted')
    fcen=float(np.mean(frequencies))
    src=mp.GaussianSource(frequency=fcen,fwidth=.012,cutoff=5)
    sim=mp.Simulation(cell_size=setup['cell'],resolution=args.resolution,
        geometry=setup['geometry'],boundary_layers=setup['boundary'],
        k_point=mp.Vector3() if args.rc_oracle else False,
        default_material=setup.get('default_material',mp.air),eps_averaging=False,force_complex_fields=True,
        sources=[mp.Source(src,mp.Ez,center=setup['port'],size=setup['source_size'])])
    monitor=sim.add_dft_fields([mp.Ez],frequencies,center=setup['port'],size=setup['monitor_size'],yee_grid=False)
    start=time.perf_counter()
    sim.init_sim()
    # Meep normalizes spatial source weights to their geometric integral.
    # Interpolating get_source onto a monitor grid loses endpoint weights.
    spatial_current=(4. if args.rc_oracle else config['patch']['probe_w'])
    snapshots=[]
    sim.run(until_after_sources=args.after_source)
    previous_reflection=None
    drift=float("inf")
    for check in range(args.max_checks+1):
        if check:sim.run(until=args.check_interval)
        voltage, injected, impedance, reflection=spectrum(sim,monitor,src,setup,frequencies,spatial_current)
        snapshots.append({'time':sim.meep_time(),'s11':[[float(z.real),float(z.imag)] for z in reflection],
                          'impedance_ohm':[[float(z.real),float(z.imag)] for z in impedance]})
        if previous_reflection is not None:
            drift=float(np.max(np.abs(reflection-previous_reflection)))
            if drift<=args.temporal_tolerance:break
        previous_reflection=reflection.copy()
    wall=COMM.allreduce(time.perf_counter()-start,op=MPI.MAX)
    rss=COMM.allreduce(resource.getrusage(resource.RUSAGE_SELF).ru_maxrss*1024,op=MPI.SUM)
    result={'schema':'geode-matched-meep/1','meep_version':mp.__version__,
            'started_utc':started_utc,'after_source':args.after_source,
            'check_interval':args.check_interval,'max_checks':args.max_checks,
            'temporal_tolerance':args.temporal_tolerance,
            'temporal_tolerance_met':drift<=args.temporal_tolerance,
            'mpi_ranks':mp.count_processors(),'architecture':platform.machine(),
            'time_step':sim.fields.dt,'estimated_memory_gib':estimated_gib,
            'resolution_cells_per_mm':args.resolution,'cell_mm':list(setup['cell']),
            'estimated_cells':cells,'wall_seconds':wall,'sum_rank_peak_rss_bytes':rss,
            'rss_note':'Sum of per-rank high-water marks; conservative, not simultaneous aggregate RSS.',
            'configuration':config,'rc_oracle':args.rc_oracle,
            'frequency_ghz':list(frequencies*C_MM_NS),'snapshots':snapshots,
            'max_complex_s11_temporal_change':drift,'spatially_converged':False,
            'source_spatial_current':[float(spatial_current.real),float(spatial_current.imag)],
            'voltage_dft':[[float(z.real),float(z.imag)] for z in voltage],
            'injected_current_dft':[[float(z.real),float(z.imag)] for z in injected],
            'source_sha256':source_hash,
            'limitations':['Finite PEC shell and resistor volume thickness = one grid cell.',
                'Dielectric conductivity matches tan(delta) only at the band centre.',
                'Meep exp(-i wt); conjugate S11 to compare with GEODE exp(+i wt).',
                'No spatial or cost-to-accuracy verdict from a single resolution.']}
    if args.rc_oracle:
        expected=1j*ETA0/(2*np.pi*frequencies*setup['analytic_capacitance_natural'])
        result['max_rc_impedance_relative_error']=float(np.max(np.abs(impedance/expected-1)))
        result['expected_impedance_ohm']=[[float(z.real),float(z.imag)] for z in expected]
    if mp.am_master():
        args.output.parent.mkdir(parents=True,exist_ok=True)
        args.output.write_text(json.dumps(result,indent=2,allow_nan=False)+'\n')
        print('RESULT',args.output,'wall',wall,'RSS',rss,'DFT drift',drift,
              'RC error',result.get('max_rc_impedance_relative_error'))


if __name__=='__main__':main()
