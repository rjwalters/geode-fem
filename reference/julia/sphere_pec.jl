#!/usr/bin/env julia
"""
Julia reference for the sphere-PEC Nédélec eigenmode pipeline (Epic #88 / Phase G.4).

Issue #129. Mirrors `reference/numpy/sphere_pec.py` algorithmically; uses the
same Float64 real-symmetric formulation as the NumPy reference (sphere PEC is
real-valued). The Julia toolchain track (Arpack.jl + SparseArrays) is the
complex-arithmetic reference backend per Epic #88 principle 4 — this phase lands
it for the Nédélec pipeline in preparation for Phase H (PML, Complex{Float64}).

Algorithmic structure:
  1. Mesh I/O — load the bundled `sphere.msh` via `CubeMesh.load_msh_with_tags`,
     which extends `load_msh` to return per-tet physical group tags.
  2. ε_r assignment — per-tet relative permittivity: `n²` inside `sphere_interior`
     (tag 1), `1.0` elsewhere (`vacuum_gap` tag 2, `pml_shell` tag 5).
  3. Edge enumeration — `CubeMesh.build_edges` returns globally-oriented
     `edges [n_edges, 2]`, `tet_edge_idx [n_tets, 6]`, `tet_edge_sign [n_tets, 6]`.
  4. PEC wall mask — `CubeMesh.sphere_pec_interior_edges` marks edges with both
     endpoints on the outer sphere (`r ≈ R_BUFFER = 2.0`) for elimination.
  5. Global assembly — COO scatter of per-tet 6×6 Nédélec local matrices into
     `(n_edges, n_edges)` sparse K and M via `SparseArrays.sparse(..., +)`.
  6. Dirichlet reduction — restrict K, M to interior-edge rows/cols.
  7. Spurious-mode classifier — `rank(d⁰_interior)` via SVD with relative cutoff
     `1e-12 × σ_max`. Mirrors `geode_core::spurious_dim_from_derham` and
     `reference/numpy/sphere_pec.py::spurious_dim_from_derham` (Issue #124).
  8. Eigensolve — `Arpack.eigs(K_int, M_int; nev=n_request, which=:SM)` in
     regular-inverse mode (no sigma). See cube_cavity.jl for the Arpack.jl 0.5
     calling-convention note; the same `which=:SM` workaround applies here.

Physical constants (mirror of geode_core::mesh::sphere):
  R_SPHERE    = 1.0    (dielectric sphere radius)
  R_PML_INNER = 1.5    (PML inner interface, unused in this phase)
  R_BUFFER    = 2.0    (outer PEC wall radius)
  N_INDEX     = 1.5    (refractive index inside sphere_interior)

Usage:
  julia --project=. sphere_pec.jl [--mesh path/to/sphere.msh]
"""

push!(LOAD_PATH, @__DIR__)
include(joinpath(@__DIR__, "mesh.jl"))

using .CubeMesh: load_msh_with_tags, build_edges, sphere_pec_interior_edges
using LinearAlgebra
using SparseArrays
using Arpack
using Printf


# ---------------------------------------------------------------------------
# Physical constants — mirror of geode_core::mesh::sphere constants.
# ---------------------------------------------------------------------------

const R_SPHERE::Float64    = 1.0
const R_PML_INNER::Float64 = 1.5
const R_BUFFER::Float64    = 2.0
const N_INDEX::Float64     = 1.5

# Physical group tags from sphere.msh $PhysicalNames:
const PHYS_SPHERE_INTERIOR::Int = 1   # 3D: tets with r ≤ R_SPHERE
const PHYS_VACUUM_GAP::Int      = 2   # 3D: tets with R_SPHERE < r ≤ R_PML_INNER
const PHYS_PML_SHELL::Int       = 5   # 3D: tets with R_PML_INNER < r ≤ R_BUFFER

# Local edge ordering on a tet — 1-indexed local vertex pairs, matching
# Python's TET_LOCAL_EDGES = [(0,1),(0,2),(0,3),(1,2),(1,3),(2,3)] shifted +1.
const TET_LOCAL_EDGES = ((1,2),(1,3),(1,4),(2,3),(2,4),(3,4))


