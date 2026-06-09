#!/usr/bin/env julia
"""
Small-mesh anisotropic-UPML dielectric-sphere Mie reference (Epic #88 /
Phase J.3, issue #172).

Julia port of `reference/numpy/sphere_mie.py` (issue #171 / PR #179)
restricted to the small-mesh dense path: the per-tet **diagonal complex
UPML tensor** (`geode_core::build_anisotropic_pml_tensor_diag`) feeding
the per-axis anisotropic Nédélec mass kernel
(`geode_core::batched_nedelec_local_mass_anisotropic_diag`), solved
**dense** via `LinearAlgebra.eigen(Matrix(K_int), Matrix(M_int))` on the
197-tet / 214-DOF small-mesh fixture
(`reference/fixtures/sphere_pml_small/sphere.msh`).

# Why dense (and not Arpack shift-invert)

Per the #160 finding, windowed shift-invert selection **saturates in
lossy clusters** and cannot honestly produce "lowest N by Re" on
dispersive spectra — the full-mesh Julia PML path needed nev ≥ 100 and
still couldn't escape the l=1 basin. The small-mesh dense path sees the
entire spectrum, so the canonical "lowest spurious_dim + 8 by |Re(λ)|"
slice is deterministic. The dense ZGGEV on the 214×214 complex pencil
takes ~1 s.

# Anisotropic tensor profile (mirror of build_anisotropic_pml_tensor_diag)

  * Tet in `sphere_interior` (tag 1): real isotropic `(n², n², n²)`.
  * Tet in `vacuum_gap` (any tag other than PHYS_PML_SHELL), or a
    PML-shell tet whose centroid sits at `r_c ≤ R_PML_INNER`: real
    isotropic `(1, 1, 1)`.
  * Tet in `pml_shell` (tag 5) with `r_c > R_PML_INNER`: simplified
    Sacks UPML with `s_r = s_t = s = 1 − jσ(r_c)/ω`:

        σ(r_c) = σ₀ · clamp((r_c − R_PML_INNER) / (R_BUFFER − R_PML_INNER), 0, 1)²
        ε_α    = (1/s) r̂_α² + s (1 − r̂_α²),   r̂ = c / |c|,  α ∈ {x, y, z}

    `ω` approximated by `max(k0_ref, 1e-12)`. For this profile
    `s_r = s_t`, so the diagonal-only kernel is exact (the full Sacks
    tensor's off-diagonals vanish identically).

# Per-axis anisotropic mass kernel

The scalar Whitney mass formula holds per Cartesian component with the
cofactor gram `gg_pq = g_p · g_q` replaced by the per-axis product
`gg^(α)_pq = g_p[α] g_q[α]`:

    M_ij = Σ_α ε_α / (120 |det|) [  (1+δ_ac) gg^(α)_bd − (1+δ_ad) gg^(α)_bc
                                  − (1+δ_bc) gg^(α)_ad + (1+δ_bd) gg^(α)_ac ]

Since `Σ_α gg^(α)_pq = gg_pq`, equal weights collapse to exactly ε ×
the scalar mass — the σ₀ = 0 isotropic-collapse regression.

# Sign conventions

`exp(+jωt)` throughout; `Im(ε) < 0` in the shell. Note (PR #179 schema
doc): unlike the scalar-isotropic PML (`Im(λ) > 0`, PR #155), the
anisotropic tensor pencil's physical `Im(λ)` sign is **mesh-dependent**
— `Im(λ) < 0` on this small mesh (the radial entry carries `1/s_r` with
`Im > 0` while the transverse entries carry `s_t` with `Im < 0`; the
net sign is a property of the pencil, agreed on by LAPACK ZGGEV and
faer QZ). Q stays sign-agnostic: `Q = Re(k) / (2|Im(k)|)`, `k = √λ` on
the `Re(k) ≥ 0` branch.

# Julia ergonomics note (Epic #88 principle 4)

The tensor builder is the same arithmetic as the Burn/NumPy sides but
with `ComplexF64` as a plain scalar type: `s = 1.0 - im * (sigma/omega)`
and `inv(s)` need no dtype ceremony, and the per-axis kernel accumulates
`ComplexF64` 6×6 blocks directly. No new complex-arithmetic friction
surfaced beyond what H.2 already recorded.

# Public API

  * `tet_centroids(nodes, tets) -> Matrix{Float64}` (n_tets × 3)
  * `build_anisotropic_pml_tensor_diag(phys_tags, centroids; ...)`
    -> `Matrix{ComplexF64}` (n_tets × 3)
  * `nedelec_local_mass_anisotropic_diag(nodes_tet, eps_diag_t)`
    -> 6×6 `Matrix{ComplexF64}`
  * `assemble_global_nedelec_anisotropic(...) -> (K, M)` complex sparse
  * `lambda_to_k(λ) -> ComplexF64` (principal branch)
  * `run_sphere_mie_small(mesh_path; ...) -> NamedTuple`

Reuses `eigensolve_complex_dense`, `apply_dirichlet_complex`,
`q_factor`, the mesh / edge / PEC-mask / d⁰-rank helpers, and the
geometry constants from the H.2 stack via the include chain below.
"""

