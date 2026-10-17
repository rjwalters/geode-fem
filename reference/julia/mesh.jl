"""
Mesh primitives for the Julia reference backends (Epic #88 / Phases E and G.4).

Local equivalent of `reference/numpy/mesh.py` — provides the canonical
programmatic n-per-side tet-split unit cube, an inline MSH 4.1 ASCII
parser, the interior-DOF Dirichlet mask, and Nédélec edge-table builders
for the sphere-PEC pipeline (Phase G.4, issue #129). We do **not** depend
on Gmsh.jl: the libgmsh native dependency is ~50 MB and the MSH 4.1 ASCII
files we consume here are simple enough to parse inline (see Open
Question #3 in the curator pass on issue #115).

Public API
==========

Cube-cavity (Phase E):
- `cube_tet_mesh(n; side=1.0)` — generate the n-per-side tet-split unit
  cube. Mirror of `geode_core::mesh::cube_tet_mesh` and
  `reference/numpy/mesh.py::cube_tet_mesh`.
- `cube_interior_mask(nodes; side=1.0)` — boolean mask, true for interior
  (free-DOF) nodes of `[0, side]^3`.
- `load_msh(path)` — read a Gmsh `.msh` (MSH 4.1 ASCII) and return
  `(nodes, tets)` as `(Matrix{Float64}, Matrix{Int})`.

Sphere-PEC Nédélec (Phase G.4):
- `load_msh_with_tags(path)` — extended version of `load_msh` that also
  returns per-tet physical group tags `(nodes, tets, phys_tags)`.
- `build_edges(tets)` — build globally-oriented edge table and per-tet
  edge-sign table. Mirror of `geode_core::TetMesh::edges` +
  `reference/numpy/sphere_pec.py::build_edges`.
- `sphere_pec_interior_edges(nodes, edges; r_outer, tol)` — boolean mask
  of interior edges (not both endpoints on the PEC wall).

Node ordering matches Burn / NumPy exactly so the same node-indexed
Dirichlet mask works on all three backends.
"""
module CubeMesh

export cube_tet_mesh, cube_interior_mask, load_msh,
       load_msh_with_tags, build_edges, sphere_pec_interior_edges

"""
    cube_tet_mesh(n; side=1.0) -> (nodes, tets)

Generate the n-per-side tet-split unit cube. Faithful mirror of
`geode_core::cube_tet_mesh` (see `crates/geode-core/src/mesh/mod.rs`).
For each `n x n x n` hex cell, we emit 6 right-handed tets sharing the
long diagonal `c[1] -> c[7]` (1-based here; Burn/NumPy use 0-based
`c[0] -> c[6]` — same diagonal, different indexing). The returned
`tets` are 1-based to match Julia conventions.

# Arguments
- `n::Int`: hexes per side.
- `side::Float64`: cube side length (default `1.0`).

# Returns
- `nodes::Matrix{Float64}` of shape `((n+1)^3, 3)`, row-major node
  coordinates in lexicographic `(i, j, k)` order with `i` fastest.
- `tets::Matrix{Int}` of shape `(6 * n^3, 4)`, 1-based linear node
  indices.
"""
function cube_tet_mesh(n::Int; side::Float64=1.0)
    nps = n + 1
    h = side / n

    nnode = nps^3
    nodes = zeros(Float64, nnode, 3)
    # Match the NumPy/Burn ordering: nodes[i + j*nps + k*nps*nps] (0-based)
    # becomes nodes[i + j*nps + k*nps*nps + 1, :] in 1-based Julia indexing.
    for k in 0:n, j in 0:n, i in 0:n
        idx = i + j * nps + k * nps * nps + 1
        nodes[idx, 1] = i * h
        nodes[idx, 2] = j * h
        nodes[idx, 3] = k * h
    end

    # Local lookup: 1-based linear index of grid point (i, j, k).
    node_idx(i, j, k) = i + j * nps + k * nps * nps + 1

    n_tets = 6 * n^3
    tets = zeros(Int, n_tets, 4)
    t = 1
    for k in 0:(n-1), j in 0:(n-1), i in 0:(n-1)
        # 8-corner hex, 1-based vertex indices c[1..8].
        c = (
            node_idx(i,     j,     k    ),
            node_idx(i + 1, j,     k    ),
            node_idx(i + 1, j + 1, k    ),
            node_idx(i,     j + 1, k    ),
            node_idx(i,     j,     k + 1),
            node_idx(i + 1, j,     k + 1),
            node_idx(i + 1, j + 1, k + 1),
            node_idx(i,     j + 1, k + 1),
        )
        # 6-tet split sharing the diagonal c[1] -> c[7] (the Burn/NumPy
        # c[0] -> c[6] diagonal in 1-based form). All right-handed.
        tets[t,     :] .= (c[1], c[2], c[3], c[7])
        tets[t + 1, :] .= (c[1], c[3], c[4], c[7])
        tets[t + 2, :] .= (c[1], c[4], c[8], c[7])
        tets[t + 3, :] .= (c[1], c[8], c[5], c[7])
        tets[t + 4, :] .= (c[1], c[5], c[6], c[7])
        tets[t + 5, :] .= (c[1], c[6], c[2], c[7])
        t += 6
    end

    return nodes, tets
