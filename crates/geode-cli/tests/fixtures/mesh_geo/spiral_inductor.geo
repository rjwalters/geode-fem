// Do not edit: regenerate with `geode mesh` (issue #704).
//
// Physical groups:
//   | dim | tag | name | role |
//   | 3 | 1 | substrate | dielectric slab |
//   | 3 | 2 | dielectric | dielectric slab |
//   | 3 | 3 | air | dielectric slab |
//   | 3 | 4 | air_buffer | dielectric slab |
//   | 2 | 101 | m1 | PEC shell (cavity walls) |
//   | 2 | 102 | m2 | PEC shell (cavity walls) |
//   | 2 | 103 | via | PEC shell (cavity walls) |
//   | 2 | 104 | port | lumped gap port |
//   | 2 | 105 | outer_boundary | outer walls (PEC) |

SetFactory("OpenCASCADE");

// Dielectric slabs (full lateral domain).
v_d0 = newv; Box(v_d0) = {-71, -95, -18, 142, 166, 18}; // substrate
v_d1 = newv; Box(v_d1) = {-71, -95, 0, 142, 166, 12}; // dielectric
v_d2 = newv; Box(v_d2) = {-71, -95, 12, 142, 166, 12}; // air
v_d3 = newv; Box(v_d3) = {-71, -95, 24, 142, 166, 8}; // air_buffer
// Conductor `m1`: solids (hollowed below).
v_c0_0 = newv; Box(v_c0_0) = {-53, -77, 2, 76, 6, 2};
v_c0_1 = newv; Box(v_c0_1) = {17, -77, 2, 6, 100, 2};
// Conductor `m2`: solids (hollowed below).
v_c1_0 = newv; Box(v_c1_0) = {-23, 17, 6.5, 46, 6, 3};
v_c1_1 = newv; Box(v_c1_1) = {-23, -23, 6.5, 6, 46, 3};
v_c1_2 = newv; Box(v_c1_2) = {-23, -23, 6.5, 56, 6, 3};
v_c1_3 = newv; Box(v_c1_3) = {27, -23, 6.5, 6, 56, 3};
v_c1_4 = newv; Box(v_c1_4) = {-33, 27, 6.5, 66, 6, 3};
v_c1_5 = newv; Box(v_c1_5) = {-33, -33, 6.5, 6, 66, 3};
v_c1_6 = newv; Box(v_c1_6) = {-33, -33, 6.5, 76, 6, 3};
v_c1_7 = newv; Box(v_c1_7) = {37, -33, 6.5, 6, 76, 3};
v_c1_8 = newv; Box(v_c1_8) = {-43, 37, 6.5, 86, 6, 3};
v_c1_9 = newv; Box(v_c1_9) = {-43, -43, 6.5, 6, 86, 3};
v_c1_10 = newv; Box(v_c1_10) = {-43, -43, 6.5, 96, 6, 3};
v_c1_11 = newv; Box(v_c1_11) = {47, -43, 6.5, 6, 96, 3};
v_c1_12 = newv; Box(v_c1_12) = {-53, 47, 6.5, 106, 6, 3};
v_c1_13 = newv; Box(v_c1_13) = {-53, -53, 6.5, 6, 106, 3};
v_c1_14 = newv; Box(v_c1_14) = {-53, -62, 6.5, 6, 15, 3};
v_c1_15 = newv; Box(v_c1_15) = {-53, -77, 6.5, 6, 11, 3};
// Conductor `via`: solids (hollowed below).
v_c2_0 = newv; Box(v_c2_0) = {-53, -77, 2, 6, 6, 7.5};
v_c2_1 = newv; Box(v_c2_1) = {17, 17, 2, 6, 6, 7.5};
// Port rectangles.
s_p0 = news; Rectangle(s_p0) = {-53, -66, 8, 6, 4}; // port

