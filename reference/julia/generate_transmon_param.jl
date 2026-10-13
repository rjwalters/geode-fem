# Parametrized DeviceLayout.jl SingleTransmon mesh generator (issue #1092).
#
# `generate_transmon_fixture.jl` drives the upstream example with its
# defaults (capacitor_length 620 um, resonator total_length 5000 um). This
# script exposes the two parameters the AWS DeviceLayout.jl blog optimizes,
# so the published initial and final designs, and any optimizer iterate,
# can be meshed with the same tool:
#
#   julia --project=reference/julia/devicelayout \
#       reference/julia/generate_transmon_param.jl \
#       --cap-length 620 --total-length 5000 \
#       --out-dir /path/to/work --tag initial \
#       [--order1-msh41] [--order2-msh2] [--dl-config] [--mesh-scale 1.0]
#
# Outputs (in --out-dir, all named <tag>_*):
#   <tag>_o1.msh          first-order Tet4 mesh, MSH 4.1 ASCII, the format
#                         geode's `mesh::transmon` adapter reads (the same
#                         route as the committed fixture: the example's
#                         hardcoded `mesh_order(2)` is overridden to 1).
#   <tag>_o2.msh2         DeviceLayout's own second-order mesh in MSH 2.2
#                         (what the example's `save_mesh=true` writes), for
#                         a Palace run at DeviceLayout's discretization
#                         (mesh_order 2, solver_order 2).
#   <tag>_dl_config.json  the example's `configfile(sm; solver_order=2)`
#                         (absorbing far field, two 50 ohm readout ports, the
#                         junction L || C), with Model.Mesh and
#                         Problem.Output rewritten to /work paths for the
#                         docker run. The physical-group attribute numbers
#                         are those gmsh assigned to <tag>_o2.msh2.
#   <tag>_report.txt      versions, parameters and the gmsh element counts
#                         per physical group, for the provenance file.
#
# The project environment `reference/julia/devicelayout/` pins
# DeviceLayout v1.15.0 (the version that produced the committed fixture),
# gmsh_jll 4.15.2 and the example's dependencies (CSV, DataFrames, JSON,
# JSONSchema, FileIO, PRIMA). Julia and gmsh are not geode-fem CI
# dependencies: meshes are generated offline and committed or regenerated.

import Pkg
Pkg.activate(joinpath(@__DIR__, "devicelayout"))

using DeviceLayout, DeviceLayout.SchematicDrivenLayout, DeviceLayout.PreferredUnits
import JSON

const DL_EXAMPLE = joinpath(
    dirname(dirname(pathof(DeviceLayout))),
    "examples",
    "SingleTransmon",
    "SingleTransmon.jl",
)
include(DL_EXAMPLE)

const GROUPS = (
    "vacuum",
    "substrate",
    "metal",
    "exterior_boundary",
    "port_1",
    "port_2",
    "lumped_element",
)

function parse_args(args)
    opts = Dict{String,Any}(
        "cap-length" => 620.0,
        "total-length" => 5000.0,
        "out-dir" => ".",
        "tag" => "initial",
        "mesh-scale" => 1.0,
        "order1-msh41" => false,
        "order2-msh2" => false,
        "dl-config" => false,
    )
    i = 1
    while i <= length(args)
        a = args[i]
        if a in ("--order1-msh41", "--order2-msh2", "--dl-config")
            opts[a[3:end]] = true
            i += 1
        elseif startswith(a, "--") && i < length(args)
            key = a[3:end]
            haskey(opts, key) || error("unknown option $a")
            v = args[i + 1]
            opts[key] = opts[key] isa Float64 ? parse(Float64, v) : v
            i += 2
        else
            error("bad argument $a")
        end
    end
    if !(opts["order1-msh41"] || opts["order2-msh2"] || opts["dl-config"])
        opts["order1-msh41"] = true
    end
    opts["dl-config"] && (opts["order2-msh2"] = true)
    return opts
end

