#!/usr/bin/env julia
"""
Generate `reference/fixtures/sphere_mie_small/julia_baseline.json` from the
Julia small-mesh anisotropic-UPML Mie pipeline (Epic #88 / Phase J.3,
issue #172).

Julia sibling of the canonical NumPy small-mesh Mie baseline
(`reference/fixtures/sphere_mie_small/baseline.json`, issue #171 /
PR #179), following the #160 / PR #167 `julia_small_baseline` pattern:
dense `LinearAlgebra.eigen` on the 214-DOF interior tensor-ε pencil
(Arpack shift-invert explicitly avoided per the #160 lossy-cluster
saturation finding).

# Output schema

Field-for-field mirror of the NumPy small-mesh Mie baseline — same
input set (including the full per-tet `epsilon_tensor_diag` c128
payload), same output set (`strict_mode_window_len`, `analytic_tm11_k`,
Q tripwires, σ₀ = 0 anchor) — so the Rust cross-IR test
(`sphere_mie_julia_small_reference.rs`) reuses the J.2 accessors.

# Generation-time gates (all THROW on violation)

  1. Lowest physical mode classifies as TM_1,1 against the J.1 analytic
     catalogue and sits inside the documented 8 % coarse-mesh band.
  2. Q tripwire: lowest-mode Q and TM_1,1-triplet median Q > 1.5
     (`Q_LOWER_BAND_TM11` from mie_sphere.rs).
  3. Cluster closure (#160): the strict window (first 3 physical modes —
     the mesh-split TM_1,1 triplet) ends at a spectral gap
     (gap > 2 × intra-triplet spread).
  4. σ₀ = 0 collapse: tensor degenerates to real isotropic, spectrum
     real to 1e-10 relative.
  5. Cross-IR vs the NumPy baseline: physical band agrees at 5e-3
     absolute on |Δ| for all 5 modes (measured ~1e-13 — LAPACK ZGGEV vs
     LAPACK ZGGEV on the identical pencil; the floor gives OpenBLAS
     build-variance headroom, same rationale as the PML small fixture).

# Sign note (PR #179 schema doc)

Physical `Im(λ) < 0` on this small mesh's tensor pencil — mesh-dependent
(the refined full mesh shows `Im(λ) > 0`), a property of the pencil and
not a solver branch choice. The generator asserts the small-mesh sign.

# Usage

    julia --project=reference/julia \\
        reference/julia/gen_sphere_mie_small_baseline.jl \\
        [--mesh reference/fixtures/sphere_pml_small/sphere.msh] \\
        [--out  reference/fixtures/sphere_mie_small/julia_baseline.json] \\
        [--sigma0 5.0] [--k0-ref 2.0]
"""

push!(LOAD_PATH, @__DIR__)
include(joinpath(@__DIR__, "sphere_mie_small.jl"))

using JSON3
using Printf

# Canonical NumPy small-mesh Mie baseline (PR #179, issue #171).
const NUMPY_MIE_SMALL_BASELINE_PATH::String =
    joinpath(@__DIR__, "..", "fixtures", "sphere_mie_small", "baseline.json")
# J.1 analytic catalogue (PR #177, issue #170).
const MIE_ROOTS_BASELINE_PATH::String =
    joinpath(@__DIR__, "..", "fixtures", "mie_roots", "baseline.json")

# Burn-side acceptance constants mirrored from
# crates/geode-core/tests/mie_sphere.rs (and the NumPy generator).
const TM11_REL_TOL::Float64 = 0.08
const Q_LOWER_BAND_TM11::Float64 = 1.5

# Strict cross-IR window (#160 cluster closure): the TM_1,1 triplet.
const STRICT_MODE_WINDOW_LEN::Int = 3

# Tolerances applied to the on-disk baseline — same floors as the NumPy
# small-mesh Mie fixture (see gen_sphere_mie_small_baseline.py for the
# derivations; the Julia side reuses them so the two fixtures make the
# same regression claim).
const EPS_C128_TOL_ABS::Float64       = 1.0e-14
const EIG_C128_TOL_ABS::Float64       = 5.0e-4
const PHYSICAL_C128_TOL_ABS::Float64  = 1.0e-4
const Q_FACTOR_TOL_ABS::Float64       = 5.0
const RE_K_TOL_ABS::Float64           = 1.0e-4
const ANALYTIC_K_TOL_ABS::Float64     = 1.0e-9
const SIGMA_ZERO_RE_TOL_ABS::Float64  = 5.0e-5

