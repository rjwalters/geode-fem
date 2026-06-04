"""
Mesh primitives for the cube-cavity Julia reference (Epic #88 / Phase E).

Local equivalent of `reference/numpy/mesh.py` — provides the canonical
programmatic n-per-side tet-split unit cube, an inline MSH 4.1 ASCII
parser, and the interior-DOF Dirichlet mask. We do **not** depend on
Gmsh.jl: the libgmsh native dependency is ~50 MB and the MSH 4.1 ASCII
files we consume here are simple enough to parse inline (see Open
Question #3 in the curator pass on issue #115).

Public API
==========

- `cube_tet_mesh(n; side=1.0)` — generate the n-per-side tet-split unit
  cube. Mirror of `geode_core::mesh::cube_tet_mesh` and
  `reference/numpy/mesh.py::cube_tet_mesh`.
- `cube_interior_mask(nodes; side=1.0)` — boolean mask, true for interior
  (free-DOF) nodes of `[0, side]^3`.
- `load_msh(path)` — read a Gmsh `.msh` (MSH 4.1 ASCII) and return
  `(nodes, tets)` as `(Matrix{Float64}, Matrix{Int})`.

Node ordering matches Burn / NumPy exactly so the same node-indexed
Dirichlet mask works on all three backends.
"""
module CubeMesh

export cube_tet_mesh, cube_interior_mask, load_msh

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

end  # module CubeMesh
