"""
Julia reference for the discrete de Rham complex (d⁰, d¹, d²).

Issue #168 (Epic #88, Phase I.2): mirrors the Burn-side operators in
`crates/geode-core/src/derham.rs` and the NumPy reference in
`reference/numpy/derham.py` so the de Rham slice has a third independent
backend. Because the operators are mathematically signed `{-1, 0, +1}`
matrices, the cross-check is **bit-exact** — there is no floating-point
tolerance question, only "do the integer patterns and signs match
between backends."

The discrete de Rham complex on a tetrahedral mesh:

    ℝ^{n_nodes}  --d⁰-->  ℝ^{n_edges}  --d¹-->  ℝ^{n_faces}  --d²-->  ℝ^{n_tets}
        H¹                H(curl)              H(div)             L²

with the bit-exact exactness identities `d¹ · d⁰ ≡ 0` and `d² · d¹ ≡ 0`
(Hiptmair, Acta Numerica 2002 §4; Arnold–Falk–Winther, Acta Numerica
2006 §1.2).

# Sign conventions

These mirror `crates/geode-core/src/derham.rs` and
`reference/numpy/derham.py` exactly. Any drift between the three
backends is a bug; the integer cross-check in
`crates/geode-validation/tests/derham_julia_reference.rs` is the canary.

**Edge orientation** (lower-tag-first, `a < b`):

    d⁰[edge, a] = -1     (tail of the oriented edge)
    d⁰[edge, b] = +1     (head of the oriented edge)

**Face orientation** (ascending global triple `a < b < c`, cycle
`a → b → c → a`):

    d¹[face, edge(a, b)] = +1
    d¹[face, edge(b, c)] = +1
    d¹[face, edge(a, c)] = -1

**Tet face orientation** (per local face slot `k` of the tet, see
`TET_LOCAL_FACES` in `crates/geode-core/src/mesh/mod.rs`):

    d²[tet, global_face_k] = (-1)^k · sign_k

where `sign_k` is the permutation parity of the local-face vertex
triple against the global ascending order. The `(-1)^k` alternation is
the simplicial boundary convention
`∂[v0,v1,v2,v3] = [v1,v2,v3] - [v0,v2,v3] + [v0,v1,v3] - [v0,v1,v2]`;
without it the `d² · d¹ ≡ 0` identity fails because two tets sharing an
interior face would contribute the same sign instead of opposite signs.

# Indexing conventions (the load-bearing Julia-specific choice)

Everything in this module is **0-based**, deliberately fighting Julia's
native 1-based indexing:

  * `tets` input is a `(n_tets, 4)` matrix of **0-based** global node
    indices. Callers loading a Gmsh `.msh` via `CubeMesh.load_msh`
    (which returns 1-based tags) must subtract 1 before calling in.
  * `edges` / `faces` tables hold 0-based node indices.
  * The CSR triples (`indptr`, `indices`, `data`) returned by
    `gradient_map` / `curl_map` / `divergence_map` are 0-based,
    row-sorted, and directly comparable to the NumPy
    `scipy.sparse.csr_matrix` canonical form and the Rust harness's
    CSR projection — no further translation.

The operators are built **directly in CSR**, not via `SparseArrays`
(which is CSC-native). `to_sparse` converts a `CsrInt` to a 1-based
`SparseMatrixCSC` for the compositional-identity products
(`d¹ · d⁰ ≡ 0`, `d² · d¹ ≡ 0`), which are layout-invariant.

# Public API

- `build_edges(tets)` — global edge table (`(n_edges, 2)`,
  lower-tag-first, lexicographically sorted).
- `build_faces(tets)` — global face table (`(n_faces, 3)`, ascending
  triples, lexicographically sorted).
- `build_tet_faces(tets, faces)` — per-tet `(global_face_idx, sign_k)`
  tables (0-based face indices).
- `gradient_map(n_nodes, edges)` — d⁰ as a row-sorted `CsrInt`.
- `curl_map(edges, faces)` — d¹ as a row-sorted `CsrInt`.
- `divergence_map(tets, faces)` — d² as a row-sorted `CsrInt`.
- `to_sparse(csr)` — 1-based `SparseMatrixCSC{Int,Int}` view of a
  `CsrInt` (for compositional-identity products only).
- `euler_ranks(n_nodes, n_edges, n_faces, n_tets)` — rank predictions
  from Euler-characteristic arithmetic on a contractible 3-mesh.
"""
module DerhamRef

