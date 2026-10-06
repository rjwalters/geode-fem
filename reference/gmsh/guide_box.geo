// Rectangular guide box a × b × d for the p=2 TM-guard measurement (issue
// #884, Epic #836 Phase 3a). Unstructured tets; the mesh size is lc at the
// z = 0 (port) face and lc1 at z = d, so lc ≠ lc1 grades the guide axially.
// Physical groups: 3-D "domain", 2-D "walls" (all six faces).
//
// gmsh reference/gmsh/guide_box.geo -3 -format msh41 -setnumber a 2 \
//   -setnumber b 1 -setnumber d 0.6 -setnumber lc 0.12 -setnumber lc1 0.6 -o out.msh
SetFactory("OpenCASCADE");
If (!Exists(lc)) lc = 0.3; EndIf
If (!Exists(lc1)) lc1 = lc; EndIf
If (!Exists(a)) a = 2.0; EndIf
If (!Exists(b)) b = 1.0; EndIf
If (!Exists(d)) d = 0.6; EndIf
Box(1) = {0, 0, 0, a, b, d};
eps = 1e-6;
p0() = Point In BoundingBox{-eps, -eps, -eps, a + eps, b + eps, eps};
MeshSize{ p0() } = lc;
p1() = Point In BoundingBox{-eps, -eps, d - eps, a + eps, b + eps, d + eps};
MeshSize{ p1() } = lc1;
Mesh.ElementOrder = 1;
Mesh.Algorithm3D = 1;
Physical Volume("domain") = {1};
Physical Surface("walls") = Surface{:};
