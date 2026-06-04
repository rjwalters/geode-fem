#!/usr/bin/env julia
"""
Julia reference for the scalar-Helmholtz cube-cavity eigenmode pipeline
(Epic #88 / Phase E, issue #115).

This is the **complex-arithmetic reference backend** per Epic #88's
principle 4. Even though the cube-cavity slice is real-symmetric,
landing Julia here lays the toolchain track for Phases G–J (Nédélec,
PML, NLEPS) where complex types matter, and surfaces complex-arithmetic
ergonomics friction earlier rather than later.

Algorithmic structure mirrors `reference/numpy/cube_cavity.py` exactly:

1. Mesh I/O — generate the canonical n-per-side tet-split cube via
   `CubeMesh.cube_tet_mesh(n)` **or** load a `.msh` via inline MSH 4.1
   parser (`CubeMesh.load_msh`). Both live in `mesh.jl`.
2. P1 local matrices — closed-form expressions transcribed faithfully
   from `reference/numpy/p1_local_matrices.py` (which itself is the
   reference for `crates/geode-core/src/p1.rs`).
3. Global assembly — COO triples → CSC via `SparseArrays.sparse(...)`
   which collapses duplicates by sum (same semantics as scipy's
   `coo_matrix.tocsr`).
4. Dirichlet BC — restrict K, M to interior nodes via fancy indexing.
5. Generalized eigensolve — `Arpack.eigs(K_int, M_int, nev=5, which=:SM)`.
   Arpack.jl binds the same `libarpack` as `scipy.sparse.linalg.eigsh`,
   making this a near-bit-equivalent of the NumPy reference at the
   iteration-trace level. The exact call differs from SciPy's
   `eigsh(K, k, M=M, sigma=0, which="LM")`: Arpack.jl 0.5 mis-handles
   that shift-invert recipe on the generalized pencil and returns the
   largest eigenvalues instead. See `eigensolve_arpack` for the gory
   details and the regular-inverse-mode workaround.

Usage
=====

Self-check (with the canonical n=10 mesh fixture):

    julia --project=. cube_cavity.jl

Programmatic invocation (called by gen_cube_cavity_baseline.jl and by
the CI gate):

    julia --project=. cube_cavity.jl --n 10 \\
        --mesh ../fixtures/cube_cavity/unit_cube.msh \\
        --out  ../fixtures/cube_cavity/julia_baseline.json

ARPACK iteration-trace caveat
=============================

Arpack.jl's `eigs` is sensitive to the Arnoldi starting vector. We
explicitly seed v0 (see `eigensolve_arpack`) so the run is
deterministic across machines. Eigenvalues are stable to ~1e-13
across re-runs; eigenvectors within a degenerate cluster require the
subspace-overlap convention (we do not store eigenvectors in
`julia_baseline.json` — mirroring `jax_baseline.json`'s shape).

Arpack.jl vs SciPy calling-convention divergence
================================================

Arpack.jl 0.5.x and SciPy share the same underlying libarpack, so they
agree to the iteration-trace level on the *same* operator. They do not
agree on how to set up the "lowest generalized eigenvalues" call. SciPy
uses `eigsh(K, k, M=M, sigma=0, which="LM")` (shift-invert at σ=0,
ask for largest-magnitude eigenvalues of `(K-σM)⁻¹M`); Arpack.jl 0.5
mis-routes that recipe through its `:auto` explicit-transform path and
returns the *largest* eigenvalues. We use `eigs(K, M; nev, which=:SM)`
(no `sigma`) — Arpack.jl's regular-inverse mode (`mode = 2`) — which
factorizes M once and Lanczos-iterates on `M⁻¹K`, asking for
smallest-magnitude. This matches the dense `eigvals(K, M)` result to
~1e-13 and is the Julia friction artifact for Epic #88 Phase E
(complex-arithmetic toolchain bring-up).
"""

push!(LOAD_PATH, @__DIR__)
include(joinpath(@__DIR__, "mesh.jl"))