using SparseArrays

export CsrInt, build_edges, build_faces, build_tet_faces,
       gradient_map, curl_map, divergence_map, to_sparse, euler_ranks,
       TET_LOCAL_EDGES, TET_LOCAL_FACES

"""
Canonical local edge → (local vertex pair) ordering on a tet.

Mirror of `crates/geode-core/src/mesh/mod.rs::TET_LOCAL_EDGES` and
`reference/numpy/derham.py::TET_LOCAL_EDGES`. Local vertex slots are
**1-based column indices** into the `(n_tets, 4)` tets matrix (the
Python table `(0,1),(0,2),(0,3),(1,2),(1,3),(2,3)` shifted by +1); the
global node indices stored in those columns stay 0-based.
"""
const TET_LOCAL_EDGES = ((1, 2), (1, 3), (1, 4), (2, 3), (2, 4), (3, 4))

"""
Canonical local face → (local vertex triple) ordering on a tet.

Mirror of `crates/geode-core/src/mesh/mod.rs::TET_LOCAL_FACES` and
`reference/numpy/derham.py::TET_LOCAL_FACES`. Face `k` (1-based slot
here; `k-1` in the 0-based Rust/NumPy convention) is opposite local
vertex `k`. Local vertex slots are 1-based column indices; the
ascending-local listing pins `d¹ ∘ d⁰ ≡ 0` bit-exactly.
"""
const TET_LOCAL_FACES = (
    (2, 3, 4),   # face slot 1 (Rust/NumPy k=0) opposite local vertex 0
    (1, 3, 4),   # face slot 2 (k=1) opposite local vertex 1
    (1, 2, 4),   # face slot 3 (k=2) opposite local vertex 2
    (1, 2, 3),   # face slot 4 (k=3) opposite local vertex 3
)


# ---------------------------------------------------------------------------
# Row-sorted 0-based integer CSR container.
# ---------------------------------------------------------------------------

"""
    CsrInt

Row-sorted, 0-based integer CSR triple — the canonical interchange form
shared with the NumPy baseline (`scipy.sparse.csr_matrix` after
`sort_indices()`) and the Rust harness's CSR projection.

Fields:
- `n_rows::Int`, `n_cols::Int` — matrix shape.
- `indptr::Vector{Int}` — length `n_rows + 1`, 0-based row pointers.
- `indices::Vector{Int}` — 0-based column indices, sorted ascending
  within each row.
- `data::Vector{Int}` — signed integer values in `{-1, +1}`.
"""
struct CsrInt
    n_rows::Int
    n_cols::Int
    indptr::Vector{Int}
    indices::Vector{Int}
    data::Vector{Int}
end

nnz_csr(m::CsrInt) = length(m.data)


"""
    to_sparse(m::CsrInt) -> SparseMatrixCSC{Int,Int}

Convert a 0-based row-sorted `CsrInt` to Julia's native (1-based,
CSC-stored) `SparseMatrixCSC`. Used only for the compositional-identity
products `d¹ · d⁰` and `d² · d¹`, which are layout-invariant — the
canonical interchange form for cross-backend comparison stays the
0-based CSR triple.
"""
function to_sparse(m::CsrInt)
    n = nnz_csr(m)
    rows = Vector{Int}(undef, n)
    cols = Vector{Int}(undef, n)
    for r in 1:m.n_rows
        for p in (m.indptr[r] + 1):m.indptr[r + 1]
            rows[p] = r                    # 1-based row
            cols[p] = m.indices[p] + 1     # 0-based → 1-based column
        end
    end
    return sparse(rows, cols, m.data, m.n_rows, m.n_cols)
end


# ---------------------------------------------------------------------------
# Mesh-level helpers — edges, faces, per-tet face/sign tables.
# ---------------------------------------------------------------------------

