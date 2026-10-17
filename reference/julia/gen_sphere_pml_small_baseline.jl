#!/usr/bin/env julia
"""
Generate `reference/fixtures/sphere_pml/julia_small_baseline.json` from the
Julia small-mesh sphere-PML pipeline (Epic #88 / Phase H.2, issue #160).

This is the **small-mesh dense tiebreaker** baseline — the Option (b)
resolution of issue #160's Arpack l=2 quintuplet unreachability finding,
made tractable by PR #164 (issue #158) shipping the small-mesh PML
fixture (`reference/fixtures/sphere_pml_small/sphere.msh`, 197 tets,
214-DOF interior pencil).

# Output schema

Mirrors the canonical NumPy small-mesh baseline at
`reference/fixtures/sphere_pml_small/baseline.json` (PR #164) — same
fixture path convention, same field set, same dtypes — so the Rust
cross-IR test (`sphere_pml_julia_small_reference.rs`) can compare both
backends mode-for-mode on the lowest 5 physical modes (l=1 triplet +
2 of l=2 quintuplet, scope [0..4]).

  * `n_nodes, n_tets, n_edges, n_interior_edges, spurious_dim,
     n_spurious_observed` — integer mesh / classifier metrics.
  * `epsilon_r_complex` — per-tet complex ε (c128, length 197).
  * `eigenvalues_lowest_complex` — lowest `spurious_dim + 8 = 39`
    complex modes (spurious cluster near λ ≈ 0 + physical band).
  * `physical_eigenvalues_complex` — lowest 5 physical modes past the
    d⁰-rank split (c128, length 5). **This is the scope [0..4] field
    used by the Rust cross-IR test**.
  * `q_factor_lowest_physical` — sign-agnostic Q of physical[0].
  * `sigma_zero_lowest_physical_re` — σ₀=0 PEC anchor (the small mesh
    has no separate PEC baseline; matches the NumPy convention).

# Why a separate Julia file (not extending gen_sphere_pml_baseline.jl)?

The full-mesh and small-mesh generators have **different fixture
schemas** (the small mesh adds `sigma_zero_lowest_physical_re` and uses
NumPy "lowest 5 by Re globally" sorting for `physical_eigenvalues_complex`,
where the full-mesh generator uses Arpack "5 closest to σ-shift" and
omits the σ₀=0 PEC anchor — it cross-references `sphere_pec/baseline.json`
instead). Keeping the generators as siblings avoids forcing
`gen_sphere_pml_baseline.jl` to grow a mesh-path-conditioned branch that
emits different fields per mesh.

# Cross-check

After generation, this script calls
`check_small_mesh_against_numpy` inline against the canonical NumPy
small-mesh baseline at
`reference/fixtures/sphere_pml_small/baseline.json` to verify the
spectrum agrees within the small-mesh tolerance budget (5e-3 absolute
on the physical band — measured ~6e-5, with headroom for
LAPACK/OpenBLAS reduction-order variance across the Julia / Python
toolchain split).

# Usage

    julia --project=reference/julia \\
        reference/julia/gen_sphere_pml_small_baseline.jl \\
        [--mesh reference/fixtures/sphere_pml_small/sphere.msh] \\
        [--out  reference/fixtures/sphere_pml/julia_small_baseline.json] \\
        [--sigma0 5.0]
"""

push!(LOAD_PATH, @__DIR__)
include(joinpath(@__DIR__, "sphere_pml_small.jl"))

using JSON3
using Dates
using Printf

# Path to the canonical NumPy small-mesh PML baseline (PR #164, issue #158).
const NUMPY_PML_SMALL_BASELINE_PATH::String =
    joinpath(@__DIR__, "..", "fixtures", "sphere_pml_small", "baseline.json")


# ---------------------------------------------------------------------------
# c128 interleave helper (duplicate of gen_sphere_pml_baseline.jl — kept
# local to make this file self-contained).
# ---------------------------------------------------------------------------

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