push!(LOAD_PATH, @__DIR__)
# sphere_pml_small.jl includes sphere_pml.jl which includes sphere_pec.jl
# (mesh I/O, build_edges, PEC mask, d⁰ classifier, nedelec_local_cc_mass,
# R_* / PHYS_* constants) — one include gives us the whole H.2 stack plus
# the dense eigensolver.
include(joinpath(@__DIR__, "sphere_pml_small.jl"))

using LinearAlgebra
using SparseArrays
using Printf


# ---------------------------------------------------------------------------
# UPML constants — mirror of crates/geode-core/tests/mie_sphere.rs.
# ---------------------------------------------------------------------------

const K0_REF_DEFAULT::Float64 = 2.0
"""Reference wavenumber ω heuristic in the UPML stretch s = 1 − jσ(r)/ω.
Mirror of `K0_REF` in `crates/geode-core/tests/mie_sphere.rs`."""


# ---------------------------------------------------------------------------
# Per-tet centroids — mirror of geode_core::tet_centroids (full vector;
# the tensor builder needs the radial *direction*, not just |c| — see
# tet_centroid_radii in sphere_pml.jl for the scalar sibling).
# ---------------------------------------------------------------------------

"""
    tet_centroids(nodes, tets) -> Matrix{Float64}

Per-tet centroid positions, shape `(n_tets, 3)`. Mirror of
`geode_core::tet_centroids` / `reference/numpy/sphere_mie.py::tet_centroids`.
"""
function tet_centroids(nodes::Matrix{Float64}, tets::Matrix{Int})
    n_tets = size(tets, 1)
    centroids = Matrix{Float64}(undef, n_tets, 3)
    @inbounds for t in 1:n_tets
        for a in 1:3
            centroids[t, a] = 0.25 * (nodes[tets[t, 1], a] + nodes[tets[t, 2], a] +
                                      nodes[tets[t, 3], a] + nodes[tets[t, 4], a])
        end
    end
    return centroids
end


# ---------------------------------------------------------------------------
# Anisotropic UPML diagonal tensor — mirror of
# geode_core::build_anisotropic_pml_tensor_diag.
# ---------------------------------------------------------------------------