"""
    build_edges(tets) -> Matrix{Int}

Build the deduplicated, globally-oriented edge table. Mirror of
`geode_core::TetMesh::edges` and `reference/numpy/derham.py::build_edges`
(lower-tag-first, sorted lexicographically by `(a, b)` — the Rust
`BTreeSet` / NumPy `np.unique` dedup order). Edge `i` (0-based row
`i+1` here) is row `i` of the discrete gradient d⁰.

Note this is **not** the first-seen ordering used by
`CubeMesh.build_edges` (the Nédélec assembly path) — the de Rham
cross-check pins global indices, so the lexicographic dedup order is
load-bearing here.

# Arguments
- `tets::AbstractMatrix{<:Integer}` of shape `(n_tets, 4)`, **0-based**
  global node indices.

# Returns
- `edges::Matrix{Int}` of shape `(n_edges, 2)`; each row `[a, b]`
  satisfies `a < b` (0-based node indices), rows sorted ascending by
  `(a, b)` lexicographically.
"""
function build_edges(tets::AbstractMatrix{<:Integer})
    n_tets = size(tets, 1)
    pairs = Set{Tuple{Int,Int}}()
    for t in 1:n_tets
        for (la, lb) in TET_LOCAL_EDGES
            va = Int(tets[t, la])
            vb = Int(tets[t, lb])
            push!(pairs, va < vb ? (va, vb) : (vb, va))
        end
    end
    sorted_pairs = sort!(collect(pairs))   # lexicographic on tuples
    edges = Matrix{Int}(undef, length(sorted_pairs), 2)
    for (i, (a, b)) in enumerate(sorted_pairs)
        edges[i, 1] = a
        edges[i, 2] = b
    end
    return edges
end


"""
    build_faces(tets) -> Matrix{Int}

Build the deduplicated, globally-oriented face table. Mirror of
`geode_core::TetMesh::faces` and `reference/numpy/derham.py::build_faces`:
each face is a triple `[a, b, c]` with `a < b < c`, sorted ascending
lexicographically. Face `i` (0-based) is row `i` of the discrete
curl d¹.

# Arguments
- `tets::AbstractMatrix{<:Integer}` of shape `(n_tets, 4)`, **0-based**
  global node indices.

# Returns
- `faces::Matrix{Int}` of shape `(n_faces, 3)`; each row `[a, b, c]`
  satisfies `a < b < c` (0-based), rows sorted by `(a, b, c)` lex order.
"""
function build_faces(tets::AbstractMatrix{<:Integer})
    n_tets = size(tets, 1)
    triples = Set{Tuple{Int,Int,Int}}()
    for t in 1:n_tets
        for lf in TET_LOCAL_FACES
            v = (Int(tets[t, lf[1]]), Int(tets[t, lf[2]]), Int(tets[t, lf[3]]))
            s = TupleTools_sort3(v)
            push!(triples, s)
        end
    end
    sorted_triples = sort!(collect(triples))
    faces = Matrix{Int}(undef, length(sorted_triples), 3)
    for (i, (a, b, c)) in enumerate(sorted_triples)
        faces[i, 1] = a
        faces[i, 2] = b
        faces[i, 3] = c
    end
    return faces
end

"""Sort a 3-tuple ascending (no allocation)."""
function TupleTools_sort3(v::Tuple{Int,Int,Int})
    a, b, c = v
    if a > b
        a, b = b, a
    end
    if b > c
        b, c = c, b
    end
    if a > b
        a, b = b, a
    end
    return (a, b, c)
end


"""
    triple_permutation_sign(local_triple) -> Int

Sign of the permutation that sorts `local_triple` into ascending order:
`+1` for an even permutation (identity or one of the two 3-cycles),
`-1` for an odd permutation (any transposition). Mirror of the private
`triple_permutation_sign` helper in `crates/geode-core/src/mesh/mod.rs`
and `reference/numpy/derham.py::_triple_permutation_sign`.
"""
function triple_permutation_sign(v::Tuple{Int,Int,Int})
    a, b, c = v
    swaps = 0
    if a > b
        a, b = b, a
        swaps += 1
    end
    if b > c
        b, c = c, b
        swaps += 1
    end
    if a > b
        a, b = b, a
        swaps += 1
    end
    return iseven(swaps) ? 1 : -1
end


