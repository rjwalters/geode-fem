// Square planar spiral inductor in a layered semiconductor process stack
// (Epic #193 Phase 3, issue #210).
//
// Generates a FEM-ready tagged tet mesh: a square spiral on the top metal
// layer (m2), an underpass return path on the lower metal layer (m1)
// connected through vias, all embedded in an oxide ("dielectric") slab on
// a silicon substrate, with air above. The conductor interior is
// EXCLUDED from the mesh (boolean-subtracted cavity): the driven solver
// treats the cavity walls with the Leontovich surface-impedance BC
// (issue #204 / PR #207) or as PEC, so no skin-depth meshing is needed
// (documented choice per issue #210).
//
// Topology of the conductor loop (single connected solid, broken only at
// the port gap):
//
//   feed stub (m2) -> square spiral (m2, n_turns) -> via1 -> vertical
//   underpass run (m1) -> horizontal underpass run (m1) -> via2 ->
//   return stub (m2) -> [port gap g] -> back to feed stub
//
// The lumped-port surface is a horizontal rectangle spanning the gap
// between the feed-stub and return-stub end faces, embedded conformally
// in the oxide mesh at the m2 mid-height plane. The exit direction
// depends on the turn count (see the outer-terminal block below):
// half-integer turns exit -y (return stub below the feed), integer
// turns exit +y (return stub above the feed). Either way the gap
// direction is along y: e_hat = +-y, width = trace width w,
// length = gap g  =>  Z_s = R*w/g (Palace-style uniform port,
// lumped_port.rs; Z = V/I is invariant under e_hat -> -e_hat since V
// and I flip together).
//
// Physical groups (consumed by crates/geode-core/src/mesh/spiral.rs —
// keep the (dim, tag, name) table below in sync with the PHYS_*
// constants there):
//
//   | dim | tag | name                | meaning                                |
//   |-----|-----|---------------------|----------------------------------------|
//   | 3   | 1   | substrate           | silicon slab, z in [-h_sub, 0]         |
//   | 3   | 2   | dielectric          | oxide slab minus conductor cavity      |
//   | 3   | 3   | air                 | air above the oxide (core region)      |
//   | 3   | 4   | air_buffer          | top air slab reserved for UPML         |
//   | 2   | 11  | port                | lumped-port rectangle across the gap   |
//   | 2   | 12  | conductor_surface   | cavity walls (Leontovich / PEC)        |
//   | 2   | 13  | outer_boundary      | all six outer walls of the domain      |
//
// Units: microns. In the solver's natural units omega = k0 is then in
// rad/um (k0 = 2*pi*f/c with c = 3e14 um/s).
//
// Run via the wrapper (recommended — adds mesh-quality checks +
// provenance):
//   python3 reference/gmsh/generate_spiral_fixture.py \
//       reference/gmsh/spiral_3p5_generic.yaml \
//       crates/geode-core/tests/fixtures/spiral_3p5.msh
//
// or directly:
//   gmsh -3 -format msh41 -o spiral_3p5.msh reference/gmsh/spiral_inductor.geo
//
// All parameters below can be overridden with -setnumber on the gmsh
// command line (the wrapper does exactly that from the YAML).

SetFactory("OpenCASCADE");

// ---------------------------------------------------------------------------
// Parameters (microns) — defaults reproduce the committed fixture.
// ---------------------------------------------------------------------------

// Spiral (square; mirrors mom-geom SpiralParams: n_turns, w, s, d_in).
DefineConstant[ n_turns = 3.5  ];  // number of turns (quarter-turn resolution)
DefineConstant[ w       = 6.0  ];  // trace width
DefineConstant[ s       = 4.0  ];  // turn-to-turn spacing
DefineConstant[ d_in    = 60.0 ];  // inner diameter (center opening)
DefineConstant[ g       = 4.0  ];  // port gap between feed and return stubs
DefineConstant[ feed_len = 20.0 ]; // feed-stub extension beyond the spiral
DefineConstant[ stub_len = 8.0 ];  // return-stub length (port gap to via2)

// Layer stack (PDK-style 2-metal back end; mirrors mom-geom LayerStack).
DefineConstant[ h_sub  = 50.0 ];   // substrate thickness (domain floor)
DefineConstant[ h_ox   = 10.0 ];   // oxide (dielectric) thickness
DefineConstant[ z1_bot = 1.0  ];   // m1 bottom (underpass layer)
DefineConstant[ t1     = 2.0  ];   // m1 thickness
DefineConstant[ z2_bot = 5.0  ];   // m2 bottom (spiral layer)
DefineConstant[ t2     = 3.0  ];   // m2 thickness
DefineConstant[ h_air  = 25.0 ];   // air core height above the oxide
DefineConstant[ h_buf  = 25.0 ];   // air buffer (UPML-reserved) height