"""
    element_counts()

Per-physical-group element counts of the current gmsh model, as
`name => (dim, tag, count)`.
"""
function element_counts()
    gm = SolidModels.gmsh.model
    out = Dict{String,Tuple{Int,Int,Int}}()
    for (dim, tag) in gm.getPhysicalGroups()
        name = gm.getPhysicalName(dim, tag)
        n = 0
        for ent in gm.getEntitiesForPhysicalGroup(dim, tag)
            _, etags, _ = gm.mesh.getElements(dim, ent)
            n += sum(length.(etags); init = 0)
        end
        out[name] = (dim, tag, n)
    end
    return out
end

function node_count()
    ntags, _, _ = SolidModels.gmsh.model.mesh.getNodes()
    return length(ntags)
end

"""
    build(cap_um, total_um, mesh_scale)

One fresh SolidModel of the example at the given parameters (the example
hardcodes `mesh_order(2)`; the order is set again before meshing).
"""
function build(cap_um, total_um, mesh_scale)
    sm = SingleTransmon.single_transmon(;
        wave_ports = false,
        save_mesh = false,
        cap_length = cap_um * μm,
        total_length = total_um * μm,
    )
    mesh_scale == 1.0 || SolidModels.mesh_scale(mesh_scale)
    GMSH_VERSION[] = SolidModels.gmsh.option.getString("General.Version")
    return sm
end

const GMSH_VERSION = Ref("")

function write_counts(io, counts)
    for name in GROUPS
        if haskey(counts, name)
            dim, tag, n = counts[name]
            println(io, "  ($dim, $tag) $(rpad(name, 18)) $n elements")
        else
            println(io, "  (?, ?) $(rpad(name, 18)) MISSING")
        end
    end
end

function main(args)
    o = parse_args(args)
    cap, total, scale = o["cap-length"], o["total-length"], o["mesh-scale"]
    outdir, tag = o["out-dir"], o["tag"]
    mkpath(outdir)
    report = IOBuffer()
    println(report, "generate_transmon_param.jl report (issue #1092)")
    println(report, "DeviceLayout:   ", pkgversion(DeviceLayout))
    println(report, "julia:          ", VERSION)
    println(report, "cap_length:     $cap um")
    println(report, "total_length:   $total um")
    println(report, "mesh_scale:     $scale")
    println(report, "example:        ", DL_EXAMPLE)

    if o["order1-msh41"]
        t = @elapsed begin
            sm = build(cap, total, scale)
            SolidModels.set_gmsh_option("Mesh.MshFileVersion", 4.1)
            SolidModels.set_gmsh_option("Mesh.Binary", 0)
            SolidModels.mesh_order(1)
            SolidModels.gmsh.model.mesh.generate(3)
            path = joinpath(outdir, "$(tag)_o1.msh")
            DeviceLayout.save(path, sm)
        end
        println(report, "\norder-1 MSH 4.1: $(tag)_o1.msh ($(round(t, digits = 1)) s)")
        println(report, "  nodes: ", node_count())
        write_counts(report, element_counts())
    end

    if o["order2-msh2"]
        t = @elapsed begin
            sm = build(cap, total, scale)
            SolidModels.mesh_order(2)
            SolidModels.gmsh.model.mesh.generate(3)
            path = joinpath(outdir, "$(tag)_o2.msh2")
            DeviceLayout.save(path, sm)
        end
        println(report, "\norder-2 MSH 2.2: $(tag)_o2.msh2 ($(round(t, digits = 1)) s)")
        println(report, "  nodes: ", node_count())
        write_counts(report, element_counts())
        if o["dl-config"]
            cfg = SingleTransmon.configfile(sm; solver_order = 2)
            cfg["Model"]["Mesh"] = "/work/$(tag)_o2.msh2"
            cfg["Problem"]["Output"] = "/work/postpro/dl_$(tag)"
            cfg["Solver"]["Eigenmode"]["Save"] = 0   # no ParaView output
            cpath = joinpath(outdir, "$(tag)_dl_config.json")
            open(cpath, "w") do f
                JSON.print(f, cfg, 2)
            end
            println(report, "  DeviceLayout config: $(tag)_dl_config.json (solver_order 2)")
        end
    end

    println(report, "\ngmsh (gmsh_jll): ", GMSH_VERSION[])
    rpt = String(take!(report))
    print(rpt)
    open(joinpath(outdir, "$(tag)_report.txt"), "w") do f
        write(f, rpt)
    end
end

if abspath(PROGRAM_FILE) == @__FILE__
    main(ARGS)
end
