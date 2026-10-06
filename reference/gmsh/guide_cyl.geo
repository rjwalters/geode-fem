// Circular guide of radius r and length d for the canonical wave-port gauge
// (issues #888 / #892): unstructured tets, mesh size lc at the z = 0 (port)
// face and lc1 at z = d. Its TE11 / TE21 pairs are continuously degenerate
// and split by the discretization, so every face solve has degenerate-
// cluster candidates. Physical groups: 3-D "domain", 2-D "walls" (all
// faces; the tests take the ports from boundary_faces() at z = 0 / z = d).
//
// gmsh reference/gmsh/guide_cyl.geo -3 -format msh41 -setnumber r 1 \
//   -setnumber d 0.2 -setnumber lc 0.11 -setnumber lc1 0.5 -o out.msh
SetFactory("OpenCASCADE");
If (!Exists(lc)) lc = 0.3; EndIf
If (!Exists(lc1)) lc1 = lc; EndIf
If (!Exists(r)) r = 1.0; EndIf
If (!Exists(d)) d = 1.0; EndIf
Cylinder(1) = {0, 0, 0, 0, 0, d, r};
eps = 1e-6;
p0() = Point In BoundingBox{-r - eps, -r - eps, -eps, r + eps, r + eps, eps};
MeshSize{ p0() } = lc;
p1() = Point In BoundingBox{-r - eps, -r - eps, d - eps, r + eps, r + eps, d + eps};
MeshSize{ p1() } = lc1;
Mesh.MeshSizeFromCurvature = 0;
Mesh.ElementOrder = 1;
Mesh.Algorithm3D = 1;
Physical Volume("domain") = {1};
Physical Surface("walls") = Surface{:};