// Hollow thick conductors: union their solids, subtract the union
// from the whole dielectric stack (interiors excluded, cavity walls kept).
v_cond() = {v_c0_0, v_c0_1, v_c1_0, v_c1_1, v_c1_2, v_c1_3, v_c1_4, v_c1_5, v_c1_6, v_c1_7, v_c1_8, v_c1_9, v_c1_10, v_c1_11, v_c1_12, v_c1_13, v_c1_14, v_c1_15, v_c2_0, v_c2_1};
Geometry.OCCUnionUnify = 0;
v_cond() = BooleanUnion{ Volume{v_cond(0)}; Delete; }{ Volume{v_cond({1 : #v_cond() - 1})}; Delete; };
v_hollow() = BooleanDifference{ Volume{v_d0, v_d1, v_d2, v_d3}; Delete; }{ Volume{v_cond()}; Delete; };
// Conformal interfaces between every slab, cavity wall, sheet and port.
BooleanFragments{ Volume{v_hollow()}; Delete; }{ Surface{s_p0}; Delete; }

// Physical groups: bounding-box selection (robust to OCC renumbering).
g_1() = Volume In BoundingBox{-71.00166, -95.00166, -18.00166, 71.00166, 71.00166, 0.0016600000000000002};
Physical Volume("substrate", 1) = {g_1()};
g_2() = Volume In BoundingBox{-71.00166, -95.00166, -0.0016600000000000002, 71.00166, 71.00166, 12.00166};
Physical Volume("dielectric", 2) = {g_2()};
g_3() = Volume In BoundingBox{-71.00166, -95.00166, 11.99834, 71.00166, 71.00166, 24.00166};
Physical Volume("air", 3) = {g_3()};
g_4() = Volume In BoundingBox{-71.00166, -95.00166, 23.99834, 71.00166, 71.00166, 32.00166};
Physical Volume("air_buffer", 4) = {g_4()};
assigned() = {};
g_104() = {};
g_104() += Surface In BoundingBox{-53.00166, -66.00166, 7.99834, -46.99834, -61.99834, 8.00166};
g_104() = Unique(g_104());
g_104() -= {assigned()};
assigned() += {g_104()};
Physical Surface("port", 104) = {g_104()};
g_101() = {};
g_101() += Surface In BoundingBox{-53.00166, -77.00166, 1.99834, 23.00166, -70.99834, 4.00166};
g_101() += Surface In BoundingBox{16.99834, -77.00166, 1.99834, 23.00166, 23.00166, 4.00166};
g_101() = Unique(g_101());
g_101() -= {assigned()};
assigned() += {g_101()};
Physical Surface("m1", 101) = {g_101()};
g_102() = {};
g_102() += Surface In BoundingBox{-23.00166, 16.99834, 6.49834, 23.00166, 23.00166, 9.50166};
g_102() += Surface In BoundingBox{-23.00166, -23.00166, 6.49834, -16.99834, 23.00166, 9.50166};
g_102() += Surface In BoundingBox{-23.00166, -23.00166, 6.49834, 33.00166, -16.99834, 9.50166};
g_102() += Surface In BoundingBox{26.99834, -23.00166, 6.49834, 33.00166, 33.00166, 9.50166};
g_102() += Surface In BoundingBox{-33.00166, 26.99834, 6.49834, 33.00166, 33.00166, 9.50166};
g_102() += Surface In BoundingBox{-33.00166, -33.00166, 6.49834, -26.99834, 33.00166, 9.50166};
g_102() += Surface In BoundingBox{-33.00166, -33.00166, 6.49834, 43.00166, -26.99834, 9.50166};
g_102() += Surface In BoundingBox{36.99834, -33.00166, 6.49834, 43.00166, 43.00166, 9.50166};
g_102() += Surface In BoundingBox{-43.00166, 36.99834, 6.49834, 43.00166, 43.00166, 9.50166};
g_102() += Surface In BoundingBox{-43.00166, -43.00166, 6.49834, -36.99834, 43.00166, 9.50166};
g_102() += Surface In BoundingBox{-43.00166, -43.00166, 6.49834, 53.00166, -36.99834, 9.50166};
g_102() += Surface In BoundingBox{46.99834, -43.00166, 6.49834, 53.00166, 53.00166, 9.50166};
g_102() += Surface In BoundingBox{-53.00166, 46.99834, 6.49834, 53.00166, 53.00166, 9.50166};
g_102() += Surface In BoundingBox{-53.00166, -53.00166, 6.49834, -46.99834, 53.00166, 9.50166};
g_102() += Surface In BoundingBox{-53.00166, -62.00166, 6.49834, -46.99834, -46.99834, 9.50166};
g_102() += Surface In BoundingBox{-53.00166, -77.00166, 6.49834, -46.99834, -65.99834, 9.50166};
g_102() = Unique(g_102());
g_102() -= {assigned()};
assigned() += {g_102()};
Physical Surface("m2", 102) = {g_102()};
g_103() = {};
g_103() += Surface In BoundingBox{-53.00166, -77.00166, 1.99834, -46.99834, -70.99834, 9.50166};
g_103() += Surface In BoundingBox{16.99834, 16.99834, 1.99834, 23.00166, 23.00166, 9.50166};
g_103() = Unique(g_103());
g_103() -= {assigned()};
assigned() += {g_103()};
Physical Surface("via", 103) = {g_103()};
g_105() = {};
g_105() += Surface In BoundingBox{-71.00166, -95.00166, -18.00166, 71.00166, 71.00166, -17.99834};
g_105() += Surface In BoundingBox{-71.00166, -95.00166, 31.99834, 71.00166, 71.00166, 32.00166};
g_105() += Surface In BoundingBox{-71.00166, -95.00166, -18.00166, -70.99834, 71.00166, 32.00166};
g_105() += Surface In BoundingBox{70.99834, -95.00166, -18.00166, 71.00166, 71.00166, 32.00166};
g_105() += Surface In BoundingBox{-71.00166, -95.00166, -18.00166, 71.00166, -94.99834, 32.00166};
g_105() += Surface In BoundingBox{-71.00166, 70.99834, -18.00166, 71.00166, 71.00166, 32.00166};
g_105() = Unique(g_105());
Physical Surface("outer_boundary", 105) = {g_105()};

// Size field: refine near conductors and ports, grow to size_max.
Field[1] = Distance;
Field[1].SurfacesList = {g_101()};
Field[1].Sampling = 29;
Field[2] = Threshold;
Field[2].InField = 1;
Field[2].SizeMin = 7;
Field[2].SizeMax = 26;
Field[2].DistMin = 6;
Field[2].DistMax = 24;
Field[3] = Distance;
Field[3].SurfacesList = {g_102()};
Field[3].Sampling = 31;
Field[4] = Threshold;
Field[4].InField = 3;
Field[4].SizeMin = 7;
Field[4].SizeMax = 26;
Field[4].DistMin = 6;
Field[4].DistMax = 24;
Field[5] = Distance;
Field[5].SurfacesList = {g_103()};
Field[5].Sampling = 20;
Field[6] = Threshold;
Field[6].InField = 5;
Field[6].SizeMin = 7;
Field[6].SizeMax = 26;
Field[6].DistMin = 6;
Field[6].DistMax = 24;
Field[7] = Distance;
Field[7].SurfacesList = {g_104()};
Field[7].Sampling = 20;
Field[8] = Threshold;
Field[8].InField = 7;
Field[8].SizeMin = 2.5;
Field[8].SizeMax = 26;
Field[8].DistMin = 4;
Field[8].DistMax = 24;
Field[9] = Min;
Field[9].FieldsList = {2, 4, 6, 8};
Background Field = 9;

// Pinned options: sizes from the field only, linear elements,
// MSH 4.1 ASCII (what geode_core::mesh::read_tagged_tet_mesh reads),
// single-threaded for a reproducible mesh.
Mesh.MeshSizeExtendFromBoundary = 0;
Mesh.MeshSizeFromPoints = 0;
Mesh.MeshSizeFromCurvature = 0;
Mesh.MeshSizeMax = 26;
Mesh.Algorithm = 6;
Mesh.Algorithm3D = 1;
Mesh.Optimize = 1;
Mesh.OptimizeNetgen = 0;
Mesh.ElementOrder = 1;
Mesh.SaveAll = 0;
Mesh.MshFileVersion = 4.1;
Mesh.Binary = 0;
General.NumThreads = 1;