# ---------------------------------------------------------------------------
# ε_r assignment — mirror of geode_core::build_epsilon_r.
# ---------------------------------------------------------------------------

"""
    build_epsilon_r(phys_tags; n_inside=N_INDEX) -> Vector{Float64}

Per-tet relative permittivity: `n_inside^2` for tets tagged
`PHYS_SPHERE_INTERIOR` (= 1), `1.0` for all others.

Mirror of `reference/numpy/sphere_pec.py::build_epsilon_r`.
"""
function build_epsilon_r(phys_tags::Vector{Int}; n_inside::Float64=N_INDEX)
    n_tets = length(phys_tags)
    eps_inside = n_inside * n_inside
    epsilon_r = Vector{Float64}(undef, n_tets)
    @inbounds for t in 1:n_tets
        epsilon_r[t] = (phys_tags[t] == PHYS_SPHERE_INTERIOR) ? eps_inside : 1.0
    end
    return epsilon_r
end


# ---------------------------------------------------------------------------
# Nédélec local matrices — per-tet 6×6 curl-curl and mass.
# Faithful port of reference/numpy/nedelec_local_matrices.py.
# ---------------------------------------------------------------------------

"""
    nedelec_local_cc_mass(nodes_tet) -> (K_local, M_local)

Compute the 6×6 Nédélec curl-curl stiffness and mass matrices for a single
tetrahedron.

`nodes_tet` is a 4×3 matrix of vertex coordinates (rows = vertices,
columns = x/y/z).

Mirrors `reference/numpy/nedelec_local_matrices.py::batched_nedelec_local_matrices`
for a single element. Named intermediates match the symbols in
`crates/geode-core/src/nedelec.rs` and the NumPy reference one-for-one:

  e1, e2, e3     — edge vectors from v0
  g1, g2, g3     — area-weighted gradients (cofactors)
  g0             — -(g1 + g2 + g3)
  det            — det(J) = e1 · g1
  gg[p,q]        — g_p · g_q (cofactor Gram matrix)

Curl-curl (cofactor form, eq. K in the NumPy docstring):
  K_{ij} = (2/3) * (gg_ac * gg_bd - gg_ad * gg_bc) / |det|^3

Mass (Whitney 1-form, eq. M):
  M_{ij} = (1 / (120 |det|)) * [  (1 + δ_ac) gg_bd
                                  - (1 + δ_ad) gg_bc
                                  - (1 + δ_bc) gg_ad
                                  + (1 + δ_bd) gg_ac ]

All local edge signs are treated as +1 (lower-local-vertex to higher); the
global s_i s_j correction is applied in the assembly loop.
"""
function nedelec_local_cc_mass(nodes_tet::Matrix{Float64})
    # Vertex slices (1-indexed rows).
    v0 = @view nodes_tet[1, :]
    v1 = @view nodes_tet[2, :]
    v2 = @view nodes_tet[3, :]
    v3 = @view nodes_tet[4, :]

    # Edge vectors from v0.
    e1 = v1 .- v0
    e2 = v2 .- v0
    e3 = v3 .- v0

    # Area-weighted basis gradients (cofactors of the Jacobian).
    g1 = cross(e2, e3)
    g2 = cross(e3, e1)
    g3 = cross(e1, e2)
    g0 = -(g1 .+ g2 .+ g3)

    det = dot(e1, g1)      # = det(J), shape scalar
    abs_det = abs(det)

    # Stack cofactor gradients as a 4×3 matrix g_mat[p, :] = g_{p-1}.
    # gg[p, q] = g_{p-1} · g_{q-1}  (1-indexed here, matches Python 0-indexed).
    g_mat = Matrix{Float64}(undef, 4, 3)
    g_mat[1, :] .= g0
    g_mat[2, :] .= g1
    g_mat[3, :] .= g2
    g_mat[4, :] .= g3
    gg = g_mat * g_mat'   # 4×4 cofactor Gram matrix

    inv_abs_det  = 1.0 / abs_det
    inv_abs_det3 = inv_abs_det^3

    K_local = zeros(Float64, 6, 6)
    M_local = zeros(Float64, 6, 6)

    # Local edges in TET_LOCAL_EDGES order: (a,b) are 1-indexed local vertex pairs.
    for (i, (a, b)) in enumerate(TET_LOCAL_EDGES)
        for (j, (c, d)) in enumerate(TET_LOCAL_EDGES)
            # Cofactor Gram entries (1-indexed into g_mat rows).
            gg_ac = gg[a, c]
            gg_ad = gg[a, d]
            gg_bc = gg[b, c]
            gg_bd = gg[b, d]

            # Curl-curl (cofactor form, eq. K):
            #   K_{ij} = (2/3) * (gg_ac gg_bd - gg_ad gg_bc) / |det|^3
            K_local[i, j] = (2.0 / 3.0) * (gg_ac * gg_bd - gg_ad * gg_bc) * inv_abs_det3

            # Mass (Whitney 1-form, eq. M):
            #   f_pq = (1 + δ_{pq}) = 2 if p==q, else 1.
            f_ac = (a == c) ? 2.0 : 1.0
            f_ad = (a == d) ? 2.0 : 1.0
            f_bc = (b == c) ? 2.0 : 1.0
            f_bd = (b == d) ? 2.0 : 1.0
            m_term = f_ac * gg_bd - f_ad * gg_bc - f_bc * gg_ad + f_bd * gg_ac
            M_local[i, j] = m_term * inv_abs_det / 120.0
        end
    end

    return K_local, M_local
