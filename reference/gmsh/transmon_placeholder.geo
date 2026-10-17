// PLACEHOLDER transmon+resonator geometry (Epic #476 Phase A, issue #485).
//
// NOT the DeviceLayout.jl mesh. This is a schema-faithful stand-in used
// to develop and CI-test geode-fem's `mesh::transmon` adapter while the
// real DeviceLayout fixture is operator-generated (see
// `reference/julia/generate_transmon_fixture.jl` for the toolchain-gap
// rationale). It carries the IDENTICAL physical-group names the adapter
// consumes, so it is a drop-in swap target:
//
//   vacuum             (3D)  vacuum box above the chip
//   substrate          (3D)  sapphire chip slab
//   metal              (2D)  PEC: 2 transmon pads + readout trace
//   exterior_boundary  (2D)  outer domain walls (far-field)
//   port_1 / port_2    (2D)  lumped-port squares on the readout CPW ends
//   lumped_element     (2D)  junction-port square between the pads
//
// Geometry: two coplanar transmon pads with a junction square bridging
// their gap, a readout trace stub, and two disjoint readout-port squares,
// all sitting on the z=0 top face of a sapphire slab inside a vacuum box.
// Every named z=0 patch is DISJOINT (no overlaps) so tags stay clean. A
// deliberately simple, coarse mesh. Dimensions are plausible microns but
// NOT the real transmon layout.
//
// Lengths are in MICRONS (same unit convention as the spiral fixture).

SetFactory("OpenCASCADE");

// ---- parameters (overridable via -setnumber) ----
DefineConstant[ chip_x   = 900.0 ];   // chip footprint x
DefineConstant[ chip_y   = 600.0 ];   // chip footprint y
DefineConstant[ h_sub    = 120.0 ];   // sapphire slab thickness
DefineConstant[ h_vac    = 300.0 ];   // vacuum height above the chip
DefineConstant[ margin   = 120.0 ];   // vacuum lateral margin beyond chip
DefineConstant[ pad_w    = 160.0 ];   // transmon pad width  (x)
DefineConstant[ pad_h    = 120.0 ];   // transmon pad height (y)
DefineConstant[ pad_gap  = 60.0  ];   // gap between the two pads (junction)
DefineConstant[ trace_w  = 40.0  ];   // readout trace width
DefineConstant[ port_s   = 40.0  ];   // port square side
DefineConstant[ lc_metal = 45.0  ];   // mesh size near metal
DefineConstant[ lc_far   = 130.0 ];   // mesh size at far walls

box_x0 = -margin;   box_y0 = -margin;
box_x1 = chip_x + margin;  box_y1 = chip_y + margin;
cx = chip_x / 2;    cy = chip_y / 2;

// ---- volumes: sapphire slab embedded in a vacuum box ----
substrate = newv;
Box(substrate) = { 0, 0, -h_sub, chip_x, chip_y, h_sub };
domain = newv;
Box(domain) = { box_x0, box_y0, -h_sub, box_x1 - box_x0, box_y1 - box_y0, h_sub + h_vac };
BooleanFragments{ Volume{domain}; Delete; }{ Volume{substrate}; Delete; }

// ---- z=0 conductor / port patches (all disjoint) ----
// Two transmon pads straddling the junction gap along y.
pad1 = news;  Rectangle(pad1) = { cx - pad_w/2, cy - pad_gap/2 - pad_h, 0, pad_w, pad_h };
pad2 = news;  Rectangle(pad2) = { cx - pad_w/2, cy + pad_gap/2, 0, pad_w, pad_h };
// Junction (lumped_element) square bridging the pad gap, centered.
junc = news;  Rectangle(junc) = { cx - port_s/2, cy - port_s/2, 0, port_s, port_s };
// Readout trace stub running +x from the right edge of the pads, ending
// short of the right port square (a clean gap keeps the tags disjoint).
trace_x0 = cx + pad_w/2 + 30;
trace_x1 = chip_x - 3*port_s;
trace = news;  Rectangle(trace) = { trace_x0, cy - trace_w/2, 0, trace_x1 - trace_x0, trace_w };
// Two readout-port squares on the CPW ends (left and right), disjoint.
p1 = news;  Rectangle(p1) = { port_s, cy - port_s/2, 0, port_s, port_s };
p2 = news;  Rectangle(p2) = { chip_x - 2*port_s, cy - port_s/2, 0, port_s, port_s };

// Embed all z=0 patches into the volume mesh (conformal faces). Keep the
// patch surfaces (tools) alive so we can tag them by id afterwards.
patches() = { pad1, pad2, junc, trace, p1, p2 };
BooleanFragments{ Volume{:}; Delete; }{ Surface{patches()}; }

// ---- physical groups ----
// Volumes: classify by centroid z.
sub_vol = -1;  vac_vols() = {};
vol_ids() = Volume{:};
For i In {0 : #vol_ids()-1}
  id = vol_ids(i);
  bb() = BoundingBox Volume{id};
  If (0.5*(bb(2)+bb(5)) < 0)
    sub_vol = id;
  Else
    vac_vols() += id;
  EndIf
EndFor
Physical Volume("substrate", 1) = { sub_vol };
Physical Volume("vacuum", 2)    = { vac_vols() };

// Named z=0 patches keep their surface ids through fragmentation (they
// were tools, not deleted). metal = the two pads + the trace.
Physical Surface("metal", 11)          = { pad1, pad2, trace };
Physical Surface("port_1", 12)         = { p1 };
Physical Surface("port_2", 13)         = { p2 };
Physical Surface("lumped_element", 14) = { junc };

// Exterior boundary = the six outer walls of the domain box (a wall face
// has a degenerate bounding box along one axis matching a box extreme).
ext() = {};
srf() = Surface{:};
For i In {0 : #srf()-1}
  id = srf(i);
  bb() = BoundingBox Surface{id};
  xlo=bb(0); ylo=bb(1); zlo=bb(2); xhi=bb(3); yhi=bb(4); zhi=bb(5);
  on_wall = (Fabs(xlo-box_x0)<1e-3 && Fabs(xhi-box_x0)<1e-3) ||
            (Fabs(xlo-box_x1)<1e-3 && Fabs(xhi-box_x1)<1e-3) ||
            (Fabs(ylo-box_y0)<1e-3 && Fabs(yhi-box_y0)<1e-3) ||
            (Fabs(ylo-box_y1)<1e-3 && Fabs(yhi-box_y1)<1e-3) ||
            (Fabs(zlo-(-h_sub))<1e-3 && Fabs(zhi-(-h_sub))<1e-3) ||
            (Fabs(zlo-h_vac)<1e-3 && Fabs(zhi-h_vac)<1e-3);
  If (on_wall)
    ext() += id;
  EndIf
EndFor
Physical Surface("exterior_boundary", 15) = { ext() };

// ---- mesh sizing ----
MeshSize{ PointsOf{ Surface{ patches() }; } } = lc_metal;
Mesh.MeshSizeMax = lc_far;
Mesh.MeshSizeMin = lc_metal;
Mesh.Algorithm3D = 1;   // Delaunay
Mesh.Optimize = 1;
Mesh.MshFileVersion = 4.1;
