#!/usr/bin/env julia
"""
Generate `reference/fixtures/mie_roots/julia_baseline.json` from the Julia
analytic Mie root catalogue (Epic #88 / Phase J.3, issue #172) and
cross-check it root-for-root against the merged J.1 SciPy catalogue
(`reference/fixtures/mie_roots/baseline.json`, issue #170 / PR #177).

# Output schema

Mirrors the canonical NumPy baseline field-for-field (same parallel
arrays in the same canonical `(pol, l, n)` sort, `pol`: 0 = TE, 1 = TM)
so the Rust cross-IR test (`mie_roots_julia_reference.rs`) reuses the
J.1 accessors unchanged.

# Cross-check (the load-bearing step)

Before writing anything, the generator joins the Julia catalogue against
the J.1 baseline on the exact `(pol, l, n)` key and **throws** if:

  * any key exists on one side only (bracket / pole-rejection drift), or
  * any root disagrees beyond **1e-10 relative** (the issue #170/#172
    cross-check contract).

The measured worst-case relative error is recorded in the fixture as
`cross_check_max_rel_vs_scipy` so the agreement claim is pinned on disk,
not just asserted at generation time.

# Usage

    julia --project=reference/julia \\
        reference/julia/gen_mie_roots_julia_baseline.jl \\
        [--out reference/fixtures/mie_roots/julia_baseline.json]
"""

include(joinpath(@__DIR__, "mie_roots.jl"))

using JSON3
using Printf

# Catalogue extent — mirror examples/mie_sphere.rs L_MAX / N_MAX and the
# J.1 generator.
const L_MAX::Int = 4
const N_MAX::Int = 5

# Cross-check contract from issues #170 / #172: ≤ 1e-10 relative.
const CROSS_CHECK_REL_TOL::Float64 = 1e-10

# Absolute tolerance on root positions in the emitted fixture — same
# bound as the J.1 baseline (≤ 1e-10 relative on k ≤ 20 → 2e-9 abs).
const K_TOLERANCE_ABS::Float64 = 2e-9

const NUMPY_BASELINE_PATH::String =
    joinpath(@__DIR__, "..", "fixtures", "mie_roots", "baseline.json")
const DEFAULT_OUT_PATH::String =
    joinpath(@__DIR__, "..", "fixtures", "mie_roots", "julia_baseline.json")

pol_index(pol::Symbol) = pol === :TE ? 0 : 1


function _parse_gen_args(argv::Vector{String})
    out_path = DEFAULT_OUT_PATH
    i = 1
    while i <= length(argv)
        arg = argv[i]
        if arg == "--out"
            out_path = argv[i + 1]; i += 2
        elseif arg in ("-h", "--help")
            println(stderr,
                "Usage: julia --project=. gen_mie_roots_julia_baseline.jl [--out path]")
            exit(0)
        else
            error("unrecognized argument: $arg")
        end
    end
    return (out = out_path,)
end


# ---------------------------------------------------------------------------
# Cross-check against the J.1 SciPy catalogue.
# ---------------------------------------------------------------------------

"""
    load_numpy_root_map(path) -> Dict{Tuple{Int,Int,Int},Float64}

Load the J.1 baseline's parallel root arrays into a
`(pol, l, n) → k` map. Throws if the file is missing — the cross-check
is the point of this generator, not an optional extra.
"""
function load_numpy_root_map(path::AbstractString)
    isfile(path) || error("J.1 catalogue not found at $path — Phase J.3 " *
                          "is blocked on the merged J.1 baseline (PR #177).")
    blob = JSON3.read(read(path, String))
    out  = blob.outputs
    pols  = collect(Float64, out.root_pol.data)
    ls    = collect(Float64, out.root_l.data)
    ns    = collect(Float64, out.root_n.data)
    ks    = collect(Float64, out.root_k.data)
    n     = length(ks)
    @assert length(pols) == n && length(ls) == n && length(ns) == n
    map = Dict{Tuple{Int,Int,Int},Float64}()
    for i in 1:n
        key = (Int(round(pols[i])), Int(round(ls[i])), Int(round(ns[i])))
        @assert !haskey(map, key) "duplicate J.1 key $key"
        map[key] = ks[i]
    end
    return map
end

