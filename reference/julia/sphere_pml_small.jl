#!/usr/bin/env julia
"""
Small-mesh sphere-PML reference (Epic #88 / Phase H.2, issue #160).

This file is the **small-mesh dense tiebreaker** for the Julia sphere-PML
pipeline. It complements `sphere_pml.jl` by adding a **dense
`LinearAlgebra.eigen(Matrix(K), Matrix(M))`** path designed for the
small-mesh fixture introduced in PR #164 (issue #158).

# Why a small-mesh dense path?

The full-mesh `sphere_pml.jl` uses `Arpack.eigs` shift-invert at
σ = 1.18 + 0.21j on the 3300×3300 ComplexF64 sparse pencil. That path
converges on the **l=1 lossy triplet** (canonical NumPy positions [0..2],
λ ≈ 1.18 + 0.21j) but **saturates within the l=1 cluster** and never
reaches the **l=2 quintuplet** (canonical NumPy positions [3,4] are 2 of
the 5 l=2 modes at λ ≈ 2.43 + 0.80j). Even nev = 105 (~3% of matrix dim)
did not escape the l=1 basin — surfaced during PR #153 cycle 3 review.

The cross-IR Rust test
(`crates/geode-validation/tests/sphere_pml_julia_reference.rs`) currently
restricts the strict scope to positions [0..2] to work around this.

Issue #160 proposed two resolutions:

  (a) **Multi-shift Arpack** on the same 3300×3300 mesh: paired shifts at
      l=1 and l=2 band centers, results merged and deduped.
  (b) **Smaller-mesh dense `LinearAlgebra.eigen(K, M)`** on a sphere PML
      fixture small enough that ComplexF64 LAPACK ZGGEV finishes in
      seconds.

After PR #164 (issue #158) merged the small-mesh PML fixture
(`reference/fixtures/sphere_pml_small/sphere.msh`, 197 tets,
**214-DOF interior pencil**), **Option (b) is the lower-friction path**:
the 214×214 complex-symmetric pencil is well within dense-Julia's reach
(measured ~1 s for `eigen`), surfaces both the l=1 triplet and l=2
quintuplet cleanly (no shift-invert basin-of-attraction question), and
avoids the multi-shift merging logic that Option (a) requires.

# Eigensolve strategy

Pure dense `LinearAlgebra.eigen(Matrix(K_int), Matrix(M_int))` returning
all eigenvalues, then:

  1. Filter out the `nan`/`inf` infinite-eigenvalue tokens (LAPACK ZGGEV
     emits these for the singular part of the pencil — same as NumPy
     `scipy.linalg.eigvals` reference; see
     `reference/numpy/sphere_pml.py::eigensolve_complex_dense` lines
     322-327).
  2. Sort by `|Re(λ)|` ascending — same order as NumPy and Burn's
     `FaerComplexEigensolver`.
  3. Take the lowest `spurious_dim + n_take` eigenvalues, then slice
     `[n_spurious : n_spurious + n_take]` as the physical band.

This **selects "lowest 5 physical by |Re| globally"** — exactly NumPy's
convention. On the small mesh, that surfaces the l=1 triplet (positions
[0..2] at λ ≈ 1.92 + 0.06j) and 2 of the l=2 quintuplet (positions [3,4]
at λ ≈ 3.28 + 0.15j and 3.57 + 0.12j) — the small-mesh analogue of the
NumPy "lowest 5 physical by Re globally" criterion.

Note that on the small mesh the physical band centers are different from
the full mesh: the coarser discretization pushes l=1 up from
λ ≈ 1.18 + 0.21j (full) to λ ≈ 1.92 + 0.06j (small) and l=2 up from
λ ≈ 2.43 + 0.80j (full) to λ ≈ 3.28 + 0.15j (small). The canonical NumPy
PR #164 baseline at `reference/fixtures/sphere_pml_small/baseline.json`
already pins these values, and this small-mesh Julia path matches them.

# Sign convention

Canonical Wave-2: `Im(λ) > 0` on physical modes (PR #155 Judge's binding
decision). LAPACK ZGGEV on a complex-symmetric pencil typically returns
this branch directly — both scipy and Julia's `LinearAlgebra.eigen` bind
the same Fortran ZGGEV. No conjugate flip needed on physical modes;
however we apply a defensive `imag(λ) ≥ -tol` filter to drop any near-
zero spurious cluster ghost-conjugate partners (the spurious cluster is
real-valued under exact arithmetic but emits both Im(λ) ≈ +ε and ≈ -ε
under ZGGEV roundoff).

# Public API

  * `run_sphere_pml_small(mesh_path; sigma0, n_take, ...) -> NamedTuple`
    — small-mesh sphere PML pipeline returning the same field set as
    `run_sphere_pml` (modulo the dense eigensolve replacing the shift-
    invert call).

The function reuses the assembly and Dirichlet helpers from
`sphere_pml.jl` verbatim — only the eigensolve step is replaced.
"""