# Cross-IR gate vs NumPy — same 5e-3 floor as the PML small tiebreaker.
const CROSS_IR_ABS_TOL::Float64 = 5.0e-3


# ---------------------------------------------------------------------------
# Helpers.
# ---------------------------------------------------------------------------

"""Interleave a ComplexF64 vector to the canonical real-imag flat list."""
function interleave_c128(z::AbstractVector{ComplexF64})
    out = Vector{Float64}(undef, 2 * length(z))
    @inbounds for (i, c) in enumerate(z)
        out[2*i - 1] = real(c)
        out[2*i]     = imag(c)
    end
    return out
end

"""Row-major flatten + interleave an (n × 3) ComplexF64 matrix."""
function interleave_c128_rowmajor(z::Matrix{ComplexF64})
    n, m = size(z)
    flat = Vector{ComplexF64}(undef, n * m)
    @inbounds for r in 1:n, c in 1:m
        flat[(r - 1) * m + c] = z[r, c]
    end
    return interleave_c128(flat)
end

"""Decode a c128 fixture field (real-imag interleaved) to ComplexF64."""
function decode_c128(field)
    flat = collect(Float64, field.data)
    @assert iseven(length(flat)) "c128 interleave must be even length"
    n = length(flat) ÷ 2
    out = Vector{ComplexF64}(undef, n)
    @inbounds for i in 1:n
        out[i] = ComplexF64(flat[2*i - 1], flat[2*i])
    end
    return out
end

"""Load the TM_1,1 root from the J.1 analytic catalogue."""
function load_analytic_tm11_k(path::AbstractString)
    isfile(path) || error("J.1 catalogue not found at $path")
    blob = JSON3.read(read(path, String))
    out  = blob.outputs
    pols = collect(Float64, out.root_pol.data)
    ls   = collect(Float64, out.root_l.data)
    ns   = collect(Float64, out.root_n.data)
    ks   = collect(Float64, out.root_k.data)
    for i in eachindex(ks)
        if Int(round(pols[i])) == 1 && Int(round(ls[i])) == 1 && Int(round(ns[i])) == 1
            return ks[i]
        end
    end
    error("TM_1,1 not found in the J.1 catalogue")
end

function _parse_gen_mie_args(argv::Vector{String})
    mesh_path = joinpath(@__DIR__, "..", "fixtures", "sphere_pml_small", "sphere.msh")
    out_path  = joinpath(@__DIR__, "..", "fixtures", "sphere_mie_small", "julia_baseline.json")
    sigma0    = SIGMA_0_DEFAULT
    k0_ref    = K0_REF_DEFAULT
    i = 1
    while i <= length(argv)
        arg = argv[i]
        if arg == "--mesh"
            mesh_path = argv[i + 1]; i += 2
        elseif arg == "--out"
            out_path = argv[i + 1]; i += 2
        elseif arg == "--sigma0"
            sigma0 = parse(Float64, argv[i + 1]); i += 2
        elseif arg == "--k0-ref"
            k0_ref = parse(Float64, argv[i + 1]); i += 2
        elseif arg in ("-h", "--help")
            println(stderr,
                "Usage: julia --project=. gen_sphere_mie_small_baseline.jl " *
                "[--mesh ...] [--out ...] [--sigma0 5.0] [--k0-ref 2.0]")
            exit(0)
        else
            error("unrecognized argument: $arg")
        end
    end
    return (mesh=mesh_path, out=out_path, sigma0=sigma0, k0_ref=k0_ref)
end


# ---------------------------------------------------------------------------
# Main.
# ---------------------------------------------------------------------------

