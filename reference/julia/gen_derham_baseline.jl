#!/usr/bin/env julia
"""
Generate `reference/fixtures/derham/julia_baseline.json` from the Julia
de Rham reference (Epic #88 / Phase I.2, issue #168).

Sibling of `reference/numpy/gen_derham_baseline.py` (issue #149): same
mesh fixture (`reference/fixtures/sphere_pec/sphere.msh`), same output
schema (`reference/fixtures/derham/baseline.schema.md`), independent
backend. The payload is the row-sorted 0-based CSR triple of each of
d⁰, d¹, d² — **bit-exact integers**, no tolerance question.

# Built-in cross-check

After building the operators, this generator loads the canonical NumPy
baseline at `reference/fixtures/derham/baseline.json` and asserts exact
integer equality of every CSR triple (indptr, indices, data), shape,
nnz, cell count, and rank prediction. Generation **fails** on any
mismatch — the emitted `julia_baseline.json` is guaranteed equal to the
NumPy baseline at the canonicalized CSR-triple level, so the Rust
cross-check in `derham_julia_reference.rs` pins Julia ↔ Burn agreement
through an independently-derived fixture.

# Reproduction

    julia --project=reference/julia reference/julia/gen_derham_baseline.jl \\
        [--mesh reference/fixtures/sphere_pec/sphere.msh] \\
        [--out  reference/fixtures/derham/julia_baseline.json]
"""

push!(LOAD_PATH, @__DIR__)
include(joinpath(@__DIR__, "mesh.jl"))
include(joinpath(@__DIR__, "derham.jl"))

using .CubeMesh: load_msh
using .DerhamRef
using JSON3
using LinearAlgebra
using SparseArrays
using Dates

const NUMPY_BASELINE_PATH = joinpath(
    @__DIR__, "..", "fixtures", "derham", "baseline.json")


# ---------------------------------------------------------------------------
# Argument parsing.
# ---------------------------------------------------------------------------

function _parse_args(argv::Vector{String})
    mesh_path = joinpath(@__DIR__, "..", "fixtures", "sphere_pec", "sphere.msh")
    out_path = joinpath(@__DIR__, "..", "fixtures", "derham", "julia_baseline.json")
    i = 1
    while i <= length(argv)
        arg = argv[i]
        if arg == "--mesh"
            mesh_path = argv[i + 1]; i += 2
        elseif arg == "--out"
            out_path = argv[i + 1]; i += 2
        elseif arg in ("-h", "--help")
            println(stderr,
                "Usage: julia --project=. gen_derham_baseline.jl " *
                "[--mesh ...] [--out ...]")
            exit(0)
        else
            error("unrecognized argument: $arg")
        end
    end
    return (mesh = mesh_path, out = out_path)
end


# ---------------------------------------------------------------------------
# Cross-check vs the canonical NumPy baseline.
# ---------------------------------------------------------------------------

"""Exact integer-vector comparison with a loud first-mismatch report."""
function _assert_int_eq(name::AbstractString, julia_v::Vector{Int}, numpy_v::Vector{Int})
    if length(julia_v) != length(numpy_v)
        error("$name: length mismatch (Julia $(length(julia_v)) vs NumPy $(length(numpy_v)))")
    end
    for i in eachindex(julia_v)
        if julia_v[i] != numpy_v[i]
            lo = max(1, i - 2)
            hi = min(length(julia_v), i + 2)
            error("$name: first disagreement at 0-based index $(i - 1): " *
                  "Julia = $(julia_v[i]), NumPy = $(numpy_v[i])\n" *
                  "  Julia window: $(julia_v[lo:hi])\n" *
                  "  NumPy window: $(numpy_v[lo:hi])")
        end
    end
end

"""Decode an integer-valued f64 output field from a schema-v1 fixture."""
function _numpy_int_field(outputs, name::Symbol)
    haskey(outputs, name) || error("NumPy baseline missing output `$name`")
    return [Int(round(Float64(v))) for v in outputs[name].data]
end