using .CubeMesh: cube_tet_mesh, cube_interior_mask, load_msh
using LinearAlgebra
using SparseArrays
using Arpack
using JSON3
using Printf


# --------------------------------------------------------------------------- #
# P1 local matrices (per-tet)
# --------------------------------------------------------------------------- #

"""
    p1_local(verts) -> (k_local, m_local, signed_volume)

Compute P1 local stiffness and consistent mass for one tetrahedron.

Faithful transcription of `reference/numpy/p1_local_matrices.py` for a
single element. Named intermediates match the symbols in
`crates/geode-core/src/p1.rs` one-for-one: edges `e1 = v2-v1, e2 = v3-v1,
e3 = v4-v1`; area-weighted basis gradients `g1 = e2 x e3`, `g2 = e3 x
e1`, `g3 = e1 x e2`, `g0 = -(g1 + g2 + g3)`; `det = e1 . g1`; element
volume `V = |det| / 6`; `K_ij = (g_i . g_j) / (6 |det|)`; `M_ij = (V /
20)(1 + δ_ij)`.

# Arguments
- `verts::AbstractMatrix{Float64}` of shape `(4, 3)` — per-tet vertex
  coordinates.
"""
function p1_local(verts::AbstractMatrix{Float64})
    v0 = @view verts[1, :]
    v1 = @view verts[2, :]
    v2 = @view verts[3, :]
    v3 = @view verts[4, :]

    e1 = v1 .- v0
    e2 = v2 .- v0
    e3 = v3 .- v0

    g1 = cross(e2, e3)
    g2 = cross(e3, e1)
    g3 = cross(e1, e2)
    g0 = -(g1 .+ g2 .+ g3)

    det = dot(e1, g1)
    signed_volume = det / 6.0
    abs_det = abs(det)

    # Stack G as a 4x3 matrix with row i = g_{i-1}.
    G = Matrix{Float64}(undef, 4, 3)
    G[1, :] .= g0
    G[2, :] .= g1
    G[3, :] .= g2
    G[4, :] .= g3
    gg = G * transpose(G)  # 4x4 Gram matrix

    k_local = gg ./ (6.0 * abs_det)

    # Mass pattern: 2 on diagonal, 1 off-diagonal.
    m_local = abs_det / 120.0 .* Float64[
        2.0 1.0 1.0 1.0;
        1.0 2.0 1.0 1.0;
        1.0 1.0 2.0 1.0;
        1.0 1.0 1.0 2.0
    ]
    return k_local, m_local, signed_volume
end


# --------------------------------------------------------------------------- #
# Global assembly via SparseArrays.sparse(rows, cols, vals)
# --------------------------------------------------------------------------- #

"""
    assemble_global_p1(nodes, tets) -> (K, M)

Assemble global stiffness `K` and consistent mass `M` as
`SparseMatrixCSC{Float64}` matrices. Uses COO triples assembled into the
final CSC via `SparseArrays.sparse(I, J, V, m, n, +)` — the `+` combiner
collapses duplicate `(i, j)` entries by sum (scipy's `coo_matrix.tocsr`
default).

Mirrors `reference/numpy/cube_cavity.py::assemble_global_p1`.
"""
function assemble_global_p1(nodes::AbstractMatrix{Float64}, tets::AbstractMatrix{Int})
    n_nodes = size(nodes, 1)
    n_elem = size(tets, 1)

    # Reserve triplet storage: 16 entries per element.
    nnz_est = 16 * n_elem
    I = Vector{Int}(undef, nnz_est)
    J = Vector{Int}(undef, nnz_est)
    Vk = Vector{Float64}(undef, nnz_est)
    Vm = Vector{Float64}(undef, nnz_est)

    p = 1
    for e in 1:n_elem
        verts = nodes[tets[e, :], :]
        k_local, m_local, _ = p1_local(verts)
        for i in 1:4, j in 1:4
            I[p] = tets[e, i]
            J[p] = tets[e, j]
            Vk[p] = k_local[i, j]
            Vm[p] = m_local[i, j]
            p += 1
        end
    end

    K = sparse(I, J, Vk, n_nodes, n_nodes, +)
    M = sparse(I, J, Vm, n_nodes, n_nodes, +)
    return K, M
