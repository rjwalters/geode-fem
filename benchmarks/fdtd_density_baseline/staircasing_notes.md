# Staircase and facet geometry diagnostics (#1089)

Run from the repository root with Python 3.11+, NumPy and Matplotlib:

```sh
python3 benchmarks/fdtd_density_baseline/staircasing_demo.py
python3 benchmarks/fdtd_density_baseline/staircasing_verify.py
```

The JSON and PNG supersede the old h=1.6 mm smoke-geometry diagnostic.
The committed smoke mesh actually has h=2 mm, bend radius 40 mm, patch
half-angle 0.2 rad and substrate half-angle 0.3 rad. The top/bottom radii
are therefore 41/39 mm. This is the historical smoke fixture, not #1084's
new physical 2.4 GHz antenna (h=1.6 mm and bend radius 100 mm).

The main metric is the dimensionless RMS top-profile displacement divided
by the top arc length, retained as `resonance_shift_proxy_abs` for continuity.
At 1–16 cells/mm it falls from 0.01611 to 0.001064 on the patch and from
0.01092 to 0.0007256 over the substrate. The fitted powers of cell size
are 0.980 and 0.978. This is an uncalibrated geometric proxy, not a solved
frequency shift or S11 error. A column's arc height is sampled at its centre
and rounded to a cell face; the JSON records the independent occupied-sector
area and perimeter too. End-column centres are clipped to the arc endpoint.
The grid origin and dense x sampling are fixed; grid-phase sensitivity is not
studied. The patch perimeter diagnostic refers to the *slab sector under the
patch footprint*, not the perimeter of the 3D metal patch.

The nonzero Cartesian perimeter limit is the taxicab staircase paradox.
It cannot establish non-converging electromagnetic error or the impossibility
of structured-grid inverse design. The dashed limits in panel (d) follow from
the total variation of the circular arcs and radial end caps. This occupancy
model does not represent Meep's subpixel averaging or conformal-FDTD methods.

Linear FEM facets have finite error between vertices. The script reads the
actual physical patch/ground triangles from the SHA-pinned smoke mesh, bends
them using the same nodal cylindrical map, and integrates squared radial
distance to the cylinder with triangle quadrature. L0 patch maximum error
is 16.87 µm, RMS 6.92 µm; the ground values are 15.09 and 6.56 µm. The maximum
is computed from the nearest point in each facet's x-z projection, not merely
from its centroid. An analytic chord test and doubled-order quadrature check
validate this calculation.

No existing L1–L3 smoke volume meshes were available. Here L1–L3 explicitly
mean a surface-only midpoint subdivision in flat coordinates before bending,
so newly introduced nodes lie on the cylinder. Maximum and RMS errors decrease
by about four per level. These are geometric refinement diagnostics, **not
solved FEM refinement levels**; they must not be paired with solver timings.
The later #1085 volume-mesh ladder needs its own geometry measurements.

The arbitrary 1 µm target and extrapolated cost-to-match claim have been
removed. Cartesian vertical RMS and FEM radial RMS have different weighting;
no equal-error cost comparison is inferred from these plots. The actual
matched Meep S11-vs-resolution experiment is #1088.