"""
    build_anisotropic_pml_tensor_diag(phys_tags, centroids;
                                      n_inside=N_INDEX, sigma0=SIGMA_0_DEFAULT,
                                      k0_ref=K0_REF_DEFAULT)
    -> Matrix{ComplexF64}  (n_tets × 3)

Per-tet diagonal anisotropic complex permittivity tensor `(ε_x, ε_y, ε_z)`
in the global Cartesian basis. Line-for-line mirror of
`geode_core::build_anisotropic_pml_tensor_diag` (issue #54) — see the
module docstring for the profile.
"""
function build_anisotropic_pml_tensor_diag(
        phys_tags ::Vector{Int},
        centroids ::Matrix{Float64};
        n_inside  ::Float64 = N_INDEX,
        sigma0    ::Float64 = SIGMA_0_DEFAULT,
        k0_ref    ::Float64 = K0_REF_DEFAULT,
)
    n_tets = length(phys_tags)
    @assert size(centroids) == (n_tets, 3) "centroids shape mismatch"

    eps_inside = n_inside * n_inside
    width = R_BUFFER - R_PML_INNER
    omega = max(k0_ref, 1e-12)

    eps_diag = Matrix{ComplexF64}(undef, n_tets, 3)
    @inbounds for t in 1:n_tets
        cx, cy, cz = centroids[t, 1], centroids[t, 2], centroids[t, 3]
        r_c = sqrt(cx * cx + cy * cy + cz * cz)

        # Background scalar: n² in the dielectric, 1 elsewhere.
        bg = (phys_tags[t] == PHYS_SPHERE_INTERIOR) ? eps_inside : 1.0

        if phys_tags[t] == PHYS_PML_SHELL && r_c > R_PML_INNER
            u = clamp((r_c - R_PML_INNER) / width, 0.0, 1.0)
            sigma = sigma0 * u * u
            s = 1.0 - im * (sigma / omega)   # s_r = s_t = 1 − jσ/ω
            s_inv = inv(s)
            # Radial unit vector at the centroid (guarded |c| ≈ 0,
            # matching the Burn-side defensive branch).
            inv_r = r_c > 1e-12 ? inv(r_c) : 0.0
            rx, ry, rz = cx * inv_r, cy * inv_r, cz * inv_r
            # ε_α = bg · (s_inv r̂_α² + s (1 − r̂_α²))
            for (a, w) in ((1, rx * rx), (2, ry * ry), (3, rz * rz))
                eps_diag[t, a] = bg * (s_inv * w + s * (1.0 - w))
            end
        else
            # Interior, vacuum gap, or the defensive r_c ≤ R_PML_INNER
            # guard inside the shell: real isotropic.
            eps_diag[t, 1] = ComplexF64(bg, 0.0)
            eps_diag[t, 2] = ComplexF64(bg, 0.0)
            eps_diag[t, 3] = ComplexF64(bg, 0.0)
        end
    end
    return eps_diag
end


# ---------------------------------------------------------------------------
# Anisotropic local mass — mirror of
# geode_core::batched_nedelec_local_mass_anisotropic_diag (single element).
# ---------------------------------------------------------------------------

"""
    nedelec_local_mass_anisotropic_diag(nodes_tet, eps_diag_t)
    -> Matrix{ComplexF64}  (6 × 6)

Per-element Nédélec local mass under a diagonal permittivity tensor
`eps_diag_t = (ε_x, ε_y, ε_z)`. Same cofactor machinery as
`nedelec_local_cc_mass` (sphere_pec.jl) with the gram contracted per
axis — see the module docstring for the formula. All local edge signs
are +1; the global `s_i s_j` correction is the caller's responsibility.
"""
function nedelec_local_mass_anisotropic_diag(
        nodes_tet ::Matrix{Float64},
        eps_diag_t::AbstractVector{ComplexF64},
)
    @assert length(eps_diag_t) == 3 "eps_diag_t must be (ε_x, ε_y, ε_z)"

    v0 = @view nodes_tet[1, :]
    v1 = @view nodes_tet[2, :]
    v2 = @view nodes_tet[3, :]
    v3 = @view nodes_tet[4, :]

    e1 = v1 .- v0
    e2 = v2 .- v0
    e3 = v3 .- v0

    g1 = cross(e2, e3)
    g2 = cross(e3, e1)
    g3 = cross(e1, e2)
    g0 = -(g1 .+ g2 .+ g3)

    det = dot(e1, g1)
    inv_abs_det = 1.0 / abs(det)

    # g_mat[p, a] = (g_{p-1})_a — same layout as nedelec_local_cc_mass.
    g_mat = Matrix{Float64}(undef, 4, 3)
    g_mat[1, :] .= g0
    g_mat[2, :] .= g1
    g_mat[3, :] .= g2
    g_mat[4, :] .= g3

    M_local = zeros(ComplexF64, 6, 6)
    for (i, (a, b)) in enumerate(TET_LOCAL_EDGES)
        for (j, (c, d)) in enumerate(TET_LOCAL_EDGES)
            f_ac = (a == c) ? 2.0 : 1.0
            f_ad = (a == d) ? 2.0 : 1.0
            f_bc = (b == c) ? 2.0 : 1.0
            f_bd = (b == d) ? 2.0 : 1.0
            # Per-axis Kronecker-lifted term, weighted by ε_α and summed.
            acc = zero(ComplexF64)
            @inbounds for axis in 1:3
                gg_bd = g_mat[b, axis] * g_mat[d, axis]
                gg_bc = g_mat[b, axis] * g_mat[c, axis]
                gg_ad = g_mat[a, axis] * g_mat[d, axis]
                gg_ac = g_mat[a, axis] * g_mat[c, axis]
                m_term = f_ac * gg_bd - f_ad * gg_bc - f_bc * gg_ad + f_bd * gg_ac
                acc += m_term * eps_diag_t[axis]
            end
            M_local[i, j] = acc * inv_abs_det / 120.0
        end
    end
    return M_local
