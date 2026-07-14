# Transmon + readout-resonator Gmsh fixture generator (Epic #476, Phase A,
# issue #485).
#
# Drives DeviceLayout.jl's `SingleTransmon` example (the AWS
# `aws-cqc/DeviceLayout.jl` transmon benchmark) with the blog's default
# parameters and emits an **MSH 4.1 ASCII, first-order (Tet4)** mesh
# carrying the physical-group schema geode-fem's `mesh::transmon` adapter
# consumes. The generated mesh is a committed *fixture* — Julia and gmsh
# are NOT geode-fem CI dependencies (the same offline-generation policy as
# `reference/gmsh/generate_spiral_fixture.py`).
#
# ─────────────────────────────────────────────────────────────────────
# OPERATOR STEP (one command)
# ─────────────────────────────────────────────────────────────────────
#
#   julia --project=reference/julia \
#       reference/julia/generate_transmon_fixture.jl \
#       crates/geode-core/tests/fixtures/transmon_smoke.msh
#
# then update `transmon_smoke.provenance.txt` from the printed report and
# refresh the per-group element-count assertions in
# `crates/geode-core/tests/transmon_mesh.rs`.
#
# ─────────────────────────────────────────────────────────────────────
# TOOLCHAIN GAP (why a placeholder fixture ships first)
# ─────────────────────────────────────────────────────────────────────
#
# DeviceLayout.jl (v1.15.0, verified 2026-07-13) unconditionally
# `include`s `src/backends/graphics.jl` at module load, which pulls in
# `Cairo.jl`. On the Loom build environment Cairo fails to precompile
# (`UndefVarError: libpango not defined` — a broken Pango_jll/Cairo_jll
# artifact), so `import DeviceLayout` errors before any mesh can be
# generated. This is the established "builders/Doctors cannot run Julia
# locally" toolchain gap. Until an operator runs this script in a working
# Julia+Cairo environment, the adapter is developed and CI-tested against
# a gmsh-generated PLACEHOLDER carrying the identical group schema
# (`reference/gmsh/transmon_placeholder.geo` /
# `generate_transmon_placeholder.py`). The real DeviceLayout mesh is a
# drop-in replacement — same group names, same adapter, updated counts.
#
# ─────────────────────────────────────────────────────────────────────
# PROVENANCE / UPSTREAM FACTS (cite-checked against the installed
# DeviceLayout v1.15.0 `examples/SingleTransmon/SingleTransmon.jl`)
# ─────────────────────────────────────────────────────────────────────
#
# Physical groups the (non-wave-port, `wave_ports=false`) Palace config
# consumes — `single_transmon()` passes these names to
# `singlechip_solidmodel_target(...)` so gmsh retains them:
#
#   vacuum             (3D)  vacuum box,     Permittivity = 1.0
#   substrate          (3D)  sapphire chip,  Permittivity = [9.3,9.3,11.5]
#   metal              (2D)  PEC (ground + pads + trace + readout line)
#   exterior_boundary  (2D)  first-order Absorbing far-field wall
#   port_1             (2D)  lumped port, R = 50 Ω,  Direction +X
#   port_2             (2D)  lumped port, R = 50 Ω,  Direction +X
#   lumped_element     (2D)  junction port, L = 14.860 nH, C = 5.5 fF,
#                            Direction +Y
#
# Substrate is SAPPHIRE, anisotropic AND ROTATED: the lab-frame ε tensor
# is  R · diag(9.3, 9.3, 11.5) · Rᵀ  with
#   R = [[0.8, 0.6, 0.0], [-0.6, 0.8, 0.0], [0.0, 0.0, 1.0]]
# (a ~36.87° in-plane rotation → off-diagonal xy-coupling). Also upstream
# (Phase B, not used here): μ_r = [0.99999975, 0.99999975, 0.99999979]
# (≈1) and LossTan = [3.0e-5, 3.0e-5, 8.6e-5]. See
# `mesh::transmon::SAPPHIRE_*` for the mirrored constants.
#
# Geometry defaults (blog optimization start, [4.14, 5.591] GHz):
#   cap_length = 620μm, cap_gap = 30μm, total_length = 5000μm,
#   cpw_width = 10μm, cpw_gap = 6μm, readout_length = 2700μm (wave=false),
#   substrate 4mm × 3.7mm.
#
# FORMAT: `single_transmon()` hardcodes `SolidModels.mesh_order(2)` at
# SingleTransmon.jl:178, INDEPENDENT of its own `mesh_order` kwarg (line
# 49, never read in the body). The base `GmshReader` is Tet4-only, so we
# force first order + MSH 4.1 ASCII at the gmsh-option level here (see
# `_force_first_order_msh41!`), overriding that hardcoded call, and save
# with a `.msh` (not `.msh2`) extension so `DeviceLayout.save()` → its
# `gmsh.write()` wrapper emits the most-recent (4.1) format directly.
# Record which route worked in the provenance file.

import Pkg
Pkg.activate(joinpath(@__DIR__))

using DeviceLayout, DeviceLayout.SchematicDrivenLayout, DeviceLayout.PreferredUnits

# Pull in the upstream example module verbatim from the installed
# DeviceLayout package (examples/SingleTransmon/SingleTransmon.jl) so we
# track the canonical `single_transmon()` builder exactly.
const DL_EXAMPLE = joinpath(
    dirname(dirname(pathof(DeviceLayout))),
    "examples",
    "SingleTransmon",
    "SingleTransmon.jl",
)
include(DL_EXAMPLE)

"""
    _force_first_order_msh41!()

Override the example's hardcoded `SolidModels.mesh_order(2)` so the mesh
is first-order (Tet4) and pin MSH 4.1 ASCII output. Called *after*
`single_transmon()` builds the SolidModel but the mesh generation itself
runs inside the example; we therefore set the gmsh options globally and
re-generate + write the mesh ourselves rather than via `save_mesh=true`.
"""
function _force_first_order_msh41!()
    SolidModels.set_gmsh_option("Mesh.MshFileVersion", 4.1)
    SolidModels.set_gmsh_option("Mesh.Binary", 0)      # ASCII
    SolidModels.mesh_order(1)                          # override the hardcoded (2)
end

"""
    generate(out_path; mesh_scale=1.0)

Build the single-transmon SolidModel, force first-order MSH 4.1 output,
run the 3D mesher, and write `out_path`. Prints the physical-group
element counts for the provenance file.
"""
function generate(out_path::AbstractString)
    sm = SingleTransmon.single_transmon(; wave_ports=false, save_mesh=false)

    _force_first_order_msh41!()
    SolidModels.gmsh.model.mesh.generate(3)

    # `DeviceLayout.save` wraps `gmsh.write(filename)`; a `.msh` extension
    # (not `.msh2`) yields the most-recent (4.1) Gmsh format.
    save(out_path, sm)

    attrs = SolidModels.attributes(sm)
    println("\n--- physical groups (name → gmsh attribute/tag) ---")
    for name in (
        "vacuum",
        "substrate",
        "metal",
        "exterior_boundary",
        "port_1",
        "port_2",
        "lumped_element",
    )
        println("  $(rpad(name, 18)) → attribute ", get(attrs, name, "MISSING"))
    end
    println("\nOK — wrote $out_path")
    println("Update the .provenance.txt and transmon_mesh.rs counts from",
            " the gmsh element-count report above.")
    return sm
end

if abspath(PROGRAM_FILE) == @__FILE__
    if length(ARGS) != 1
        println(stderr, "usage: julia --project=reference/julia ",
                "generate_transmon_fixture.jl <output.msh>")
        exit(2)
    end
    generate(ARGS[1])
end
