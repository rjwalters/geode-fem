// Coaxial inductance fixture for `geode inductance`
// (issue #714, Epic #702 Phase 3b).
//
// Concentric regions of length L along +z, all meshed (the current flows
// in conductor volumes; the field fills everything):
//
//   core  r < a           solid conductor
//   gap1  a < r < r1      dielectric (mu_r = 1)
//   tube  r1 < r < r2     tubular conductor
//   gap2  r2 < r < b      dielectric
//   shield at r = b       PEC wall (the return conductor)
//
// Each conductor's current enters through its z = 0 face and leaves
// through its z = L face (the path's source / sink terminals, which
// `geode inductance` treats as PEC contacts); the gap annuli at z = 0 and
// z = L are PEC end caps. With PEC ends (n x A = 0, i.e. B.n = 0) the 2-D
// coax field extends exactly along z, so per-length closed forms hold for
// the finite segment. ONE mesh serves two specs:
//
//   * coax (the `benchmarks/magnetostatic_inductance` coax oracle,
//     a = 1, b = 3): one path through `core`; the tube carries no current
//     and its end faces are listed as PEC (so the whole z = 0 / z = L
//     plane stays a magnetic wall):
//       L = L_len [ mu0/(2 pi) ln(b/a) + mu0/(8 pi) ]
//   * triax (a genuine 2x2 matrix with an off-diagonal): paths `core` and
//     `tube`. With D = r2^2 - r1^2,
//       L11 = L_len mu0/(2 pi) [ ln(b/a) + 1/4 ]
//       L22 = L_len mu0/(2 pi) [ ln(b/r2)
//               + ((r2^4 - r1^4)/4 - r1^2 D + r1^4 ln(r2/r1)) / D^2 ]
//       L12 = L_len mu0/(2 pi) [ ln(b/r2) + (D/2 - r1^2 ln(r2/r1)) / D ]
//     (energy of the enclosed-current field B = mu0 I_enc(r) / (2 pi r)).
//
// Physical groups:
//
//   | dim | tag | name     | meaning                                   |
//   |-----|-----|----------|-------------------------------------------|
//   | 3   | 1   | core     | solid conductor r < a                     |
//   | 3   | 2   | tube     | tubular conductor r1 < r < r2             |
//   | 3   | 3   | gaps     | dielectric annuli a<r<r1 and r2<r<b       |
//   | 2   | 11  | core_in  | z = 0 disk of the core                    |
//   | 2   | 12  | core_out | z = L disk of the core                    |
//   | 2   | 13  | tube_in  | z = 0 annulus of the tube                 |
//   | 2   | 14  | tube_out | z = L annulus of the tube                 |
//   | 2   | 15  | shield   | r = b lateral surface (PEC)               |
//   | 2   | 16  | end_caps | z = 0 and z = L gap annuli (PEC)          |
//
// Generate (committed fixtures, see the .provenance.txt files):
//
//   gmsh reference/gmsh/coax_inductance.geo -3 -format msh41 \
//     -setnumber lc 0.35 -o crates/geode-core/tests/fixtures/coax_inductance_smoke.msh
//   gmsh reference/gmsh/coax_inductance.geo -3 -format msh41 \
//     -setnumber lc 0.18 -o crates/geode-core/tests/fixtures/coax_inductance.msh

SetFactory("OpenCASCADE");

DefineConstant[
  a  = {1.0, Name "radius core"},
  r1 = {1.5, Name "radius tube inner"},
  r2 = {2.2, Name "radius tube outer"},
  b  = {3.0, Name "radius shield"},
  L  = {1.0, Name "length"},
  lc = {0.35, Name "mesh size"}
];

Disk(1) = {0, 0, 0, b, b};
Disk(2) = {0, 0, 0, r2, r2};
Disk(3) = {0, 0, 0, r1, r1};
Disk(4) = {0, 0, 0, a, a};
frag[] = BooleanFragments{ Surface{1, 2, 3, 4}; Delete; }{};
Extrude {0, 0, L} { Surface{frag[]}; }
Coherence;

e = 1e-6;
v_a[] = Volume In BoundingBox{-a - e, -a - e, -e, a + e, a + e, L + e};
v_r1[] = Volume In BoundingBox{-r1 - e, -r1 - e, -e, r1 + e, r1 + e, L + e};
v_r2[] = Volume In BoundingBox{-r2 - e, -r2 - e, -e, r2 + e, r2 + e, L + e};
v_b[] = Volume In BoundingBox{-b - e, -b - e, -e, b + e, b + e, L + e};
v_tube[] = v_r2[];
v_tube[] -= v_r1[];
v_gaps[] = v_b[];
v_gaps[] -= v_r2[];
v_gap1[] = v_r1[];
v_gap1[] -= v_a[];
v_gaps[] += v_gap1[];

// z = 0 / z = L faces, by radius band.
s0_a[] = Surface In BoundingBox{-a - e, -a - e, -e, a + e, a + e, e};
s1_a[] = Surface In BoundingBox{-a - e, -a - e, L - e, a + e, a + e, L + e};
s0_r1[] = Surface In BoundingBox{-r1 - e, -r1 - e, -e, r1 + e, r1 + e, e};
s1_r1[] = Surface In BoundingBox{-r1 - e, -r1 - e, L - e, r1 + e, r1 + e, L + e};
s0_r2[] = Surface In BoundingBox{-r2 - e, -r2 - e, -e, r2 + e, r2 + e, e};
s1_r2[] = Surface In BoundingBox{-r2 - e, -r2 - e, L - e, r2 + e, r2 + e, L + e};
s0_b[] = Surface In BoundingBox{-b - e, -b - e, -e, b + e, b + e, e};
s1_b[] = Surface In BoundingBox{-b - e, -b - e, L - e, b + e, b + e, L + e};
tube_in[] = s0_r2[];
tube_in[] -= s0_r1[];
tube_out[] = s1_r2[];
tube_out[] -= s1_r1[];
caps[] = s0_b[];
caps[] += s1_b[];
caps[] -= s0_r2[];
caps[] -= s1_r2[];
gap1_0[] = s0_r1[];
gap1_0[] -= s0_a[];
gap1_1[] = s1_r1[];
gap1_1[] -= s1_a[];
caps[] += gap1_0[];
caps[] += gap1_1[];
s_all[] = Surface In BoundingBox{-b - e, -b - e, -e, b + e, b + e, L + e};
s_in[] = Surface In BoundingBox{-r2 - e, -r2 - e, -e, r2 + e, r2 + e, L + e};
shield[] = s_all[];
shield[] -= s_in[];
shield[] -= caps[];

Physical Volume("core", 1) = {v_a[]};
Physical Volume("tube", 2) = {v_tube[]};
Physical Volume("gaps", 3) = {v_gaps[]};
Physical Surface("core_in", 11) = {s0_a[]};
Physical Surface("core_out", 12) = {s1_a[]};
Physical Surface("tube_in", 13) = {tube_in[]};
Physical Surface("tube_out", 14) = {tube_out[]};
Physical Surface("shield", 15) = {shield[]};
Physical Surface("end_caps", 16) = {caps[]};

Mesh.MeshSizeMin = lc;
Mesh.MeshSizeMax = lc;
Mesh.MeshSizeFromCurvature = 0;
Mesh.MeshSizeExtendFromBoundary = 1;
Mesh.Algorithm3D = 1;
Mesh.Optimize = 1;
Mesh.Binary = 0;