end


"""
    cube_interior_mask(nodes; side=1.0) -> BitVector

Boolean mask: `true` if the node is strictly inside the cube `[0, side]^3`.
Mirrors `geode_core::cube_interior_mask` and
`reference/numpy/mesh.py::cube_interior_mask`. A node is "boundary" iff
any coordinate is within `1e-9 * max(side, 1)` of `0` or `side`.
"""
function cube_interior_mask(nodes::AbstractMatrix{Float64}; side::Float64=1.0)
    tol = 1e-9 * max(side, 1.0)
    n = size(nodes, 1)
    mask = BitVector(undef, n)
    @inbounds for i in 1:n
        x, y, z = nodes[i, 1], nodes[i, 2], nodes[i, 3]
        on_boundary = (
            x < tol || abs(x - side) < tol ||
            y < tol || abs(y - side) < tol ||
            z < tol || abs(z - side) < tol
        )
        mask[i] = !on_boundary
    end
    return mask
end


"""
    load_msh(path) -> (nodes, tets)

Inline parser for Gmsh MSH 4.1 ASCII files. We deliberately avoid the
Gmsh.jl native dependency (~50 MB libgmsh) since the `.msh` files we
consume here have a simple, well-documented structure.

The MSH 4.1 ASCII layout we expect (matches what `meshio` emits in
`reference/numpy/mesh.py::write_msh`):

```
\$MeshFormat
4.1 0 8
\$EndMeshFormat
\$Nodes
<numEntityBlocks> <numNodes> <minNodeTag> <maxNodeTag>
<entityDim> <entityTag> <parametric> <numNodesInBlock>
<nodeTag>
... (numNodesInBlock node tags)
<x> <y> <z>
... (numNodesInBlock coordinates)
\$EndNodes
\$Elements
<numEntityBlocks> <numElements> <minElTag> <maxElTag>
<entityDim> <entityTag> <elementType> <numElementsInBlock>
<elTag> <node1> <node2> <node3> <node4>
... (only `elementType = 4` (tetrahedra) is kept)
\$EndElements
```

Surface triangles (`elementType = 2`) and lines (`elementType = 1`) are
silently dropped — they are valid inputs but not the volume elements we
care about.

# Returns
- `nodes::Matrix{Float64}` of shape `(n_nodes, 3)`.
- `tets::Matrix{Int}` of shape `(n_tets, 4)`, **1-based** vertex indices
  (the `.msh` file is already 1-based; we keep that convention).
"""
function load_msh(path::AbstractString)
    open(path, "r") do io
        # --- $MeshFormat header ---
        line = strip(readline(io))
        if line != "\$MeshFormat"
            error("load_msh: expected \$MeshFormat, got '$line'")
        end
        # version filetype datasize
        parts = split(strip(readline(io)))
        version = parts[1]
        if !startswith(version, "4.1")
            error("load_msh: only MSH 4.1 ASCII supported, got version $version")
        end
        line = strip(readline(io))
        if line != "\$EndMeshFormat"
            error("load_msh: expected \$EndMeshFormat, got '$line'")
        end

        nodes = Matrix{Float64}(undef, 0, 3)
        tets = Matrix{Int}(undef, 0, 4)

        # Walk sections until EOF.
        while !eof(io)
            line = strip(readline(io))
            if isempty(line)
                continue
            elseif line == "\$Nodes"
                nodes = _read_nodes_section(io)
            elseif line == "\$Elements"
                tets = _read_elements_section(io)
            elseif startswith(line, "\$") && !startswith(line, "\$End")
                # Skip any other section (PhysicalNames, Entities, etc).
                _skip_section(io, line)
            end
        end

        if size(nodes, 1) == 0
            error("load_msh: no nodes found in $path")
        end
        if size(tets, 1) == 0
            error("load_msh: no tetrahedra (elementType = 4) found in $path")
        end
        return nodes, tets
    end
