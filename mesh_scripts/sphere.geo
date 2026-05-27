// Sphere-in-vacuum mesh for dielectric eigenmode + PML work
// (issues #25, #28, #38).
//
// Generates a tetrahedralized dielectric sphere of radius R_sphere = 1.0
// embedded in two concentric vacuum shells:
//
//   - "vacuum_gap"  shell:  R_sphere <  r <= R_pml_inner   (real vacuum,
//                                                          ~half a unit
//                                                          buffer)
//   - "pml_shell"   shell:  R_pml_inner < r <= R_buffer    (absorbing PML)
//
// The vacuum gap provides a few wavelengths of un-stretched vacuum
// between the dielectric scatterer and the PML start — standard PML
// practice. The PML quadratic absorption ramp is anchored at
// R_pml_inner, not R_sphere, so the dielectric interface no longer sits
// directly inside the lossy region.
//
// Five physical groups are tagged so downstream consumers can apply
// per-region material parameters and outer-boundary conditions:
//
//   (3, 1) "sphere_interior" -- tets inside r <= R_sphere
//   (3, 2) "vacuum_gap"      -- tets in R_sphere < r <= R_pml_inner
//   (3, 5) "pml_shell"       -- tets in R_pml_inner < r <= R_buffer
//   (2, 3) "outer_boundary"  -- surface triangles on r = R_buffer
//   (2, 4) "sphere_surface"  -- surface triangles on r = R_sphere (interface)
//   (2, 6) "pml_interface"   -- surface triangles on r = R_pml_inner
//
// Run with:
//   gmsh -3 -format msh4 -o sphere.msh sphere.geo
//
// To regenerate the bundled fixture used by `read_sphere_fixture()`:
//   gmsh -3 -format msh4 -o ../crates/geode-core/tests/fixtures/sphere.msh sphere.geo

SetFactory("OpenCASCADE");

R_sphere    = 1.0;
R_pml_inner = 1.5;
R_buffer    = 2.0;

// Mesh size: coarse on purpose. The fixture is meant for fast smoke
// tests; refinement comes from regenerating with a smaller `lc`.
lc_sphere = 0.35;
lc_buffer = 0.6;

// Three concentric solid balls. BooleanFragments then carves them into
// three nested shells with conformal interfaces.
Sphere(1) = {0, 0, 0, R_sphere};
Sphere(2) = {0, 0, 0, R_pml_inner};
Sphere(3) = {0, 0, 0, R_buffer};

// Carve all three; preserve every fragment. The shared interfaces
// (r = R_sphere and r = R_pml_inner) become conformal stitched surfaces.
BooleanFragments{ Volume{3}; Delete; }{ Volume{1}; Volume{2}; Delete; }

// After fragments, Gmsh renumbers entities. Verified by bounding-box
// inspection of the generated entity entries (`$Entities` block):
//   Volume 1 -> interior ball       (r <= R_sphere)        [bbox = 1.0]
//   Volume 2 -> outer pml shell     (R_pml_inner < r <= R_buffer) [bbox = 2.0]
//   Volume 3 -> middle vacuum gap   (R_sphere    < r <= R_pml_inner) [bbox = 1.5]
// The OCC `BooleanFragments` engine assigns OCC volume ids in the order
// of the original largest-to-smallest containing sphere; the smaller
// fragments inherit the larger's id last. The mapping below is fixed.

Physical Volume("sphere_interior", 1) = {1};
Physical Volume("vacuum_gap",      2) = {3};
Physical Volume("pml_shell",       5) = {2};

// Surfaces after the fragments (verified by bounding-box inspection
// of the generated `$Entities` block):
//   Surface 1 -> sphere interface       (r = R_sphere)    [bbox = 1.0]
//   Surface 2 -> outer sphere wall      (r = R_buffer)    [bbox = 2.0]
//   Surface 3 -> pml inner interface    (r = R_pml_inner) [bbox = 1.5]
Physical Surface("outer_boundary", 3) = {2};
Physical Surface("sphere_surface", 4) = {1};
Physical Surface("pml_interface",  6) = {3};

// Mesh sizing: smaller cells inside the dielectric, coarser in the buffer.
Mesh.CharacteristicLengthMin = lc_sphere;
Mesh.CharacteristicLengthMax = lc_buffer;

// MSH 4.1 ASCII output keeps the fixture diff-able.
Mesh.MshFileVersion = 4.1;
Mesh.Binary = 0;
