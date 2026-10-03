// Shielded 50-ohm microstrip line for `geode driven` hybrid wave ports
// (geode-cli cookbook, issue #807). Units: mm (spec length_unit_m = 1e-3).
//
// Cross-section (x, y), extruded along z in [0, L]:
//   PEC box [-W/2, W/2] x [0, H] ("shield": floor, lid, side walls);
//   substrate eps_r = 4.4 for 0 <= y <= h ("substrate"), air above ("air");
//   a zero-thickness PEC strip of width w on y = h ("strip": interior
//   faces between the substrate and the air);
//   the two end caps z = 0 / z = L are the wave ports ("port_in",
//   "port_out").
// w/h = 1.91 is the Hammerstad-Jensen 50-ohm width on eps_r = 4.4.
//
//   gmsh -3 microstrip_line.geo -format msh41 -o microstrip_line.msh
SetFactory("OpenCASCADE");

W = 8; H = 5; h = 1; w = 1.91; L = 4;
lc_far = 0.8;    // mesh size away from the strip
lc_strip = 0.2;  // mesh size on the strip (the r^-1/2 edge singularity)

Box(1) = {-W/2, 0, 0, W, h, L};
Box(2) = {-W/2, h, 0, W, H - h, L};
// The strip: a rectangle in the plane y = h.
p1 = newp; Point(p1) = {-w/2, h, 0};
p2 = newp; Point(p2) = { w/2, h, 0};
p3 = newp; Point(p3) = { w/2, h, L};
p4 = newp; Point(p4) = {-w/2, h, L};
l1 = newl; Line(l1) = {p1, p2};
l2 = newl; Line(l2) = {p2, p3};
l3 = newl; Line(l3) = {p3, p4};
l4 = newl; Line(l4) = {p4, p1};
cl = newcl; Curve Loop(cl) = {l1, l2, l3, l4};
s = news; Plane Surface(s) = {cl};
BooleanFragments{ Volume{1, 2}; Delete; }{ Surface{s}; Delete; }

eps = 1e-6;
sub[] = Volume In BoundingBox{-W/2 - eps, -eps, -eps, W/2 + eps, h + eps, L + eps};
air[] = Volume In BoundingBox{-W/2 - eps, h - eps, -eps, W/2 + eps, H + eps, L + eps};
strip[] = Surface In BoundingBox{-w/2 - eps, h - eps, -eps, w/2 + eps, h + eps, L + eps};
port_in[] = Surface In BoundingBox{-W/2 - eps, -eps, -eps, W/2 + eps, H + eps, eps};
port_out[] = Surface In BoundingBox{-W/2 - eps, -eps, L - eps, W/2 + eps, H + eps, L + eps};
floor[] = Surface In BoundingBox{-W/2 - eps, -eps, -eps, W/2 + eps, eps, L + eps};
lid[] = Surface In BoundingBox{-W/2 - eps, H - eps, -eps, W/2 + eps, H + eps, L + eps};
left[] = Surface In BoundingBox{-W/2 - eps, -eps, -eps, -W/2 + eps, H + eps, L + eps};
right[] = Surface In BoundingBox{W/2 - eps, -eps, -eps, W/2 + eps, H + eps, L + eps};

Physical Volume("substrate") = {sub[]};
Physical Volume("air") = {air[]};
Physical Surface("strip") = {strip[]};
Physical Surface("port_in") = {port_in[]};
Physical Surface("port_out") = {port_out[]};
Physical Surface("shield") = {floor[], lid[], left[], right[]};

// Refine toward the strip (its edges carry the field singularity).
Field[1] = Distance;
Field[1].SurfacesList = {strip[]};
Field[1].Sampling = 40;
Field[2] = Threshold;
Field[2].InField = 1;
Field[2].SizeMin = lc_strip;
Field[2].SizeMax = lc_far;
Field[2].DistMin = 0.1;
Field[2].DistMax = 2.5;
Background Field = 2;
Mesh.MeshSizeExtendFromBoundary = 0;
Mesh.MeshSizeFromPoints = 0;
Mesh.MeshSizeFromCurvature = 0;