end


function _read_nodes_section(io::IO)
    # Header line: numEntityBlocks numNodes minNodeTag maxNodeTag
    parts = split(strip(readline(io)))
    n_entity_blocks = parse(Int, parts[1])
    num_nodes = parse(Int, parts[2])

    nodes = Matrix{Float64}(undef, num_nodes, 3)
    seen = falses(num_nodes)

    for _ in 1:n_entity_blocks
        # entityDim entityTag parametric numNodesInBlock
        bparts = split(strip(readline(io)))
        n_in_block = parse(Int, bparts[4])
        if n_in_block == 0
            continue
        end
        # nodeTags first, then coords.
        tags = Vector{Int}(undef, n_in_block)
        for j in 1:n_in_block
            tags[j] = parse(Int, strip(readline(io)))
        end
        for j in 1:n_in_block
            cparts = split(strip(readline(io)))
            tag = tags[j]
            if tag < 1 || tag > num_nodes
                error("load_msh: node tag $tag out of range [1, $num_nodes]")
            end
            nodes[tag, 1] = parse(Float64, cparts[1])
            nodes[tag, 2] = parse(Float64, cparts[2])
            nodes[tag, 3] = parse(Float64, cparts[3])
            seen[tag] = true
        end
    end

    line = strip(readline(io))
    if line != "\$EndNodes"
        error("load_msh: expected \$EndNodes, got '$line'")
    end
    if !all(seen)
        n_missing = count(!, seen)
        error("load_msh: $n_missing node(s) missing coordinates in the \$Nodes section")
    end
    return nodes
end


function _read_elements_section(io::IO)
    # Header line: numEntityBlocks numElements minElTag maxElTag
    parts = split(strip(readline(io)))
    n_entity_blocks = parse(Int, parts[1])

    # We collect only element-type-4 (4-node tetrahedra) blocks.
    tet_rows = Vector{NTuple{4,Int}}()

    for _ in 1:n_entity_blocks
        # entityDim entityTag elementType numElementsInBlock
        bparts = split(strip(readline(io)))
        elem_type = parse(Int, bparts[3])
        n_in_block = parse(Int, bparts[4])

        if elem_type == 4
            # Tetrahedra: each line is "<elTag> <n1> <n2> <n3> <n4>"
            for _ in 1:n_in_block
                lparts = split(strip(readline(io)))
                v1 = parse(Int, lparts[2])
                v2 = parse(Int, lparts[3])
                v3 = parse(Int, lparts[4])
                v4 = parse(Int, lparts[5])
                push!(tet_rows, (v1, v2, v3, v4))
            end
        else
            # Skip the lines for this element type.
            for _ in 1:n_in_block
                readline(io)
            end
        end
    end

    line = strip(readline(io))
    if line != "\$EndElements"
        error("load_msh: expected \$EndElements, got '$line'")
    end

    tets = Matrix{Int}(undef, length(tet_rows), 4)
    for (i, t) in enumerate(tet_rows)
        tets[i, 1] = t[1]
        tets[i, 2] = t[2]
        tets[i, 3] = t[3]
        tets[i, 4] = t[4]
    end
    return tets
end


function _skip_section(io::IO, opener::AbstractString)
    # Strip leading $ to derive the matching closer ($End<Name>).
    name = opener[2:end]  # e.g. "Entities"
    closer = "\$End" * name
    while !eof(io)
        line = strip(readline(io))
        if line == closer
            return
        end
    end
    error("load_msh: section $opener never closed with $closer")
end


# ---------------------------------------------------------------------------
# Extended MSH reader with physical group tags (Phase G.4 — sphere-PEC).
# ---------------------------------------------------------------------------