"""
    cross_check_against_numpy(d0, d1, d2, counts, ranks)

Assert exact integer equality of every Julia-side quantity against the
canonical NumPy `baseline.json`. Throws on any mismatch.
"""
function cross_check_against_numpy(d0::CsrInt, d1::CsrInt, d2::CsrInt, counts, ranks)
    isfile(NUMPY_BASELINE_PATH) ||
        error("NumPy baseline not found at $NUMPY_BASELINE_PATH — " *
              "regenerate with `python3 reference/numpy/gen_derham_baseline.py`")
    blob = JSON3.read(read(NUMPY_BASELINE_PATH, String))
    out = blob.outputs

    # Cell counts + Euler characteristic + rank predictions.
    for (name, val) in (
        :n_nodes => counts.n_nodes, :n_edges => counts.n_edges,
        :n_faces => counts.n_faces, :n_tets => counts.n_tets,
        :euler_chi => ranks.euler_chi, :rank_d0 => ranks.rank_d0,
        :rank_d1 => ranks.rank_d1, :rank_d2 => ranks.rank_d2,
        :d1_d0_nnz => 0, :d2_d1_nnz => 0,
    )
        npv = _numpy_int_field(out, name)[1]
        npv == val || error("$name: Julia = $val, NumPy = $npv")
    end

    # CSR triples — the load-bearing bit-exact payload.
    for (prefix, m) in ("d0" => d0, "d1" => d1, "d2" => d2)
        np_shape = _numpy_int_field(out, Symbol("$(prefix)_shape"))
        np_shape == [m.n_rows, m.n_cols] ||
            error("$prefix shape: Julia = $([m.n_rows, m.n_cols]), NumPy = $np_shape")
        np_nnz = _numpy_int_field(out, Symbol("$(prefix)_nnz"))[1]
        np_nnz == length(m.data) ||
            error("$prefix nnz: Julia = $(length(m.data)), NumPy = $np_nnz")
        _assert_int_eq("$(prefix)_indptr", m.indptr,
                       _numpy_int_field(out, Symbol("$(prefix)_indptr")))
        _assert_int_eq("$(prefix)_indices", m.indices,
                       _numpy_int_field(out, Symbol("$(prefix)_indices")))
        _assert_int_eq("$(prefix)_data", m.data,
                       _numpy_int_field(out, Symbol("$(prefix)_data")))
    end

    @info "Cross-check vs NumPy baseline.json PASSED — exact integer " *
          "equality on all CSR triples, counts, and rank predictions."
end


# ---------------------------------------------------------------------------
# Fixture field helpers (schema v1, integer-as-f64 with tolerance 0.5).
# ---------------------------------------------------------------------------

function _scalar_field(description::AbstractString, value::Int)
    return Dict{String,Any}(
        "shape" => [1],
        "dtype" => "f64",
        "description" => description,
        "tolerance_abs" => 0.5,
        "data" => [value],
    )
end

function _shape_field(description::AbstractString, m::CsrInt)
    return Dict{String,Any}(
        "shape" => [2],
        "dtype" => "f64",
        "description" => description,
        "tolerance_abs" => 0.5,
        "data" => [m.n_rows, m.n_cols],
    )
end

function _csr_field(description::AbstractString, arr::Vector{Int})
    return Dict{String,Any}(
        "shape" => [length(arr)],
        "dtype" => "f64",
        "description" => description,
        "tolerance_abs" => 0.5,
        "data" => arr,
    )
end


# ---------------------------------------------------------------------------
# Main.
# ---------------------------------------------------------------------------