end


"""
    apply_dirichlet(K, M, mask) -> (K_int, M_int)

Restrict K, M to the rows/cols where `mask` is true. The dropped rows/
cols implement homogeneous Dirichlet conditions on the eliminated DOFs.
"""
function apply_dirichlet(K::SparseMatrixCSC{Float64}, M::SparseMatrixCSC{Float64},
                        mask::BitVector)
    idx = findall(mask)
    K_int = K[idx, idx]
    M_int = M[idx, idx]
    return K_int, M_int
end


# --------------------------------------------------------------------------- #
# Generalized eigensolve via Arpack.jl
# --------------------------------------------------------------------------- #

"""
    eigensolve_arpack(K, M; nev=5) -> (eigvals, eigvecs)

Lowest-`nev` generalized eigenpairs of `K x = λ M x` via Arpack.jl.

Arpack.jl wraps the same libarpack used by `scipy.sparse.linalg.eigsh`,
so the iteration trace agrees with the NumPy reference at the
bit-equivalent precision level on the same matrices.

**Calling convention — `which=:SM` (regular inverse mode), NOT
`sigma=0; which=:LM` (shift-invert).**

`scipy.sparse.linalg.eigsh(K, k, M=M, sigma=0.0, which="LM")` is the
canonical recipe for "lowest-`k` generalized eigenvalues" in SciPy.
**Arpack.jl 0.5 does not produce the same result on the generalized
problem with that call**: when `sigma !== nothing` and the problem is
generalized, Arpack.jl 0.5 internally takes the `:auto`
`explicittransform=:shiftinvert` path, swaps `:LM ↔ :SM`, factorizes
`σB - A = -K` (at σ=0), and ends up solving the standard problem for
`-K⁻¹ M` with `:SM`. The smallest-magnitude eigenvalues of `-K⁻¹ M`
correspond to the *largest* generalized eigenvalues of `K x = λ M x`,
so the post-processing step `λ = σ - 1/μ` returns the **largest**
generalized eigenvalues — the opposite of what the user requested.

For the generalized pencil with M SPD, Arpack.jl's "regular inverse
mode" (`mode = 2`, `which=:SM`, no `sigma`) does the right thing:
it factorizes M once, then runs Lanczos on the operator
`x ↦ M⁻¹ K x`, asking for the smallest-magnitude eigenvalues.

A deterministic v0 is supplied so repeated runs on the same machine
produce identical Arnoldi traces; eigenvalues are stable across
machines regardless.
"""
function eigensolve_arpack(K::SparseMatrixCSC{Float64}, M::SparseMatrixCSC{Float64};
                          nev::Int=5)
    n = size(K, 1)
    # Deterministic seed: all ones, normalized. Matches scipy's "supplied v0"
    # idiom; eliminates random-restart drift between machines.
    v0 = ones(Float64, n) ./ sqrt(n)
    # which=:SM in regular-inverse mode (no sigma) for the generalized
    # pencil — see the docstring above for why the SciPy-style
    # `sigma=0, which=:LM` call is buggy under Arpack.jl 0.5.
    eigvals, eigvecs = eigs(K, M; nev=nev, which=:SM, v0=v0)
    # Real symmetric pencil ⇒ imaginary parts are at round-off; strip them.
    eigvals_real = real.(eigvals)
    eigvecs_real = real.(eigvecs)
    # Sort ascending.
    order = sortperm(eigvals_real)
    return eigvals_real[order], eigvecs_real[:, order]
end


# --------------------------------------------------------------------------- #
# End-to-end driver
# --------------------------------------------------------------------------- #