end


# ---------------------------------------------------------------------------
# Global assembly — ε-scaled COO scatter into sparse K, M.
# ---------------------------------------------------------------------------

"""
    assemble_global_nedelec(nodes, tets, edges, tet_edge_idx, tet_edge_sign, epsilon_r)
    -> (K, M)

Assemble global Nédélec curl-curl stiffness `K` and ε-scaled mass `M` as
`SparseMatrixCSC{Float64}` matrices.

Mirror of `reference/numpy/sphere_pec.py::assemble_global_nedelec`.
For each tet:
  1. Compute per-tet 6×6 K_local and M_local via `nedelec_local_cc_mass`.
  2. Apply sign outer product: `s_i s_j` flip (from `tet_edge_sign`).
  3. Scale mass by `epsilon_r[t]`.
  4. Scatter into global COO triplets.
`SparseArrays.sparse(I, J, V, n, n, +)` collapses duplicate `(i, j)` entries
by sum — same semantics as scipy's `coo_matrix.tocsr`.
"""
function assemble_global_nedelec(
        nodes        ::Matrix{Float64},
        tets         ::Matrix{Int},
        edges        ::Matrix{Int},
        tet_edge_idx ::Matrix{Int},
        tet_edge_sign::Matrix{Int},
        epsilon_r    ::Vector{Float64},
)
    n_edges = size(edges, 1)
    n_tets  = size(tets, 1)

    # Pre-allocate COO triplet storage: 36 entries per tet.
    nnz_est = 36 * n_tets
    I_idx  = Vector{Int}(undef, nnz_est)
    J_idx  = Vector{Int}(undef, nnz_est)
    V_cc   = Vector{Float64}(undef, nnz_est)
    V_mass = Vector{Float64}(undef, nnz_est)

    p = 1
    for t in 1:n_tets
        nodes_tet = nodes[tets[t, :], :]   # 4×3 view
        eps_t = epsilon_r[t]
        K_local, M_local = nedelec_local_cc_mass(nodes_tet)

        for le_i in 1:6
            gi = tet_edge_idx[t, le_i]
            si = tet_edge_sign[t, le_i]
            for le_j in 1:6
                gj = tet_edge_idx[t, le_j]
                sj = tet_edge_sign[t, le_j]
                I_idx[p]  = gi
                J_idx[p]  = gj
                V_cc[p]   = Float64(si * sj) * K_local[le_i, le_j]
                V_mass[p] = Float64(si * sj) * M_local[le_i, le_j] * eps_t
                p += 1
            end
        end
    end

    K = sparse(I_idx, J_idx, V_cc,   n_edges, n_edges, +)
    M = sparse(I_idx, J_idx, V_mass, n_edges, n_edges, +)
    return K, M
end


