#!/usr/bin/env julia
"""
Generate `reference/fixtures/sphere_pec/julia_baseline.json` from the Julia
sphere-PEC Nédélec pipeline (Epic #88 / Phase G.4, issue #129).

The fixture carries the same schema (v1) and sub-stage fields as
`reference/fixtures/sphere_pec/baseline.json` (the NumPy canonical baseline),
so the Rust harness `crates/geode-validation/tests/sphere_pec_julia_reference.rs`
can load it with the same `Fixture` helper and compare Burn output against it.

At generation time the script cross-checks the Julia eigenvalues against the
NumPy `baseline.json` (loaded from the same fixture directory) at the 1e-5
relative tolerance that Epic #88 defines for cross-language f64 agreement.

Usage:
    julia --project=reference/julia reference/julia/gen_sphere_pec_baseline.jl \\
        [--mesh reference/fixtures/sphere_pec/sphere.msh] \\
        [--out  reference/fixtures/sphere_pec/julia_baseline.json]
"""

push!(LOAD_PATH, @__DIR__)
include(joinpath(@__DIR__, "sphere_pec.jl"))

using JSON3
using Dates
using Printf


function _parse_args(argv::Vector{String})
    mesh_path = joinpath(@__DIR__, "..", "fixtures", "sphere_pec", "sphere.msh")
    out_path  = joinpath(@__DIR__, "..", "fixtures", "sphere_pec", "julia_baseline.json")
    i = 1
    while i <= length(argv)
        arg = argv[i]
        if arg == "--mesh"
            mesh_path = argv[i + 1]; i += 2
        elseif arg == "--out"
            out_path = argv[i + 1]; i += 2
        elseif arg in ("-h", "--help")
            println(stderr, "Usage: julia --project=. gen_sphere_pec_baseline.jl [--mesh ...] [--out ...]")
            exit(0)
        else
            error("unrecognized argument: $arg")
        end
    end
    return (mesh=mesh_path, out=out_path)
end