"""
    build_tet_faces(tets, faces) -> (tet_face_idx, tet_face_sign)

Per-tet `(global_face_idx, sign_k)` table. Mirror of
`geode_core::TetMesh::tet_faces` and
`reference/numpy/derham.py::build_tet_faces` — for each tet and each
local-face slot `k` (in `TET_LOCAL_FACES` order), return the **0-based**
global face index and the parity `sign_k = ±1` of the permutation that
sorts the local-face vertex triple into ascending global order.

# Returns
- `tet_face_idx::Matrix{Int}` of shape `(n_tets, 4)` — 0-based global
  face indices.
- `tet_face_sign::Matrix{Int}` of shape `(n_tets, 4)` — parities in
  `{-1, +1}`.
"""
function build_tet_faces(tets::AbstractMatrix{<:Integer}, faces::AbstractMatrix{<:Integer})
    n_tets = size(tets, 1)
    n_faces = size(faces, 1)

    face_to_idx = Dict{Tuple{Int,Int,Int},Int}()
    sizehint!(face_to_idx, n_faces)
    for i in 1:n_faces
        face_to_idx[(Int(faces[i, 1]), Int(faces[i, 2]), Int(faces[i, 3]))] = i - 1
    end

    tet_face_idx = Matrix{Int}(undef, n_tets, 4)
    tet_face_sign = Matrix{Int}(undef, n_tets, 4)
    for t in 1:n_tets
        for (k, lf) in enumerate(TET_LOCAL_FACES)
            local_triple = (Int(tets[t, lf[1]]), Int(tets[t, lf[2]]), Int(tets[t, lf[3]]))
            sorted_triple = TupleTools_sort3(local_triple)
            tet_face_idx[t, k] = face_to_idx[sorted_triple]
            tet_face_sign[t, k] = triple_permutation_sign(local_triple)
        end
    end
    return tet_face_idx, tet_face_sign
end


# ---------------------------------------------------------------------------
# d⁰, d¹, d² — signed integer CSR incidence matrices.
# ---------------------------------------------------------------------------

"""
    gradient_map(n_nodes, edges) -> CsrInt

Discrete gradient d⁰ as a row-sorted 0-based CSR. Mirror of
`geode_core::derham::gradient_map` /
`reference/numpy/derham.py::gradient_map`: `n_edges × n_nodes`, each
row (edge `[a, b]`, `a < b`) has exactly two nonzeros — `-1` at column
`a` (tail), `+1` at column `b` (head). Applied to a nodal field φ, row
`[a, b]` yields `φ[b] − φ[a]`. Rows are already column-sorted because
`a < b`.
"""
function gradient_map(n_nodes::Integer, edges::AbstractMatrix{<:Integer})
    n_edges = size(edges, 1)
    indptr = collect(0:2:(2 * n_edges))
    indices = Vector{Int}(undef, 2 * n_edges)
    data = Vector{Int}(undef, 2 * n_edges)
    for e in 1:n_edges
        a = Int(edges[e, 1])
        b = Int(edges[e, 2])
        @assert a < b "edge ($a, $b) violates the lower-tag-first contract"
        indices[2e - 1] = a
        indices[2e] = b
        data[2e - 1] = -1
        data[2e] = +1
    end
    return CsrInt(n_edges, Int(n_nodes), indptr, indices, data)
end


"""
    curl_map(edges, faces) -> CsrInt

Discrete curl d¹ as a row-sorted 0-based CSR. Mirror of
`geode_core::derham::curl_map` /
`reference/numpy/derham.py::curl_map`: `n_faces × n_edges`, each row
(face `[a, b, c]`, `a < b < c`) has exactly three nonzeros encoding the
signed boundary cycle `a → b → c → a`:

    d¹[face, edge(a, b)] = +1
    d¹[face, edge(b, c)] = +1
    d¹[face, edge(a, c)] = -1

Because the edge table is lexicographically sorted,
`idx(a,b) < idx(a,c) < idx(b,c)`, so the column-sorted row order is
`(ab, +1), (ac, -1), (bc, +1)` — asserted rather than re-sorted.
"""
function curl_map(edges::AbstractMatrix{<:Integer}, faces::AbstractMatrix{<:Integer})
    n_edges = size(edges, 1)
    n_faces = size(faces, 1)

    edge_to_idx = Dict{Tuple{Int,Int},Int}()
    sizehint!(edge_to_idx, n_edges)
    for i in 1:n_edges
        edge_to_idx[(Int(edges[i, 1]), Int(edges[i, 2]))] = i - 1
    end

    indptr = collect(0:3:(3 * n_faces))
    indices = Vector{Int}(undef, 3 * n_faces)
    data = Vector{Int}(undef, 3 * n_faces)
    for f in 1:n_faces
        a = Int(faces[f, 1])
        b = Int(faces[f, 2])
        c = Int(faces[f, 3])
        @assert a < b < c "face ($a, $b, $c) violates the ascending-triple contract"
        e_ab = edge_to_idx[(a, b)]
        e_ac = edge_to_idx[(a, c)]
        e_bc = edge_to_idx[(b, c)]
        # Lexicographic edge order guarantees e_ab < e_ac < e_bc.
        @assert e_ab < e_ac < e_bc "edge-table lex order violated for face ($a, $b, $c)"
        indices[3f - 2] = e_ab
        indices[3f - 1] = e_ac
        indices[3f] = e_bc
        data[3f - 2] = +1
        data[3f - 1] = -1
        data[3f] = +1
    end
    return CsrInt(n_faces, n_edges, indptr, indices, data)
