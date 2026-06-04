#!/usr/bin/env julia
"""
Generate `reference/fixtures/cube_cavity/julia_baseline.json` from the
Julia cube-cavity pipeline (Epic #88 / Phase E, issue #115).

The fixture has the same shape as `jax_baseline.json` (eigenvalues +
trace diagnostics, no eigenvectors — the subspace-overlap check is
anchored to the NumPy canonical `baseline.json` per Epic #88; Julia
agreement on eigenvalues is the headline cross-IR signal), extended
with the cross-platform f64 sub-stage fields `k_int_frobenius`,
`m_int_frobenius`, `k_int_diag`, `m_int_diag` that AC#3 of issue #115
pins against `baseline.json` to the 1e-9 / 5e-9 cross-platform floor
calibrated by PR #113 (issue #110).

Usage
=====

    julia --project=. gen_cube_cavity_baseline.jl \\
        [--n 10] [--side 1.0] [--k 5] \\
        [--mesh ../fixtures/cube_cavity/unit_cube.msh] \\
        [--out  ../fixtures/cube_cavity/julia_baseline.json]

Reproduction is deterministic on a pinned `Arpack.jl` (see
`Project.toml`). Re-runs against a different `Arpack.jl` version may
produce eigenvectors differing by orthogonal rotation within degenerate
clusters; the **eigenvalues** are stable, and the eigenvectors are not
shipped in this fixture.
"""

push!(LOAD_PATH, @__DIR__)
include(joinpath(@__DIR__, "cube_cavity.jl"))

using JSON3
using Dates
using Printf


function _parse_args(argv::Vector{String})
    n = 10
    k = 5
    side = 1.0
    mesh_path::Union{Nothing,String} = nothing
    out_path::String = joinpath(
        @__DIR__, "..", "fixtures", "cube_cavity", "julia_baseline.json"
    )
    i = 1
    while i <= length(argv)
        arg = argv[i]
        if arg == "--n"
            n = parse(Int, argv[i + 1]); i += 2
        elseif arg == "--k"
            k = parse(Int, argv[i + 1]); i += 2
        elseif arg == "--side"
            side = parse(Float64, argv[i + 1]); i += 2
        elseif arg == "--mesh"
            mesh_path = argv[i + 1]; i += 2
        elseif arg == "--out"
            out_path = argv[i + 1]; i += 2
        elseif arg in ("-h", "--help")
            println(stderr, "Usage: julia --project=. gen_cube_cavity_baseline.jl [options]")
            exit(0)
        else
            error("unrecognized argument: $arg")
        end
    end
    return (n=n, k=k, side=side, mesh=mesh_path, out=out_path)
end