end


# ---------------------------------------------------------------------------
# Global assembly with the diagonal tensor ε.
# ---------------------------------------------------------------------------

"""
    assemble_global_nedelec_anisotropic(nodes, tets, edges, tet_edge_idx,
                                        tet_edge_sign, eps_tensor_diag)
    -> (K, M) :: (SparseMatrixCSC{ComplexF64,Int}, SparseMatrixCSC{ComplexF64,Int})

Assemble global Nédélec stiffness K (real-valued, typed complex) and the
tensor-ε complex mass M. Mirror of
`geode_core::assemble_global_nedelec_with_anisotropic_epsilon` /
`reference/numpy/sphere_mie.py::assemble_global_nedelec_anisotropic`.
The per-element curl-curl is reused from `nedelec_local_cc_mass`
(ε-independent); the per-element mass picks up the diagonal tensor.
"""
function assemble_global_nedelec_anisotropic(
        nodes          ::Matrix{Float64},
        tets           ::Matrix{Int},
        edges          ::Matrix{Int},
        tet_edge_idx   ::Matrix{Int},
        tet_edge_sign  ::Matrix{Int},
        eps_tensor_diag::Matrix{ComplexF64},
)
    n_edges = size(edges, 1)
    n_tets  = size(tets,  1)
    @assert size(eps_tensor_diag) == (n_tets, 3)

    nnz_est = 36 * n_tets
    I_idx  = Vector{Int}(undef, nnz_est)
    J_idx  = Vector{Int}(undef, nnz_est)
    V_cc   = Vector{ComplexF64}(undef, nnz_est)
    V_mass = Vector{ComplexF64}(undef, nnz_est)

    p = 1
    for t in 1:n_tets
        nodes_tet = nodes[tets[t, :], :]
        # Stiffness from the scalar kernel (curl-curl is ε-independent;
        # the scalar M_local is discarded).
        K_local, _ = nedelec_local_cc_mass(nodes_tet)
        # Mass from the per-axis anisotropic kernel.
        M_local = nedelec_local_mass_anisotropic_diag(
            nodes_tet, @view eps_tensor_diag[t, :]
        )

        for le_i in 1:6
            gi = tet_edge_idx[t, le_i]
            si = tet_edge_sign[t, le_i]
            for le_j in 1:6
                gj = tet_edge_idx[t, le_j]
                sj = tet_edge_sign[t, le_j]
                s = Float64(si * sj)
                I_idx[p]  = gi
                J_idx[p]  = gj
                V_cc[p]   = ComplexF64(s * K_local[le_i, le_j], 0.0)
                V_mass[p] = s * M_local[le_i, le_j]
                p += 1
            end
        end
    end

    K = sparse(I_idx, J_idx, V_cc,   n_edges, n_edges, +)
    M = sparse(I_idx, J_idx, V_mass, n_edges, n_edges, +)
    return K, M
end


# ---------------------------------------------------------------------------
# λ → k helper (principal branch with sign(Im k) = sign(Im λ)).
# ---------------------------------------------------------------------------

"""
    lambda_to_k(lam) -> ComplexF64

`k = √λ` on the principal branch `Re(k) ≥ 0`, with
`sign(Im(k)) = sign(Im(λ))` — mirror of the λ → k conversion in
`crates/geode-core/tests/mie_sphere.rs` and
`reference/numpy/sphere_mie.py::lambda_to_k`.
"""
function lambda_to_k(lam::ComplexF64)
    r = abs(lam)
    re_k = sqrt(max(0.5 * (r + real(lam)), 0.0))
    im_k_mag = sqrt(max(0.5 * (r - real(lam)), 0.0))
    im_k = imag(lam) >= 0.0 ? im_k_mag : -im_k_mag
    return ComplexF64(re_k, im_k)
end


# ---------------------------------------------------------------------------
# Small-mesh anisotropic-UPML Mie pipeline.
# ---------------------------------------------------------------------------