# ---------------------------------------------------------------------------
# Dirichlet reduction — restrict K, M to interior DOFs.
# ---------------------------------------------------------------------------

"""
    apply_dirichlet(K, M, interior_mask) -> (K_int, M_int)

Restrict `K, M` to the rows/cols where `interior_mask` is `true`.
Implements `n × E = 0` PEC boundary conditions on the outer wall.

Mirror of `reference/numpy/sphere_pec.py::apply_dirichlet`.
"""
function apply_dirichlet(
        K            ::SparseMatrixCSC{Float64},
        M            ::SparseMatrixCSC{Float64},
        interior_mask::BitVector,
)
    idx   = findall(interior_mask)
    K_int = K[idx, idx]
    M_int = M[idx, idx]
    return K_int, M_int
end


# ---------------------------------------------------------------------------
# d⁰ operator + spurious-mode dimension (Issue #124).
# ---------------------------------------------------------------------------

"""
    build_d0_interior(nodes, edges, interior_edge_mask, node_interior_mask)
    -> SparseMatrixCSC{Float64}

Build the interior-restricted discrete gradient operator `d⁰`.

Each interior edge `e = (va, vb)` with `va < vb` contributes:
  - column `local_node(va)`: entry `-1.0` (if `va` is an interior node)
  - column `local_node(vb)`: entry `+1.0` (if `vb` is an interior node)

Interior nodes are those *strictly* inside the PEC sphere
(`|r| < R_BUFFER - tol`). Interior edges that connect to a boundary node
contribute only one ±1 entry (or none if both endpoints are boundary nodes,
but those edges are already excluded by `interior_edge_mask`).

Mirror of `reference/numpy/sphere_pec.py::restrict_gradient_dense` but
returned as a sparse matrix.
"""
function build_d0_interior(
        nodes              ::Matrix{Float64},
        edges              ::Matrix{Int},
        interior_edge_mask ::BitVector,
        node_interior_mask ::BitVector,
)
    int_edges = findall(interior_edge_mask)  # global → local interior edge indices
    int_nodes = findall(node_interior_mask)  # global → local interior node indices
    n_int_e   = length(int_edges)
    n_int_n   = length(int_nodes)

    # Reverse map: global node index → interior column (0 if boundary).
    node_local = Dict{Int,Int}(v => i for (i, v) in enumerate(int_nodes))

    I_idx = Int[]
    J_idx = Int[]
    V_idx = Float64[]

    for (e_local, e_global) in enumerate(int_edges)
        va = edges[e_global, 1]   # lo-index vertex (canonical orientation)
        vb = edges[e_global, 2]   # hi-index vertex
        if haskey(node_local, va)
            push!(I_idx, e_local)
            push!(J_idx, node_local[va])
            push!(V_idx, -1.0)
        end
        if haskey(node_local, vb)
            push!(I_idx, e_local)
            push!(J_idx, node_local[vb])
            push!(V_idx,  1.0)
        end
    end

    return sparse(I_idx, J_idx, V_idx, n_int_e, n_int_n)
end


"""
    spurious_dim_from_derham(d0_int) -> Int

Algebraic spurious-mode dimension = `rank(d⁰_interior)` via SVD.

Uses a relative cutoff `1e-12 × σ_max` to identify near-zero singular
values (matching the Burn-side `DERHAM_RANK_THRESHOLD_REL` constant and
the NumPy `1e-12 * np.linalg.norm(d0, ord=2)` cutoff). Mirror of
`reference/numpy/sphere_pec.py::spurious_dim_from_derham` (Issue #124).

On the bundled 774-node fixture this returns `368` (= number of strictly
interior nodes), consistent with `kernel(K) = image(d⁰)` (Epic #57 Phase
3.A).
"""
function spurious_dim_from_derham(d0_int::SparseMatrixCSC{Float64})
    # Dense SVD via LinearAlgebra.svd on the dense version of d0_int.
    # (d0_int is typically ~3300 × 368, manageable in dense form.)
    S = svd(Matrix(d0_int)).S
    if isempty(S)
        return 0
    end
    tol = 1e-12 * S[1]
    return count(s -> s > tol, S)
end


# ---------------------------------------------------------------------------
# Eigensolve via Arpack.jl.
# ---------------------------------------------------------------------------