function main()
    args = _parse_args(ARGS)
    @info "Generating Julia baseline fixture" n=args.n k=args.k side=args.side mesh=args.mesh out=args.out

    # Default to loading the canonical mesh fixture if it exists and no
    # explicit `--mesh` was supplied — matches the NumPy baseline
    # generator's behavior.
    mesh_path = args.mesh
    if mesh_path === nothing
        default_mesh = joinpath(
            @__DIR__, "..", "fixtures", "cube_cavity", "unit_cube.msh"
        )
        if isfile(default_mesh)
            mesh_path = default_mesh
            @info "Using canonical mesh fixture" mesh_path
        end
    end

    result = run_cube_cavity(
        n=args.n, k=args.k, side=args.side, mesh_path=mesh_path
    )

    # Sanity check: lowest 5 eigenvalues land in the analytic O(h²) band.
    analytic = analytic_lowest_five()
    rel_err = abs.(result.eigenvalues[1:length(analytic)] .- analytic) ./ analytic
    if any(rel_err .> 0.12)
        worst = argmax(rel_err)
        error("Julia eigenvalue $(worst) drifted off analytic target: " *
              "got $(result.eigenvalues[worst]), want $(analytic[worst]), " *
              "rel_err $(rel_err[worst]) > 0.12")
    end

    fixture = Dict{String,Any}(
        "schema_version" => "1",
        "fixture_id" => "cube_cavity/n$(args.n)_julia_first_$(args.k)_modes",
        "description" =>
            "Lowest $(args.k) scalar Helmholtz eigenvalues of the unit cube " *
            "with homogeneous Dirichlet BC, computed by reference/julia/" *
            "cube_cavity.jl via Arpack.jl on the n=$(args.n) tet-split unit " *
            "cube ($(result.n_int) interior DOFs). Cross-IR companion to " *
            "baseline.json (NumPy canonical) and jax_baseline.json (XLA " *
            "trace anchor). Arpack.jl binds the same libarpack as " *
            "scipy.sparse.linalg.eigsh, so eigenvalues agree with the NumPy " *
            "canonical to ~1e-13 relative at fixture-gen time. " *
            "Per issue #115 Acceptance Criterion 3, the sub-stage Frobenius " *
            "and diagonal fields pin Julia's f64 path against the same " *
            "cross-platform tolerance floor that PR #113 calibrated " *
            "(1e-9 / 5e-9 absolute).",
        "units" => "dimensionless (unit cube, P1 Laplacian; eigenvalues in units of 1/length²)",
        "inputs" => Dict{String,Any}(
            "n_per_side" => Dict{String,Any}(
                "shape" => [1],
                "dtype" => "i64",
                "description" => "Hexes per cube side (mesh refinement level).",
                "data" => [args.n],
            ),
            "side" => Dict{String,Any}(
                "shape" => [1],
                "dtype" => "f64",
                "description" => "Cube edge length.",
                "data" => [args.side],
            ),
        ),
        "outputs" => Dict{String,Any}(
            "eigenvalues" => Dict{String,Any}(
                "shape" => [args.k],
                "dtype" => "f64",
                "description" =>
                    "Lowest $(args.k) generalized eigenvalues of K x = λ M x on " *
                    "the interior P1 DOFs (Dirichlet eliminated). Ascending order. " *
                    "Acceptance criterion #2: 1e-5 relative agreement with the " *
                    "NumPy baseline.json and (when meshes match) JAX " *
                    "jax_baseline.json. Tolerance below stored as absolute " *
                    "(1e-4 ≈ 3.3e-6 relative at λ_min ≈ 3π²).",
                "tolerance_abs" => 1.0e-4,
                "data" => collect(result.eigenvalues[1:args.k]),
            ),
            "k_int_frobenius" => Dict{String,Any}(
                "shape" => [1],
                "dtype" => "f64",
                "description" =>
                    "Frobenius norm of K_int. Acceptance criterion #3 sub-stage " *
                    "diagnostic — pins assembly agreement before the eigensolve " *
                    "runs. Tolerance is the 1e-9 cross-platform floor from PR #113.",
                "tolerance_abs" => 1.0e-9,
                "data" => [result.k_int_frobenius],
            ),
            "m_int_frobenius" => Dict{String,Any}(
                "shape" => [1],
                "dtype" => "f64",
                "description" =>
                    "Frobenius norm of M_int. Companion to k_int_frobenius. " *
                    "Tolerance is the 1e-8 cross-platform floor from PR #113 " *
                    "(M entries scale as h^3, so the absolute floor relaxes one " *
                    "order vs K).",
                "tolerance_abs" => 1.0e-8,
                "data" => [result.m_int_frobenius],
            ),
            "k_int_diag" => Dict{String,Any}(
                "shape" => [result.n_int],
                "dtype" => "f64",
                "description" =>
                    "Diagonal of K_int. Per-row sub-stage diagnostic — if " *
                    "assembly disagrees anywhere, at least one diagonal entry " *
                    "surfaces it. Tolerance is the 1e-9 cross-platform floor " *
                    "from PR #113.",
                "tolerance_abs" => 1.0e-9,
                "data" => result.k_int_diag,
            ),
            "m_int_diag" => Dict{String,Any}(
                "shape" => [result.n_int],
                "dtype" => "f64",
                "description" =>
                    "Diagonal of M_int. Per-row sub-stage diagnostic. Tolerance " *
                    "is the 5e-9 cross-platform floor from PR #113.",
                "tolerance_abs" => 5.0e-9,
                "data" => result.m_int_diag,
            ),
            "n_int" => Dict{String,Any}(
                "shape" => [1],
                "dtype" => "f64",
                "description" =>
                    "Number of interior (non-Dirichlet) DOFs. Stored as f64 " *
                    "because schema v1 doesn't have an integer output kind; " *
                    "compared with strict equality (tolerance < 1).",
                "tolerance_abs" => 0.5,
                "data" => [Float64(result.n_int)],
            ),
            "analytic_eigenvalues" => Dict{String,Any}(
                "shape" => [length(analytic)],
                "dtype" => "f64",
                "description" =>
                    "Analytic Dirichlet Laplacian eigenvalues on the unit cube: " *
                    "{3, 6, 6, 6, 9}·π². Julia reproduces these to the same " *
                    "O(h²) discretization band that NumPy hits; tolerance below " *
                    "is set to 12% of the largest analytic eigenvalue (9π²).",
                "tolerance_abs" => 0.12 * 9.0 * pi * pi,
                "data" => analytic,
            ),
        ),
        "provenance" => Dict{String,Any}(
            "source" =>
                "reference/julia/cube_cavity.jl @ Epic #88 / #115 ; " *
                "julia $(VERSION)",
            "verified_against" =>
                "reference/numpy/cube_cavity.py — eigenvalues agree to ~1e-13 " *
                "relative because Arpack.jl and scipy.sparse.linalg.eigsh both " *
                "bind libarpack (note: the calling convention differs — Arpack.jl " *
                "uses regular-inverse mode via `which=:SM` with no `sigma`, " *
                "while scipy uses shift-invert via `sigma=0` + `which=\"LM\"`; " *
                "see reference/julia/cube_cavity.jl::eigensolve_arpack for the " *
                "Arpack.jl 0.5 friction artifact)",
            "issue" => "#115 (Phase E — Julia cube-cavity Helmholtz reference)",
            "regenerated_at" => string(now()),
        ),
    )

    open(args.out, "w") do io
        JSON3.pretty(io, fixture)
        write(io, "\n")
    end

    @info "Wrote Julia baseline fixture" path=args.out n_int=result.n_int
    println()
    println("Eigenvalues (lowest $(args.k)) vs analytic targets:")
    π² = pi * pi
    println("idx  target/π²   λ_j/π²   rel err")
    println("---  ---------   ------   --------")
    for i in 1:args.k
        got = result.eigenvalues[i]
        want = analytic[i]
        rel = abs(got - want) / want * 100.0
        @printf("%-3d  %.4f      %.4f   %+.4f%%\n", i - 1, want / π², got / π², rel)
    end
end


if abspath(PROGRAM_FILE) == @__FILE__
    main()
end