function main_gen_mie_small()
    args = _parse_gen_mie_args(ARGS)
    @info "Generating Julia small-mesh anisotropic-UPML Mie baseline" mesh=args.mesh out=args.out sigma0=args.sigma0 k0_ref=args.k0_ref

    # ----------------------- σ₀ = 5 UPML run -----------------------------
    result = run_sphere_mie_small(args.mesh; sigma0=args.sigma0, k0_ref=args.k0_ref)
    physical = result.physical_eigenvalues_complex
    n_take = length(physical)
    @assert n_take == 5 "expected 5 physical modes, got $n_take"

    @printf("  n_nodes=%d, n_tets=%d, n_edges=%d, n_interior=%d, spurious=%d/%d\n",
            result.n_nodes, result.n_tets, result.n_edges,
            result.n_interior_edges, result.spurious_dim, result.n_spurious)
    @printf("  M complex-symmetry residual = %.3e\n",
            result.m_int_complex_symmetry_residual)

    println("  lowest 5 physical modes:")
    for (i, (lam, k)) in enumerate(zip(physical, result.physical_ks))
        @printf("    [%d] λ = %+.6e %+.6ej  k = %.5f %+.5fj  Q = %.4f\n",
                i, real(lam), imag(lam), real(k), imag(k), q_factor(lam))
    end

    # Gate (sign note): physical Im(λ) < 0 on this small-mesh tensor
    # pencil (PR #179 schema doc — mesh-dependent, see module docstring).
    for (i, lam) in enumerate(physical[1:STRICT_MODE_WINDOW_LEN])
        imag(lam) <= 1e-10 || error(
            "physical[$i] has Im(λ) = $(imag(lam)) > 0 — small-mesh " *
            "anisotropic-pencil sign regression (expected Im(λ) < 0)")
    end

    # Gate 1: TM_1,1 classification + 8 % band.
    analytic_tm11_k = load_analytic_tm11_k(MIE_ROOTS_BASELINE_PATH)
    lowest_re_k = real(result.physical_ks[1])
    rel_err_tm11 = abs(lowest_re_k - analytic_tm11_k) / analytic_tm11_k
    @printf("  lowest mode vs analytic TM_1,1 (k = %.6f): rel err = %.2f%%\n",
            analytic_tm11_k, rel_err_tm11 * 100)
    rel_err_tm11 < TM11_REL_TOL || error(
        "lowest mode rel err $(rel_err_tm11 * 100)% exceeds the documented " *
        "$(TM11_REL_TOL * 100)% Burn-side acceptance band")

    # Gate 2: Q tripwire — lowest mode + TM_1,1-triplet median.
    q_lowest = result.q_factor_lowest_physical
    triplet_qs = sort([q_factor(lam) for lam in physical[1:STRICT_MODE_WINDOW_LEN]])
    q_median_triplet = triplet_qs[2]
    @printf("  Q lowest = %.4f, TM_1,1 triplet median Q = %.4f (band > %.1f)\n",
            q_lowest, q_median_triplet, Q_LOWER_BAND_TM11)
    q_lowest > Q_LOWER_BAND_TM11 || error("Q tripwire: lowest-mode Q below band")
    q_median_triplet > Q_LOWER_BAND_TM11 || error("Q tripwire: triplet median below band")

    # Gate 3: cluster closure (#160) — strict window ends at a spectral gap.
    triplet_spread = real(physical[STRICT_MODE_WINDOW_LEN]) - real(physical[1])
    gap_to_next = real(physical[STRICT_MODE_WINDOW_LEN + 1]) -
                  real(physical[STRICT_MODE_WINDOW_LEN])
    @printf("  TM_1,1 triplet spread = %.4f, gap to next band = %.4f\n",
            triplet_spread, gap_to_next)
    gap_to_next > 2.0 * triplet_spread || error(
        "strict mode window does not end at a spectral gap — " *
        "cluster-closure convention (#160) violated")

    # ----------------------- σ₀ = 0 PEC anchor ---------------------------
    @info "Running σ₀ = 0 PEC-anchor pass..."
    result_pec = run_sphere_mie_small(args.mesh; sigma0=0.0, k0_ref=args.k0_ref)
    pec_lowest_re = real(result_pec.physical_eigenvalues_complex[1])
    pec_max_imag_rel = result_pec.max_imag_eigval_rel
    @printf("  σ₀=0 anchor: lowest physical Re(λ) = %.6e, max|Im|/max|Re| = %.3e\n",
            pec_lowest_re, pec_max_imag_rel)
    pec_max_imag_rel < 1e-10 || error(
        "σ₀ = 0 spectrum should be real to 1e-10 relative; tensor did not " *
        "collapse to the real isotropic scalar")

    # ----------------------- cross-IR vs NumPy ---------------------------
    isfile(NUMPY_MIE_SMALL_BASELINE_PATH) || error(
        "NumPy small-mesh Mie baseline not found at " *
        "$NUMPY_MIE_SMALL_BASELINE_PATH — Phase J.3 consumes the merged " *
        "J.2 fixture (PR #179)")
    numpy_blob = JSON3.read(read(NUMPY_MIE_SMALL_BASELINE_PATH, String))
    numpy_phys = decode_c128(numpy_blob.outputs.physical_eigenvalues_complex)
    @assert length(numpy_phys) == n_take
    max_abs = 0.0
    println("  cross-IR vs NumPy J.2 small-mesh baseline:")
    for i in 1:n_take
        delta = abs(physical[i] - numpy_phys[i])
        max_abs = max(max_abs, delta)
        @printf("    [%d] Julia λ = (%.6f, %+.6f)   NumPy λ = (%.6f, %+.6f)   |Δ| = %.3e\n",
                i, real(physical[i]), imag(physical[i]),
                real(numpy_phys[i]), imag(numpy_phys[i]), delta)
        delta <= CROSS_IR_ABS_TOL || error(
            "physical[$i]: |Δ| = $delta > $CROSS_IR_ABS_TOL vs NumPy — " *
            "both sides are LAPACK ZGGEV on the identical pencil; investigate")
    end
    @printf("  cross-IR check passed (scope [0..%d]): max |Δ| = %.3e <= %.0e\n",
            n_take - 1, max_abs, CROSS_IR_ABS_TOL)

    # ----------------------- fixture assembly ----------------------------
    n_tets = result.n_tets
    n_request = length(result.eigenvalues_lowest_complex)

    fixture = Dict{String,Any}(
        "schema_version" => "1",
        "fixture_id"     => "sphere_mie_small/n48_aniso_upml_mie_julia",
        "description"    =>
            "Small-mesh anisotropic-UPML dielectric-sphere Mie eigenmode " *
            "— Julia reference (issue #172, Epic #88 Phase J.3). Sibling " *
            "of the canonical NumPy J.2 baseline " *
            "(sphere_mie_small/baseline.json, PR #179) on the same " *
            "197-tet mesh (reference/fixtures/sphere_pml_small/sphere.msh) " *
            "with the same diagonal UPML tensor (σ₀ = 5.0, k₀_ref = 2.0). " *
            "Dense LinearAlgebra.eigen (LAPACK ZGGEV) on the 214-DOF " *
            "interior pencil per the #160 tiebreaker pattern — Arpack " *
            "shift-invert avoided (lossy-cluster saturation). Anchored to " *
            "the J.1 analytic catalogue: lowest mode is the mesh-split " *
            "TM_1,1 triplet's leading member at ~6.6% of k ≈ 1.30343. " *
            "Strict cross-IR window = first 3 physical modes (closed " *
            "TM_1,1 triplet, #160 cluster-closure convention). Sign note: " *
            "physical Im(λ) < 0 on this small mesh's tensor pencil " *
            "(mesh-dependent; see the NumPy fixture description).",
        "units" =>
            "λ = k² (inverse-length squared) under exp(+jωt); " *
            "dimensionless mesh coordinates",
        "inputs" => Dict{String,Any}(
            "mesh_path" => Dict{String,Any}(
                "shape"       => [0],
                "dtype"       => "f64",
                "description" =>
                    "Mesh fixture (relative to repo root): " *
                    "reference/fixtures/sphere_pml_small/sphere.msh " *
                    "($(result.n_nodes) nodes, $n_tets tets) — shared with " *
                    "the #158 sphere_pml_small fixture, not duplicated.",
                "data"        => [],
            ),
            "sigma_0" => Dict{String,Any}(
                "shape"       => [1],
                "dtype"       => "f64",
                "description" =>
                    "UPML absorption strength at r=R_BUFFER (mie_sphere.rs " *
                    "acceptance value).",
                "data"        => [args.sigma0],
            ),
            "k0_ref" => Dict{String,Any}(
                "shape"       => [1],
                "dtype"       => "f64",
                "description" =>
                    "Reference wavenumber ω heuristic in the UPML stretch " *
                    "s = 1 - jσ(r)/ω (K0_REF in mie_sphere.rs).",
                "data"        => [args.k0_ref],
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
                "description" =>
                    "Refractive index inside the dielectric sphere; " *
                    "ε_r = n² = 2.25 inside.",
                "data"        => [result.n_index],
            ),
            "epsilon_tensor_diag" => Dict{String,Any}(
                "shape"       => [n_tets, 3],
                "dtype"       => "c128",
                "description" =>
                    "Per-tet diagonal anisotropic complex permittivity " *
                    "tensor (ε_x, ε_y, ε_z) in the global Cartesian basis " *
                    "— mirror of geode_core::build_anisotropic_pml_tensor_" *
                    "diag (see the NumPy fixture for the profile formula). " *
                    "On-disk: row-major (tet, axis) flattened, real-imag " *
                    "interleaved per reference/SCHEMA.md.",
                "data"        => interleave_c128_rowmajor(result.epsilon_tensor_diag),
            ),
        ),
        "outputs" => Dict{String,Any}(
            "n_nodes" => Dict{String,Any}(
                "shape" => [1], "dtype" => "f64",
                "description"   => "Number of mesh nodes.",
                "tolerance_abs" => 0.5,
                "data"          => [Float64(result.n_nodes)],
            ),
            "n_tets" => Dict{String,Any}(
                "shape" => [1], "dtype" => "f64",
                "description"   => "Number of mesh tets.",
                "tolerance_abs" => 0.5,
                "data"          => [Float64(result.n_tets)],
            ),
            "n_edges" => Dict{String,Any}(
                "shape" => [1], "dtype" => "f64",
                "description"   => "Number of globally-deduplicated edges.",
                "tolerance_abs" => 0.5,
                "data"          => [Float64(result.n_edges)],
            ),
            "n_interior_edges" => Dict{String,Any}(
                "shape" => [1], "dtype" => "f64",
                "description"   =>
                    "Number of edges that survive PEC reduction.",
                "tolerance_abs" => 0.5,
                "data"          => [Float64(result.n_interior_edges)],
            ),
            "spurious_dim" => Dict{String,Any}(
                "shape" => [1], "dtype" => "f64",
                "description"   =>
                    "Predicted gradient-kernel dimension = interior nodes.",
                "tolerance_abs" => 0.5,
                "data"          => [Float64(result.spurious_dim)],
            ),
            "n_spurious_observed" => Dict{String,Any}(
                "shape" => [1], "dtype" => "f64",
                "description"   =>
                    "Algebraic spurious-mode dimension = rank(d⁰_interior). " *
                    "Invariant under the tensor-ε scaling on the mass.",
                "tolerance_abs" => 0.5,
                "data"          => [Float64(result.n_spurious)],
            ),
            "eigenvalues_lowest_complex" => Dict{String,Any}(
                "shape" => [n_request], "dtype" => "c128",
                "description"   =>
                    "Lowest $n_request = spurious_dim + 8 complex " *
                    "eigenvalues of the tensor-ε pencil K x = λ M x " *
                    "(K real, M complex-symmetric), sorted by |Re(λ)| " *
                    "ascending. On-disk: real-imag interleaved.",
                "tolerance_abs" => EIG_C128_TOL_ABS,
                "data"          => interleave_c128(result.eigenvalues_lowest_complex),
            ),
            "physical_eigenvalues_complex" => Dict{String,Any}(
                "shape" => [n_take], "dtype" => "c128",
                "description"   =>
                    "Lowest 5 physical complex eigenvalues past the " *
                    "d⁰-rank spurious split, |Re(λ)| ascending. Physical " *
                    "Im(λ) < 0 on this tensor pencil. Strict cross-IR " *
                    "window = first strict_mode_window_len entries (the " *
                    "closed TM_1,1 triplet); positions [3, 4] belong to " *
                    "the next band, compared at the same tolerance but " *
                    "excluded from the cluster-closure claim.",
                "tolerance_abs" => PHYSICAL_C128_TOL_ABS,
                "data"          => interleave_c128(Vector{ComplexF64}(physical)),
            ),
            "strict_mode_window_len" => Dict{String,Any}(
                "shape" => [1], "dtype" => "f64",
                "description"   =>
                    "Length of the strict cross-IR mode window: the " *
                    "mesh-split TM_1,1 triplet (multiplicity 2l+1 = 3), " *
                    "closed at a spectral gap per the #160 convention.",
                "tolerance_abs" => 0.5,
                "data"          => [Float64(STRICT_MODE_WINDOW_LEN)],
            ),
            "analytic_tm11_k" => Dict{String,Any}(
                "shape" => [1], "dtype" => "f64",
                "description"   =>
                    "Analytic TM_1,1 PEC-cavity root re-exported from the " *
                    "J.1 catalogue (reference/fixtures/mie_roots/" *
                    "baseline.json, pol=TM, l=1, n=1).",
                "tolerance_abs" => ANALYTIC_K_TOL_ABS,
                "data"          => [analytic_tm11_k],
            ),
            "lowest_physical_re_k" => Dict{String,Any}(
                "shape" => [1], "dtype" => "f64",
                "description"   =>
                    "Re(k) = Re(√λ) (principal branch) of the lowest " *
                    "physical mode.",
                "tolerance_abs" => RE_K_TOL_ABS,
                "data"          => [lowest_re_k],
            ),
            "tm11_rel_err_lowest" => Dict{String,Any}(
                "shape" => [1], "dtype" => "f64",
                "description"   =>
                    "Relative error of the lowest physical Re(k) vs the " *
                    "analytic TM_1,1 root. Must stay below the documented " *
                    "8% coarse-mesh band (mie_sphere.rs).",
                "tolerance_abs" => RE_K_TOL_ABS,
                "data"          => [rel_err_tm11],
            ),
            "q_factor_lowest_physical" => Dict{String,Any}(
                "shape" => [1], "dtype" => "f64",
                "description"   =>
                    "Q = Re(k)/(2|Im(k)|) of the lowest physical mode " *
                    "(sign-agnostic). Loose tolerance — dQ/dIm(λ) ≈ " *
                    "Q/|Im(λ)| amplifies eigenvalue residuals; the " *
                    "load-bearing assertion is the Q > 1.5 tripwire.",
                "tolerance_abs" => Q_FACTOR_TOL_ABS,
                "data"          => [q_lowest],
            ),
            "q_median_tm11_triplet" => Dict{String,Any}(
                "shape" => [1], "dtype" => "f64",
                "description"   =>
                    "Median Q over the TM_1,1 triplet (strict window) — " *
                    "mirrors the Q_LOWER_BAND_TM11 = 1.5 Burn-side tripwire.",
                "tolerance_abs" => Q_FACTOR_TOL_ABS,
                "data"          => [q_median_triplet],
            ),
            "sigma_zero_lowest_physical_re" => Dict{String,Any}(
                "shape" => [1], "dtype" => "f64",
                "description"   =>
                    "Lowest physical Re(λ) at σ₀ = 0 (PEC limit; the " *
                    "tensor collapses to real isotropic scalar ε). " *
                    "Numerically identical to the sphere_pml_small anchor.",
                "tolerance_abs" => SIGMA_ZERO_RE_TOL_ABS,
                "data"          => [pec_lowest_re],
            ),
        ),
        "provenance" => Dict{String,Any}(
            "source" =>
                "reference/julia/sphere_mie_small.jl — Julia port of the " *
                "anisotropic UPML assembly (build_anisotropic_pml_tensor_" *
                "diag + per-axis Nédélec mass kernel), dense " *
                "LinearAlgebra.eigen (LAPACK ZGGEV) on the small-mesh " *
                "interior pencil per the #160 tiebreaker pattern.",
            "julia_version" => string(VERSION),
            "verified_against" =>
                "reference/fixtures/sphere_mie_small/baseline.json (NumPy " *
                "J.2 canonical small-mesh Mie baseline; cross-IR scope " *
                "[0..4] at 5e-3 absolute in this generator, measured " *
                @sprintf("%.3e", max_abs) * ") AND crates/geode-validation/" *
                "tests/sphere_mie_julia_small_reference.rs (Burn faer QZ " *
                "on the identical pencil).",
            "issue" => "#172 (Epic #88 Phase J.3; mesh from #158, tensor " *
                       "from #171/#54, anchor from #170)",
        ),
    )

    out_dir = dirname(args.out)
    isdir(out_dir) || mkpath(out_dir)
    open(args.out, "w") do io
        JSON3.pretty(io, fixture)
        write(io, "\n")
    end
    @info "Wrote Julia small-mesh Mie baseline fixture" path=args.out
end


if abspath(PROGRAM_FILE) == @__FILE__
    main_gen_mie_small()
end
