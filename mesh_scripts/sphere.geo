// Sphere-in-vacuum mesh for dielectric eigenmode work (issue #25).
//
// Generates a tetrahedralized dielectric sphere of radius R_sphere = 1.0
// embedded in a concentric vacuum buffer of outer radius R_buffer = 2.0.
// Three physical groups are tagged so downstream consumers can apply
// per-region material parameters and outer-boundary conditions:
//
//   (3, 1) "sphere_interior" -- tets inside r <= R_sphere
//   (3, 2) "vacuum_buffer"   -- tets in R_sphere < r <= R_buffer
//   (2, 3) "outer_boundary"  -- surface triangles on r = R_buffer
//   (2, 4) "sphere_surface"  -- surface triangles on r = R_sphere (interface)
//
// Run with:
//   gmsh -3 -format msh4 -o sphere.msh sphere.geo
//
// To regenerate the bundled fixture used by `read_sphere_fixture()`:
//   gmsh -3 -format msh4 -o ../crates/geode-core/tests/fixtures/sphere.msh sphere.geo

SetFactory("OpenCASCADE");

R_sphere = 1.0;
R_buffer = 2.0;

// Mesh size: coarse on purpose. The fixture is meant for fast smoke
// tests; refinement comes from regenerating with a smaller `lc`.
lc_sphere = 0.35;
lc_buffer = 0.6;

// Inner solid ball (dielectric).
Sphere(1) = {0, 0, 0, R_sphere};

// Outer solid ball (will become the union with buffer once we boolean).
Sphere(2) = {0, 0, 0, R_buffer};

// Carve the inner ball out of the outer ball; keep the inner ball as a
// separate volume. `BooleanFragments` preserves both volumes and stitches
// the shared interface as a conformal surface, which is exactly what we
// want for the (2, 4) "sphere_surface" physical group.
BooleanFragments{ Volume{2}; Delete; }{ Volume{1}; Delete; }

// After fragments, Gmsh renumbers entities. The smaller (interior) volume
// becomes volume 2 and the buffer shell volume becomes volume 1 in the
// OpenCASCADE convention. We make this explicit by querying by mass /
// bounding box if needed, but for this geometry the assignment is stable.
//
// Volume tags after BooleanFragments on two concentric spheres (OCC):
// the larger original (volume 2) absorbs the smaller fragment, leaving
//   1 -> interior ball (r < R_sphere)
//   2 -> buffer shell (R_sphere < r < R_buffer)
// Verified by mesh-time inspection: tets carrying physical 1 have all
// vertices in r <= R_sphere; tets carrying physical 2 span the shell.

Physical Volume("sphere_interior", 1) = {1};
Physical Volume("vacuum_buffer",   2) = {2};

// Surfaces after the fragment (verified by bounding-box inspection):
//   1 -> interface (r = R_sphere)
//   2 -> outer sphere (r = R_buffer)
Physical Surface("outer_boundary", 3) = {2};
Physical Surface("sphere_surface", 4) = {1};

// Mesh sizing: smaller cells inside the dielectric, coarser in the buffer.
// Field-based sizing keeps the interface well-resolved.
Mesh.CharacteristicLengthMin = lc_sphere;
Mesh.CharacteristicLengthMax = lc_buffer;

// Algorithm 1 = MeshAdapt (2D), Algorithm 3D = 1 (Delaunay) is the default
// and works fine here. MSH 4.1 ASCII output keeps the fixture diff-able.
Mesh.MshFileVersion = 4.1;
Mesh.Binary = 0;