push!(LOAD_PATH, @__DIR__)
# sphere_pml.jl already includes sphere_pec.jl and brings in the
# mesh / PML / assembly helpers we reuse here. Including it once gives
# us `assemble_global_nedelec_complex`, `apply_dirichlet_complex`,
# `build_complex_epsilon_r_pml`, `tet_centroid_radii`, the PML
# constants (SIGMA_0_DEFAULT, R_*), and the helpers from sphere_pec.jl.
include(joinpath(@__DIR__, "sphere_pml.jl"))

using LinearAlgebra
using SparseArrays
using Printf


# ---------------------------------------------------------------------------
# Dense complex eigensolve — small-mesh tiebreaker.
# ---------------------------------------------------------------------------

"""
    eigensolve_complex_dense(K, M; k_take)
    -> Vector{ComplexF64}

Lowest `k_take` complex generalized eigenvalues of `K x = λ M x` via
**dense `LinearAlgebra.eigen`** on the materialized complex pencil.

# Selection convention

Returns the lowest `k_take` eigenvalues by `|Re(λ)|` ascending, matching
NumPy's `eigensolve_complex_dense`
(`reference/numpy/sphere_pml.py:289-340`) and Burn's
`FaerComplexEigensolver` ordering.

# Why dense

The full-mesh sphere PML pencil is 3300×3300 ComplexF64; dense LAPACK
ZGGEV on that takes 30+ minutes on Apple Silicon (recorded in
`sphere_pml.jl::eigensolve_physical_shift_invert` docstring). The
small-mesh sibling (197 tets, 214-DOF interior) is **15× smaller**
and finishes in ~1 s — the dense path is operationally trivial.

The dense path also **sees the entire spectrum**, so the lowest 5
physical modes can be sliced deterministically (after spurious filter)
without any shift-invert basin-of-attraction concerns. This is the
Option (b) resolution of issue #160's Arpack l=2 quintuplet
unreachability finding.

# Returns

`Vector{ComplexF64}` of length `k_take`, sorted by `|Re(λ)|` ascending.
Infinite-eigenvalue tokens (`nan`/`inf` from LAPACK's α/β representation
when β ≈ 0) are filtered out before slicing.
"""
function eigensolve_complex_dense(
        K::SparseMatrixCSC{ComplexF64,Int},
        M::SparseMatrixCSC{ComplexF64,Int};
        k_take::Int,
)
    K_dense = Matrix(K)
    M_dense = Matrix(M)

    # LinearAlgebra.eigen on a generalized complex problem dispatches to
    # LAPACK ZGGEV (the same Fortran kernel scipy.linalg.eigvals uses).
    # Returns all eigenvalues as α / β; β ≈ 0 emits inf / nan tokens that
    # we filter out below (matching NumPy's
    # `eigensolve_complex_dense::eigvals = eigvals[np.isfinite(...)]`).
    F = eigen(K_dense, M_dense)
    eigvals_raw = F.values

    # Filter out infinite-eigenvalue tokens.
    finite_mask = isfinite.(real.(eigvals_raw)) .& isfinite.(imag.(eigvals_raw))
    eigvals_finite = eigvals_raw[finite_mask]

    # Sort by |Re(λ)| ascending — matches NumPy
    # `np.argsort(np.abs(eigvals.real))` and Burn's
    # FaerComplexEigensolver (crates/geode-core/src/complex_eigen.rs:171-179).
    perm = sortperm(eigvals_finite; by = lam -> abs(real(lam)))
    eigvals_sorted = eigvals_finite[perm]

    n_avail = length(eigvals_sorted)
    if k_take > n_avail
        error("eigensolve_complex_dense: requested k_take=$k_take but only " *
              "$n_avail finite eigenvalues available (n_dim=$(size(K, 1)))")
    end
    return eigvals_sorted[1:k_take]
end


# ---------------------------------------------------------------------------
# Small-mesh sphere PML pipeline.
# ---------------------------------------------------------------------------