"""
    eigensolve_arpack(K, M; nev) -> Vector{Float64}

Lowest-`nev` generalized eigenvalues of `K x = λ M x` via Arpack.jl.

Uses `which=:SM` (regular-inverse mode, no sigma) — the same calling
convention as `cube_cavity.jl::eigensolve_arpack`. See that file for the
Arpack.jl 0.5 friction artifact: `sigma=0, which=:LM` returns the *largest*
eigenvalues on the generalized pencil, not the smallest.

For the sphere-PEC problem, `K_int` has a large gradient nullspace
(dimension ≈ `spurious_dim = 368`) that clusters near zero. Arpack.jl's
regular-inverse mode (`M⁻¹K`, `:SM`) recovers these near-zero modes
efficiently without shift-invert at σ=0, matching SciPy's shift-and-invert
result to ~1e-13 on non-spurious eigenvalues (both bind the same libarpack).

Returns eigenvalues sorted ascending (spurious cluster first, then physical).
"""
function eigensolve_arpack(
        K::SparseMatrixCSC{Float64},
        M::SparseMatrixCSC{Float64};
        nev::Int,
)
    n  = size(K, 1)
    v0 = ones(Float64, n) ./ sqrt(Float64(n))   # deterministic seed
    eigvals_raw, _ = eigs(K, M; nev=nev, which=:SM, v0=v0)
    eigvals_real   = real.(eigvals_raw)
    sort!(eigvals_real)
    return eigvals_real
end


# ---------------------------------------------------------------------------
# End-to-end driver — returns a NamedTuple of all sub-stage quantities.
# ---------------------------------------------------------------------------

"""
    run_sphere_pec(mesh_path; n_index=N_INDEX, n_take=5, r_outer=R_BUFFER) -> NamedTuple

Full sphere-PEC Nédélec pipeline. Returns a `NamedTuple` with the same
diagnostic fields as `reference/numpy/sphere_pec.py::run_sphere_pec` for
cross-backend comparison.

Fields in the returned `NamedTuple`:
  `n_nodes, n_tets, n_edges, n_interior_edges, spurious_dim`
  `k_int_frobenius, m_int_frobenius`
  `k_int_diag, m_int_diag`
  `eigenvalues_lowest`   — length `spurious_dim + 8`, ascending
  `n_spurious`           — algebraic d⁰-rank spurious count
  `physical_eigenvalues` — lowest `n_take` physical modes after filtering
"""
function run_sphere_pec(
        mesh_path ::AbstractString;
        n_index   ::Float64 = N_INDEX,
        n_take    ::Int     = 5,
        r_outer   ::Float64 = R_BUFFER,
)
    # 1. Mesh I/O.
    nodes, tets, phys_tags = load_msh_with_tags(mesh_path)
    n_nodes = size(nodes, 1)
    n_tets  = size(tets,  1)

    # 2. ε_r assignment.
    epsilon_r = build_epsilon_r(phys_tags; n_inside=n_index)

    # 3. Edge enumeration.
    edges, tet_edge_idx, tet_edge_sign = build_edges(tets)
    n_edges = size(edges, 1)

    # 4. PEC wall mask.
    interior_mask   = sphere_pec_interior_edges(nodes, edges; r_outer=r_outer)
    n_interior_edges = count(interior_mask)

    # 5. Global assembly.
    K, M = assemble_global_nedelec(
        nodes, tets, edges, tet_edge_idx, tet_edge_sign, epsilon_r
    )

    # 6. Dirichlet reduction.
    K_int, M_int = apply_dirichlet(K, M, interior_mask)
    n_int = size(K_int, 1)

    # 7. Spurious-mode classifier via d⁰ rank (Issue #124).
    # Interior nodes: strictly inside the outer PEC sphere.
    abs_tol = 1e-6 * max(r_outer, 1.0)
    node_r  = [sqrt(nodes[i,1]^2 + nodes[i,2]^2 + nodes[i,3]^2) for i in 1:n_nodes]
    node_interior_mask = BitVector([abs(node_r[i] - r_outer) >= abs_tol for i in 1:n_nodes])

    d0_int    = build_d0_interior(nodes, edges, interior_mask, node_interior_mask)
    n_spurious = spurious_dim_from_derham(d0_int)

    # spurious_dim is also the predicted count (= number of interior nodes).
    spurious_dim = count(node_interior_mask)

    # 8. Eigensolve.
    n_request    = spurious_dim + 8
    eigvals_all  = eigensolve_arpack(K_int, M_int; nev=n_request)

    # 9. Spurious filter: skip first n_spurious near-zero modes.
    if n_spurious + n_take > length(eigvals_all)
        error("spectrum too short: n_spurious=$n_spurious, n_take=$n_take, " *
              "length(eigvals_all)=$(length(eigvals_all))")
    end
    physical_eigenvalues = eigvals_all[n_spurious+1 : n_spurious+n_take]

    return (
        n_nodes           = n_nodes,
        n_tets            = n_tets,
        n_edges           = n_edges,
        n_interior_edges  = n_interior_edges,
        spurious_dim      = spurious_dim,
        n_spurious        = n_spurious,
        epsilon_r         = epsilon_r,
        edges             = edges,
        tet_edge_idx      = tet_edge_idx,
        tet_edge_sign     = tet_edge_sign,
        interior_mask     = interior_mask,
        K                 = K,
        M                 = M,
        K_int             = K_int,
        M_int             = M_int,
        k_int_frobenius   = norm(K_int),     # Frobenius for SparseMatrixCSC
        m_int_frobenius   = norm(M_int),
        k_int_diag        = Vector{Float64}(diag(K_int)),
        m_int_diag        = Vector{Float64}(diag(M_int)),
        eigenvalues_lowest    = eigvals_all,
        physical_eigenvalues  = physical_eigenvalues,
    )