"""
    load_msh_with_tags(path) -> (nodes, tets, phys_tags)

Extended version of `load_msh` that also returns per-tet 3D physical group
tags. Used by the sphere-PEC pipeline (Phase G.4, issue #129) for ε_r
assignment: tags distinguish `sphere_interior` (1), `vacuum_gap` (2), and
`pml_shell` (5) volume groups in the bundled `sphere.msh`.

Two-pass reader:
  1. Parse `\$Entities` to build a `(dim=3, entity_tag) → phys_tag` map.
  2. Parse `\$Nodes` + `\$Elements`, resolving each tet's entity_tag → phys_tag.

# Returns
- `nodes::Matrix{Float64}` of shape `(n_nodes, 3)`.
- `tets::Matrix{Int}` of shape `(n_tets, 4)`, **1-based** vertex indices.
- `phys_tags::Vector{Int}` of length `n_tets` — per-tet physical group tag.
"""
function load_msh_with_tags(path::AbstractString)
    # Pass 1: build (dim=3, entity_tag) → phys_tag map from $Entities.
    entity_phys_map = _parse_entity_phys_map(path)

    # Pass 2: read nodes + elements, resolving entity_tag → phys_tag.
    open(path, "r") do io
        line = strip(readline(io))
        line != "\$MeshFormat" && error("load_msh_with_tags: expected \$MeshFormat, got '$line'")
        parts = split(strip(readline(io)))
        startswith(parts[1], "4.1") || error("load_msh_with_tags: only MSH 4.1 ASCII supported")
        strip(readline(io))  # $EndMeshFormat

        nodes    = Matrix{Float64}(undef, 0, 3)
        tets     = Matrix{Int}(undef, 0, 4)
        phys_tags = Int[]

        while !eof(io)
            line = strip(readline(io))
            if isempty(line)
                continue
            elseif line == "\$Nodes"
                nodes = _read_nodes_section(io)
            elseif line == "\$Elements"
                tets, raw_entity_tags = _read_elements_with_entity_tags(io)
                # Resolve entity_tag → phys_tag (0 if not in map).
                phys_tags = [get(entity_phys_map, (3, tag), 0) for tag in raw_entity_tags]
            elseif startswith(line, "\$") && !startswith(line, "\$End")
                _skip_section(io, line)
            end
        end

        size(nodes, 1) == 0 && error("load_msh_with_tags: no nodes found in $path")
        size(tets,  1) == 0 && error("load_msh_with_tags: no tets found in $path")
        return nodes, tets, phys_tags
    end
end


"""
Parse `\$Entities` from an MSH 4.1 file.

Returns a `Dict{Tuple{Int,Int}, Int}` mapping `(dim, entity_tag) → phys_tag`
for entities that carry exactly one physical group tag. Entities with zero
tags (unnamed internal entities) are omitted; entities with multiple tags
store only the first.

MSH 4.1 `\$Entities` format (ASCII, by dimension):
  Points  (dim=0): tag  x  y  z  numPhysTags  [physTag...]  numBndCurves  [curveTag...]
  Curves  (dim=1): tag  xmin..zmax  numPhysTags  [physTag...]  numBndPts  [ptTag...]
  Surfaces (dim=2): tag  xmin..zmax  numPhysTags  [physTag...]  numBndCurves  [curveTag...]
  Volumes (dim=3): tag  xmin..zmax  numPhysTags  [physTag...]  numBndSurfaces  [surfTag...]

All lines are single-line records (no continuation). We read each line
as a split token sequence and index into it.
"""
function _parse_entity_phys_map(path::AbstractString)
    entity_phys = Dict{Tuple{Int,Int}, Int}()
    open(path, "r") do io
        while !eof(io)
            line = strip(readline(io))
            if line == "\$Entities"
                # Header: numPoints numCurves numSurfaces numVolumes
                hparts = split(strip(readline(io)))
                n_pts  = parse(Int, hparts[1])
                n_cur  = parse(Int, hparts[2])
                n_surf = parse(Int, hparts[3])
                n_vol  = parse(Int, hparts[4])

                # Skip points (dim=0): tag x y z numPhysTags [physTag...] numBndCurves [curveTag...]
                # Points have 4 fixed tokens before numPhysTags (tag + x + y + z).
                for _ in 1:n_pts
                    readline(io)  # single-line format; skip entirely
                end
                # Skip curves (dim=1): skip entirely.
                for _ in 1:n_cur
                    readline(io)
                end
                # Skip surfaces (dim=2): skip entirely.
                for _ in 1:n_surf
                    readline(io)
                end
                # Parse volumes (dim=3): tag xmin ymin zmin xmax ymax zmax numPhysTags [physTag...] ...
                # Tokens: [1]=tag [2..7]=bbox(6) [8]=numPhysTags [9..]=physTags...
                for _ in 1:n_vol
                    lparts = split(strip(readline(io)))
                    vtag   = parse(Int, lparts[1])
                    n_phys = parse(Int, lparts[8])
                    if n_phys > 0
                        ptag = parse(Int, lparts[9])
                        entity_phys[(3, vtag)] = ptag
                    end
                end

                line2 = strip(readline(io))
                line2 != "\$EndEntities" &&
                    error("_parse_entity_phys_map: expected \$EndEntities, got '$line2'")
                break
            end
        end
    end
    return entity_phys
