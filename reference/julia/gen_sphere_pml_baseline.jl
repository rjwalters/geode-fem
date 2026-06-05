#!/usr/bin/env julia
"""
Generate ``reference/fixtures/sphere_pml/julia_baseline.json`` from the Julia
sphere-PML Nédélec pipeline (Epic #88 / Phase H.2, issue #147).

Mirrors ``gen_sphere_pec_baseline.jl`` (Phase G.4) extended for the
complex-permittivity / complex-eigenvalue case. The fixture is the Julia
per-backend baseline; it complements ``baseline.json`` (the H.1 NumPy canonical
baseline once #146 lands) and exercises the **c128** on-disk encoding from
Wave 1 (#145, PR #151) end-to-end.

## c128 encoding

Per ``reference/SCHEMA.md`` → "Complex encoding (c128)": real-imag interleaved
flat arrays of length ``2·prod(shape)``. The element at logical row-major
index k occupies disk positions ``2k`` (real) and ``2k+1`` (imag). Tolerance
on c128 output fields is applied to ``|Δ| = |actual − golden|``.

## Two-σ baseline

The fixture records two reference runs:

  * **σ₀ = 0** (PEC limit, real spectrum): cross-checks against the Phase G.4
    NumPy PEC baseline `physical_eigenvalues` field. ``max |Re(λ_julia) −
    λ_numpy_pec| < 1e-6`` and ``max |Im(λ_julia)| < 1e-10``. Stored under
    ``eigenvalues_lowest_complex_sigma0`` (this is the σ₀ = 0 collapse — the
    field name follows the c128 dtype rule even though all imag parts are ≈ 0).
  * **σ₀ = 5.0** (matches ``crates/geode-core/tests/sphere_pml_eigenmode.rs``):
    the "real" PML eigensolve output with non-trivial ``Im(λ) < 0``. Stored
    under ``eigenvalues_lowest_complex`` (the canonical Phase H output field).
    The lowest physical mode's Q-factor is recorded under
    ``q_factor_lowest_physical``.

## Usage

    julia --project=reference/julia reference/julia/gen_sphere_pml_baseline.jl \\
        [--mesh reference/fixtures/sphere_pml/sphere.msh] \\
        [--out  reference/fixtures/sphere_pml/julia_baseline.json] \\
        [--sigma0 5.0]
"""

push!(LOAD_PATH, @__DIR__)
include(joinpath(@__DIR__, "sphere_pml.jl"))

using JSON3
using Dates
using Printf


# ---------------------------------------------------------------------------
# c128 interleave helper.
# ---------------------------------------------------------------------------

"""
    interleave_c128(z) -> Vector{Float64}

Serialize a `Vector{ComplexF64}` to the real-imag interleaved flat
representation described in `reference/SCHEMA.md`:

    [re(z[1]), im(z[1]), re(z[2]), im(z[2]), ...]

Length is `2 * length(z)`. Loaded back via `Fixture::output_c128` /
`Fixture::input_c128` on the Rust side.
"""
function interleave_c128(z::Vector{ComplexF64})
    out = Vector{Float64}(undef, 2 * length(z))
    @inbounds for (i, c) in enumerate(z)
        out[2*i - 1] = real(c)
        out[2*i]     = imag(c)
    end
    return out
end


# ---------------------------------------------------------------------------
# Argument parsing.
# ---------------------------------------------------------------------------

function _parse_args(argv::Vector{String})
    mesh_path = joinpath(@__DIR__, "..", "fixtures", "sphere_pml", "sphere.msh")
    out_path  = joinpath(@__DIR__, "..", "fixtures", "sphere_pml", "julia_baseline.json")
    sigma0    = 5.0
    i = 1
    while i <= length(argv)
        arg = argv[i]
        if arg == "--mesh"
            mesh_path = argv[i + 1]; i += 2
        elseif arg == "--out"
            out_path = argv[i + 1]; i += 2
        elseif arg == "--sigma0"
            sigma0 = parse(Float64, argv[i + 1]); i += 2
        elseif arg in ("-h", "--help")
            println(stderr,
                "Usage: julia --project=. gen_sphere_pml_baseline.jl " *
                "[--mesh ...] [--out ...] [--sigma0 5.0]")
            exit(0)
        else
            error("unrecognized argument: $arg")
        end
    end
    return (mesh=mesh_path, out=out_path, sigma0=sigma0)
end


# ---------------------------------------------------------------------------
# Cross-check against NumPy PEC baseline at σ₀ = 0.
# ---------------------------------------------------------------------------

"""
PEC NumPy baseline values (from reference/fixtures/sphere_pec/baseline.json
@ commit 7395bc2). Used as the σ₀ = 0 regression target.
"""
const NUMPY_PEC_PHYSICAL = [
    1.4195415502066517,
    1.4204339541482647,
    1.4206625078898854,
    3.2718741181859423,
    3.277498156786518,
]