// Domain margin around the conductor footprint.
DefineConstant[ margin = 45.0 ];

// Mesh sizing.
DefineConstant[ lc_cond = 3.0  ];  // target size on conductor surfaces
DefineConstant[ lc_port = 1.5  ];  // target size on the port rectangle
DefineConstant[ lc_far  = 22.0 ];  // far-field size
DefineConstant[ dist_far = 55.0 ]; // distance over which lc grows to lc_far

// ---------------------------------------------------------------------------
// Derived geometry.
// ---------------------------------------------------------------------------

p = w + s;                       // turn pitch
z1_top = z1_bot + t1;
z2_top = z2_bot + t2;
z2_mid = z2_bot + t2 / 2;

// Square-spiral centerline. Left-turn (CCW in the (dx,dy)->(-dy,dx)
// sense) cycle starting in -x: dir(i) = cycle[i % 4],
// cycle = (-x, -y, +x, +y); leg i has length d_in + Floor(i/2)*p.
// Start vertex (inner terminal): (d_in/2, d_in/2). The LAST leg heads
// -y for half-integer turn counts (n_legs = 4*n_turns % 4 == 2) and +y
// for integer turn counts (n_legs % 4 == 0) — the stub/port block
// below branches on this.
n_legs = Round(4 * n_turns);
dirx[] = {-1, 0, 1, 0};
diry[] = {0, -1, 0, 1};

cx = d_in / 2;  cy = d_in / 2;   // running vertex
cond[] = {};                     // conductor box volume tags

// Track conductor footprint extents for region/bbox bookkeeping.
fp_xmin = cx; fp_xmax = cx; fp_ymin = cy; fp_ymax = cy;

For i In {0 : n_legs - 1}
  L  = d_in + Floor(i / 2) * p;
  nx = cx + dirx[i % 4] * L;
  ny = cy + diry[i % 4] * L;
  // Axis-aligned leg box: bounding box of the two centerline endpoints
  // inflated by w/2 in x and y (gives clean square corner joints; the
  // boolean union dissolves the overlaps).
  bx0 = (cx + nx) / 2 - Fabs(cx - nx) / 2 - w / 2;
  by0 = (cy + ny) / 2 - Fabs(cy - ny) / 2 - w / 2;
  bx1 = (cx + nx) / 2 + Fabs(cx - nx) / 2 + w / 2;
  by1 = (cy + ny) / 2 + Fabs(cy - ny) / 2 + w / 2;
  v = newv;
  Box(v) = {bx0, by0, z2_bot, bx1 - bx0, by1 - by0, t2};
  cond[] += {v};
  cx = nx; cy = ny;
  fp_xmin = (fp_xmin + bx0) / 2 - Fabs(fp_xmin - bx0) / 2;
  fp_ymin = (fp_ymin + by0) / 2 - Fabs(fp_ymin - by0) / 2;
  fp_xmax = (fp_xmax + bx1) / 2 + Fabs(fp_xmax - bx1) / 2;
  fp_ymax = (fp_ymax + by1) / 2 + Fabs(fp_ymax - by1) / 2;
EndFor

// Outer terminal of the spiral. The exit direction depends on the
// turn count: the left-turn leg cycle (-x, -y, +x, +y) means the last
// leg heads -y for half-integer turns (n_legs % 4 == 2, the original
// issue-#210 fixture) and +y for integer turns (n_legs % 4 == 0, the
// issue-#212 SLCFET 3HP fixture). The feed/return stubs, port gap and
// underpass mirror accordingly; everything else is shared.
x_out = cx;  y_out = cy;
// Inner terminal (spiral start).
x_in = d_in / 2;  y_in = d_in / 2;

If (Fabs(Fmod(n_legs, 4) - 2) < 0.5)
  // ---- half-integer turns: last leg heads -y, exit downward --------
  // Feed stub (m2): continue -y from the outer terminal.
  y_feed_end = y_out - feed_len;             // flat end face of the feed
  v = newv; Box(v) = {x_out - w/2, y_feed_end, z2_bot, w, (y_out + w/2) - y_feed_end, t2};
  cond[] += {v};

  // Port gap g below the feed end, then the m2 return stub.
  y_ret_end = y_feed_end - g;                // flat end face of the return stub
  y_via2    = y_ret_end - stub_len;          // via2 center
  v = newv; Box(v) = {x_out - w/2, y_via2 - w/2, z2_bot, w, (y_ret_end - y_via2) + w/2, t2};
  cond[] += {v};

  // Port rectangle y-extent (gap span, low to high).
  y_port_lo = y_ret_end;  y_port_hi = y_feed_end;
