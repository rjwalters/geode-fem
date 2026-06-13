// Probe-fed rectangular microstrip patch antenna on FR-4
// (Epic #226 Phase 1, issue #227).
//
// Generates a FEM-ready tagged tet mesh of a 2.4 GHz rectangular
// microstrip patch antenna: a finite PEC ground plane on the bottom of
// an FR-4 substrate slab, a rectangular PEC patch on the substrate top,
// fed by a vertical coax probe whose driven gap is a lumped-port
// rectangle spanning the substrate thickness at the ~50 Ω inset
// position. The whole structure sits in an air box terminated by a
// matched-UPML shell with a PEC outer boundary — the project's first
// driven OPEN RADIATOR (open-air domain + matched UPML, like the Mie
// sphere, but port-driven like the spiral inductor).
//
// Conductors are treated as PEC (thin copper) for Phase 1: the patch
// and ground faces become PEC edge masks
// (pec_interior_mask_from_triangles over the `patch` + `ground`
// triangle tags), so no metal volume is meshed. The probe is likewise
// not meshed as a solid; its driven gap is the vertical port rectangle.
//
// Layout (units mm — in the solver's natural units omega = k0 is then
// in rad/mm with c = 3e11 mm/s; lambda0 ~ 125 mm at 2.4 GHz):
//
//   z = h + air + pml   ___________________  outer boundary (PEC)
//                       |  UPML shell       |
//                       |   _____________   |
//                       |  |  air        |  |
//   z = h  patch -------|--|---[====]----|--|--   (PEC, on substrate top)
//                       |  | substrate   |  |
//   z = 0  ground ------|--|=============|--|--   (PEC, substrate bottom)
//                       |  |  air        |  |
//                       |__|_____________|__|
//
//   The port rectangle is a thin vertical rectangle in the x = x_feed
//   plane spanning the substrate gap from the ground (z = 0) to the
//   patch (z = h), oriented along +z: e_hat = +z, length = h,
//   width = probe_w  =>  Z_s = R*probe_w/h (Palace-style uniform
//   lumped port, lumped_port.rs; Z = V/I is invariant under
//   e_hat -> -e_hat).
//
// Physical groups (consumed by crates/geode-core/src/mesh/patch.rs —
// keep the (dim, tag, name) table below in sync with the PHYS_*
// constants there):
//
//   | dim | tag | name           | meaning                              |
//   |-----|-----|----------------|--------------------------------------|
//   | 3   | 1   | substrate      | FR-4 slab, z in [0, h]               |
//   | 3   | 2   | air            | air core around the patch            |
//   | 3   | 3   | upml           | matched-UPML shell tets              |
//   | 2   | 11  | port           | probe-gap lumped-port rectangle      |
//   | 2   | 12  | patch          | patch-conductor face (PEC mask)      |
//   | 2   | 13  | ground         | ground-plane face (PEC mask)         |
//   | 2   | 14  | outer_boundary | UPML outer walls (PEC)               |
//
// Run via the wrapper (recommended — adds mesh-quality checks +
// provenance):
//   python3 reference/gmsh/generate_patch_fixture.py \
//       reference/gmsh/patch_2g4_benchmark.yaml \
//       crates/geode-core/tests/fixtures/patch_2g4.msh
//
// or directly:
//   gmsh -3 -format msh41 -o patch_2g4.msh reference/gmsh/patch_antenna.geo
//
// All parameters below can be overridden with -setnumber on the gmsh
// command line (the wrapper does exactly that from the YAML).

SetFactory("OpenCASCADE");

// ---------------------------------------------------------------------------
// Parameters (mm) — defaults reproduce the committed benchmark fixture.
// ---------------------------------------------------------------------------

// Patch + substrate (2.4 GHz, FR-4).
DefineConstant[ patch_w = 38.0 ];   // patch width  (x extent)
DefineConstant[ patch_l = 29.0 ];   // patch length (y extent)
DefineConstant[ h       = 1.6  ];   // substrate thickness
DefineConstant[ sub_pad = 12.0 ];   // substrate margin beyond the patch

// Probe feed (coax inner-conductor footprint).
DefineConstant[ probe_w   = 1.6 ];  // probe square footprint side
DefineConstant[ probe_inset = 8.0 ]; // inset of the feed from the patch edge
                                     // along -y (measured from the +y edge)

// Air box + UPML shell (open radiator).
DefineConstant[ air_margin = 60.0 ];  // air gap from the structure to the PML
                                       // (~lambda/2 at 2.4 GHz)
DefineConstant[ pml_thick  = 25.0 ];  // matched-UPML shell thickness

// Mesh sizing (graded: fine on the substrate/patch/probe, coarse far).
DefineConstant[ lc_patch = 4.0  ];   // target size on the patch/substrate
DefineConstant[ lc_sub   = 1.6  ];   // in-slab size (~ substrate thickness h)
DefineConstant[ lc_port  = 1.0  ];   // target size at the probe gap
DefineConstant[ lc_far   = 22.0 ];   // far-field size (air/UPML)
DefineConstant[ dist_far = 35.0 ];   // distance over which size grows to lc_far