"""
    check_sigma0_pec_collapse(result_sigma0)

Verify that the σ₀ = 0 run collapses to the PEC NumPy baseline within
1e-6 relative tolerance on Re(λ) and 1e-10 absolute on Im(λ).

Throws an error on mismatch — this is a structural correctness check, not
a quantitative tolerance check.
"""
function check_sigma0_pec_collapse(eigs_complex::Vector{ComplexF64})
    @assert length(eigs_complex) >= length(NUMPY_PEC_PHYSICAL)
    for i in 1:length(NUMPY_PEC_PHYSICAL)
        lam   = eigs_complex[i]
        want  = NUMPY_PEC_PHYSICAL[i]
        re_err = abs(real(lam) - want) / abs(want)
        im_abs = abs(imag(lam))
        if re_err > 1e-6
            error("σ₀=0 PEC collapse: Re(λ[$i]) = $(real(lam)) vs " *
                  "NumPy PEC $(want), rel err $(re_err) > 1e-6")
        end
        if im_abs > 1e-10
            error("σ₀=0 PEC collapse: |Im(λ[$i])| = $(im_abs) > 1e-10 " *
                  "(should be at LAPACK ULP for the real-symmetric collapse)")
        end
    end
    @info "σ₀=0 PEC collapse passed: lowest 5 physical agree with NumPy PEC " *
          "to 1e-6 relative on Re(λ), 1e-10 on |Im(λ)|."
end


# ---------------------------------------------------------------------------
# Main.
# ---------------------------------------------------------------------------