function _parse_small_gen_args(argv::Vector{String})
    mesh_path = joinpath(@__DIR__, "..", "fixtures", "sphere_pml_small", "sphere.msh")
    out_path  = joinpath(@__DIR__, "..", "fixtures", "sphere_pml", "julia_small_baseline.json")
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
                "Usage: julia --project=. gen_sphere_pml_small_baseline.jl " *
                "[--mesh ...] [--out ...] [--sigma0 5.0]")
            exit(0)
        else
            error("unrecognized argument: $arg")
        end
    end
    return (mesh=mesh_path, out=out_path, sigma0=sigma0)
end


# ---------------------------------------------------------------------------
# Cross-check against the NumPy small-mesh baseline.
# ---------------------------------------------------------------------------

"""
    load_numpy_small_physical_eigenvalues(path) -> Vector{ComplexF64}

Decode the canonical NumPy small-mesh PR #164 baseline's
`physical_eigenvalues_complex` output field (c128 on-disk: real-imag
interleaved) into a `Vector{ComplexF64}`. Returns `nothing` if the
file or field is missing.
"""
function load_numpy_small_physical_eigenvalues(path::AbstractString)
    isfile(path) || (return nothing)
    blob = JSON3.read(read(path, String))
    haskey(blob, :outputs) || (return nothing)
    out = blob.outputs
    haskey(out, :physical_eigenvalues_complex) || (return nothing)
    field = out.physical_eigenvalues_complex
    haskey(field, :data) || (return nothing)
    flat = collect(Float64, field.data)
    @assert iseven(length(flat)) "physical_eigenvalues_complex interleave must be even length"
    n = length(flat) ÷ 2
    out_z = Vector{ComplexF64}(undef, n)
    @inbounds for i in 1:n
        out_z[i] = ComplexF64(flat[2*i - 1], flat[2*i])
    end
    return out_z
end


"""
    check_small_mesh_against_numpy(julia_phys, numpy_phys; abs_tol=5e-3,
                                    n_compare=5)

Cross-check the Julia small-mesh σ₀=5 physical band against the canonical
NumPy PR #164 baseline. **Strict scope [0..4]** — both backends use the
same LAPACK ZGGEV (dense complex generalized eigensolve) on the
identical pencil, so the agreement is expected at LAPACK-vs-LAPACK
roundoff (~6e-5 measured) with headroom for Julia OpenBLAS vs scipy
OpenBLAS reduction-order variance.

# Why 5e-3 absolute (not 1e-4)?

The NumPy fixture's own `physical_eigenvalues_complex` tolerance is
1e-4 absolute (matches faer 0.24 QZ vs LAPACK ZGGEV measured residual
on the small mesh). For Julia-LAPACK vs scipy-LAPACK we expect tighter
agreement (~6e-5 measured), but the 5e-3 floor gives a comfortable
50× headroom for OpenBLAS reduction-order drift across the Julia vs
Python toolchain split (Julia uses `libopenblas` from JLL; scipy uses
the system OpenBLAS or its bundled wheel — different builds, different
threading defaults).

Logs `@info` on per-mode diff for ALL 5 entries, and THROWS on
tolerance violation. This is the **strict scope [0..4] cross-IR check**
that issue #160's acceptance criteria specifies.
"""
function check_small_mesh_against_numpy(
        julia_phys ::Vector{ComplexF64},
        numpy_phys ::Vector{ComplexF64};
        abs_tol    ::Float64 = 5e-3,
        n_compare  ::Int     = 5,
)
    n = min(length(julia_phys), length(numpy_phys))
    @assert n >= n_compare "expected at least $n_compare physical modes for cross-IR check, got $n"
    max_abs = 0.0
    @info "Small-mesh σ₀=5 cross-check vs NumPy PR #164 baseline" n_compare
    for i in 1:n_compare
        lj = julia_phys[i]
        ln = numpy_phys[i]
        delta = abs(lj - ln)
        max_abs = max(max_abs, delta)
        @printf("  [%d] Julia λ = (%.6f, %+.6f)   NumPy λ = (%.6f, %+.6f)   |Δ| = %.3e\n",
                i, real(lj), imag(lj), real(ln), imag(ln), delta)
        if delta > abs_tol
            error("small-mesh physical[$i]: |Δ| = $delta > $abs_tol; " *
                  "Julia $lj vs NumPy $ln. Tighten abs_tol if expected, " *
                  "investigate eigensolver if not.")
        end
    end
    @info "Small-mesh σ₀=5 cross-IR check passed (scope [0..$(n_compare-1)])" max_abs abs_tol
    return max_abs
