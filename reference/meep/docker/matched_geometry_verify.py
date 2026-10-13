#!/usr/bin/env python3
"""Check curved finite materials and port placement without a field solve."""
import json
import math
from pathlib import Path

import yaml

import matched_antenna as a


def main():
    config = json.loads(Path(__file__).with_name('matched_initial_2g4.json').read_text())
    upstream = yaml.safe_load(Path(__file__).with_name('matched_initial_2g4.source.yaml').read_text())
    for key in ['patch', 'domain', 'materials', 'band']:
        assert config[key] == upstream[key], key
    assert config['bend_radius_mm'] == upstream['bend']['bend_radius']
    assert config['port_resistance_ohm'] == upstream['solver']['port_resistance_ohm']
    for resolution in (1, 2, 4, 8):
        s = a.antenna_setup(config, resolution, [2.45/a.C_MM_NS])
        substrate, port, ground, patch = s['geometry']
        def pt(radius, angle=0, y=0):
            return a.mp.Vector3(radius*math.sin(angle), y, radius*math.cos(angle)-100)
        assert pt(100) in substrate
        assert pt(98) not in substrate and pt(102) not in substrate
        assert pt(100, .3, 26) in substrate
        assert pt(100, .32) not in substrate and pt(100, 0, 27) not in substrate
        assert pt(100.8+.5/resolution, .1, 14) in patch
        assert pt(100.8+.5/resolution, .2) not in patch
        assert pt(100.8+.5/resolution, 0, 15) not in patch
        assert pt(99.2-.5/resolution, .3, 26) in ground
        assert pt(99.2-.5/resolution, .32) not in ground
        assert pt(99.2-.5/resolution, 0, 27) not in ground
        assert pt(100) not in patch and pt(100) not in ground
        assert s['port'] == a.mp.Vector3(0, 7.5, 0)
        assert s['port'] in port
        assert tuple(s['cell']) == (192, 183, 131.6)
        assert s['source_size'] == a.mp.Vector3(0, 1.6, 1.6)
        assert abs(port.material.D_conductivity_diag.y-port.material.D_conductivity_diag.z) < 1e-14
    print('PASS: curved finite patch/substrate/ground, fixed inner radii, physical port, box')


if __name__ == '__main__':
    main()