"""
    cross_check_against_numpy(julia_roots, numpy_map; rel_tol) -> Float64

Join the Julia catalogue against the J.1 map on `(pol, l, n)`; assert
key-set equality and per-root relative agreement ≤ `rel_tol`. Returns
the worst observed relative error.
"""
function cross_check_against_numpy(
        julia_roots::Vector{MieRootJl},
        numpy_map  ::Dict{Tuple{Int,Int,Int},Float64};
        rel_tol    ::Float64 = CROSS_CHECK_REL_TOL,
)
    julia_map = Dict{Tuple{Int,Int,Int},Float64}()
    for r in julia_roots
        key = (pol_index(r.pol), r.l, r.n)
        @assert !haskey(julia_map, key) "duplicate Julia key $key"
        julia_map[key] = r.k
    end

    for key in keys(julia_map)
        haskey(numpy_map, key) || error(
            "Julia has root $key that the J.1 catalogue lacks " *
            "(bracket / pole-rejection drift — friction finding, do not paper over)")
    end
    for key in keys(numpy_map)
        haskey(julia_map, key) || error(
            "J.1 catalogue has root $key that Julia lacks " *
            "(root-window edge effect — friction finding, do not paper over)")
    end

    worst_rel = 0.0
    worst_key = (0, 0, 0)
    for (key, k_jl) in julia_map
        k_np = numpy_map[key]
        rel  = abs(k_jl - k_np) / abs(k_np)
        if rel > worst_rel
            worst_rel = rel
            worst_key = key
        end
        rel <= rel_tol || error(
            "root $key disagrees: Julia k = $k_jl, SciPy k = $k_np, " *
            "relative error $rel > $rel_tol — characteristic-function or " *
            "Bessel-lineage disagreement, record as friction on #5")
    end
    @printf("cross-check vs J.1 SciPy catalogue: %d roots agree; worst rel err %.3e at (pol=%d, l=%d, n=%d)\n",
            length(julia_map), worst_rel, worst_key[1], worst_key[2], worst_key[3])
    return worst_rel
end


# ---------------------------------------------------------------------------
# Fixture field helpers (canonical schema v1).
# ---------------------------------------------------------------------------

function int_field(description::String, values::Vector{Int})
    return Dict{String,Any}(
        "shape"         => [length(values)],
        "dtype"         => "f64",
        "description"   => description,
        "tolerance_abs" => 0.5,
        "data"          => Float64.(values),
    )
end

scalar_int_field(description::String, value::Int) = int_field(description, [value])

function scalar_f64_field(description::String, value::Float64, tol::Float64)
    return Dict{String,Any}(
        "shape"         => [1],
        "dtype"         => "f64",
        "description"   => description,
        "tolerance_abs" => tol,
        "data"          => [value],
    )
end


# ---------------------------------------------------------------------------
# Main.
# ---------------------------------------------------------------------------

