// Rectangular PEC cavity box a × b × d for the p=2 tagged-mesh eigen
// goldens (issue #871, Epic #836 Phase 2). Unstructured tets, flat walls.
//
// Physical groups:
//   3-D "domain"            the box interior
//   2-D "x0" "x1" "y0" "y1" "z0" "z1"   the six walls, one group each
//
// gmsh reference/gmsh/pec_box.geo -3 -format msh41 -setnumber lc 0.2 -o out.msh
SetFactory("OpenCASCADE");
If (!Exists(lc))
  lc = 0.2;
EndIf
a = 1.0;
b = 0.8;
d = 0.6;
Box(1) = {0, 0, 0, a, b, d};
Mesh.CharacteristicLengthMin = lc;
Mesh.CharacteristicLengthMax = lc;
Mesh.ElementOrder = 1;
Mesh.Algorithm3D = 1;
Physical Volume("domain") = {1};
eps = 1e-6;
Physical Surface("x0") = Surface In BoundingBox{-eps, -eps, -eps, eps, b + eps, d + eps};
Physical Surface("x1") = Surface In BoundingBox{a - eps, -eps, -eps, a + eps, b + eps, d + eps};
Physical Surface("y0") = Surface In BoundingBox{-eps, -eps, -eps, a + eps, eps, d + eps};
Physical Surface("y1") = Surface In BoundingBox{-eps, b - eps, -eps, a + eps, b + eps, d + eps};
Physical Surface("z0") = Surface In BoundingBox{-eps, -eps, -eps, a + eps, b + eps, eps};
Physical Surface("z1") = Surface In BoundingBox{-eps, -eps, d - eps, a + eps, b + eps, d + eps};