function main()
    args = _parse_args(ARGS)
    @info "Generating Julia de Rham baseline fixture" mesh = args.mesh out = args.out

    nodes, tets_1based = load_msh(args.mesh)
    # `load_msh` keeps the .msh file's 1-based node tags; the de Rham
    # cross-check pins **0-based** global indices (NumPy / Rust
    # convention), so the shift happens exactly once, here.
    tets = tets_1based .- 1
    n_nodes = size(nodes, 1)
    n_tets = size(tets, 1)

    edges = build_edges(tets)
    faces = build_faces(tets)
    n_edges = size(edges, 1)
    n_faces = size(faces, 1)

    d0 = gradient_map(n_nodes, edges)
    d1 = curl_map(edges, faces)
    d2 = divergence_map(tets, faces)

    println("  n_nodes=$n_nodes, n_edges=$n_edges, n_faces=$n_faces, n_tets=$n_tets")
    println("  d0: shape=($(d0.n_rows), $(d0.n_cols)), nnz=$(length(d0.data))")
    println("  d1: shape=($(d1.n_rows), $(d1.n_cols)), nnz=$(length(d1.data))")
    println("  d2: shape=($(d2.n_rows), $(d2.n_cols)), nnz=$(length(d2.data))")

    # Compositional identities — go through SparseArrays (CSC-native;
    # layout-invariant for a zero test).
    s0 = to_sparse(d0)
    s1 = to_sparse(d1)
    s2 = to_sparse(d2)
    d1_d0 = dropzeros!(s1 * s0)
    d2_d1 = dropzeros!(s2 * s1)
    println("  d1 * d0: nnz=$(nnz(d1_d0)) (should be 0)")
    println("  d2 * d1: nnz=$(nnz(d2_d1)) (should be 0)")
    nnz(d1_d0) == 0 ||
        error("d¹ · d⁰ is not exactly zero (nnz = $(nnz(d1_d0))); the Julia " *
              "de Rham reference is broken — sign-convention drift in " *
              "build_edges / build_faces / curl_map.")
    nnz(d2_d1) == 0 ||
        error("d² · d¹ is not exactly zero (nnz = $(nnz(d2_d1))); the Julia " *
              "de Rham reference is broken — sign-convention drift in " *
              "build_tet_faces / divergence_map.")

    ranks = euler_ranks(n_nodes, n_edges, n_faces, n_tets)
    println("  Euler χ = $(ranks.euler_chi) (expected 1 for a ball); " *
            "rank predictions: d0=$(ranks.rank_d0), d1=$(ranks.rank_d1), " *
            "d2=$(ranks.rank_d2)")
    ranks.euler_chi == 1 ||
        error("Euler χ = $(ranks.euler_chi) ≠ 1 — the mesh is not a " *
              "contractible 3-ball as assumed.")

    # Sanity: measured ranks (dense SVD on the integer matrices) match
    # the Euler predictions exactly — same guard as the NumPy generator.
    @info "Measuring dense SVD ranks (one-time generator cost)..."
    r0 = rank(Matrix{Float64}(to_sparse(d0)))
    r1 = rank(Matrix{Float64}(s1))
    r2 = rank(Matrix{Float64}(s2))
    (r0, r1, r2) == (ranks.rank_d0, ranks.rank_d1, ranks.rank_d2) ||
        error("measured ranks ($r0, $r1, $r2) disagree with Euler " *
              "predictions ($(ranks.rank_d0), $(ranks.rank_d1), " *
              "$(ranks.rank_d2)).")
    println("  measured ranks: d0=$r0, d1=$r1, d2=$r2 (match Euler predictions)")

    # Cross-check vs the canonical NumPy baseline before emitting.
    counts = (n_nodes = n_nodes, n_edges = n_edges, n_faces = n_faces, n_tets = n_tets)
    cross_check_against_numpy(d0, d1, d2, counts, ranks)

    # ----------------------- fixture assembly ----------------------------
    fixture = Dict{String,Any}(
        "schema_version" => "1",
        "fixture_id" => "derham/sphere_n774_d0_d1_d2_julia",
        "description" =>
            "Discrete de Rham complex (d⁰, d¹, d²) on the bundled sphere " *
            "fixture — issue #168, parent epic #88, Phase I.2. Julia " *
            "reference for geode_core::derham::{gradient_map, curl_map, " *
            "divergence_map}, generated by reference/julia/derham.jl and " *
            "verified exactly equal to the NumPy baseline " *
            "(reference/fixtures/derham/baseline.json) at the " *
            "canonicalized 0-based row-sorted CSR-triple level.",
        "units" => "dimensionless integer incidence matrices",
        "inputs" => Dict{String,Any}(
            "mesh_path" => Dict{String,Any}(
                "shape" => [0],
                "dtype" => "f64",
                "description" =>
                    "Mesh fixture (relative to repo root): " *
                    "reference/fixtures/sphere_pec/sphere.msh — bundled " *
                    "sphere-in-vacuum mesh, $n_nodes nodes, $n_tets tets. " *
                    "Same .msh consumed by the NumPy de Rham baseline.",
                "data" => [],
            ),
        ),
        "outputs" => Dict{String,Any}(
            "n_nodes" => _scalar_field("Number of mesh nodes.", n_nodes),
            "n_edges" => _scalar_field(
                "Number of mesh edges (TetMesh::edges dedup order).", n_edges),
            "n_faces" => _scalar_field(
                "Number of mesh faces (TetMesh::faces dedup order).", n_faces),
            "n_tets" => _scalar_field("Number of mesh tets.", n_tets),
            "euler_chi" => _scalar_field(
                "Euler characteristic χ = n_nodes − n_edges + n_faces − " *
                "n_tets. Equals 1 for a contractible 3-ball.",
                ranks.euler_chi),
            # d⁰ — discrete gradient.
            "d0_shape" => _shape_field("d⁰ shape [n_edges, n_nodes].", d0),
            "d0_nnz" => _scalar_field("d⁰ nnz (= 2 · n_edges).", length(d0.data)),
            "d0_indptr" => _csr_field("d⁰ CSR indptr (row pointers).", d0.indptr),
            "d0_indices" => _csr_field(
                "d⁰ CSR column indices, sorted ascending within each row.",
                d0.indices),
            "d0_data" => _csr_field(
                "d⁰ CSR data — signed integers in {-1, +1}.", d0.data),
            # d¹ — discrete curl.
            "d1_shape" => _shape_field("d¹ shape [n_faces, n_edges].", d1),
            "d1_nnz" => _scalar_field("d¹ nnz (= 3 · n_faces).", length(d1.data)),
            "d1_indptr" => _csr_field("d¹ CSR indptr (row pointers).", d1.indptr),
            "d1_indices" => _csr_field(
                "d¹ CSR column indices, sorted ascending within each row.",
                d1.indices),
            "d1_data" => _csr_field(
                "d¹ CSR data — signed integers in {-1, +1}.", d1.data),
            # d² — discrete divergence.
            "d2_shape" => _shape_field("d² shape [n_tets, n_faces].", d2),
            "d2_nnz" => _scalar_field("d² nnz (= 4 · n_tets).", length(d2.data)),
            "d2_indptr" => _csr_field("d² CSR indptr (row pointers).", d2.indptr),
            "d2_indices" => _csr_field(
                "d² CSR column indices, sorted ascending within each row.",
                d2.indices),
            "d2_data" => _csr_field(
                "d² CSR data — signed integers in {-1, +1}.", d2.data),
            # Compositional identities.
            "d1_d0_nnz" => _scalar_field(
                "Bit-exact d¹ · d⁰ ≡ 0 (nnz after dropzeros!).", 0),
            "d2_d1_nnz" => _scalar_field(
                "Bit-exact d² · d¹ ≡ 0 (nnz after dropzeros!).", 0),
            # Rank predictions (Euler arithmetic on a contractible 3-ball).
            "rank_d0" => _scalar_field(
                "rank(d⁰) = n_nodes − 1 (β_0 = 1 on a connected mesh).",
                ranks.rank_d0),
            "rank_d1" => _scalar_field(
                "rank(d¹) = n_edges − n_nodes + 1 (β_1 = 0 on a ball).",
                ranks.rank_d1),
            "rank_d2" => _scalar_field(
                "rank(d²) = n_faces − n_edges + n_nodes − 1 (β_2 = 0 on a ball).",
                ranks.rank_d2),
        ),
        "provenance" => Dict{String,Any}(
            "source" =>
                "reference/julia/derham.jl — Julia reimplementation of " *
                "geode_core::derham::{gradient_map, curl_map, divergence_map} " *
                "with matched sign conventions (TET_LOCAL_EDGES, " *
                "TET_LOCAL_FACES, simplicial boundary alternation). " *
                "Operators built directly in 0-based row-sorted CSR " *
                "(SparseArrays' CSC storage is used only for the " *
                "layout-invariant compositional-identity zero tests).",
            "verified_against" =>
                "reference/fixtures/derham/baseline.json (NumPy #149 " *
                "canonical baseline; exact integer CSR-triple equality " *
                "asserted by this generator) AND " *
                "crates/geode-validation/tests/derham_julia_reference.rs " *
                "(bit-exact integer CSR equality between Burn and Julia).",
            "julia_version" => string(VERSION),
            "issue" => "#168 (Epic #88, Phase I.2)",
            "regenerated_at" => string(now()),
        ),
    )

    out_dir = dirname(args.out)
    isdir(out_dir) || mkpath(out_dir)
    open(args.out, "w") do io
        JSON3.pretty(io, fixture)
        write(io, "\n")
    end
    @info "Wrote Julia de Rham baseline fixture" path = args.out size_kb = round(filesize(args.out) / 1024; digits = 1)
end


if abspath(PROGRAM_FILE) == @__FILE__
    main()
end
