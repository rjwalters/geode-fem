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
v_d0 = newv; Box(v_d0) = {-108, -140, -50, 216, 248, 50}; // substrate
v_d1 = newv; Box(v_d1) = {-108, -140, 0, 216, 248, 10}; // dielectric
v_d2 = newv; Box(v_d2) = {-108, -140, 10, 216, 248, 25}; // air
v_d3 = newv; Box(v_d3) = {-108, -140, 35, 216, 248, 25}; // air_buffer
// Conductor `m1`: solids (hollowed below).
v_c0_0 = newv; Box(v_c0_0) = {-63, -95, 1, 96, 6, 2};
v_c0_1 = newv; Box(v_c0_1) = {27, -95, 1, 6, 128, 2};
// Conductor `m2`: solids (hollowed below).
v_c1_0 = newv; Box(v_c1_0) = {-33, 27, 5, 66, 6, 3};
v_c1_1 = newv; Box(v_c1_1) = {-33, -33, 5, 6, 66, 3};
v_c1_2 = newv; Box(v_c1_2) = {-33, -33, 5, 76, 6, 3};
v_c1_3 = newv; Box(v_c1_3) = {37, -33, 5, 6, 76, 3};
v_c1_4 = newv; Box(v_c1_4) = {-43, 37, 5, 86, 6, 3};
v_c1_5 = newv; Box(v_c1_5) = {-43, -43, 5, 6, 86, 3};
v_c1_6 = newv; Box(v_c1_6) = {-43, -43, 5, 96, 6, 3};
v_c1_7 = newv; Box(v_c1_7) = {47, -43, 5, 6, 96, 3};
v_c1_8 = newv; Box(v_c1_8) = {-53, 47, 5, 106, 6, 3};
v_c1_9 = newv; Box(v_c1_9) = {-53, -53, 5, 6, 106, 3};
v_c1_10 = newv; Box(v_c1_10) = {-53, -53, 5, 116, 6, 3};
v_c1_11 = newv; Box(v_c1_11) = {57, -53, 5, 6, 116, 3};
v_c1_12 = newv; Box(v_c1_12) = {-63, 57, 5, 126, 6, 3};
v_c1_13 = newv; Box(v_c1_13) = {-63, -63, 5, 6, 126, 3};
v_c1_14 = newv; Box(v_c1_14) = {-63, -80, 5, 6, 23, 3};
v_c1_15 = newv; Box(v_c1_15) = {-63, -95, 5, 6, 11, 3};
// Conductor `via`: solids (hollowed below).
v_c2_0 = newv; Box(v_c2_0) = {-63, -95, 1, 6, 6, 7};
v_c2_1 = newv; Box(v_c2_1) = {27, 27, 1, 6, 6, 7};
// Port rectangles.
s_p0 = news; Rectangle(s_p0) = {-63, -84, 6.5, 6, 4}; // port

