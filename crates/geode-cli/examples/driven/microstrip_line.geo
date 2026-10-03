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
// w/h = 1.91 is the Hammerstad-Jensen 50-ohm width on eps_r = 4.4 (an OPEN
// microstrip formula: 50.06 ohm).
//
// The shield box is part of the design: a shielded line's Z0 depends on it.
// The 20h x 12h box here sits about 0.6 % below the open-line Z0 (an 8h x 5h
// box: -3.3 %; 16h x 10h: -1 %; 40h x 40h: -0.3 %). That offset is physics,
// so no mesh refinement removes it and geode's impedance accuracy estimate
// does not flag it.
//
// The mesh is graded toward the two strip EDGES (Distance on the edge
// curves): the field there is singular (r^-1/2), so the line impedance
// converges only about O(h) in the edge cell size, much more slowly than the
// propagation constant. Sizes grow linearly from lc_edge at the edges and
// from lc_strip on the strip, capped at lc_far.
//
//   gmsh -3 microstrip_line.geo -format msh41 -o microstrip_line.msh
SetFactory("OpenCASCADE");

W = 20; H = 12; h = 1; w = 1.91; L = 2;
lc_edge = 0.02;   // cell size at the strip edges (the r^-1/2 singularity)
lc_strip = 0.15;  // cell size on the strip surface
grow = 0.25;      // linear growth of the cell size with the distance
lc_far = 1.5;     // largest cell size

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

// The strip's two edge curves (along z at x = -w/2 and x = w/2, y = h).
edges[] = Curve In BoundingBox{-w/2 - eps, h - eps, -eps, -w/2 + eps, h + eps, L + eps};
edges[] += Curve In BoundingBox{w/2 - eps, h - eps, -eps, w/2 + eps, h + eps, L + eps};

// Grade toward the strip edges (size lc_edge + grow * distance) and,
// more gently, toward the strip surface; the finer of the two wins.
Field[1] = Distance;
Field[1].CurvesList = {edges[]};
Field[1].Sampling = 200;
Field[2] = MathEval;
Field[2].F = Sprintf("Min(%g + %g * F1, %g)", lc_edge, grow, lc_far);
Field[3] = Distance;
Field[3].SurfacesList = {strip[]};
Field[3].Sampling = 60;
Field[4] = MathEval;
Field[4].F = Sprintf("Min(%g + 0.3 * F3, %g)", lc_strip, lc_far);
Field[5] = Min;
Field[5].FieldsList = {2, 4};
Background Field = 5;
Mesh.MeshSizeExtendFromBoundary = 0;
Mesh.MeshSizeFromPoints = 0;
Mesh.MeshSizeFromCurvature = 0;