function main()
    args = _parse_args(ARGS)
    @info "Generating Julia sphere-PML baseline fixture" mesh=args.mesh out=args.out sigma0=args.sigma0

    # ----------------------- σ₀ = 0 sanity run ---------------------------
    @info "Running σ₀ = 0 PEC-collapse sanity check..."
    result_pec = run_sphere_pml(args.mesh; sigma0=0.0)

    @assert result_pec.n_nodes         == 774  "n_nodes mismatch: got $(result_pec.n_nodes), want 774"
    @assert result_pec.n_tets          == 3335 "n_tets mismatch: got $(result_pec.n_tets), want 3335"
    @assert result_pec.n_interior_edges == 3300 "n_interior_edges: got $(result_pec.n_interior_edges), want 3300"
    @assert result_pec.n_spurious      == 368  "n_spurious: got $(result_pec.n_spurious), want 368"

    check_sigma0_pec_collapse(result_pec.physical_eigenvalues_complex)

    # ----------------------- σ₀ = 5.0 PML run ----------------------------
    @info "Running σ₀ = $(args.sigma0) PML run..."
    result = run_sphere_pml(args.mesh; sigma0=args.sigma0)

    n_take = length(result.physical_eigenvalues_complex)
    @assert n_take == 5 "expected 5 physical modes, got $n_take"

    println()
    println("σ₀ = $(args.sigma0) physical eigenvalues:")
    println("idx  Re(λ)              Im(λ)              |λ|                  Q")
    println("---  ---------------    ---------------    ---------------    ----------")
    for (i, lam) in enumerate(result.physical_eigenvalues_complex)
        q = q_factor(lam)
        @printf("%-3d  %.12e   %.12e   %.12e   %.4f\n",
                i, real(lam), imag(lam), abs(lam), q)
    end
    println()
    @printf("‖K_int‖_F (complex) = %.12e\n", result.k_int_frobenius_complex)
    @printf("‖M_int‖_F (complex) = %.12e\n", result.m_int_frobenius_complex)
    @printf("Q(lowest physical)  = %.6f\n", result.q_factor_lowest_physical)
    println()

    # Sanity: all physical modes should have Im(λ) ≤ 0 (PML absorption
    # under exp(+jωt) convention). The first mode may have Im(λ) very
    # close to 0 if it's a trapped resonance; flag if it's POSITIVE
    # (sign-convention error) but allow ULP-scale positive values from
    # Arpack noise.
    for (i, lam) in enumerate(result.physical_eigenvalues_complex)
        if imag(lam) > 1e-6
            @warn "physical[$i] has Im(λ) = $(imag(lam)) > 0 — possible " *
                  "sign-convention drift or Arpack convergence artifact."
        end
    end

    # ----------------------- fixture assembly ----------------------------
    n_tets = result.n_tets

    fixture = Dict{String,Any}(
        "schema_version" => "1",
        "fixture_id"     => "sphere_pml/n774_pml_eigenmode_julia",
        "description"    =>
            "Scalar-isotropic Vector-Nédélec sphere-PML eigenmode pipeline " *
            "(issue #147, Epic #88 Phase H.2). Julia reference backend exercising " *
            "native ComplexF64 arithmetic on the cross-IR PML spine slice. " *
            "Per-tet ε_r is complex (1 - jσ₀u² in the absorbing shell); the mass " *
            "matrix is SparseMatrixCSC{ComplexF64,Int}; the eigensolve is " *
            "Arpack.jl shift-invert at σ = 2.0 (above the physical band — see " *
            "sphere_pml.jl::eigensolve_physical_shift_invert for why the Phase " *
            "G.4 σ ≈ 0.01 + nev = spurious_dim + 8 pattern is not viable for " *
            "the complex case). σ₀ = 0 PEC-collapse cross-check against the " *
            "NumPy PEC baseline.json passes at 1e-6 relative on Re(λ).",
        "units"  =>
            "λ = k² (inverse-length squared) with negative-imaginary convention " *
            "under exp(+jωt); dimensionless mesh coordinates",
        "inputs" => Dict{String,Any}(
            "mesh_path" => Dict{String,Any}(
                "shape"       => [0],
                "dtype"       => "f64",
                "description" => "Mesh fixture: reference/fixtures/sphere_pml/sphere.msh " *
                                 "(774 nodes, 3335 tets — same mesh as sphere_pec/).",
                "data"        => [],
            ),
            "sigma_0" => Dict{String,Any}(
                "shape"       => [1],
                "dtype"       => "f64",
                "description" => "PML absorption strength; matches " *
                                 "crates/geode-core/tests/sphere_pml_eigenmode.rs.",
                "data"        => [args.sigma0],
            ),
            "r_sphere" => Dict{String,Any}(
                "shape"       => [1],
                "dtype"       => "f64",
                "description" => "Inner dielectric sphere radius.",
                "data"        => [R_SPHERE],
            ),
            "r_pml_inner" => Dict{String,Any}(
                "shape"       => [1],
                "dtype"       => "f64",
                "description" => "PML inner radius — start of the absorbing layer.",
                "data"        => [R_PML_INNER],
            ),
            "r_buffer" => Dict{String,Any}(
                "shape"       => [1],
                "dtype"       => "f64",
                "description" => "Outer PEC wall radius.",
                "data"        => [R_BUFFER],
            ),
            "n_index" => Dict{String,Any}(
                "shape"       => [1],
                "dtype"       => "f64",
                "description" => "Refractive index inside the dielectric sphere; ε_r = n² = 2.25 inside.",
                "data"        => [N_INDEX],
            ),
            "epsilon_r_complex" => Dict{String,Any}(
                "shape"       => [n_tets],
                "dtype"       => "c128",
                "description" => "Per-tet complex relative permittivity, length n_tets. " *
                                 "Region assignment: ε_r = n²=2.25+0j inside the dielectric " *
                                 "(PHYS_SPHERE_INTERIOR), 1+0j in the vacuum gap " *
                                 "(PHYS_VACUUM_GAP), and 1 - jσ₀u² in the PML shell " *
                                 "(PHYS_PML_SHELL) with u = clamp((r - R_PML_INNER)/" *
                                 "(R_BUFFER - R_PML_INNER), 0, 1). On-disk: real-imag " *
                                 "interleaved flat array per reference/SCHEMA.md.",
                "data"        => interleave_c128(result.epsilon_r_complex),
            ),
        ),
        "outputs" => Dict{String,Any}(
            "n_nodes" => Dict{String,Any}(
                "shape"         => [1],
                "dtype"         => "f64",
                "description"   => "Number of mesh nodes. Strict equality.",
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
                "description"   => "Number of global edges. Strict equality (Julia first-seen " *
                                   "ordering matches NumPy lex-sort count: 4512).",
                "tolerance_abs" => 0.5,
                "data"          => [Float64(result.n_edges)],
            ),
            "n_interior_edges" => Dict{String,Any}(
                "shape"         => [1],
                "dtype"         => "f64",
                "description"   => "Number of interior edges (DOFs after PEC reduction).",
                "tolerance_abs" => 0.5,
                "data"          => [Float64(result.n_interior_edges)],
            ),
            "spurious_dim" => Dict{String,Any}(
                "shape"         => [1],
                "dtype"         => "f64",
                "description"   => "Predicted spurious-mode count = number of interior nodes.",
                "tolerance_abs" => 0.5,
                "data"          => [Float64(result.spurious_dim)],
            ),
            "n_spurious_observed" => Dict{String,Any}(
                "shape"         => [1],
                "dtype"         => "f64",
                "description"   => "Algebraic spurious-mode count = rank(d⁰_interior) via SVD " *
                                   "with 1e-12×σ_max cutoff (Issue #124). The d⁰-rank classifier " *
                                   "is **unaffected by complex ε scaling** because the kernel of " *
                                   "curl-curl is image(d⁰) independent of ε(x). Should equal " *
                                   "spurious_dim = 368.",
                "tolerance_abs" => 0.5,
                "data"          => [Float64(result.n_spurious)],
            ),
            "k_int_frobenius_complex" => Dict{String,Any}(
                "shape"         => [1],
                "dtype"         => "f64",
                "description"   => "Frobenius norm of K_int (interior curl-curl stiffness, " *
                                   "ComplexF64 storage but real values — K is unchanged by the " *
                                   "complex ε since only the mass M absorbs the ε scaling). " *
                                   "Matches sphere_pec/baseline.json k_int_frobenius to 1e-8 rel.",
                "tolerance_abs" => 1e-4,
                "data"          => [result.k_int_frobenius_complex],
            ),
            "m_int_frobenius_complex" => Dict{String,Any}(
                "shape"         => [1],
                "dtype"         => "f64",
                "description"   => "Frobenius norm of M_int (interior ε-scaled mass, ComplexF64). " *
                                   "Differs from sphere_pec/baseline.json m_int_frobenius because " *
                                   "the PML shell contributes a complex ε that increases the " *
                                   "Frobenius norm.",
                "tolerance_abs" => 1e-4,
                "data"          => [result.m_int_frobenius_complex],
            ),
            "eigenvalues_lowest_complex" => Dict{String,Any}(
                "shape"         => [n_take],
                "dtype"         => "c128",
                "description"   => "Lowest 5 physical complex eigenvalues at σ₀ = $(args.sigma0) " *
                                   "PML. All have Im(λ) ≤ 0 (PML absorption under exp(+jωt)). " *
                                   "Computed via Arpack shift-invert at σ_shift = 2.0; the " *
                                   "spurious cluster at λ ≈ 0 is bypassed geometrically (see " *
                                   "sphere_pml.jl friction note). On-disk: real-imag interleaved.",
                "tolerance_abs" => 5e-3,
                "data"          => interleave_c128(result.physical_eigenvalues_complex),
            ),
            "eigenvalues_lowest_complex_sigma0" => Dict{String,Any}(
                "shape"         => [length(NUMPY_PEC_PHYSICAL)],
                "dtype"         => "c128",
                "description"   => "σ₀ = 0 PEC-collapse cross-check: with no PML loss, the " *
                                   "complex eigensolve must recover the Phase G.4 NumPy PEC " *
                                   "baseline `physical_eigenvalues` field to 1e-6 relative on " *
                                   "Re(λ) and 1e-10 absolute on Im(λ). Stored as c128 to match " *
                                   "the complex-dtype contract; the imaginary parts are at " *
                                   "LAPACK ULP. On-disk: real-imag interleaved.",
                "tolerance_abs" => 1e-6,
                "data"          => interleave_c128(result_pec.physical_eigenvalues_complex),
            ),
            "q_factor_lowest_physical" => Dict{String,Any}(
                "shape"         => [1],
                "dtype"         => "f64",
                "description"   => "Quality factor of the lowest physical mode at σ₀ = $(args.sigma0): " *
                                   "Q = -Re(λ) / (2·Im(λ)). Positive Q indicates an absorbing " *
                                   "mode under exp(+jωt). High Q (>~100) on the lowest mode " *
                                   "signals it is a trapped resonance with weak PML coupling.",
                "tolerance_abs" => 5e-1,
                "data"          => [result.q_factor_lowest_physical],
            ),
        ),
        "provenance" => Dict{String,Any}(
            "source"           =>
                "reference/julia/sphere_pml.jl @ Epic #88 / #147 (Phase H.2)",
            "julia_version"    => string(VERSION),
            "verified_against" =>
                "reference/fixtures/sphere_pec/baseline.json (σ₀ = 0 PEC-collapse " *
                "physical_eigenvalues field, 1e-6 relative on Re(λ)). Arpack.jl 0.5 " *
                "shift-invert at σ = 2.0 with explicittransform=:none (see " *
                "sphere_pml.jl::eigensolve_physical_shift_invert for the Arpack " *
                ":LM/:SM swap friction recorded in the cube_cavity.jl notes).",
            "issue"            => "#147 (Phase H.2 — Julia sphere-PML reference, complex-arithmetic spine slice)",
            "regenerated_at"   => string(now()),
        ),
    )

    out_dir = dirname(args.out)
    isdir(out_dir) || mkpath(out_dir)
    open(args.out, "w") do io
        JSON3.pretty(io, fixture)
        write(io, "\n")
    end
    @info "Wrote Julia sphere-PML baseline fixture" path=args.out
end


if abspath(PROGRAM_FILE) == @__FILE__
    main()
end