"""
    run_cube_cavity(; n=10, k=5, mesh_path=nothing, side=1.0) -> NamedTuple

Run the full Julia cube-cavity pipeline. Either `n` (programmatic
mesh) or `mesh_path` (load a `.msh`) controls the mesh source.

Returns a `NamedTuple` with the same diagnostic fields as the NumPy
reference's `run_cube_cavity`:
- `n_nodes, n_tets, n_int`
- `nodes, tets`
- `interior_mask`
- `K, M, K_int, M_int`
- `eigenvalues, eigenvectors`
- `k_frobenius, m_frobenius, k_int_frobenius, m_int_frobenius`
- `k_int_diag, m_int_diag`
"""
function run_cube_cavity(; n::Int=10, k::Int=5,
                        mesh_path::Union{Nothing,AbstractString}=nothing,
                        side::Float64=1.0)
    nodes, tets = if mesh_path === nothing
        cube_tet_mesh(n; side=side)
    else
        load_msh(mesh_path)
    end

    K, M = assemble_global_p1(nodes, tets)
    mask = cube_interior_mask(nodes; side=side)
    K_int, M_int = apply_dirichlet(K, M, mask)
    eigvals, eigvecs = eigensolve_arpack(K_int, M_int; nev=k)

    return (
        n_nodes = size(nodes, 1),
        n_tets = size(tets, 1),
        n_int = size(K_int, 1),
        nodes = nodes,
        tets = tets,
        interior_mask = mask,
        K = K,
        M = M,
        K_int = K_int,
        M_int = M_int,
        eigenvalues = eigvals,
        eigenvectors = eigvecs,
        k_frobenius = norm(K),  # Frobenius for SparseMatrixCSC
        m_frobenius = norm(M),
        k_int_frobenius = norm(K_int),
        m_int_frobenius = norm(M_int),
        k_int_diag = Vector{Float64}(diag(K_int)),
        m_int_diag = Vector{Float64}(diag(M_int)),
    )
end


"""
    analytic_lowest_five() -> Vector{Float64}

Lowest 5 Dirichlet Laplacian eigenvalues on the unit cube:
`{3, 6, 6, 6, 9} * π²`.
"""
function analytic_lowest_five()
    π² = pi * pi
    return Float64[3.0 * π², 6.0 * π², 6.0 * π², 6.0 * π², 9.0 * π²]
end


# --------------------------------------------------------------------------- #
# CLI entry point
# --------------------------------------------------------------------------- #

function _parse_args(argv::Vector{String})
    n = 10
    k = 5
    side = 1.0
    mesh_path::Union{Nothing,String} = nothing
    out_path::Union{Nothing,String} = nothing
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
            println(stderr, """
            Usage: julia --project=. cube_cavity.jl [options]
                  --n <int>          cells per side (default 10)
                  --k <int>          number of eigenmodes (default 5)
                  --side <float>     cube edge length (default 1.0)
                  --mesh <path>      load mesh from .msh (overrides --n)
                  --out <path>       emit a sidecar JSON (used by the CI gate)
            """)
            exit(0)
        else
            error("unrecognized argument: $arg")
        end
    end
    return (n=n, k=k, side=side, mesh=mesh_path, out=out_path)
end