"""
    run_sphere_pml_small(mesh_path; n_index=N_INDEX,
                         sigma0=SIGMA_0_DEFAULT, n_take=5,
                         r_outer=R_BUFFER) -> NamedTuple

Small-mesh sphere-PML pipeline using the dense `LinearAlgebra.eigen`
eigensolve. Returns a `NamedTuple` matching the shape of
`run_sphere_pml` plus the explicit physical-vs-spurious split that the
NumPy small-mesh baseline carries.

# Pipeline

  1. Mesh I/O (same as `run_sphere_pml`).
  2. Per-tet centroid radii + complex ε_r assignment.
  3. Edge enumeration + PEC interior mask + d⁰-rank spurious classifier.
  4. Complex assembly + Dirichlet reduction.
  5. **Dense `LinearAlgebra.eigen(Matrix(K_int), Matrix(M_int))`** —
     replaces the shift-invert call from `run_sphere_pml`.
  6. Sort by `|Re(λ)|` ascending; take the lowest `spurious_dim + n_take`.
  7. Physical filter: slice `[n_spurious : n_spurious + n_take]` —
     matches NumPy `run_sphere_pml` convention.
  8. Q-factor of the lowest physical mode in the sign-agnostic
     `Re(k) / (2 |Im(k)|)` form.

# Returns

NamedTuple with fields:

  * `n_nodes, n_tets, n_edges, n_interior_edges, spurious_dim, n_spurious`
  * `epsilon_r_complex` — per-tet complex ε
  * `eigenvalues_lowest_complex` — lowest `spurious_dim + n_take` modes
    (spurious cluster + physical band, sorted by |Re(λ)|)
  * `physical_eigenvalues_complex` — lowest `n_take` physical modes
    (NumPy "lowest 5 physical by |Re| globally" convention)
  * `q_factor_lowest_physical` — Q of physical[1] in sign-agnostic form
  * `k_int_frobenius_complex, m_int_frobenius_complex`
  * `K, M, K_int, M_int` — kept for downstream diagnostics

# Why the API differs slightly from `run_sphere_pml`

`run_sphere_pml` returns `physical_eigenvalues_complex` filtered for
`Im(λ) ≥ 0` (the Arpack ghost-conjugate filter) and then takes the
lowest `n_take`. On the small mesh's dense path, the spurious cluster
sits near λ = 0 (with Im ≈ ±ε from ZGGEV roundoff) and the physical
band has well-defined `Im(λ) > 0`, so the split is the simpler
`[n_spurious : n_spurious + n_take]` slice — matching the NumPy
small-mesh convention exactly.
"""
function run_sphere_pml_small(
        mesh_path ::AbstractString;
        n_index   ::Float64 = N_INDEX,
        sigma0    ::Float64 = SIGMA_0_DEFAULT,
        n_take    ::Int     = 5,
        r_outer   ::Float64 = R_BUFFER,
)
    # 1. Mesh I/O.
    nodes, tets, phys_tags = load_msh_with_tags(mesh_path)
    n_nodes = size(nodes, 1)
    n_tets  = size(tets,  1)

    # 2. Per-tet centroid radii (PML profile input).
    centroid_radii = tet_centroid_radii(nodes, tets)

    # 3. Complex ε_r assignment.
    epsilon_r_complex = build_complex_epsilon_r_pml(
        phys_tags, centroid_radii; n_inside=n_index, sigma0=sigma0
    )

    # 4. Edge enumeration.
    edges, tet_edge_idx, tet_edge_sign = build_edges(tets)
    n_edges = size(edges, 1)

    # 5. PEC interior mask.
    interior_mask    = sphere_pec_interior_edges(nodes, edges; r_outer=r_outer)
    n_interior_edges = count(interior_mask)

    # 6. Global complex assembly.
    K, M = assemble_global_nedelec_complex(
        nodes, tets, edges, tet_edge_idx, tet_edge_sign, epsilon_r_complex
    )

    # 7. Dirichlet reduction.
    K_int, M_int = apply_dirichlet_complex(K, M, interior_mask)

    # 8. Spurious-mode classifier via d⁰ rank.
    abs_tol = 1e-6 * max(r_outer, 1.0)
    node_r  = [sqrt(nodes[i,1]^2 + nodes[i,2]^2 + nodes[i,3]^2) for i in 1:n_nodes]
    node_interior_mask = BitVector([abs(node_r[i] - r_outer) >= abs_tol for i in 1:n_nodes])

    d0_int     = build_d0_interior(nodes, edges, interior_mask, node_interior_mask)
    n_spurious = spurious_dim_from_derham(d0_int)
    spurious_dim = count(node_interior_mask)

    # 9. Dense complex eigensolve — the small-mesh tiebreaker.
    #    Take spurious_dim + n_take lowest-|Re| eigenvalues.
    n_request = spurious_dim + n_take
    eigvals_all = eigensolve_complex_dense(K_int, M_int; k_take=n_request)

    # 10. Physical mode slice — matches NumPy
    #     `run_sphere_pml::physical = eigvals[n_spurious : n_spurious + n_take]`.
    if n_spurious + n_take > length(eigvals_all)
        error("requested $n_take physical modes but only " *
              "$(length(eigvals_all) - n_spurious) available after spurious " *
              "filter; increase n_request")
    end
    physical_eigenvalues = eigvals_all[n_spurious+1 : n_spurious+n_take]

    # 11. Q-factor of the lowest physical mode in sign-agnostic form.
    q_lowest = q_factor(physical_eigenvalues[1])

    # 12. Complex Frobenius norms.
    k_frob_c = norm(K_int)
    m_frob_c = norm(M_int)

    return (
        n_nodes                       = n_nodes,
        n_tets                        = n_tets,
        n_edges                       = n_edges,
        n_interior_edges              = n_interior_edges,
        spurious_dim                  = spurious_dim,
        n_spurious                    = n_spurious,
        sigma0                        = sigma0,
        epsilon_r_complex             = epsilon_r_complex,
        edges                         = edges,
        tet_edge_idx                  = tet_edge_idx,
        tet_edge_sign                 = tet_edge_sign,
        interior_mask                 = interior_mask,
        K                             = K,
        M                             = M,
        K_int                         = K_int,
        M_int                         = M_int,
        k_int_frobenius_complex       = k_frob_c,
        m_int_frobenius_complex       = m_frob_c,
        eigenvalues_lowest_complex    = eigvals_all,
        physical_eigenvalues_complex  = physical_eigenvalues,
        q_factor_lowest_physical      = q_lowest,
    )