function main()
    args = _parse_args(ARGS)
    @info "Generating Julia sphere-PEC baseline fixture" mesh=args.mesh out=args.out

    # Run the full pipeline.
    result = run_sphere_pec(args.mesh)

    # Sanity check: integer field agreement.
    @assert result.n_nodes         == 774  "n_nodes mismatch: got $(result.n_nodes), want 774"
    @assert result.n_tets          == 3335 "n_tets mismatch: got $(result.n_tets), want 3335"
    @assert result.n_interior_edges == 3300 "n_interior_edges: got $(result.n_interior_edges), want 3300"
    @assert result.n_spurious      == 368  "n_spurious: got $(result.n_spurious), want 368"

    # Cross-check physical eigenvalues against NumPy baseline.json.
    numpy_phys = [1.4195415502066517, 1.4204339541482647, 1.4206625078898854,
                  3.2718741181859423, 3.277498156786518]
    for (i, (got, want)) in enumerate(zip(result.physical_eigenvalues, numpy_phys))
        rel_err = abs(got - want) / abs(want)
        if rel_err > 1e-5
            error("Physical eigenvalue $i exceeds 1e-5 relative tolerance: " *
                  "got $got, want $want, rel_err $(rel_err)")
        end
    end
    @info "Physical eigenvalue cross-check passed (all within 1e-5 rel vs NumPy)"

    # Compute best_gap diagnostic: λ[n_spurious] / λ[n_spurious - 1].
    n_sp = result.n_spurious
    best_gap = if n_sp >= 1 && n_sp < length(result.eigenvalues_lowest)
        a = abs(result.eigenvalues_lowest[n_sp])
        b = abs(result.eigenvalues_lowest[n_sp + 1])
        a > 0.0 ? b / a : Inf
    else
        NaN
    end

    n_int = result.n_interior_edges

    fixture = Dict{String,Any}(
        "schema_version" => "1",
        "fixture_id"     => "sphere_pec/n774_pec_eigenmode_julia",
        "description"    =>
            "Vector-Nédélec sphere-PEC eigenmode pipeline (issue #129, Epic #88 Phase G.4). " *
            "Julia reference backend for cross-IR agreement against the NumPy canonical " *
            "baseline.json. Assembly uses the same cofactor Gram matrix formula as " *
            "reference/numpy/nedelec_local_matrices.py and crates/geode-core/src/nedelec.rs. " *
            "Eigensolve via dense LinearAlgebra.eigen (n≤5000 path, Issue #133 fix) — " *
            "consistent with Rust faer generalized_eigen path. " *
            "Cross-checked against NumPy baseline.json at 1e-5 relative tolerance " *
            "for physical eigenvalues at generation time.",
        "units"  => "λ = k² (inverse-length squared); dimensionless mesh coordinates",
        "inputs" => Dict{String,Any}(
            "mesh_path" => Dict{String,Any}(
                "shape"       => [0],
                "dtype"       => "f64",
                "description" => "Mesh fixture: reference/fixtures/sphere_pec/sphere.msh " *
                                 "(774 nodes, 3335 tets).",
                "data"        => [],
            ),
            "n_index" => Dict{String,Any}(
                "shape"       => [1],
                "dtype"       => "f64",
                "description" => "Refractive index inside sphere_interior; ε_r = n² = 2.25.",
                "data"        => [N_INDEX],
            ),
            "r_buffer" => Dict{String,Any}(
                "shape"       => [1],
                "dtype"       => "f64",
                "description" => "Outer PEC wall radius.",
                "data"        => [R_BUFFER],
            ),
        ),
        "outputs" => Dict{String,Any}(
            "n_nodes" => Dict{String,Any}(
                "shape"         => [1],
                "dtype"         => "f64",
                "description"   => "Number of mesh nodes. Strict equality (tolerance < 1).",
                "tolerance_abs" => 0.5,
                "data"          => [Float64(result.n_nodes)],
            ),
            "n_tets" => Dict{String,Any}(
                "shape"         => [1],
                "dtype"         => "f64",
                "description"   => "Number of mesh tets. Strict equality.",
                "tolerance_abs" => 0.5,
                "data"          => [Float64(result.n_tets)],
            ),
            "n_edges" => Dict{String,Any}(
                "shape"         => [1],
                "dtype"         => "f64",
                "description"   => "Number of global edges. Note: Julia uses first-seen " *
                                   "ordering; the global edge count matches NumPy (4512) " *
                                   "but individual edge indices differ.",
                "tolerance_abs" => 0.5,
                "data"          => [Float64(result.n_edges)],
            ),
            "n_interior_edges" => Dict{String,Any}(
                "shape"         => [1],
                "dtype"         => "f64",
                "description"   => "Number of interior edges (DOFs after PEC elimination). " *
                                   "Strict equality.",
                "tolerance_abs" => 0.5,
                "data"          => [Float64(result.n_interior_edges)],
            ),
            "spurious_dim" => Dict{String,Any}(
                "shape"         => [1],
                "dtype"         => "f64",
                "description"   => "Predicted spurious-mode count = number of interior " *
                                   "nodes. Strict equality.",
                "tolerance_abs" => 0.5,
                "data"          => [Float64(result.spurious_dim)],
            ),
            "n_spurious_observed" => Dict{String,Any}(
                "shape"         => [1],
                "dtype"         => "f64",
                "description"   => "Algebraic spurious-mode count = rank(d⁰_interior) " *
                                   "via SVD with 1e-12×σ_max cutoff (Issue #124). " *
                                   "Should equal spurious_dim = 368 on this fixture.",
                "tolerance_abs" => 0.5,
                "data"          => [Float64(result.n_spurious)],
            ),
            "best_gap" => Dict{String,Any}(
                "shape"         => [1],
                "dtype"         => "f64",
                "description"   => "Diagnostic ratio λ[n_spurious] / λ[n_spurious-1] " *
                                   "(spurious→physical transition). Large value (~1e12) " *
                                   "confirms clean spurious/physical separation.",
                "tolerance_abs" => 1e10,
                "data"          => [isnan(best_gap) ? 0.0 : best_gap],
            ),
            "k_int_frobenius" => Dict{String,Any}(
                "shape"         => [1],
                "dtype"         => "f64",
                "description"   => "Frobenius norm of K_int (interior curl-curl stiffness). " *
                                   "Cross-platform f64 floor ≈ 1e-8 relative vs NumPy.",
                "tolerance_abs" => 1e-4,
                "data"          => [result.k_int_frobenius],
            ),
            "m_int_frobenius" => Dict{String,Any}(
                "shape"         => [1],
                "dtype"         => "f64",
                "description"   => "Frobenius norm of M_int (interior ε-scaled mass). " *
                                   "Cross-platform f64 floor ≈ 1e-8 relative vs NumPy.",
                "tolerance_abs" => 1e-5,
                "data"          => [result.m_int_frobenius],
            ),
            "k_int_diag" => Dict{String,Any}(
                "shape"         => [n_int],
                "dtype"         => "f64",
                "description"   => "Diagonal of K_int. Per-DOF sub-stage diagnostic.",
                "tolerance_abs" => 1e-6,
                "data"          => result.k_int_diag,
            ),
            "m_int_diag" => Dict{String,Any}(
                "shape"         => [n_int],
                "dtype"         => "f64",
                "description"   => "Diagonal of M_int. Per-DOF sub-stage diagnostic.",
                "tolerance_abs" => 1e-7,
                "data"          => result.m_int_diag,
            ),
            "eigenvalues_lowest" => Dict{String,Any}(
                "shape"         => [length(result.eigenvalues_lowest)],
                "dtype"         => "f64",
                "description"   => "Lowest spurious_dim+8=$(result.spurious_dim+8) eigenvalues " *
                                   "from Arpack.jl which=:SM (regular-inverse mode). Ascending " *
                                   "order; first $(result.n_spurious) are near-zero spurious modes.",
                "tolerance_abs" => 1e-4,
                "data"          => result.eigenvalues_lowest,
            ),
            "physical_eigenvalues" => Dict{String,Any}(
                "shape"         => [length(result.physical_eigenvalues)],
                "dtype"         => "f64",
                "description"   => "Lowest 5 physical eigenvalues after spurious filtering " *
                                   "(λ ≈ 1.42 triplet + λ ≈ 3.27 doublet). " *
                                   "Acceptance criterion: 1e-5 relative vs NumPy baseline.json.",
                "tolerance_abs" => 1e-5,
                "data"          => result.physical_eigenvalues,
            ),
        ),
        "provenance" => Dict{String,Any}(
            "source"           => "reference/julia/sphere_pec.jl @ Epic #88 / #129 (Phase G.4)",
            "julia_version"    => string(VERSION),
            "verified_against" =>
                "reference/fixtures/sphere_pec/baseline.json — physical eigenvalues " *
                "agree to < 1e-5 relative (cross-IR f64 floor per Epic #88). " *
                "Dense LinearAlgebra.eigen path (Issue #133 fix) — consistent with " *
                "Rust faer generalized_eigen.",
            "issue"            => "#129 / #133 (Phase G.4 — Julia sphere-PEC Nédélec reference, dense eigensolve fix)",
            "regenerated_at"   => string(now()),
        ),
    )

    # Write fixture JSON.
    out_dir = dirname(args.out)
    isdir(out_dir) || mkpath(out_dir)
    open(args.out, "w") do io
        JSON3.pretty(io, fixture)
        write(io, "\n")
    end
    @info "Wrote Julia sphere-PEC baseline fixture" path=args.out

    println()
    println("Physical eigenvalues vs NumPy baseline:")
    println("idx  Julia λ          NumPy λ          rel err")
    println("---  ---------------  ---------------  --------")
    for (i, (got, want)) in enumerate(zip(result.physical_eigenvalues, numpy_phys))
        rel_err = abs(got - want) / abs(want)
        @printf("%-3d  %.10f  %.10f  %.2e\n", i, got, want, rel_err)
    end
    println()
    @printf("‖K_int‖_F = %.12e  (NumPy: 1.093850528510e+03)\n", result.k_int_frobenius)
    @printf("‖M_int‖_F = %.12e  (NumPy: 9.743795100368e+00)\n", result.m_int_frobenius)
    @printf("n_spurious (d⁰ rank) = %d  (expected 368)\n", result.n_spurious)
    @printf("best_gap = %.4e  (spurious→physical ratio)\n", best_gap)
end


if abspath(PROGRAM_FILE) == @__FILE__
    main()
end