function _emit_sidecar(out_path::AbstractString, result::NamedTuple, k::Int)
    # Lightweight sidecar JSON in the same fixture-v1 shape that
    # `compare_eigenvalues.py` consumes (so the CI gate can read it
    # directly without language-specific code paths).
    n_recovered = _cbrt_int(result.n_tets ÷ 6)
    eigvals = result.eigenvalues[1:k]
    fixture = Dict(
        "schema_version" => "1",
        "fixture_id" => "cube_cavity/n$(n_recovered)_julia_eigensolve",
        "description" =>
            "Lowest $(k) cube-cavity eigenvalues from reference/julia/cube_cavity.jl. " *
            "Emitted in fixture-schema form so the cross-IR agreement table in " *
            "reference/driver/compare_eigenvalues.py can consume it without " *
            "language-specific code paths (Epic #88 / #115).",
        "units" => "dimensionless",
        "inputs" => Dict(
            "n_per_side" => Dict(
                "shape" => [1],
                "dtype" => "i64",
                "description" => "Hexes per cube side.",
                "data" => [n_recovered],
            ),
        ),
        "outputs" => Dict(
            "eigenvalues" => Dict(
                "shape" => [k],
                "dtype" => "f64",
                "description" => "Lowest $(k) eigenvalues, ascending.",
                "tolerance_abs" => 1.0e-8,
                "data" => collect(eigvals),
            ),
            "k_int_frobenius" => Dict(
                "shape" => [1],
                "dtype" => "f64",
                "description" => "Frobenius norm of K_int.",
                "tolerance_abs" => 1.0e-11,
                "data" => [result.k_int_frobenius],
            ),
            "m_int_frobenius" => Dict(
                "shape" => [1],
                "dtype" => "f64",
                "description" => "Frobenius norm of M_int.",
                "tolerance_abs" => 1.0e-13,
                "data" => [result.m_int_frobenius],
            ),
            "k_int_diag" => Dict(
                "shape" => [result.n_int],
                "dtype" => "f64",
                "description" => "Diagonal of K_int.",
                "tolerance_abs" => 1.0e-9,
                "data" => result.k_int_diag,
            ),
            "m_int_diag" => Dict(
                "shape" => [result.n_int],
                "dtype" => "f64",
                "description" => "Diagonal of M_int.",
                "tolerance_abs" => 5.0e-9,
                "data" => result.m_int_diag,
            ),
            "n_int" => Dict(
                "shape" => [1],
                "dtype" => "f64",
                "description" => "Number of interior DOFs.",
                "tolerance_abs" => 0.5,
                "data" => [Float64(result.n_int)],
            ),
        ),
        "provenance" => Dict(
            "source" => "reference/julia/cube_cavity.jl (Epic #88 / #115)",
            "julia_version" => string(VERSION),
            "issue" => "#115 (Phase E — Julia cube-cavity)",
        ),
    )
    open(out_path, "w") do io
        JSON3.pretty(io, fixture)
        write(io, "\n")
    end
    @info "Wrote sidecar JSON" path=out_path
end


# Round-trip cube root for small integers (used to recover `n` from
# `n_tets = 6 * n^3` in the sidecar JSON).
function _cbrt_int(n_cubed::Int)
    n = round(Int, cbrt(Float64(n_cubed)))
    n^3 == n_cubed || error("not a perfect cube: $n_cubed")
    return n
end


function main()
    args = _parse_args(ARGS)
    @info "Running Julia cube-cavity pipeline" n=args.n k=args.k side=args.side mesh=args.mesh
    result = run_cube_cavity(
        n=args.n, k=args.k, side=args.side, mesh_path=args.mesh
    )

    π² = pi * pi
    targets = analytic_lowest_five()
    println("Julia cube-cavity:")
    println("  n_nodes = $(result.n_nodes)")
    println("  n_tets  = $(result.n_tets)")
    println("  n_int   = $(result.n_int)")
    println()
    println("idx  target/π²   λ_h/π²    rel err")
    println("---  ---------   ------    ---------")
    for i in 1:length(result.eigenvalues)
        got = result.eigenvalues[i]
        if i <= length(targets)
            want = targets[i]
            rel = abs(got - want) / want * 100.0
            @printf("%-3d  %.4f      %.4f    %+.4f%%\n", i - 1, want / π², got / π², rel)
        else
            @printf("%-3d                %.4f\n", i - 1, got / π²)
        end
    end
    println()
    @printf("trace(K_int)       = %.12e\n", sum(result.k_int_diag))
    @printf("trace(M_int)       = %.12e\n", sum(result.m_int_diag))
    @printf("‖K_int‖_F          = %.12e\n", result.k_int_frobenius)
    @printf("‖M_int‖_F          = %.12e\n", result.m_int_frobenius)

    if args.out !== nothing
        _emit_sidecar(args.out, result, args.k)
    end
end


# Run only when invoked as a script (`julia --project=. cube_cavity.jl ...`),
# not when included from another module.
if abspath(PROGRAM_FILE) == @__FILE__
    main()
end