// ---------------------------------------------------------------------------
// Derived geometry.
// ---------------------------------------------------------------------------

// Substrate footprint (centered on x; the patch is centered on it).
sub_w = patch_w + 2 * sub_pad;
sub_l = patch_l + 2 * sub_pad;
sub_x0 = -sub_w / 2;  sub_y0 = -sub_l / 2;

// Patch footprint (centered).
px0 = -patch_w / 2;  py0 = -patch_l / 2;
px1 =  patch_w / 2;  py1 =  patch_l / 2;

// Feed point: on the patch centerline (x = 0), inset from the +y edge.
x_feed = 0.0;
y_feed = py1 - probe_inset;

// Air box: substrate footprint inflated by air_margin on all sides.
air_x0 = sub_x0 - air_margin;  air_x1 = -sub_x0 + air_margin;
air_y0 = sub_y0 - air_margin;  air_y1 = -sub_y0 + air_margin;
air_z0 = -air_margin;          air_z1 = h + air_margin;
air_lx = air_x1 - air_x0;  air_ly = air_y1 - air_y0;  air_lz = air_z1 - air_z0;

// Domain (air + UPML shell) extents.
dom_x0 = air_x0 - pml_thick;  dom_x1 = air_x1 + pml_thick;
dom_y0 = air_y0 - pml_thick;  dom_y1 = air_y1 + pml_thick;
dom_z0 = air_z0 - pml_thick;  dom_z1 = air_z1 + pml_thick;
dom_lx = dom_x1 - dom_x0;  dom_ly = dom_y1 - dom_y0;  dom_lz = dom_z1 - dom_z0;

// ---------------------------------------------------------------------------
// Volumes: substrate slab, air box, and the surrounding domain box. The
// air and UPML regions are carved by boolean fragments so all interfaces
// are conformal and the substrate is a distinct tagged volume.
// ---------------------------------------------------------------------------

sub_v = newv; Box(sub_v) = {sub_x0, sub_y0, 0, sub_w, sub_l, h};
air_v = newv; Box(air_v) = {air_x0, air_y0, air_z0, air_lx, air_ly, air_lz};
dom_v = newv; Box(dom_v) = {dom_x0, dom_y0, dom_z0, dom_lx, dom_ly, dom_lz};

// Port rectangle: a thin vertical rectangle in the x = x_feed plane,
// spanning the substrate gap (z in [0, h]) over the probe footprint in
// y. Built explicitly from points/lines so it lies exactly in the y-z
// plane at x = x_feed.
pp1 = newp; Point(pp1) = {x_feed, y_feed - probe_w / 2, 0};
pp2 = newp; Point(pp2) = {x_feed, y_feed + probe_w / 2, 0};
pp3 = newp; Point(pp3) = {x_feed, y_feed + probe_w / 2, h};
pp4 = newp; Point(pp4) = {x_feed, y_feed - probe_w / 2, h};
pl1 = newl; Line(pl1) = {pp1, pp2};
pl2 = newl; Line(pl2) = {pp2, pp3};
pl3 = newl; Line(pl3) = {pp3, pp4};
pl4 = newl; Line(pl4) = {pp4, pp1};
pcl = newll; Curve Loop(pcl) = {pl1, pl2, pl3, pl4};
port_s = news; Plane Surface(port_s) = {pcl};

// Patch rectangle: a horizontal rectangle on the substrate top face
// (z = h) over the patch footprint. Embedded so the patch conductor is
// a distinct tagged face (the substrate top extends beyond the patch).
patch_s = news;
Rectangle(patch_s) = {px0, py0, h, patch_w, patch_l};

// Fragment everything so the substrate, air core, and UPML shell share
// conformal faces and the port + patch rectangles are embedded in the
// mesh as distinct surfaces.
BooleanFragments{ Volume{dom_v, air_v, sub_v}; Delete; }{ Surface{port_s, patch_s}; Delete; }

// ---------------------------------------------------------------------------
// Physical groups (bounding-box selection — robust to OCC renumbering).
// ---------------------------------------------------------------------------

eps = 1e-4;

// Substrate: the only volume inside the thin substrate bbox.
vols_sub() = Volume In BoundingBox{sub_x0 - eps, sub_y0 - eps, 0 - eps,
                                   sub_x0 + sub_w + eps, sub_y0 + sub_l + eps, h + eps};
// Air core: volumes inside the air box, minus the substrate.
vols_air() = Volume In BoundingBox{air_x0 - eps, air_y0 - eps, air_z0 - eps,
                                   air_x1 + eps, air_y1 + eps, air_z1 + eps};
vols_air() -= {vols_sub()};
// UPML shell: every volume, minus the air core and substrate.
vols_all() = Volume In BoundingBox{dom_x0 - eps, dom_y0 - eps, dom_z0 - eps,
                                   dom_x1 + eps, dom_y1 + eps, dom_z1 + eps};