end


# ---------------------------------------------------------------------------
# Main.
# ---------------------------------------------------------------------------

function main_gen_small()
    args = _parse_small_gen_args(ARGS)
    @info "Generating Julia small-mesh sphere-PML baseline fixture" mesh=args.mesh out=args.out sigma0=args.sigma0

    # ----------------------- σ₀ = 0 PEC anchor ---------------------------
    @info "Running σ₀ = 0 PEC-anchor pass..."
    result_pec = run_sphere_pml_small(args.mesh; sigma0=0.0)

    # Verify the σ₀=0 spectrum is purely real (LAPACK ULP).
    max_im_pec = maximum(abs.(imag.(result_pec.eigenvalues_lowest_complex)))
    max_re_pec = max(maximum(abs.(real.(result_pec.eigenvalues_lowest_complex))), 1.0)
    @info "σ₀=0 sanity: max|Im(λ)|/max(|Re(λ)|, 1) over full slice" max_imag_rel=(max_im_pec/max_re_pec)
    if max_im_pec / max_re_pec >= 1e-10
        @warn "σ₀=0 spectrum should be real; got max|Im(λ)|/|Re(λ)| > 1e-10" max_im_pec max_re_pec
    end

    pec_lowest_re = real(result_pec.physical_eigenvalues_complex[1])
    @info "σ₀=0 PEC anchor: lowest physical Re(λ)" pec_lowest_re

    # ----------------------- σ₀ = 5.0 PML run ----------------------------
    @info "Running σ₀ = $(args.sigma0) PML run..."
    result = run_sphere_pml_small(args.mesh; sigma0=args.sigma0)

    n_take = length(result.physical_eigenvalues_complex)
    @assert n_take == 5 "expected 5 physical modes, got $n_take"

    println()
    println("σ₀ = $(args.sigma0) physical eigenvalues (small mesh, dense eigen):")
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

    # Sign convention check: Im(λ) ≥ 0 on all physical modes.
    for (i, lam) in enumerate(result.physical_eigenvalues_complex)
        if imag(lam) < -1e-6
            @warn "physical[$i] has Im(λ) = $(imag(lam)) < 0 — sign-convention regression"
        end
    end

    # ----------------------- cross-check vs NumPy ------------------------
    numpy_phys = load_numpy_small_physical_eigenvalues(NUMPY_PML_SMALL_BASELINE_PATH)
    if numpy_phys === nothing
        @warn "NumPy small-mesh baseline.json missing or unreadable at " *
              "$NUMPY_PML_SMALL_BASELINE_PATH — skipping cross-IR check."
    else
        check_small_mesh_against_numpy(
            result.physical_eigenvalues_complex, numpy_phys
        )
    end

    # ----------------------- fixture assembly ----------------------------
    n_tets = result.n_tets
    n_spec = length(result.eigenvalues_lowest_complex)

    fixture = Dict{String,Any}(
        "schema_version" => "1",
        "fixture_id"     => "sphere_pml_small/n48_pml_eigenmode_julia",
        "description"    =>
            "Small-mesh scalar-isotropic Vector-Nédélec sphere-PML " *
            "eigenmode pipeline (issue #160 Option (b) — Epic #88 " *
            "Phase H.2 follow-up). Sibling of " *
            "sphere_pml/julia_baseline.json that resolves the Arpack " *
            "shift-invert l=2 quintuplet unreachability surfaced in " *
            "PR #153 cycle 3 by switching to dense " *
            "LinearAlgebra.eigen(K, M) on the small-mesh interior " *
            "pencil (214 DOFs vs the full fixture's 3300 DOFs — fits " *
            "in <2 s instead of 30+ minutes). Selection convention " *
            "matches NumPy small-mesh baseline (PR #164 / issue #158): " *
            "lowest 5 physical modes by |Re(λ)| globally past the " *
            "d⁰-rank spurious split. Sign convention: Im(λ) > 0 per " *
            "PR #155 Judge's binding decision. Cross-IR scope [0..4] " *
            "vs NumPy baseline.",
        "units"  =>
            "λ = k² (inverse-length squared) with Im(λ) > 0 under " *
            "exp(+jωt); dimensionless mesh coordinates",
        "inputs" => Dict{String,Any}(
            "mesh_path" => Dict{String,Any}(
                "shape"       => [0],
                "dtype"       => "f64",
                "description" => "Mesh fixture: reference/fixtures/sphere_pml_small/sphere.msh " *
                                 "(48 nodes, 197 tets — sibling of sphere_pml/sphere.msh).",
                "data"        => [],
            ),
            "sigma_0" => Dict{String,Any}(
                "shape"       => [1],
                "dtype"       => "f64",
                "description" => "PML absorption strength. Matches the canonical " *
                                 "sphere_pml fixture for cross-mesh comparability.",
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
                "description" => "Refractive index inside the dielectric sphere; " *
                                 "ε_r = n² = 2.25 inside.",
                "data"        => [N_INDEX],
            ),
            "epsilon_r_complex" => Dict{String,Any}(
                "shape"       => [n_tets],
                "dtype"       => "c128",
                "description" => "Per-tet complex relative permittivity (length n_tets). " *
                                 "Same profile as the full sphere_pml fixture. On-disk: " *
                                 "real-imag interleaved per reference/SCHEMA.md.",
                "data"        => interleave_c128(result.epsilon_r_complex),
            ),
        ),
        "outputs" => Dict{String,Any}(
            "n_nodes" => Dict{String,Any}(
                "shape"         => [1],
                "dtype"         => "f64",
                "description"   => "Number of mesh nodes.",
                "tolerance_abs" => 0.5,
                "data"          => [Float64(result.n_nodes)],
            ),
            "n_tets" => Dict{String,Any}(
                "shape"         => [1],
                "dtype"         => "f64",
                "description"   => "Number of mesh tets.",
                "tolerance_abs" => 0.5,
                "data"          => [Float64(result.n_tets)],
            ),
            "n_edges" => Dict{String,Any}(
                "shape"         => [1],
                "dtype"         => "f64",
                "description"   => "Number of global edges.",
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
                "description"   => "Predicted spurious dim = number of interior nodes.",
                "tolerance_abs" => 0.5,
                "data"          => [Float64(result.spurious_dim)],
            ),
            "n_spurious_observed" => Dict{String,Any}(
                "shape"         => [1],
                "dtype"         => "f64",
                "description"   => "Algebraic spurious dim = rank(d⁰_interior). " *
                                   "Unaffected by complex ε scaling. Should equal spurious_dim.",
                "tolerance_abs" => 0.5,
                "data"          => [Float64(result.n_spurious)],
            ),
            "k_int_frobenius_complex" => Dict{String,Any}(
                "shape"         => [1],
                "dtype"         => "f64",
                "description"   => "Frobenius norm of K_int (interior curl-curl stiffness).",
                "tolerance_abs" => 1e-4,
                "data"          => [result.k_int_frobenius_complex],
            ),
            "m_int_frobenius_complex" => Dict{String,Any}(
                "shape"         => [1],
                "dtype"         => "f64",
                "description"   => "Frobenius norm of M_int (interior ε-scaled mass).",
                "tolerance_abs" => 1e-4,
                "data"          => [result.m_int_frobenius_complex],
            ),
            "eigenvalues_lowest_complex" => Dict{String,Any}(
                "shape"         => [n_spec],
                "dtype"         => "c128",
                "description"   => "Lowest spurious_dim + 8 = $n_spec complex eigenvalues " *
                                   "of the generalized pencil K x = λ M x, sorted by |Re(λ)| " *
                                   "ascending — same order as Burn's FaerComplexEigensolver " *
                                   "and the NumPy small-mesh baseline. Spurious cluster + " *
                                   "physical band. Sign convention: Im(λ) > 0 on physical " *
                                   "modes per PR #155. On-disk: real-imag interleaved.",
                "tolerance_abs" => 5e-3,
                "data"          => interleave_c128(result.eigenvalues_lowest_complex),
            ),
            "physical_eigenvalues_complex" => Dict{String,Any}(
                "shape"         => [5],
                "dtype"         => "c128",
                "description"   => "Lowest 5 physical complex eigenvalues past the d⁰-rank " *
                                   "spurious split, sorted by |Re(λ)| ascending. **Scope [0..4] " *
                                   "cross-IR pin** vs NumPy small-mesh PR #164 baseline at " *
                                   "5e-3 absolute on |Δ|. Covers the l=1 triplet (positions " *
                                   "[0..2] at λ ≈ 1.92 + 0.06j on this mesh) and 2 of the l=2 " *
                                   "quintuplet (positions [3,4] at λ ≈ 3.28 + 0.15j and " *
                                   "3.57 + 0.12j). Issue #160 resolution: dense Julia eigen " *
                                   "on the small mesh surfaces both bands cleanly, where the " *
                                   "full-mesh Arpack shift-invert path saturates within the " *
                                   "l=1 cluster and never reaches l=2. Sign convention: " *
                                   "Im(λ) > 0.",
                "tolerance_abs" => 5e-3,
                "data"          => interleave_c128(result.physical_eigenvalues_complex),
            ),
            "q_factor_lowest_physical" => Dict{String,Any}(
                "shape"         => [1],
                "dtype"         => "f64",
                "description"   => "Quality factor Q = Re(k) / (2 |Im(k)|) for k = √λ of " *
                                   "the lowest physical mode (sign-agnostic k-space form). " *
                                   "On the small mesh's coarse discretization, the ground " *
                                   "mode is high-Q (~34.8) — much higher than the full " *
                                   "fixture's ~1.2 because the small mesh's ground mode has " *
                                   "a smaller Im(λ).",
                "tolerance_abs" => 1e-2,
                "data"          => [result.q_factor_lowest_physical],
            ),
            "sigma_zero_lowest_physical_re" => Dict{String,Any}(
                "shape"         => [1],
                "dtype"         => "f64",
                "description"   => "Lowest physical Re(λ) at σ₀=0 (PEC limit). Used as the " *
                                   "in-fixture PEC anchor since the small mesh has no " *
                                   "separate PEC baseline (the full sphere_pml fixture " *
                                   "cross-references sphere_pec/baseline.json instead).",
                "tolerance_abs" => 5e-5,
                "data"          => [pec_lowest_re],
            ),
        ),
        "provenance" => Dict{String,Any}(
            "source"           =>
                "reference/julia/sphere_pml_small.jl @ Epic #88 / #160 " *
                "(Option (b) — small-mesh dense LinearAlgebra.eigen " *
                "tiebreaker, made tractable by PR #164 / #158)",
            "julia_version"    => string(VERSION),
            "verified_against" =>
                "reference/fixtures/sphere_pml_small/baseline.json (NumPy " *
                "PR #164 canonical small-mesh baseline; cross-IR scope " *
                "[0..4] at 5e-3 absolute on |Δ| via " *
                "check_small_mesh_against_numpy in this generator AND via " *
                "crates/geode-validation/tests/sphere_pml_julia_small_reference.rs)",
            "issue"            => "#160 (Multi-shift Arpack or smaller-mesh dense " *
                                  "tiebreaker for Julia l=2 quintuplet — Option (b))",
            "regenerated_at"   => string(now()),
        ),
    )

    out_dir = dirname(args.out)
    isdir(out_dir) || mkpath(out_dir)
    open(args.out, "w") do io
        JSON3.pretty(io, fixture)
        write(io, "\n")
    end
    @info "Wrote Julia small-mesh sphere-PML baseline fixture" path=args.out
end


if abspath(PROGRAM_FILE) == @__FILE__
    main_gen_small()
end
