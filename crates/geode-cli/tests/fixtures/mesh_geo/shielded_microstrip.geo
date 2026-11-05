// Do not edit: regenerate with `geode mesh` (issue #704).
//
// Physical groups:
//   | dim | tag | name | role |
//   | 3 | 1 | air | dielectric slab |
//   | 3 | 2 | sig | solid conductor (current path) |
//   | 2 | 101 | gnd | PEC shell (cavity walls) |
//   | 2 | 102 | lid | PEC shell (cavity walls) |
//   | 2 | 103 | walls | PEC shell (cavity walls) |
//   | 2 | 104 | cap_in | PEC shell (cavity walls) |
//   | 2 | 105 | cap_out | PEC shell (cavity walls) |
//   | 2 | 106 | sig_in | current-path contact |
//   | 2 | 107 | sig_out | current-path contact |
//   | 2 | 108 | outer_boundary | outer walls (PEC) |

SetFactory("OpenCASCADE");

// Dielectric slabs (full lateral domain).
v_d0 = newv; Box(v_d0) = {-0.5, -2, -0.5, 3, 4, 2.5}; // air
// Conductor `gnd`: solids (hollowed below).
v_c0_0 = newv; Box(v_c0_0) = {-0.25, -1.75, -0.25, 2.5, 3.5, 0.25};
// Conductor `lid`: solids (hollowed below).
v_c1_0 = newv; Box(v_c1_0) = {-0.25, -1.75, 1.5, 2.5, 3.5, 0.25};
// Conductor `walls`: solids (hollowed below).
v_c2_0 = newv; Box(v_c2_0) = {-0.25, -1.75, 0, 2.5, 0.25, 1.5};
v_c2_1 = newv; Box(v_c2_1) = {-0.25, 1.5, 0, 2.5, 0.25, 1.5};
v_c2_2 = newv; Box(v_c2_2) = {-0.25, -1.5, 0, 0.25, 3, 1.5};
v_c2_3 = newv; Box(v_c2_3) = {2, -1.5, 0, 0.25, 3, 1.5};
// Conductor `sig`: solid current-path volumes (kept, meshed).
v_c3_0 = newv; Box(v_c3_0) = {0, -0.5, 0.5, 2, 1, 0.25};

