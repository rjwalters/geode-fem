// Small-mesh sphere-in-vacuum mesh for default-CI Burn vs NumPy
// cross-check (issue #158).
//
// Same physical-group layout as `sphere.geo`, but aggressively coarse
// for the default-`cargo test` budget. PR #155's cross-check is
// `#[ignore]`-gated because faer 0.24 complex `gevd` on the full
// 3300×3300 fixture takes 60+ minutes; this small-mesh sibling
// brings the canonical Burn vs NumPy PML spectrum check into default
// CI by shrinking the matrix dimension by ~15×.
//
// Topology / sizing
// =================
// The 3-shell BooleanFragments topology of `sphere.geo` enforces a
// practical lower bound of ~200 tets — each spherical shell needs at
// least one layer of boundary-conforming tets, and the OCC engine
// fragmenting three concentric spheres into three nested shells
// emits roughly 23 + 84 + 90 = 197 tets at the smallest workable
// `Mesh.CharacteristicLengthFactor = 4.0` (above that, the PLC
// recovery on the inner shell starts to fail).
//
// 197 tets is ~17× smaller than the full 3335-tet fixture, putting
// the interior pencil dim at ~214 (vs ~3300), so the Burn faer 0.24
// complex GEVD takes well under 1s — far below the 30s acceptance
// budget. The "target <100 tets" in #158 is the geometric ideal; the
// 200-tet floor is set by the multi-shell topology, not by relaxed
// effort.

SetFactory("OpenCASCADE");

R_sphere    = 1.0;
R_pml_inner = 1.5;
R_buffer    = 2.0;

Sphere(1) = {0, 0, 0, R_sphere};
Sphere(2) = {0, 0, 0, R_pml_inner};
Sphere(3) = {0, 0, 0, R_buffer};

BooleanFragments{ Volume{3}; Delete; }{ Volume{1}; Volume{2}; Delete; }

Physical Volume("sphere_interior", 1) = {1};
Physical Volume("vacuum_gap",      2) = {3};
Physical Volume("pml_shell",       5) = {2};

Physical Surface("outer_boundary", 3) = {2};
Physical Surface("sphere_surface", 4) = {1};
Physical Surface("pml_interface",  6) = {3};

// OCC ignores Mesh.CharacteristicLength{Min,Max} once volumes are
// fragmented; the engine derives sizes from the input geometry. Use
// the global size factor instead. Factor = 4.0 hits the topology
// floor (~197 tets); factor >= 4.5 starts failing PLC recovery on
// the inner shell.
Mesh.MeshSizeFromPoints   = 0;
Mesh.MeshSizeFromCurvature = 0;
Mesh.CharacteristicLengthFactor = 4.0;
Mesh.CharacteristicLengthMin = 0.5;
Mesh.CharacteristicLengthMax = 10.0;

Mesh.MshFileVersion = 4.1;
Mesh.Binary = 0;