function main_gen_mie_roots()
    args = _parse_gen_args(ARGS)

    @printf("Julia Mie root catalogue: n = %.1f, R_s = %.1f, R_b = %.1f, l_max = %d, n_max = %d\n",
            MIE_N_INSIDE, MIE_R_SPHERE, MIE_R_BUFFER, L_MAX, N_MAX)
    @printf("  k window (%.1f, %.1f] @ %d samples\n", MIE_K_MIN, MIE_K_MAX, MIE_N_SAMPLES)

    # Per-channel root lists in canonical (pol, l) order.
    roots    = MieRootJl[]
    count_te = Int[]
    count_tm = Int[]
    for (pol, counts) in ((:TE, count_te), (:TM, count_tm))
        for l in 1:L_MAX
            channel = resonance_roots(pol, MIE_N_INSIDE, l,
                                      MIE_R_SPHERE, MIE_R_BUFFER, N_MAX)
            push!(counts, length(channel))
            append!(roots, channel)
            @printf("  %s l=%d: %d roots %s\n", pol, l, length(channel),
                    string([round(r.k; digits=6) for r in channel]))
        end
    end

    # Canonical ordering: (pol_index, l, n).
    sort!(roots; by = r -> (pol_index(r.pol), r.l, r.n))
    n_roots = length(roots)
    println("  total: $n_roots roots")

    # Structural sanity — same gate as the J.1 generator.
    expected = 2 * L_MAX * N_MAX
    n_roots == expected || error(
        "catalogue has $n_roots roots, expected $expected; a bracket was " *
        "rejected or a window starved — investigate before regenerating.")
    for r in roots
        (isfinite(r.k) && MIE_K_MIN < r.k <= MIE_K_MAX) ||
            error("root out of window: $r")
        r.multiplicity == 2 * r.l + 1 || error("bad multiplicity: $r")
    end

    # ------------------- cross-check vs J.1 (load-bearing) ----------------
    numpy_map = load_numpy_root_map(NUMPY_BASELINE_PATH)
    max_rel = cross_check_against_numpy(roots, numpy_map)

    # ------------------------- fixture assembly ---------------------------
    fixture = Dict{String,Any}(
        "schema_version" => "1",
        "fixture_id"     => "mie_roots/n15_pec_cavity_l4_n5_julia",
        "description"    =>
            "Analytic Mie resonance root catalogue for the dielectric " *
            "sphere (n = 1.5, R_s = 1.0) inside a PEC cavity (R_b = 2.0) " *
            "— issue #172, parent epic #88, Phase J.3. Julia reference " *
            "(reference/julia/mie_roots.jl) using SpecialFunctions.jl " *
            "half-order besselj/bessely (openspecfun/AMOS lineage — " *
            "independent of both scipy.special and the hand-rolled Burn " *
            "ladder) with the identical dense-sampling bracket walk. " *
            "TE/TM roots for l = 1..4, first 5 roots per channel, stored " *
            "sorted by the canonical (pol, l, n) key (pol: 0 = TE, 1 = TM) " *
            "— same layout as the J.1 baseline.json.",
        "units" => "k in inverse length, same units as R_s / R_b (dimensionless geometry)",
        "inputs" => Dict{String,Any}(
            "n_inside" => Dict{String,Any}(
                "shape"       => [1],
                "dtype"       => "f64",
                "description" => "Refractive index of the inner sphere.",
                "data"        => [MIE_N_INSIDE],
            ),
            "r_sphere" => Dict{String,Any}(
                "shape"       => [1],
                "dtype"       => "f64",
                "description" => "Inner sphere radius R_s (Burn: mesh::R_SPHERE).",
                "data"        => [MIE_R_SPHERE],
            ),
            "r_buffer" => Dict{String,Any}(
                "shape"       => [1],
                "dtype"       => "f64",
                "description" => "PEC wall radius R_b (Burn: mesh::R_BUFFER).",
                "data"        => [MIE_R_BUFFER],
            ),
            "k_window" => Dict{String,Any}(
                "shape"       => [2],
                "dtype"       => "f64",
                "description" => "Root search window (k_min, k_max].",
                "data"        => [MIE_K_MIN, MIE_K_MAX],
            ),
        ),
        "outputs" => Dict{String,Any}(
            "l_max" => scalar_int_field(
                "Maximum angular order in the catalogue (matches " *
                "examples/mie_sphere.rs L_MAX).", L_MAX),
            "n_max" => scalar_int_field(
                "Roots per (l, polarisation) channel (matches " *
                "examples/mie_sphere.rs N_MAX).", N_MAX),
            "n_roots" => scalar_int_field("Total catalogued roots.", n_roots),
            "n_inside" => scalar_f64_field(
                "Refractive index (replicated as output for harness-side " *
                "constant pinning).", MIE_N_INSIDE, 1e-15),
            "r_sphere" => scalar_f64_field(
                "R_s (replicated as output; must equal Burn mesh::R_SPHERE).",
                MIE_R_SPHERE, 1e-15),
            "r_buffer" => scalar_f64_field(
                "R_b (replicated as output; must equal Burn mesh::R_BUFFER).",
                MIE_R_BUFFER, 1e-15),
            "root_pol" => int_field(
                "Polarisation tag per root: 0 = TE, 1 = TM.",
                [pol_index(r.pol) for r in roots]),
            "root_l" => int_field(
                "Angular order l per root.", [r.l for r in roots]),
            "root_n" => int_field(
                "Radial order n per root (1 = lowest in window).",
                [r.n for r in roots]),
            "root_multiplicity" => int_field(
                "Degeneracy 2l + 1 per root.",
                [r.multiplicity for r in roots]),
            "root_k" => Dict{String,Any}(
                "shape"         => [n_roots],
                "dtype"         => "f64",
                "description"   =>
                    "Resonance positions k, ordered by (pol, l, n) in " *
                    "lockstep with root_pol / root_l / root_n. Bisection-" *
                    "refined to f64 exhaustion; cross-check contract is " *
                    "<= 1e-10 relative (vs both the J.1 SciPy catalogue " *
                    "and geode_core::mie).",
                "tolerance_abs" => K_TOLERANCE_ABS,
                "data"          => [r.k for r in roots],
            ),
            "root_count_te" => int_field(
                "TE root count per l = 1..l_max (after the n_max cap).",
                count_te),
            "root_count_tm" => int_field(
                "TM root count per l = 1..l_max (after the n_max cap).",
                count_tm),
            "cross_check_max_rel_vs_scipy" => scalar_f64_field(
                "Worst-case relative |Δk|/k observed in the generation-" *
                "time root-for-root join against the J.1 SciPy catalogue " *
                "(reference/fixtures/mie_roots/baseline.json). Pinned on " *
                "disk so the <= 1e-10 agreement claim is auditable; the " *
                "generator throws if any root exceeds 1e-10 relative.",
                max_rel, CROSS_CHECK_REL_TOL),
        ),
        "provenance" => Dict{String,Any}(
            "source" =>
                "reference/julia/mie_roots.jl — SpecialFunctions.jl " *
                "half-order besselj/bessely (j_l(x) = sqrt(pi/2x) " *
                "J_{l+1/2}(x)) + bisection-to-f64-exhaustion refinement, " *
                "with the identical dense-sampling bracket walk as " *
                "geode_core::mie (30000 samples on (0.1, 20.0], 1e8 " *
                "pole-rejection, 1e-5 consecutive dedup).",
            "julia_version" => string(VERSION),
            "verified_against" =>
                "reference/fixtures/mie_roots/baseline.json (J.1 SciPy " *
                "catalogue; root-for-root (pol, l, n) join at <= 1e-10 " *
                "relative in this generator) AND " *
                "crates/geode-validation/tests/mie_roots_julia_reference.rs " *
                "(Burn-side join at the same contract).",
            "issue" => "#172 (Epic #88, Phase J.3; catalogue extent from #170)",
        ),
    )

    out_dir = dirname(args.out)
    isdir(out_dir) || mkpath(out_dir)
    open(args.out, "w") do io
        JSON3.pretty(io, fixture)
        write(io, "\n")
    end
    println("Wrote $(args.out)")
end


if abspath(PROGRAM_FILE) == @__FILE__
    main_gen_mie_roots()
end