// Hollow thick conductors: union their solids, subtract the union
// from the whole dielectric stack (interiors excluded, cavity walls kept).
v_cond() = {v_c0_0, v_c1_0, v_c2_0, v_c2_1, v_c2_2, v_c2_3};
Geometry.OCCUnionUnify = 0;
v_cond() = BooleanUnion{ Volume{v_cond(0)}; Delete; }{ Volume{v_cond({1 : #v_cond() - 1})}; Delete; };
v_hollow() = BooleanDifference{ Volume{v_d0}; Delete; }{ Volume{v_cond()}; Delete; };
// Conformal interfaces between every slab, cavity wall, sheet and port.
BooleanFragments{ Volume{v_hollow(), v_c3_0}; Delete; }{ }

// Physical groups: bounding-box selection (robust to OCC renumbering).
g_2() = {};
g_2() += Volume In BoundingBox{-4e-5, -0.50004, 0.49996, 2.00004, 0.50004, 0.75004};
g_2() = Unique(g_2());
Physical Volume("sig", 2) = {g_2()};
v_solid() = {g_2()};
g_1() = Volume In BoundingBox{-0.50004, -2.00004, -0.50004, 2.50004, 2.00004, 2.00004};
g_1() -= {v_solid()};
Physical Volume("air", 1) = {g_1()};
assigned() = {};
g_106() = {};
g_106() += Surface In BoundingBox{-4e-5, -0.50004, 0.49996, 4e-5, 0.50004, 0.75004};
g_106() = Unique(g_106());
g_106() -= {assigned()};
assigned() += {g_106()};
Physical Surface("sig_in", 106) = {g_106()};
g_107() = {};
g_107() += Surface In BoundingBox{1.99996, -0.50004, 0.49996, 2.00004, 0.50004, 0.75004};
g_107() = Unique(g_107());
g_107() -= {assigned()};
assigned() += {g_107()};
Physical Surface("sig_out", 107) = {g_107()};
g_101() = {};
g_101() += Surface In BoundingBox{-0.25004, -1.75004, -0.25004, 2.25004, 1.75004, 4e-5};
g_101() = Unique(g_101());
g_101() -= {assigned()};
assigned() += {g_101()};
Physical Surface("gnd", 101) = {g_101()};
g_102() = {};
g_102() += Surface In BoundingBox{-0.25004, -1.75004, 1.49996, 2.25004, 1.75004, 1.75004};
g_102() = Unique(g_102());
g_102() -= {assigned()};
assigned() += {g_102()};
Physical Surface("lid", 102) = {g_102()};
g_103() = {};
g_103() += Surface In BoundingBox{-0.25004, -1.75004, -4e-5, 2.25004, -1.49996, 1.50004};
g_103() += Surface In BoundingBox{-0.25004, 1.49996, -4e-5, 2.25004, 1.75004, 1.50004};
g_103() = Unique(g_103());
g_103() -= {assigned()};
assigned() += {g_103()};
Physical Surface("walls", 103) = {g_103()};
g_104() = {};
g_104() += Surface In BoundingBox{-0.25004, -1.50004, -4e-5, 4e-5, 1.50004, 1.50004};
g_104() = Unique(g_104());
g_104() -= {assigned()};
assigned() += {g_104()};
Physical Surface("cap_in", 104) = {g_104()};
g_105() = {};
g_105() += Surface In BoundingBox{1.99996, -1.50004, -4e-5, 2.25004, 1.50004, 1.50004};
g_105() = Unique(g_105());
g_105() -= {assigned()};
assigned() += {g_105()};
Physical Surface("cap_out", 105) = {g_105()};
g_108() = {};
g_108() += Surface In BoundingBox{-0.50004, -2.00004, -0.50004, 2.50004, 2.00004, -0.49996};
g_108() += Surface In BoundingBox{-0.50004, -2.00004, 1.99996, 2.50004, 2.00004, 2.00004};
g_108() += Surface In BoundingBox{-0.50004, -2.00004, -0.50004, -0.49996, 2.00004, 2.00004};
g_108() += Surface In BoundingBox{2.49996, -2.00004, -0.50004, 2.50004, 2.00004, 2.00004};
g_108() += Surface In BoundingBox{-0.50004, -2.00004, -0.50004, 2.50004, -1.99996, 2.00004};
g_108() += Surface In BoundingBox{-0.50004, 1.99996, -0.50004, 2.50004, 2.00004, 2.00004};
g_108() = Unique(g_108());
Physical Surface("outer_boundary", 108) = {g_108()};

// Size field: refine near conductors and ports, grow to size_max.
Field[1] = Distance;
b_2() = Abs(CombinedBoundary{ Volume{g_2()}; });
Field[1].SurfacesList = {b_2()};
Field[1].Sampling = 40;
Field[2] = Threshold;
Field[2].InField = 1;
Field[2].SizeMin = 0.1;
Field[2].SizeMax = 0.3;
Field[2].DistMin = 0.1;
Field[2].DistMax = 0.25;
Field[3] = Distance;
Field[3].SurfacesList = {g_101()};
Field[3].Sampling = 28;
Field[4] = Threshold;
Field[4].InField = 3;
Field[4].SizeMin = 0.25;
Field[4].SizeMax = 0.3;
Field[4].DistMin = 0.25;
Field[4].DistMax = 0.25;
Field[5] = Distance;
Field[5].SurfacesList = {g_102()};
Field[5].Sampling = 28;
Field[6] = Threshold;
Field[6].InField = 5;
Field[6].SizeMin = 0.25;
Field[6].SizeMax = 0.3;
Field[6].DistMin = 0.25;
Field[6].DistMax = 0.25;
Field[7] = Distance;
Field[7].SurfacesList = {g_103()};
Field[7].Sampling = 20;
Field[8] = Threshold;
Field[8].InField = 7;
Field[8].SizeMin = 0.25;
Field[8].SizeMax = 0.3;
Field[8].DistMin = 0.25;
Field[8].DistMax = 0.25;
Field[9] = Distance;
Field[9].SurfacesList = {g_104()};
Field[9].Sampling = 24;
Field[10] = Threshold;
Field[10].InField = 9;
Field[10].SizeMin = 0.25;
Field[10].SizeMax = 0.3;
Field[10].DistMin = 0.25;
Field[10].DistMax = 0.25;
Field[11] = Distance;
Field[11].SurfacesList = {g_105()};
Field[11].Sampling = 24;
Field[12] = Threshold;
Field[12].InField = 11;
Field[12].SizeMin = 0.25;
Field[12].SizeMax = 0.3;
Field[12].DistMin = 0.25;
Field[12].DistMax = 0.25;
Field[13] = Min;
Field[13].FieldsList = {2, 4, 6, 8, 10, 12};
Background Field = 13;

// Pinned options: sizes from the field only, linear elements,
// MSH 4.1 ASCII (what geode_core::mesh::read_tagged_tet_mesh reads),
// single-threaded for a reproducible mesh.
Mesh.MeshSizeExtendFromBoundary = 0;
Mesh.MeshSizeFromPoints = 0;
Mesh.MeshSizeFromCurvature = 0;
Mesh.MeshSizeMax = 0.3;
Mesh.Algorithm = 6;
Mesh.Algorithm3D = 1;
Mesh.Optimize = 1;
Mesh.OptimizeNetgen = 0;
Mesh.ElementOrder = 1;
Mesh.SaveAll = 0;
Mesh.MshFileVersion = 4.1;
Mesh.Binary = 0;
General.NumThreads = 1;