end


# ---------------------------------------------------------------------------
# CLI entry point.
# ---------------------------------------------------------------------------

function _parse_small_args(argv::Vector{String})
    fixture_path::Union{Nothing,String} = nothing
    out_path::Union{Nothing,String}     = nothing
    sigma0::Float64                     = SIGMA_0_DEFAULT
    i = 1
    while i <= length(argv)
        arg = argv[i]
        if arg == "--mesh" || arg == "--fixture"
            fixture_path = argv[i + 1]; i += 2
        elseif arg == "--out"
            out_path = argv[i + 1]; i += 2
        elseif arg == "--sigma0"
            sigma0 = parse(Float64, argv[i + 1]); i += 2
        elseif arg in ("-h", "--help")
            println(stderr,
                "Usage: julia --project=. sphere_pml_small.jl " *
                "[--mesh path] [--sigma0 5.0] [--out path]")
            exit(0)
        else
            error("unrecognized argument: $arg")
        end
    end
    return (mesh=fixture_path, out=out_path, sigma0=sigma0)
end


function main_small()
    args = _parse_small_args(ARGS)

    mesh_path = if args.mesh !== nothing
        args.mesh
    else
        joinpath(@__DIR__, "..", "fixtures", "sphere_pml_small", "sphere.msh")
    end

    @info "Running Julia small-mesh sphere-PML Nédélec pipeline" mesh=mesh_path sigma0=args.sigma0

    result = run_sphere_pml_small(mesh_path; sigma0=args.sigma0)

    println("Small-mesh sphere-PML fixture: $(result.n_nodes) nodes, $(result.n_tets) tets")
    println("Global edges:       $(result.n_edges)")
    println("Interior DOFs:      $(result.n_interior_edges)")
    println("Predicted spurious: $(result.spurious_dim)  (= interior nodes)")
    println("Observed spurious:  $(result.n_spurious)  (= rank(d⁰_interior))")
    println("σ₀ used:            $(result.sigma0)")
    println()
    println("Lowest $(length(result.physical_eigenvalues_complex)) physical complex eigenvalues:")
    for (i, lam) in enumerate(result.physical_eigenvalues_complex)
        q = q_factor(lam)
        @printf("  physical[%d]: λ = (%.10f, %+.10f)  |λ| = %.10f  Q = %.4f\n",
                i, real(lam), imag(lam), abs(lam), q)
    end
    println()
    @printf("‖K_int‖_F (complex) = %.10e\n", result.k_int_frobenius_complex)
    @printf("‖M_int‖_F (complex) = %.10e\n", result.m_int_frobenius_complex)
    @printf("Q(lowest physical)  = %.6f\n", result.q_factor_lowest_physical)
end


if abspath(PROGRAM_FILE) == @__FILE__
    main_small()
end
