// Do not edit: regenerate with `geode mesh` (issue #704).
//
// Physical groups:
//   | dim | tag | name | role |
//   | 3 | 1 | below | dielectric slab |
//   | 3 | 2 | gap | dielectric slab |
//   | 3 | 3 | above | dielectric slab |
//   | 2 | 101 | bottom | PEC sheet |
//   | 2 | 102 | plate | PEC sheet |
//   | 2 | 103 | guard | PEC sheet |
//   | 2 | 104 | outer_boundary | outer walls (PEC) |

SetFactory("OpenCASCADE");

// Dielectric slabs (full lateral domain).
v_d0 = newv; Box(v_d0) = {-2.1, -2.1, -0.25, 5.2, 5.2, 0.25}; // below
v_d1 = newv; Box(v_d1) = {-2.1, -2.1, 0, 5.2, 5.2, 1}; // gap
v_d2 = newv; Box(v_d2) = {-2.1, -2.1, 1, 5.2, 5.2, 1}; // above
// Conductor `bottom`: PEC sheets.
s_c0_0 = news; Rectangle(s_c0_0) = {-1.6, -1.6, 0, 4.2, 4.2};
// Conductor `top`: PEC sheets.
s_c1_0 = news; Rectangle(s_c1_0) = {0, 0, 1, 1, 1};
s_c1_1 = news; Rectangle(s_c1_1) = {-1.6, -1.6, 1, 1.5, 4.2};
s_c1_2 = news; Rectangle(s_c1_2) = {1.1, -1.6, 1, 1.5, 4.2};
s_c1_3 = news; Rectangle(s_c1_3) = {-0.1, -1.6, 1, 1.2000000000000002, 1.5};
s_c1_4 = news; Rectangle(s_c1_4) = {-0.1, 1.1, 1, 1.2000000000000002, 1.5};

// Conformal interfaces between every slab, cavity wall, sheet and port.
BooleanFragments{ Volume{v_d0, v_d1, v_d2}; Delete; }{ Surface{s_c0_0, s_c1_0, s_c1_1, s_c1_2, s_c1_3, s_c1_4}; Delete; }

// Physical groups: bounding-box selection (robust to OCC renumbering).
g_1() = Volume In BoundingBox{-2.1000520000000003, -2.1000520000000003, -0.250052, 3.1000520000000003, 3.1000520000000003, 5.2000000000000004e-5};
Physical Volume("below", 1) = {g_1()};
g_2() = Volume In BoundingBox{-2.1000520000000003, -2.1000520000000003, -5.2000000000000004e-5, 3.1000520000000003, 3.1000520000000003, 1.000052};
Physical Volume("gap", 2) = {g_2()};
g_3() = Volume In BoundingBox{-2.1000520000000003, -2.1000520000000003, 0.999948, 3.1000520000000003, 3.1000520000000003, 2.000052};
Physical Volume("above", 3) = {g_3()};
assigned() = {};
g_101() = {};
g_101() += Surface In BoundingBox{-1.600052, -1.600052, -5.2000000000000004e-5, 2.6000520000000003, 2.6000520000000003, 5.2000000000000004e-5};
g_101() = Unique(g_101());
g_101() -= {assigned()};
assigned() += {g_101()};
Physical Surface("bottom", 101) = {g_101()};
g_102() = {};
g_102() += Surface In BoundingBox{-5.2000000000000004e-5, -5.2000000000000004e-5, 0.999948, 1.000052, 1.000052, 1.000052};
g_102() = Unique(g_102());
g_102() -= {assigned()};
assigned() += {g_102()};
Physical Surface("plate", 102) = {g_102()};
g_103() = {};
g_103() += Surface In BoundingBox{-1.600052, -1.600052, 0.999948, -0.09994800000000001, 2.6000520000000003, 1.000052};
g_103() += Surface In BoundingBox{1.0999480000000001, -1.600052, 0.999948, 2.6000520000000003, 2.6000520000000003, 1.000052};
g_103() += Surface In BoundingBox{-0.100052, -1.600052, 0.999948, 1.100052, -0.09994800000000001, 1.000052};
g_103() += Surface In BoundingBox{-0.100052, 1.0999480000000001, 0.999948, 1.100052, 2.6000520000000003, 1.000052};
g_103() = Unique(g_103());
g_103() -= {assigned()};
assigned() += {g_103()};
Physical Surface("guard", 103) = {g_103()};
g_104() = {};
g_104() += Surface In BoundingBox{-2.1000520000000003, -2.1000520000000003, -0.250052, 3.1000520000000003, 3.1000520000000003, -0.249948};
g_104() += Surface In BoundingBox{-2.1000520000000003, -2.1000520000000003, 1.999948, 3.1000520000000003, 3.1000520000000003, 2.000052};
g_104() += Surface In BoundingBox{-2.1000520000000003, -2.1000520000000003, -0.250052, -2.099948, 3.1000520000000003, 2.000052};
g_104() += Surface In BoundingBox{3.099948, -2.1000520000000003, -0.250052, 3.1000520000000003, 3.1000520000000003, 2.000052};
g_104() += Surface In BoundingBox{-2.1000520000000003, -2.1000520000000003, -0.250052, 3.1000520000000003, -2.099948, 2.000052};
g_104() += Surface In BoundingBox{-2.1000520000000003, 3.099948, -0.250052, 3.1000520000000003, 3.1000520000000003, 2.000052};
g_104() = Unique(g_104());
Physical Surface("outer_boundary", 104) = {g_104()};

// Size field: refine near conductors and ports, grow to size_max.
Field[1] = Distance;
Field[1].SurfacesList = {g_101()};
Field[1].Sampling = 29;
Field[2] = Threshold;
Field[2].InField = 1;
Field[2].SizeMin = 0.3;
Field[2].SizeMax = 0.4;
Field[2].DistMin = 0.3;
Field[2].DistMax = 0.5;
Field[3] = Distance;
Field[3].SurfacesList = {g_102()};
Field[3].Sampling = 20;
Field[4] = Threshold;
Field[4].InField = 3;
Field[4].SizeMin = 0.1;
Field[4].SizeMax = 0.4;
Field[4].DistMin = 0.1;
Field[4].DistMax = 0.5;
Field[5] = Distance;
Field[5].SurfacesList = {g_103()};
Field[5].Sampling = 84;
Field[6] = Threshold;
Field[6].InField = 5;
Field[6].SizeMin = 0.1;
Field[6].SizeMax = 0.4;
Field[6].DistMin = 0.1;
Field[6].DistMax = 0.5;
Field[7] = Min;
Field[7].FieldsList = {2, 4, 6};
Background Field = 7;

// Pinned options: sizes from the field only, linear elements,
// MSH 4.1 ASCII (what geode_core::mesh::read_tagged_tet_mesh reads),
// single-threaded for a reproducible mesh.
Mesh.MeshSizeExtendFromBoundary = 0;
Mesh.MeshSizeFromPoints = 0;
Mesh.MeshSizeFromCurvature = 0;
Mesh.MeshSizeMax = 0.4;
Mesh.Algorithm = 6;
Mesh.Algorithm3D = 1;
Mesh.Optimize = 1;
Mesh.OptimizeNetgen = 0;
Mesh.ElementOrder = 1;
Mesh.SaveAll = 0;
Mesh.MshFileVersion = 4.1;
Mesh.Binary = 0;
General.NumThreads = 1;