Else
  // ---- integer turns: last leg heads +y, exit upward ---------------
  // Feed stub (m2): continue +y from the outer terminal.
  y_feed_end = y_out + feed_len;             // flat end face of the feed
  v = newv; Box(v) = {x_out - w/2, y_out - w/2, z2_bot, w, y_feed_end - (y_out - w/2), t2};
  cond[] += {v};

  // Port gap g above the feed end, then the m2 return stub.
  y_ret_end = y_feed_end + g;                // flat end face of the return stub
  y_via2    = y_ret_end + stub_len;          // via2 center
  v = newv; Box(v) = {x_out - w/2, y_ret_end, z2_bot, w, (y_via2 + w/2) - y_ret_end, t2};
  cond[] += {v};

  y_port_lo = y_feed_end;  y_port_hi = y_ret_end;
EndIf

// Via2 (full m1-bottom to m2-top span; the union absorbs the overlaps).
v = newv; Box(v) = {x_out - w/2, y_via2 - w/2, z1_bot, w, w, z2_top - z1_bot};
cond[] += {v};

// Underpass on m1: horizontal run at y = y_via2 from via2 to x_in, then
// vertical run at x = x_in to the inner terminal. Boxes are written as
// bounding boxes of the two endpoints (inflated by w/2) so they hold
// for both exit directions.
ux0 = (x_out + x_in) / 2 - Fabs(x_out - x_in) / 2 - w/2;
ux1 = (x_out + x_in) / 2 + Fabs(x_out - x_in) / 2 + w/2;
v = newv; Box(v) = {ux0, y_via2 - w/2, z1_bot, ux1 - ux0, w, t1};
cond[] += {v};
uy0 = (y_via2 + y_in) / 2 - Fabs(y_via2 - y_in) / 2 - w/2;
uy1 = (y_via2 + y_in) / 2 + Fabs(y_via2 - y_in) / 2 + w/2;
v = newv; Box(v) = {x_in - w/2, uy0, z1_bot, w, uy1 - uy0, t1};
cond[] += {v};

// Via1 at the inner terminal.
v = newv; Box(v) = {x_in - w/2, y_in - w/2, z1_bot, w, w, z2_top - z1_bot};
cond[] += {v};