end


"""Read `\$Elements`, returning `(tets, raw_entity_tags)` without resolving phys tags."""
function _read_elements_with_entity_tags(io::IO)
    parts = split(strip(readline(io)))
    n_entity_blocks = parse(Int, parts[1])

    tet_rows        = Vector{NTuple{4,Int}}()
    tet_entity_tags = Vector{Int}()

    for _ in 1:n_entity_blocks
        # entityDim entityTag elementType numElementsInBlock
        bparts     = split(strip(readline(io)))
        entity_tag = parse(Int, bparts[2])
        elem_type  = parse(Int, bparts[3])
        n_in_block = parse(Int, bparts[4])

        if elem_type == 4  # tetrahedra
            for _ in 1:n_in_block
                lparts = split(strip(readline(io)))
                v1 = parse(Int, lparts[2])
                v2 = parse(Int, lparts[3])
                v3 = parse(Int, lparts[4])
                v4 = parse(Int, lparts[5])
                push!(tet_rows, (v1, v2, v3, v4))
                push!(tet_entity_tags, entity_tag)
            end
        else
            for _ in 1:n_in_block
                readline(io)
            end
        end
    end

    line = strip(readline(io))
    line != "\$EndElements" &&
        error("_read_elements_with_entity_tags: expected \$EndElements, got '$line'")

    tets = Matrix{Int}(undef, length(tet_rows), 4)
    for (i, t) in enumerate(tet_rows)
        tets[i, 1] = t[1]; tets[i, 2] = t[2]
        tets[i, 3] = t[3]; tets[i, 4] = t[4]
    end
    return tets, tet_entity_tags
end


# ---------------------------------------------------------------------------
# Nédélec edge-table builders (Phase G.4 — sphere-PEC pipeline).
# Mirror of reference/numpy/sphere_pec.py::build_edges and
# ::sphere_pec_interior_edges.
# ---------------------------------------------------------------------------

