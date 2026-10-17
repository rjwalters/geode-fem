"""Re-express reference/gmsh/spiral_inductor.geo as `geode mesh` layouts (issue #704).

Writes crates/geode-cli/tests/fixtures/spiral_layout_{smoke,benchmark}.json:
the spiral_3p5_{smoke,generic}.yaml geometries (half-integer turns only)
as layout schema v1 -- the same leg / stub / underpass / via boxes as the
.geo loop, as PEC-shell conductor layers m1 / m2 / via, with a gap port
between the named `feed` and `return` stubs.

    python3 reference/gmsh/generate_spiral_layout.py crates/geode-cli/tests/fixtures
"""
import json, math, sys

def layout(name, desc, sp, st, mesh):
    n_turns, w, s, d_in, g, feed_len, stub_len = (sp[k] for k in ("n_turns","w","s","d_in","g","feed_len","stub_len"))
    p = w + s
    n_legs = round(4 * n_turns)
    assert n_legs % 4 == 2, "half-integer turns only"
    dirx = [-1, 0, 1, 0]; diry = [0, -1, 0, 1]
    cx = cy = d_in / 2
    def rect(x0, y0, x1, y1, nm=None):
        d = {"outer": [[x0, y0], [x1, y0], [x1, y1], [x0, y1]]}
        if nm: d = {"name": nm, **d}
        return d
    m2 = []
    for i in range(n_legs):
        L = d_in + (i // 2) * p
        nx = cx + dirx[i % 4] * L; ny = cy + diry[i % 4] * L
        m2.append(rect(min(cx, nx) - w/2, min(cy, ny) - w/2, max(cx, nx) + w/2, max(cy, ny) + w/2, f"leg{i}"))
        cx, cy = nx, ny
    x_out, y_out = cx, cy
    x_in = y_in = d_in / 2
    y_feed_end = y_out - feed_len
    m2.append(rect(x_out - w/2, y_feed_end, x_out + w/2, y_out + w/2, "feed"))
    y_ret_end = y_feed_end - g
    y_via2 = y_ret_end - stub_len
    m2.append(rect(x_out - w/2, y_via2 - w/2, x_out + w/2, y_ret_end, "return"))
    m1 = [
        rect(min(x_out, x_in) - w/2, y_via2 - w/2, max(x_out, x_in) + w/2, y_via2 + w/2, "underpass_x"),
        rect(x_in - w/2, min(y_via2, y_in) - w/2, x_in + w/2, max(y_via2, y_in) + w/2, "underpass_y"),
    ]
    vias = [
        rect(x_out - w/2, y_via2 - w/2, x_out + w/2, y_via2 + w/2, "via2"),
        rect(x_in - w/2, y_in - w/2, x_in + w/2, y_in + w/2, "via1"),
    ]
    h_sub, h_ox, z1, t1, z2, t2, h_air, h_buf = (st[k] for k in ("h_sub","h_ox","z1_bot","t1","z2_bot","t2","h_air","h_buf"))
    return {
        "schema_version": 1,
        "description": desc,
        "length_unit_m": 1e-6,
        "dielectrics": [
            {"name": "substrate", "z_bottom": -h_sub, "thickness": h_sub, "eps_r": [11.9, -0.0595]},
            {"name": "dielectric", "z_bottom": 0.0, "thickness": h_ox, "eps_r": [4.0, -0.004]},
            {"name": "air", "z_bottom": h_ox, "thickness": h_air},
            {"name": "air_buffer", "z_bottom": h_ox + h_air, "thickness": h_buf},
        ],
        "conductors": [
            {"name": "m1", "z_bottom": z1, "thickness": t1, "polygons": m1},
            {"name": "m2", "z_bottom": z2, "thickness": t2, "polygons": m2},
            {"name": "via", "z_bottom": z1, "thickness": z2 + t2 - z1, "polygons": vias},
        ],
        "ports": [{"name": "port", "layer": "m2", "between": ["feed", "return"], "resistance_ohm": 50.0}],
        "margin": st["margin"],
        "boundary": {"kind": "pec"},
        "mesh": mesh,
    }

def fmt(o):
    return json.dumps(o, indent=2)

smoke = layout("smoke",
    "3.5-turn square spiral (reference/gmsh/spiral_3p5_smoke.yaml geometry) re-expressed as a geode layout v1: PEC-shell m1/m2/vias, lumped gap port between the feed and return stubs (issue #704 golden test, smoke tier).",
    dict(n_turns=3.5, w=6.0, s=4.0, d_in=40.0, g=4.0, feed_len=12.0, stub_len=8.0),
    dict(h_sub=18.0, h_ox=12.0, z1_bot=2.0, t1=2.0, z2_bot=6.5, t2=3.0, h_air=12.0, h_buf=8.0, margin=18.0),
    {"size_max": 26.0, "size_conductor": 7.0, "size_port": 2.5, "near_distance": 6.0, "far_distance": 24.0})
bench = layout("benchmark",
    "3.5-turn square spiral (reference/gmsh/spiral_3p5_generic.yaml geometry: w 6, s 4, d_in 60 um) re-expressed as a geode layout v1: PEC-shell m1/m2/vias, lumped gap port between the feed and return stubs (issue #704 golden test, benchmark tier vs the issue-#211 Mohan / mom oracle bands).",
    dict(n_turns=3.5, w=6.0, s=4.0, d_in=60.0, g=4.0, feed_len=20.0, stub_len=8.0),
    dict(h_sub=50.0, h_ox=10.0, z1_bot=1.0, t1=2.0, z2_bot=5.0, t2=3.0, h_air=25.0, h_buf=25.0, margin=45.0),
    {"size_max": 30.0, "size_conductor": 4.0, "size_port": 1.5, "near_distance": 6.0, "far_distance": 45.0})
out = sys.argv[1]
open(f"{out}/spiral_layout_smoke.json", "w").write(fmt(smoke) + "\n")
open(f"{out}/spiral_layout_benchmark.json", "w").write(fmt(bench) + "\n")
