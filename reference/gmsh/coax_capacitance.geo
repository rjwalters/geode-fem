// Triaxial coax capacitance fixture for `geode capacitance`
// (issue #705, Epic #702 Phase 3).
//
// Three concentric PEC cylinders of length L along +z, radii
// a < m < b, with the two annular dielectric gaps meshed as separate
// volumes (the conductor interiors are NOT meshed). The r = m surface is
// an interior face set shared by both annuli, so ONE mesh serves two
// specs:
//
//   * plain coax (the `benchmarks/electrostatic` coax oracle,
//     a = 1, b = 2.5): terminals = [inner], ground = [outer]; the
//     `shield` group is left unlisted, i.e. just interior faces.
//       C = 2 pi eps0 eps_r L / ln(b/a)
//   * triaxial (a genuine 2x2 Maxwell matrix with an off-diagonal):
//     terminals = [inner, shield], ground = [outer]; with gap
//     capacitances C1 = 2 pi eps0 eps1 L / ln(m/a) and
//     C2 = 2 pi eps0 eps2 L / ln(b/m):
//       C = [[ C1, -C1 ], [ -C1, C1 + C2 ]]
//
// The end caps (z = 0, z = L) are left as natural (zero normal-flux)
// boundaries, so the 2-D coax field extends exactly along z and the
// per-length formulas above hold for the finite segment.
//
// Physical groups:
//
//   | dim | tag | name             | meaning                          |
//   |-----|-----|------------------|----------------------------------|
//   | 3   | 1   | dielectric_inner | annulus a < r < m                |
//   | 3   | 2   | dielectric_outer | annulus m < r < b                |
//   | 2   | 11  | inner            | r = a conductor surface          |
//   | 2   | 12  | shield           | r = m (interior) surface         |
//   | 2   | 13  | outer            | r = b conductor surface          |
//   | 2   | 14  | end_caps         | z = 0 and z = L annuli           |
//
// Generate (committed fixtures, see the .provenance.txt files):
//
//   gmsh reference/gmsh/coax_capacitance.geo -3 -format msh41 \
//     -setnumber lc 0.3 -o crates/geode-core/tests/fixtures/coax_capacitance_smoke.msh
//   gmsh reference/gmsh/coax_capacitance.geo -3 -format msh41 \
//     -setnumber lc 0.15 -o crates/geode-core/tests/fixtures/coax_capacitance.msh

SetFactory("OpenCASCADE");

DefineConstant[
  a  = {1.0, Name "radius inner"},
  m  = {1.6, Name "radius shield"},
  b  = {2.5, Name "radius outer"},
  L  = {1.0, Name "length"},
  lc = {0.3, Name "mesh size"}
];

Disk(1) = {0, 0, 0, b, b};
Disk(2) = {0, 0, 0, m, m};
Disk(3) = {0, 0, 0, a, a};
frag[] = BooleanFragments{ Surface{1, 2, 3}; Delete; }{};
// Drop the r < a disk (inner conductor interior is not meshed).
core[] = Surface In BoundingBox{-a - 1e-6, -a - 1e-6, -1e-6, a + 1e-6, a + 1e-6, 1e-6};
Recursive Delete { Surface{core[]}; }
ann_in[] = Surface In BoundingBox{-m - 1e-6, -m - 1e-6, -1e-6, m + 1e-6, m + 1e-6, 1e-6};
ann_all[] = Surface In BoundingBox{-b - 1e-6, -b - 1e-6, -1e-6, b + 1e-6, b + 1e-6, 1e-6};
ann_out[] = ann_all[];
ann_out[] -= ann_in[];

Extrude {0, 0, L} { Surface{ann_in[], ann_out[]}; }
Coherence;

e = 1e-6;
vol_in[] = Volume In BoundingBox{-m - e, -m - e, -e, m + e, m + e, L + e};
vol_all[] = Volume In BoundingBox{-b - e, -b - e, -e, b + e, b + e, L + e};
vol_out[] = vol_all[];
vol_out[] -= vol_in[];

caps[] = Surface In BoundingBox{-b - e, -b - e, -e, b + e, b + e, e};
caps[] += Surface In BoundingBox{-b - e, -b - e, L - e, b + e, b + e, L + e};
s_a[] = Surface In BoundingBox{-a - e, -a - e, -e, a + e, a + e, L + e};
s_m[] = Surface In BoundingBox{-m - e, -m - e, -e, m + e, m + e, L + e};
s_m[] -= s_a[];
s_m[] -= caps[];
s_b[] = Surface In BoundingBox{-b - e, -b - e, -e, b + e, b + e, L + e};
s_b[] -= s_m[];
s_b[] -= s_a[];
s_b[] -= caps[];

Physical Volume("dielectric_inner", 1) = {vol_in[]};
Physical Volume("dielectric_outer", 2) = {vol_out[]};
Physical Surface("inner", 11) = {s_a[]};
Physical Surface("shield", 12) = {s_m[]};
Physical Surface("outer", 13) = {s_b[]};
Physical Surface("end_caps", 14) = {caps[]};

Mesh.MeshSizeMin = lc;
Mesh.MeshSizeMax = lc;
Mesh.MeshSizeFromCurvature = 0;
Mesh.MeshSizeExtendFromBoundary = 1;
Mesh.Algorithm3D = 1;
Mesh.Optimize = 1;
Mesh.Binary = 0;