// Union into a single conductor solid.
uni() = BooleanUnion{ Volume{cond[0]}; Delete; }{ Volume{cond[{1 : #cond[] - 1}]}; Delete; };

// Conductor footprint extents including stubs/underpass.
fp_ymin = (fp_ymin + (y_via2 - w/2)) / 2 - Fabs(fp_ymin - (y_via2 - w/2)) / 2;
fp_ymax = (fp_ymax + (y_via2 + w/2)) / 2 + Fabs(fp_ymax - (y_via2 + w/2)) / 2;

// ---------------------------------------------------------------------------
// Domain slabs.
// ---------------------------------------------------------------------------

dom_x0 = fp_xmin - margin;  dom_x1 = fp_xmax + margin;
dom_y0 = fp_ymin - margin;  dom_y1 = fp_ymax + margin;
Lx = dom_x1 - dom_x0;  Ly = dom_y1 - dom_y0;

sub_v = newv; Box(sub_v) = {dom_x0, dom_y0, -h_sub, Lx, Ly, h_sub};
ox_v  = newv; Box(ox_v)  = {dom_x0, dom_y0, 0,      Lx, Ly, h_ox};
air_v = newv; Box(air_v) = {dom_x0, dom_y0, h_ox,   Lx, Ly, h_air};
buf_v = newv; Box(buf_v) = {dom_x0, dom_y0, h_ox + h_air, Lx, Ly, h_buf};

// Subtract the conductor cavity from the oxide (interiors excluded).
oxc() = BooleanDifference{ Volume{ox_v}; Delete; }{ Volume{uni()}; Delete; };

// Port rectangle at the m2 mid-height plane, spanning the gap.
port_s = news;
Rectangle(port_s) = {x_out - w/2, y_port_lo, z2_mid, w, g};

// Fragment everything for conformal interfaces (slab/slab, cavity walls,
// embedded port surface).
BooleanFragments{ Volume{sub_v, air_v, buf_v}; Volume{oxc()}; Delete; }{ Surface{port_s}; Delete; }

// ---------------------------------------------------------------------------
// Physical groups (bounding-box selection — robust to OCC renumbering).
// ---------------------------------------------------------------------------

eps = 1e-4;
z_top = h_ox + h_air + h_buf;

vols_sub() = Volume In BoundingBox{dom_x0 - eps, dom_y0 - eps, -h_sub - eps,
                                   dom_x1 + eps, dom_y1 + eps, 0 + eps};
vols_ox()  = Volume In BoundingBox{dom_x0 - eps, dom_y0 - eps, 0 - eps,
                                   dom_x1 + eps, dom_y1 + eps, h_ox + eps};
vols_air() = Volume In BoundingBox{dom_x0 - eps, dom_y0 - eps, h_ox - eps,
                                   dom_x1 + eps, dom_y1 + eps, h_ox + h_air + eps};
vols_buf() = Volume In BoundingBox{dom_x0 - eps, dom_y0 - eps, h_ox + h_air - eps,
                                   dom_x1 + eps, dom_y1 + eps, z_top + eps};

Physical Volume("substrate",  1) = {vols_sub()};
Physical Volume("dielectric", 2) = {vols_ox()};
Physical Volume("air",        3) = {vols_air()};
Physical Volume("air_buffer", 4) = {vols_buf()};

// Port: the only surface inside its own (thin) bounding box.
surf_port() = Surface In BoundingBox{x_out - w/2 - eps, y_port_lo - eps, z2_mid - eps,
                                     x_out + w/2 + eps, y_port_hi + eps, z2_mid + eps};
Physical Surface("port", 11) = {surf_port()};

// Conductor cavity walls: all surfaces inside the conductor z-band and
// footprint bbox, minus the port rectangle (which lies inside that box).
surf_cond() = Surface In BoundingBox{fp_xmin - eps, fp_ymin - eps, z1_bot - eps,
                                     fp_xmax + eps, fp_ymax + eps, z2_top + eps};
surf_cond() -= {surf_port()};
Physical Surface("conductor_surface", 12) = {surf_cond()};

// Outer boundary: the six outer walls (each may be split by the slab
// interfaces, so collect per wall with thin bounding boxes).
surf_out() = Surface In BoundingBox{dom_x0 - eps, dom_y0 - eps, -h_sub - eps,
                                    dom_x1 + eps, dom_y1 + eps, -h_sub + eps}; // floor
surf_out() += Surface In BoundingBox{dom_x0 - eps, dom_y0 - eps, z_top - eps,
                                     dom_x1 + eps, dom_y1 + eps, z_top + eps}; // ceiling
surf_out() += Surface In BoundingBox{dom_x0 - eps, dom_y0 - eps, -h_sub - eps,
                                     dom_x0 + eps, dom_y1 + eps, z_top + eps}; // x = dom_x0
surf_out() += Surface In BoundingBox{dom_x1 - eps, dom_y0 - eps, -h_sub - eps,
                                     dom_x1 + eps, dom_y1 + eps, z_top + eps}; // x = dom_x1
surf_out() += Surface In BoundingBox{dom_x0 - eps, dom_y0 - eps, -h_sub - eps,
                                     dom_x1 + eps, dom_y0 + eps, z_top + eps}; // y = dom_y0
surf_out() += Surface In BoundingBox{dom_x0 - eps, dom_y1 - eps, -h_sub - eps,
                                     dom_x1 + eps, dom_y1 + eps, z_top + eps}; // y = dom_y1
Physical Surface("outer_boundary", 13) = {surf_out()};

// ---------------------------------------------------------------------------
// Mesh sizing: refine near conductor surfaces and the port, grow to
// lc_far over dist_far. Distance+Threshold keeps mid-leg resolution
// (point-based sizing would coarsen between box corners).
// ---------------------------------------------------------------------------

Field[1] = Distance;
Field[1].SurfacesList = {surf_cond()};
Field[1].Sampling = 60;

Field[2] = Threshold;
Field[2].InField  = 1;
Field[2].SizeMin  = lc_cond;
Field[2].SizeMax  = lc_far;
Field[2].DistMin  = w;
Field[2].DistMax  = dist_far;

Field[3] = Distance;
Field[3].SurfacesList = {surf_port()};
Field[3].Sampling = 30;

Field[4] = Threshold;
Field[4].InField  = 3;
Field[4].SizeMin  = lc_port;
Field[4].SizeMax  = lc_far;
Field[4].DistMin  = g;
Field[4].DistMax  = dist_far;

Field[5] = Min;
Field[5].FieldsList = {2, 4};
Background Field = 5;

Mesh.MeshSizeExtendFromBoundary = 0;
Mesh.MeshSizeFromPoints = 0;
Mesh.MeshSizeFromCurvature = 0;

// MSH 4.1 ASCII keeps the fixture diff-able (sphere.geo precedent).
Mesh.MshFileVersion = 4.1;
Mesh.Binary = 0;
// HXT (Algorithm3D = 10) produces noticeably better dihedral-angle
// quality than the default Delaunay on the thin-slab oxide gaps here.
// (Netgen optimization is avoided: gmsh 4.15 segfaults when Netgen
// touches the volume containing the embedded port rectangle.)
Mesh.Algorithm3D = 10;
Mesh.Optimize = 1;
