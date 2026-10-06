// Coaxial guide (outer radius r, inner radius ri, length d) for the
// canonical wave-port gauge (issues #892 / #896): unstructured tets, mesh
// size lc at the z = 0 port and lc1 at z = d, so the two ports have
// different face meshes. Its TE11 pair is continuously degenerate; with the
// polygonal rims the two ports' splits differ in kind (the #896 Judge
// probe). Physical groups: 3-D "domain", 2-D "walls" (all faces; the tests
// take the ports from boundary_faces() at z = 0 / z = d).
//
// gmsh reference/gmsh/guide_coax.geo -3 -format msh41 -setnumber lc 0.18 \
//   -setnumber lc1 0.15 -o out.msh
SetFactory("OpenCASCADE");
If (!Exists(lc)) lc = 0.2; EndIf
If (!Exists(lc1)) lc1 = lc; EndIf
If (!Exists(ri)) ri = 0.35; EndIf
If (!Exists(r)) r = 1.0; EndIf
If (!Exists(d)) d = 1.0; EndIf
Cylinder(1) = {0, 0, 0, 0, 0, d, r};
Cylinder(2) = {0, 0, 0, 0, 0, d, ri};
BooleanDifference(3) = { Volume{1}; Delete; }{ Volume{2}; Delete; };
eps = 1e-6;
p0() = Point In BoundingBox{-r - eps, -r - eps, -eps, r + eps, r + eps, eps};
MeshSize{ p0() } = lc;
p1() = Point In BoundingBox{-r - eps, -r - eps, d - eps, r + eps, r + eps, d + eps};
MeshSize{ p1() } = lc1;
Mesh.MeshSizeFromCurvature = 0;
Mesh.ElementOrder = 1;
Mesh.Algorithm3D = 1;
Physical Volume("domain") = {3};
Physical Surface("walls") = Surface{:};