// Hollow thick conductors: union their solids, subtract the union
// from the whole dielectric stack (interiors excluded, cavity walls kept).
v_cond() = {v_c0_0, v_c0_1, v_c1_0, v_c1_1, v_c1_2, v_c1_3, v_c1_4, v_c1_5, v_c1_6, v_c1_7, v_c1_8, v_c1_9, v_c1_10, v_c1_11, v_c1_12, v_c1_13, v_c1_14, v_c1_15, v_c2_0, v_c2_1};
Geometry.OCCUnionUnify = 0;
v_cond() = BooleanUnion{ Volume{v_cond(0)}; Delete; }{ Volume{v_cond({1 : #v_cond() - 1})}; Delete; };
v_hollow() = BooleanDifference{ Volume{v_d0, v_d1, v_d2, v_d3}; Delete; }{ Volume{v_cond()}; Delete; };
// Conformal interfaces between every slab, cavity wall, sheet and port.
BooleanFragments{ Volume{v_hollow()}; Delete; }{ Surface{s_p0}; Delete; }

// Physical groups: bounding-box selection (robust to OCC renumbering).
g_1() = Volume In BoundingBox{-108.002, -140.002, -50.002, 108.002, 108.002, 0.002};
Physical Volume("substrate", 1) = {g_1()};
g_2() = Volume In BoundingBox{-108.002, -140.002, -0.002, 108.002, 108.002, 10.002};
Physical Volume("dielectric", 2) = {g_2()};
g_3() = Volume In BoundingBox{-108.002, -140.002, 9.998, 108.002, 108.002, 35.002};
Physical Volume("air", 3) = {g_3()};
g_4() = Volume In BoundingBox{-108.002, -140.002, 34.998, 108.002, 108.002, 60.002};
Physical Volume("air_buffer", 4) = {g_4()};
assigned() = {};
g_104() = {};
g_104() += Surface In BoundingBox{-63.002, -84.002, 6.498, -56.998, -79.998, 6.502};
g_104() = Unique(g_104());
g_104() -= {assigned()};
assigned() += {g_104()};
Physical Surface("port", 104) = {g_104()};
g_101() = {};
g_101() += Surface In BoundingBox{-63.002, -95.002, 0.998, 33.002, -88.998, 3.002};
g_101() += Surface In BoundingBox{26.998, -95.002, 0.998, 33.002, 33.002, 3.002};
g_101() = Unique(g_101());
g_101() -= {assigned()};
assigned() += {g_101()};
Physical Surface("m1", 101) = {g_101()};
g_102() = {};
g_102() += Surface In BoundingBox{-33.002, 26.998, 4.998, 33.002, 33.002, 8.002};
g_102() += Surface In BoundingBox{-33.002, -33.002, 4.998, -26.998, 33.002, 8.002};
g_102() += Surface In BoundingBox{-33.002, -33.002, 4.998, 43.002, -26.998, 8.002};
g_102() += Surface In BoundingBox{36.998, -33.002, 4.998, 43.002, 43.002, 8.002};
g_102() += Surface In BoundingBox{-43.002, 36.998, 4.998, 43.002, 43.002, 8.002};
g_102() += Surface In BoundingBox{-43.002, -43.002, 4.998, -36.998, 43.002, 8.002};
g_102() += Surface In BoundingBox{-43.002, -43.002, 4.998, 53.002, -36.998, 8.002};
g_102() += Surface In BoundingBox{46.998, -43.002, 4.998, 53.002, 53.002, 8.002};
g_102() += Surface In BoundingBox{-53.002, 46.998, 4.998, 53.002, 53.002, 8.002};
g_102() += Surface In BoundingBox{-53.002, -53.002, 4.998, -46.998, 53.002, 8.002};
g_102() += Surface In BoundingBox{-53.002, -53.002, 4.998, 63.002, -46.998, 8.002};
g_102() += Surface In BoundingBox{56.998, -53.002, 4.998, 63.002, 63.002, 8.002};
g_102() += Surface In BoundingBox{-63.002, 56.998, 4.998, 63.002, 63.002, 8.002};
g_102() += Surface In BoundingBox{-63.002, -63.002, 4.998, -56.998, 63.002, 8.002};
g_102() += Surface In BoundingBox{-63.002, -80.002, 4.998, -56.998, -56.998, 8.002};
g_102() += Surface In BoundingBox{-63.002, -95.002, 4.998, -56.998, -83.998, 8.002};
g_102() = Unique(g_102());
g_102() -= {assigned()};
assigned() += {g_102()};
Physical Surface("m2", 102) = {g_102()};
g_103() = {};
g_103() += Surface In BoundingBox{-63.002, -95.002, 0.998, -56.998, -88.998, 8.002};
g_103() += Surface In BoundingBox{26.998, 26.998, 0.998, 33.002, 33.002, 8.002};
g_103() = Unique(g_103());
g_103() -= {assigned()};
assigned() += {g_103()};
Physical Surface("via", 103) = {g_103()};
g_105() = {};
g_105() += Surface In BoundingBox{-108.002, -140.002, -50.002, 108.002, 108.002, -49.998};
g_105() += Surface In BoundingBox{-108.002, -140.002, 59.998, 108.002, 108.002, 60.002};
g_105() += Surface In BoundingBox{-108.002, -140.002, -50.002, -107.998, 108.002, 60.002};
g_105() += Surface In BoundingBox{107.998, -140.002, -50.002, 108.002, 108.002, 60.002};
g_105() += Surface In BoundingBox{-108.002, -140.002, -50.002, 108.002, -139.998, 60.002};
g_105() += Surface In BoundingBox{-108.002, 107.998, -50.002, 108.002, 108.002, 60.002};
g_105() = Unique(g_105());
Physical Surface("outer_boundary", 105) = {g_105()};

// Size field: refine near conductors and ports, grow to size_max.
Field[1] = Distance;
Field[1].SurfacesList = {g_101()};
Field[1].Sampling = 64;
Field[2] = Threshold;
Field[2].InField = 1;
Field[2].SizeMin = 4;
Field[2].SizeMax = 30;
Field[2].DistMin = 6;
Field[2].DistMax = 45;
Field[3] = Distance;
Field[3].SurfacesList = {g_102()};
Field[3].Sampling = 63;
Field[4] = Threshold;
Field[4].InField = 3;
Field[4].SizeMin = 4;
Field[4].SizeMax = 30;
Field[4].DistMin = 6;
Field[4].DistMax = 45;
Field[5] = Distance;
Field[5].SurfacesList = {g_103()};
Field[5].Sampling = 20;
Field[6] = Threshold;
Field[6].InField = 5;
Field[6].SizeMin = 4;
Field[6].SizeMax = 30;
Field[6].DistMin = 6;
Field[6].DistMax = 45;
Field[7] = Distance;
Field[7].SurfacesList = {g_104()};
Field[7].Sampling = 20;
Field[8] = Threshold;
Field[8].InField = 7;
Field[8].SizeMin = 1.5;
Field[8].SizeMax = 30;
Field[8].DistMin = 4;
Field[8].DistMax = 45;
Field[9] = Min;
Field[9].FieldsList = {2, 4, 6, 8};
Background Field = 9;

// Pinned options: sizes from the field only, linear elements,
// MSH 4.1 ASCII (what geode_core::mesh::read_tagged_tet_mesh reads),
// single-threaded for a reproducible mesh.
Mesh.MeshSizeExtendFromBoundary = 0;
Mesh.MeshSizeFromPoints = 0;
Mesh.MeshSizeFromCurvature = 0;
Mesh.MeshSizeMax = 30;
Mesh.Algorithm = 6;
Mesh.Algorithm3D = 1;
Mesh.Optimize = 1;
Mesh.OptimizeNetgen = 0;
Mesh.ElementOrder = 1;
Mesh.SaveAll = 0;
Mesh.MshFileVersion = 4.1;
Mesh.Binary = 0;
General.NumThreads = 1;