"""
    run_sphere_mie_small(mesh_path; n_index=N_INDEX, sigma0=SIGMA_0_DEFAULT,
                         k0_ref=K0_REF_DEFAULT, n_take=5, r_outer=R_BUFFER)
    -> NamedTuple

Full anisotropic-UPML Mie pipeline on the small mesh — mirror of
`reference/numpy/sphere_mie.py::run_sphere_mie` with the dense Julia
eigensolve (`eigensolve_complex_dense` from sphere_pml_small.jl).

# Pipeline

  1. Mesh I/O + per-tet centroids (vector).
  2. Diagonal UPML tensor `build_anisotropic_pml_tensor_diag`.
  3. Edge enumeration + PEC interior mask + d⁰-rank spurious classifier.
  4. Tensor-ε complex assembly + Dirichlet reduction.
  5. Dense eigensolve, lowest `spurious_dim + 8` by `|Re(λ)|` (the NumPy
     `n_request` convention — note `+ 8`, not `+ n_take`, so the fixture
     slice matches the NumPy `eigenvalues_lowest_complex` shape).
  6. Physical slice `[n_spurious : n_spurious + n_take]`.
  7. `k = √λ`, Q of the lowest mode, M complex-symmetry residual,
     σ₀ = 0 regression metric.
"""
function run_sphere_mie_small(
        mesh_path ::AbstractString;
        n_index   ::Float64 = N_INDEX,
        sigma0    ::Float64 = SIGMA_0_DEFAULT,
        k0_ref    ::Float64 = K0_REF_DEFAULT,
        n_take    ::Int     = 5,
        r_outer   ::Float64 = R_BUFFER,
)
    # 1. Mesh I/O.
    nodes, tets, phys_tags = load_msh_with_tags(mesh_path)
    n_nodes = size(nodes, 1)
    n_tets  = size(tets,  1)

    # 2. Per-tet centroids + diagonal UPML tensor.
    centroids = tet_centroids(nodes, tets)
    eps_tensor_diag = build_anisotropic_pml_tensor_diag(
        phys_tags, centroids; n_inside=n_index, sigma0=sigma0, k0_ref=k0_ref
    )

    # 3. Edge enumeration + PEC interior mask.
    edges, tet_edge_idx, tet_edge_sign = build_edges(tets)
    n_edges = size(edges, 1)
    interior_mask    = sphere_pec_interior_edges(nodes, edges; r_outer=r_outer)
    n_interior_edges = count(interior_mask)

    # 4. Tensor-ε global assembly + Dirichlet reduction.
    K, M = assemble_global_nedelec_anisotropic(
        nodes, tets, edges, tet_edge_idx, tet_edge_sign, eps_tensor_diag
    )
    K_int, M_int = apply_dirichlet_complex(K, M, interior_mask)

    # 5. Spurious-mode classifier via d⁰ rank — invariant under the
    #    tensor-ε scaling on the mass.
    abs_tol = 1e-6 * max(r_outer, 1.0)
    node_r  = [sqrt(nodes[i,1]^2 + nodes[i,2]^2 + nodes[i,3]^2) for i in 1:n_nodes]
    node_interior_mask = BitVector([abs(node_r[i] - r_outer) >= abs_tol for i in 1:n_nodes])

    d0_int       = build_d0_interior(nodes, edges, interior_mask, node_interior_mask)
    n_spurious   = spurious_dim_from_derham(d0_int)
    spurious_dim = count(node_interior_mask)

    # 6. Dense complex eigensolve — NumPy n_request = spurious_dim + 8.
    n_request   = spurious_dim + 8
    eigvals_all = eigensolve_complex_dense(K_int, M_int; k_take=n_request)

    # 7. Physical slice [n_spurious : n_spurious + n_take].
    if n_spurious + n_take > length(eigvals_all)
        error("requested $n_take physical modes but only " *
              "$(length(eigvals_all) - n_spurious) available after spurious " *
              "filter; increase n_request")
    end
    physical_eigenvalues = eigvals_all[n_spurious+1 : n_spurious+n_take]
    physical_ks = [lambda_to_k(lam) for lam in physical_eigenvalues]
    q_lowest = q_factor(physical_eigenvalues[1])

    # σ₀ = 0 regression metric over the full returned slice.
    max_abs_re = max(maximum(abs.(real.(eigvals_all))), 1.0)
    max_imag_rel = maximum(abs.(imag.(eigvals_all))) / max_abs_re

    # Complex-symmetry residual on the interior mass — the diagonal
    # tensor weights per-axis blocks that are individually symmetric in
    # (i, j), so M must stay complex-symmetric (not Hermitian).
    sym_diff = M_int - transpose(M_int)
    m_sym_residual = nnz(sym_diff) > 0 ? maximum(abs.(nonzeros(sym_diff))) : 0.0

    return (
        n_nodes                      = n_nodes,
        n_tets                       = n_tets,
        n_edges                      = n_edges,
        n_interior_edges             = n_interior_edges,
        spurious_dim                 = spurious_dim,
        n_spurious                   = n_spurious,
        sigma0                       = sigma0,
        n_index                      = n_index,
        k0_ref                       = k0_ref,
        epsilon_tensor_diag          = eps_tensor_diag,
        edges                        = edges,
        tet_edge_idx                 = tet_edge_idx,
        tet_edge_sign                = tet_edge_sign,
        interior_mask                = interior_mask,
        K_int                        = K_int,
        M_int                        = M_int,
        k_int_frobenius_complex      = norm(K_int),
        m_int_frobenius_complex      = norm(M_int),
        eigenvalues_lowest_complex   = eigvals_all,
        physical_eigenvalues_complex = physical_eigenvalues,
        physical_ks                  = physical_ks,
        q_factor_lowest_physical     = q_lowest,
        m_int_complex_symmetry_residual = m_sym_residual,
        max_imag_eigval_rel          = max_imag_rel,
    )