vols_pml() = vols_all();
vols_pml() -= {vols_air()};
vols_pml() -= {vols_sub()};

Physical Volume("substrate", 1) = {vols_sub()};
Physical Volume("air",       2) = {vols_air()};
Physical Volume("upml",      3) = {vols_pml()};

// Port: the only surface inside its own thin bounding box at x = x_feed.
surf_port() = Surface In BoundingBox{x_feed - eps, y_feed - probe_w/2 - eps, 0 - eps,
                                     x_feed + eps, y_feed + probe_w/2 + eps, h + eps};
Physical Surface("port", 11) = {surf_port()};

// Patch: the surface(s) on the substrate top face (z = h) within the
// patch footprint.
surf_patch() = Surface In BoundingBox{px0 - eps, py0 - eps, h - eps,
                                      px1 + eps, py1 + eps, h + eps};
Physical Surface("patch", 12) = {surf_patch()};

// Ground: the substrate bottom face (z = 0) over the full substrate
// footprint (finite ground plane).
surf_ground() = Surface In BoundingBox{sub_x0 - eps, sub_y0 - eps, 0 - eps,
                                       sub_x0 + sub_w + eps, sub_y0 + sub_l + eps, 0 + eps};
Physical Surface("ground", 13) = {surf_ground()};

// Outer boundary: the six outer walls of the domain box (each collected
// with a thin bounding box; interfaces may split them).
surf_out() = Surface In BoundingBox{dom_x0 - eps, dom_y0 - eps, dom_z0 - eps,
                                    dom_x1 + eps, dom_y1 + eps, dom_z0 + eps}; // floor
surf_out() += Surface In BoundingBox{dom_x0 - eps, dom_y0 - eps, dom_z1 - eps,
                                     dom_x1 + eps, dom_y1 + eps, dom_z1 + eps}; // ceiling
surf_out() += Surface In BoundingBox{dom_x0 - eps, dom_y0 - eps, dom_z0 - eps,
                                     dom_x0 + eps, dom_y1 + eps, dom_z1 + eps}; // x = dom_x0
surf_out() += Surface In BoundingBox{dom_x1 - eps, dom_y0 - eps, dom_z0 - eps,
                                     dom_x1 + eps, dom_y1 + eps, dom_z1 + eps}; // x = dom_x1
surf_out() += Surface In BoundingBox{dom_x0 - eps, dom_y0 - eps, dom_z0 - eps,
                                     dom_x1 + eps, dom_y0 + eps, dom_z1 + eps}; // y = dom_y0
surf_out() += Surface In BoundingBox{dom_x0 - eps, dom_y1 - eps, dom_z0 - eps,
                                     dom_x1 + eps, dom_y1 + eps, dom_z1 + eps}; // y = dom_y1
Physical Surface("outer_boundary", 14) = {surf_out()};

// ---------------------------------------------------------------------------
// Mesh sizing: refine on the patch/substrate top face and the port gap,
// grow to lc_far over dist_far. Distance+Threshold keeps the substrate
// resolution between the box corners (point-based sizing would coarsen).
// ---------------------------------------------------------------------------

Field[1] = Distance;
Field[1].SurfacesList = {surf_patch(), surf_ground()};
Field[1].Sampling = 80;

Field[2] = Threshold;
Field[2].InField  = 1;
Field[2].SizeMin  = lc_patch;
Field[2].SizeMax  = lc_far;
Field[2].DistMin  = h;
Field[2].DistMax  = dist_far;

// Substrate slab refinement: cap the in-slab size near the substrate
// thickness so the thin FR-4 slab is not squashed into sliver tets
// (the dominant min-dihedral risk for the patch fixture).
Field[6] = Box;
Field[6].VIn   = lc_sub;
Field[6].VOut  = lc_far;
Field[6].XMin  = sub_x0;  Field[6].XMax  = sub_x0 + sub_w;
Field[6].YMin  = sub_y0;  Field[6].YMax  = sub_y0 + sub_l;
Field[6].ZMin  = -h;      Field[6].ZMax  = 2 * h;
Field[6].Thickness = 4 * h;

Field[3] = Distance;
Field[3].SurfacesList = {surf_port()};
Field[3].Sampling = 40;

Field[4] = Threshold;
Field[4].InField  = 3;
Field[4].SizeMin  = lc_port;
Field[4].SizeMax  = lc_far;
Field[4].DistMin  = probe_w;
Field[4].DistMax  = dist_far;

Field[5] = Min;
Field[5].FieldsList = {2, 4, 6};
Background Field = 5;

Mesh.MeshSizeExtendFromBoundary = 0;
Mesh.MeshSizeFromPoints = 0;
Mesh.MeshSizeFromCurvature = 0;

// MSH 4.1 ASCII keeps the fixture diff-able (sphere/spiral precedent).
Mesh.MshFileVersion = 4.1;
Mesh.Binary = 0;
// HXT (Algorithm3D = 10) gives better dihedral quality on the thin
// substrate slab than the default Delaunay.
Mesh.Algorithm3D = 10;
Mesh.Optimize = 1;