end


"""
    divergence_map(tets, faces) -> CsrInt

Discrete divergence d² as a row-sorted 0-based CSR. Mirror of
`geode_core::derham::divergence_map` /
`reference/numpy/derham.py::divergence_map`: `n_tets × n_faces`, each
row has exactly four nonzeros, one per local face of the tet:

    d²[tet, global_face_k] = (-1)^k · sign_k

with `k ∈ {0, 1, 2, 3}` the (0-based) local-face slot and `sign_k` the
permutation parity from `build_tet_faces`. The `(-1)^k` alternation is
the simplicial boundary convention; without it `d² · d¹ ≡ 0` fails.

Unlike d⁰/d¹, the four global face indices of a tet arrive in local-slot
order, not column order — each row is explicitly sorted by column index
to land in canonical row-sorted CSR.
"""
function divergence_map(tets::AbstractMatrix{<:Integer}, faces::AbstractMatrix{<:Integer})
    n_tets = size(tets, 1)
    n_faces = size(faces, 1)

    tet_face_idx, tet_face_sign = build_tet_faces(tets, faces)

    indptr = collect(0:4:(4 * n_tets))
    indices = Vector{Int}(undef, 4 * n_tets)
    data = Vector{Int}(undef, 4 * n_tets)
    entry = Vector{Tuple{Int,Int}}(undef, 4)   # (col, val) per local slot
    for t in 1:n_tets
        for k in 1:4
            alt = isodd(k) ? 1 : -1   # (-1)^(k-1) for 0-based slot k-1
            entry[k] = (tet_face_idx[t, k], alt * tet_face_sign[t, k])
        end
        sort!(entry; by = first)
        for k in 1:4
            indices[4(t - 1) + k] = entry[k][1]
            data[4(t - 1) + k] = entry[k][2]
        end
    end
    return CsrInt(n_tets, n_faces, indptr, indices, data)
end


# ---------------------------------------------------------------------------
# Rank predictions from Euler-characteristic arithmetic.
# ---------------------------------------------------------------------------

"""
    euler_ranks(n_nodes, n_edges, n_faces, n_tets) -> NamedTuple

Predict `rank(d⁰)`, `rank(d¹)`, `rank(d²)` on a closed contractible
3-mesh (the bundled sphere fixture is a 3-ball). Mirror of
`reference/numpy/derham.py::euler_ranks`:

    rank(d⁰) = n_nodes − 1                       (β₀ = 1)
    rank(d¹) = n_edges − n_nodes + 1             (β₁ = 0)
    rank(d²) = n_faces − n_edges + n_nodes − 1   (β₂ = 0)

Returns `(rank_d0, rank_d1, rank_d2, euler_chi)`.
"""
function euler_ranks(n_nodes::Integer, n_edges::Integer, n_faces::Integer, n_tets::Integer)
    return (
        rank_d0 = Int(n_nodes) - 1,
        rank_d1 = Int(n_edges) - Int(n_nodes) + 1,
        rank_d2 = Int(n_faces) - Int(n_edges) + Int(n_nodes) - 1,
        euler_chi = Int(n_nodes) - Int(n_edges) + Int(n_faces) - Int(n_tets),
    )
end

end  # module DerhamRef