"""
    build_edges(tets) -> (edges, tet_edge_idx, tet_edge_sign)

Build the globally-oriented edge table and per-tet edge-sign table for a
tetrahedral mesh. Mirror of `geode_core::TetMesh::edges` and
`reference/numpy/sphere_pec.py::build_edges`.

Local edge ordering on each tet (1-indexed local vertices):

    local edge 1: (v1, v2)
    local edge 2: (v1, v3)
    local edge 3: (v1, v4)
    local edge 4: (v2, v3)
    local edge 5: (v2, v4)
    local edge 6: (v3, v4)

This matches the Python 0-indexed `TET_LOCAL_EDGES = [(0,1),(0,2),(0,3),(1,2),(1,3),(2,3)]`
shifted by 1.

Global canonical orientation: `min(va, vb) → max(va, vb)`.
`tet_edge_sign[t, k] = +1` if the local pair `(va, vb)` has `va < vb`
(agrees with canonical), else `-1`.

**Global edge ordering**: edges are numbered in first-seen order as tets
are visited. This differs from the NumPy reference's lexicographic sort
(`np.unique`), so the global edge index of a given `(lo, hi)` pair will
differ between Julia and NumPy. This is intentional and harmless: only the
assembled K, M matrices must agree — the global edge indices are internal.

# Arguments
- `tets::Matrix{Int}` of shape `(n_tets, 4)`, **1-based** vertex indices.

# Returns
- `edges::Matrix{Int}` of shape `(n_edges, 2)` — first-seen global edge
  table; each row `[lo, hi]` satisfies `lo < hi`.
- `tet_edge_idx::Matrix{Int}` of shape `(n_tets, 6)` — per-tet global
  edge indices (1-based).
- `tet_edge_sign::Matrix{Int}` of shape `(n_tets, 6)` — orientation signs
  `+1` or `-1`.
"""
function build_edges(tets::Matrix{Int})
    # TET_LOCAL_EDGES: 1-indexed local vertex pairs.
    # Corresponds to Python's (0,1),(0,2),(0,3),(1,2),(1,3),(2,3) shifted +1.
    TET_LOCAL_EDGES = ((1,2),(1,3),(1,4),(2,3),(2,4),(3,4))
    n_tets = size(tets, 1)

    edge_pairs    = Dict{Tuple{Int,Int}, Int}()  # (lo, hi) → 1-based global index
    edge_list     = Vector{Tuple{Int,Int}}()      # ordered list of (lo, hi)
    tet_edge_idx  = zeros(Int, n_tets, 6)
    tet_edge_sign = zeros(Int, n_tets, 6)

    for t in 1:n_tets
        for (k, (la, lb)) in enumerate(TET_LOCAL_EDGES)
            va = tets[t, la]
            vb = tets[t, lb]
            lo, hi = va < vb ? (va, vb) : (vb, va)
            key = (lo, hi)
            if !haskey(edge_pairs, key)
                push!(edge_list, key)
                edge_pairs[key] = length(edge_list)
            end
            gidx = edge_pairs[key]
            tet_edge_idx[t, k]  = gidx
            tet_edge_sign[t, k] = (va < vb) ? 1 : -1
        end
    end

    n_edges = length(edge_list)
    edges = Matrix{Int}(undef, n_edges, 2)
    for (i, (lo, hi)) in enumerate(edge_list)
        edges[i, 1] = lo
        edges[i, 2] = hi
    end

    return edges, tet_edge_idx, tet_edge_sign
end


"""
    sphere_pec_interior_edges(nodes, edges; r_outer=2.0, tol=1e-6) -> BitVector

Return a boolean mask of interior edges for the sphere-PEC problem.

An edge is *interior* (mask entry `true`) iff at least one endpoint is
strictly inside the outer PEC wall. Equivalently, an edge is eliminated
iff **both** endpoints lie on the outer sphere within
`tol * max(r_outer, 1.0)` of `r_outer`.

Mirror of `reference/numpy/sphere_pec.py::sphere_pec_interior_edges`.

# Arguments
- `nodes::Matrix{Float64}` of shape `(n_nodes, 3)`.
- `edges::Matrix{Int}` of shape `(n_edges, 2)` — from `build_edges`.
- `r_outer::Float64` — outer PEC wall radius (default `2.0`).
- `tol::Float64` — relative tolerance for "on the wall" test (default
  `1e-6`; absolute tol = `tol * max(r_outer, 1.0)`).

# Returns
- `interior::BitVector` of length `n_edges`, `true` for interior edges.
"""
function sphere_pec_interior_edges(nodes::Matrix{Float64}, edges::Matrix{Int};
                                    r_outer::Float64=2.0, tol::Float64=1e-6)
    abs_tol = tol * max(r_outer, 1.0)
    n_nodes = size(nodes, 1)
    node_r  = [sqrt(nodes[i,1]^2 + nodes[i,2]^2 + nodes[i,3]^2) for i in 1:n_nodes]
    on_wall = [abs(node_r[i] - r_outer) < abs_tol for i in 1:n_nodes]

    n_edges  = size(edges, 1)
    interior = BitVector(undef, n_edges)
    @inbounds for e in 1:n_edges
        va = edges[e, 1]
        vb = edges[e, 2]
        # Interior iff NOT (both endpoints on the PEC wall).
        interior[e] = !(on_wall[va] && on_wall[vb])
    end
    return interior
end


end  # module CubeMesh