end


# ---------------------------------------------------------------------------
# CLI entry point.
# ---------------------------------------------------------------------------

function _parse_args(argv::Vector{String})
    mesh_path::Union{Nothing,String} = nothing
    out_path::Union{Nothing,String}  = nothing
    i = 1
    while i <= length(argv)
        arg = argv[i]
        if arg == "--mesh"
            mesh_path = argv[i + 1]; i += 2
        elseif arg == "--out"
            out_path = argv[i + 1]; i += 2
        elseif arg in ("-h", "--help")
            println(stderr, "Usage: julia --project=. sphere_pec.jl [--mesh path] [--out path]")
            exit(0)
        else
            error("unrecognized argument: $arg")
        end
    end
    return (mesh=mesh_path, out=out_path)
end


function main()
    args = _parse_args(ARGS)

    mesh_path = if args.mesh !== nothing
        args.mesh
    else
        joinpath(@__DIR__, "..", "fixtures", "sphere_pec", "sphere.msh")
    end

    @info "Running Julia sphere-PEC Nédélec pipeline" mesh=mesh_path

    result = run_sphere_pec(mesh_path)

    println("Sphere-PEC fixture: $(result.n_nodes) nodes, $(result.n_tets) tets")
    println("Global edges:       $(result.n_edges)")
    println("Interior DOFs:      $(result.n_interior_edges)")
    println("Predicted spurious: $(result.spurious_dim)  (= interior nodes)")
    println("Observed spurious:  $(result.n_spurious)  (= rank(d⁰_interior))")
    println()
    println("Lowest 5 physical eigenvalues (λ = k²) vs NumPy baseline:")
    expected = [1.4195415502066517, 1.4204339541482647, 1.4206625078898854,
                3.2718741181859423, 3.277498156786518]
    for (i, (lam, exp_v)) in enumerate(zip(result.physical_eigenvalues, expected))
        rel_err = abs(lam - exp_v) / abs(exp_v)
        @printf("  physical[%d]: λ = %.10f  (expected %.10f, rel err %.2e)\n",
                i, lam, exp_v, rel_err)
    end
    println()
    @printf("‖K_int‖_F = %.10e  (NumPy: 1.0938505285e+03)\n", result.k_int_frobenius)
    @printf("‖M_int‖_F = %.10e  (NumPy: 9.7437951004e+00)\n", result.m_int_frobenius)
end


if abspath(PROGRAM_FILE) == @__FILE__
    main()
end