end


# ---------------------------------------------------------------------------
# CLI entry point.
# ---------------------------------------------------------------------------

function _parse_mie_small_args(argv::Vector{String})
    fixture_path::Union{Nothing,String} = nothing
    sigma0::Float64 = SIGMA_0_DEFAULT
    k0_ref::Float64 = K0_REF_DEFAULT
    i = 1
    while i <= length(argv)
        arg = argv[i]
        if arg == "--mesh" || arg == "--fixture"
            fixture_path = argv[i + 1]; i += 2
        elseif arg == "--sigma0"
            sigma0 = parse(Float64, argv[i + 1]); i += 2
        elseif arg == "--k0-ref"
            k0_ref = parse(Float64, argv[i + 1]); i += 2
        elseif arg in ("-h", "--help")
            println(stderr,
                "Usage: julia --project=. sphere_mie_small.jl " *
                "[--mesh path] [--sigma0 5.0] [--k0-ref 2.0]")
            exit(0)
        else
            error("unrecognized argument: $arg")
        end
    end
    return (mesh=fixture_path, sigma0=sigma0, k0_ref=k0_ref)
end


function main_mie_small()
    args = _parse_mie_small_args(ARGS)

    mesh_path = if args.mesh !== nothing
        args.mesh
    else
        joinpath(@__DIR__, "..", "fixtures", "sphere_pml_small", "sphere.msh")
    end

    @info "Running Julia small-mesh anisotropic-UPML Mie pipeline" mesh=mesh_path sigma0=args.sigma0 k0_ref=args.k0_ref

    result = run_sphere_mie_small(mesh_path; sigma0=args.sigma0, k0_ref=args.k0_ref)

    println("Small-mesh Mie fixture: $(result.n_nodes) nodes, $(result.n_tets) tets")
    println("Global edges:       $(result.n_edges)")
    println("Interior DOFs:      $(result.n_interior_edges)")
    println("Predicted spurious: $(result.spurious_dim)  (= interior nodes)")
    println("Observed spurious:  $(result.n_spurious)  (= rank(d⁰_interior))")
    @printf("σ₀ = %.2f, k₀_ref = %.2f; max|Im|/max|Re| over slice = %.3e\n",
            result.sigma0, result.k0_ref, result.max_imag_eigval_rel)
    @printf("M complex-symmetry residual: %.3e\n",
            result.m_int_complex_symmetry_residual)
    println()
    println("Lowest $(length(result.physical_eigenvalues_complex)) physical modes (λ = k²):")
    for (i, (lam, k)) in enumerate(zip(result.physical_eigenvalues_complex,
                                       result.physical_ks))
        @printf("  [%d] λ = %+.6e %+.6ej   k = %.5f %+.5fj   Q = %.4f\n",
                i, real(lam), imag(lam), real(k), imag(k), q_factor(lam))
    end
    println()
    @printf("Q(lowest physical) = %.6f\n", result.q_factor_lowest_physical)
end


if abspath(PROGRAM_FILE) == @__FILE__
    main_mie_small()
end
