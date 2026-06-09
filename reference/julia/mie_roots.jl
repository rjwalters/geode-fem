#!/usr/bin/env julia
"""
Julia reference for the analytic Mie root catalogue (Epic #88 / Phase J.3,
issue #172).

Independent second opinion on the Phase J.1 analytic anchor
(`reference/fixtures/mie_roots/baseline.json`, issue #170): the analytic
resonance roots of a dielectric sphere (`n = 1.5`, `R_s = 1.0`) inside a
PEC cavity (`R_b = 2.0`) — the v0 Mie benchmark ground truth shared by
`crates/geode-core/src/mie.rs` (hand-rolled Bessel ladders + bisection)
and `reference/numpy/mie_roots.py` (scipy.special + brentq).

# Why this is a *genuinely independent* cross-check

The spherical Bessel functions here come from **SpecialFunctions.jl
half-order cylindrical Bessel functions** (openspecfun/AMOS lineage):

    j_l(x) = sqrt(π / (2x)) · J_{l+1/2}(x)
    y_l(x) = sqrt(π / (2x)) · Y_{l+1/2}(x)

scipy's `spherical_jn` / `spherical_yn` use their own recurrence-based
implementation, and the Burn side hand-rolls the ladder (upward for
`l ≤ x + 1`, Miller's downward above). Three implementations from three
lineages agreeing at ≤ 1e-10 relative pins the *mathematics* of the
characteristic functions, not any shared special-function code path.

# Bracketing parity (structural, not accidental)

The dense-sampling bracket walk replicates `geode_core::mie::find_roots`
and `reference/numpy/mie_roots.py::find_roots` exactly: same `k` window
`(0.1, 20.0]`, same 30_000-interval grid, same pole-rejection heuristic
(skip sign changes where both endpoint magnitudes exceed 1e8), same
consecutive dedup at 1e-5. Root-count agreement per `(l, polarisation)`
channel is therefore structural. Refinement is plain bisection driven to
floating-point exhaustion (the midpoint stops moving) — ~50 iterations
from the 6.6e-4-wide grid bracket, well inside the 1e-10 relative
cross-check budget and dependency-free (no Roots.jl needed; the issue
allows "an equivalent bracketing+refinement approach").

# Riccati-Bessel derivative identities

Rather than differentiating the half-order Bessel calls numerically, the
derivatives use the standard recurrences (Abramowitz & Stegun 10.1.21/22
in Riccati form):

    ψ_l(x)  =  x · j_l(x)            ψ_l'(x) =  x · j_{l-1}(x) − l · j_l(x)
    χ_l(x)  = −x · y_l(x)            χ_l'(x) =  l · y_l(x) − x · y_{l-1}(x)

(`l ≥ 1` throughout, so the `l − 1` order is ≥ 0 — order `1/2`
cylindrical Bessel at the lowest.)

# Public API

  * `psi(l, x)`, `psi_prime(l, x)`, `chi(l, x)`, `chi_prime(l, x)`
  * `characteristic_te(n, l, r_s, r_b, k)`, `characteristic_tm(...)`
  * `find_roots(f, k_min, k_max, n_samples) -> Vector{Float64}`
  * `resonance_roots(pol, n, l, r_s, r_b, n_max) -> Vector{MieRootJl}`
  * `merged_roots(n, l_set, r_s, r_b, n_max)`
  * `mie_roots_catalog(m, l_max, n_max)`

Run `julia --project=. mie_roots.jl` for the built-in self-check.
"""

using SpecialFunctions: besselj, bessely
using Printf

# ---------------------------------------------------------------------------
# Geometry / material constants — mirror the Burn and NumPy sides.
# `N_INSIDE` matches `crates/geode-core/examples/mie_sphere.rs::N_INSIDE`;
# the radii match `crates/geode-core/src/mesh/sphere.rs` (and the
# R_SPHERE / R_BUFFER constants in sphere_pec.jl — kept module-local here
# so mie_roots.jl is standalone-includable without the mesh stack).
# ---------------------------------------------------------------------------

const MIE_N_INSIDE::Float64 = 1.5
const MIE_R_SPHERE::Float64 = 1.0
const MIE_R_BUFFER::Float64 = 2.0

# Root-search window and sampling density — mirror
# `geode_core::mie::resonance_roots` exactly so the bracket set matches.
const MIE_K_MIN::Float64 = 0.1
const MIE_K_MAX::Float64 = 20.0
const MIE_N_SAMPLES::Int = 30_000

# Consecutive near-duplicate dedup tolerance (Rust: `dedup_by` at 1e-5).
const MIE_DEDUP_TOL::Float64 = 1e-5

# Pole-rejection scale (Rust: skip brackets where min(|fa|, |fb|) > 1e8).
const MIE_POLE_SCALE_REJECT::Float64 = 1e8

"""One analytic resonance root (mirror of `geode_core::mie::MieRoot`)."""
struct MieRootJl
    pol          ::Symbol   # :TE or :TM
    l            ::Int      # angular order, ≥ 1
    n            ::Int      # radial order, ≥ 1 (1 = lowest in window)
    k            ::Float64  # resonance position
    multiplicity ::Int      # 2l + 1
end


# ---------------------------------------------------------------------------
# Spherical Bessel via half-order cylindrical Bessel (SpecialFunctions.jl).
# ---------------------------------------------------------------------------

"""Spherical Bessel ``j_l(x) = sqrt(π/(2x)) J_{l+1/2}(x)``."""
sph_jl(l::Int, x::Float64) = sqrt(pi / (2.0 * x)) * besselj(l + 0.5, x)

"""Spherical Bessel ``y_l(x) = sqrt(π/(2x)) Y_{l+1/2}(x)``."""
sph_yl(l::Int, x::Float64) = sqrt(pi / (2.0 * x)) * bessely(l + 0.5, x)


# ---------------------------------------------------------------------------
# Riccati-Bessel functions (Bohren-Huffman convention, matching mie.rs).
# ---------------------------------------------------------------------------

"""Riccati-Bessel ``ψ_l(x) = x · j_l(x)``."""
psi(l::Int, x::Float64) = x * sph_jl(l, x)

"""Derivative ``ψ_l'(x) = x · j_{l-1}(x) − l · j_l(x)`` (recurrence form)."""
psi_prime(l::Int, x::Float64) = x * sph_jl(l - 1, x) - l * sph_jl(l, x)

"""Riccati-Bessel ``χ_l(x) = −x · y_l(x)``."""
chi(l::Int, x::Float64) = -x * sph_yl(l, x)

"""Derivative ``χ_l'(x) = l · y_l(x) − x · y_{l-1}(x)`` (recurrence form)."""
chi_prime(l::Int, x::Float64) = l * sph_yl(l, x) - x * sph_yl(l - 1, x)


# ---------------------------------------------------------------------------
# Characteristic functions — direct transliteration of
# `characteristic_te` / `characteristic_tm` in mie.rs (and mie_roots.py).
# ---------------------------------------------------------------------------

"""
    characteristic_te(n, l, r_s, r_b, k) -> Float64

TE characteristic function; zeros in `k` are TE resonances.

Buffer coefficients up to overall scale: `A = χ(x_b)`, `B = ψ(x_b)` so
that `A ψ(x_b) − B χ(x_b) = 0` (PEC, E_θ = 0 at the wall) with no
spurious pole when `χ(x_b) → 0`. Matching at `r = R_s`:
`ψ(x_in) buf' − ψ'(x_in)/n · buf = 0`.
"""
function characteristic_te(n::Float64, l::Int, r_s::Float64, r_b::Float64, k::Float64)
    x_in = n * k * r_s
    x_s  = k * r_s
    x_b  = k * r_b

    big_a = chi(l, x_b)
    big_b = psi(l, x_b)

    buf       = big_a * psi(l, x_s)       - big_b * chi(l, x_s)
    buf_prime = big_a * psi_prime(l, x_s) - big_b * chi_prime(l, x_s)

    return psi(l, x_in) * buf_prime - (psi_prime(l, x_in) / n) * buf
end

"""
    characteristic_tm(n, l, r_s, r_b, k) -> Float64

TM characteristic function; zeros in `k` are TM resonances.

TM PEC condition is `ψ'(x_b) = 0`, so `A = χ'(x_b)`, `B = ψ'(x_b)`;
the magnetic matching picks up the permittivity factor:
`ψ(x_in) buf' − n ψ'(x_in) buf = 0`.
"""
function characteristic_tm(n::Float64, l::Int, r_s::Float64, r_b::Float64, k::Float64)
    x_in = n * k * r_s
    x_s  = k * r_s
    x_b  = k * r_b

    big_a = chi_prime(l, x_b)
    big_b = psi_prime(l, x_b)

    buf       = big_a * psi(l, x_s)       - big_b * chi(l, x_s)
    buf_prime = big_a * psi_prime(l, x_s) - big_b * chi_prime(l, x_s)

    return psi(l, x_in) * buf_prime - n * psi_prime(l, x_in) * buf
end


# ---------------------------------------------------------------------------
# Root finding — same dense-sampling bracket walk as mie.rs::find_roots,
# with bisection-to-float-exhaustion refinement (no Roots.jl dependency).
# ---------------------------------------------------------------------------

"""
    bisect_root(f, a, b, fa, fb) -> Float64

Refine a sign-change bracket `[a, b]` (with `f(a) = fa`, `f(b) = fb`,
`fa * fb ≤ 0`) by plain bisection until the midpoint stops moving in
Float64 (floating-point exhaustion — ≤ ~60 iterations from the 6.6e-4
grid bracket). Deterministic and dependency-free; terminal accuracy is
the f64 spacing at the root, far inside the 1e-10 relative cross-check
budget (mie.rs bisects 60 fixed steps to the same effect).
"""
function bisect_root(f, a::Float64, b::Float64, fa::Float64, fb::Float64)
    if fa == 0.0
        return a
    end
    if fb == 0.0
        return b
    end
    while true
        m = 0.5 * (a + b)
        if m <= a || m >= b
            # Interval collapsed to adjacent floats — done.
            return m
        end
        fm = f(m)
        if fm == 0.0
            return m
        end
        if (fa < 0.0) == (fm < 0.0)
            a, fa = m, fm
        else
            b, fb = m, fm
        end
    end
end

"""
    find_roots(f, k_min, k_max, n_samples) -> Vector{Float64}

Real roots of `f` on `[k_min, k_max]`. Dense sampling on the same grid
as the Rust side (`n_samples` intervals, endpoints included),
sign-change bracketing with the same pole-rejection heuristic, then
bisection refinement.
"""
function find_roots(f, k_min::Float64, k_max::Float64, n_samples::Int)
    @assert k_max > k_min
    @assert n_samples >= 3

    dk = (k_max - k_min) / n_samples
    ks = [k_min + dk * i for i in 0:n_samples]
    fs = [f(k) for k in ks]

    roots = Float64[]
    for i in 1:n_samples
        a, b   = ks[i], ks[i + 1]
        fa, fb = fs[i], fs[i + 1]
        (isfinite(fa) && isfinite(fb)) || continue
        (fa == 0.0 && fb == 0.0) && continue
        fa * fb > 0.0 && continue
        # Reject brackets where the *magnitude* on both sides is
        # enormous — spurious sign flips across large excursions of the
        # characteristic function (mirror of the Rust 1e8 heuristic).
        min(abs(fa), abs(fb)) > MIE_POLE_SCALE_REJECT && continue
        push!(roots, bisect_root(f, a, b, fa, fb))
    end
    return roots
end

"""
    dedup_consecutive(values, tol) -> Vector{Float64}

Mirror of Rust `Vec::dedup_by` with `|a − b| < tol`: drop a value when
it is within `tol` of the previously *retained* one.
"""
function dedup_consecutive(values::Vector{Float64}, tol::Float64)
    out = Float64[]
    for v in values
        if !isempty(out) && abs(v - out[end]) < tol
            continue
        end
        push!(out, v)
    end
    return out
end

"""
    resonance_roots(pol, n, l, r_s, r_b, n_max) -> Vector{MieRootJl}

Lowest `n_max` resonance roots for one `(l, polarisation)` channel —
mirror of `geode_core::mie::resonance_roots`. `pol` is `:TE` or `:TM`.
"""
function resonance_roots(pol::Symbol, n::Float64, l::Int,
                         r_s::Float64, r_b::Float64, n_max::Int)
    @assert n > 0.0
    @assert l >= 1
    @assert r_b > r_s
    @assert pol in (:TE, :TM)

    f = pol === :TE ?
        (k -> characteristic_te(n, l, r_s, r_b, k)) :
        (k -> characteristic_tm(n, l, r_s, r_b, k))

    raw = find_roots(f, MIE_K_MIN, MIE_K_MAX, MIE_N_SAMPLES)
    raw = dedup_consecutive(raw, MIE_DEDUP_TOL)

    n_keep = min(n_max, length(raw))
    return [MieRootJl(pol, l, idx, raw[idx], 2 * l + 1) for idx in 1:n_keep]
end

"""
    merged_roots(n, l_set, r_s, r_b, n_max) -> Vector{MieRootJl}

Lowest `n_max` TE and TM roots for `l in l_set`, merged and sorted by
ascending `k` (mirror of `geode_core::mie::merged_roots`).
"""
function merged_roots(n::Float64, l_set::Vector{Int},
                      r_s::Float64, r_b::Float64, n_max::Int)
    all_roots = MieRootJl[]
    for l in l_set
        append!(all_roots, resonance_roots(:TE, n, l, r_s, r_b, n_max))
        append!(all_roots, resonance_roots(:TM, n, l, r_s, r_b, n_max))
    end
    sort!(all_roots; by = r -> r.k)
    return all_roots
end

"""
    mie_roots_catalog(m, l_max, n_max) -> Vector{MieRootJl}

Extended catalogue: lowest `n_max` roots for every `(l, polarisation)`
with `l in 1:l_max`, sorted globally by ascending `k` — mirror of
`geode_core::mie::mie_roots_catalog` on the bundled fixture geometry
(`MIE_R_SPHERE`, `MIE_R_BUFFER`).
"""
function mie_roots_catalog(m::Float64, l_max::Int, n_max::Int)
    @assert m > 0.0
    @assert l_max >= 1
    @assert n_max >= 1
    return merged_roots(m, collect(1:l_max), MIE_R_SPHERE, MIE_R_BUFFER, n_max)
end


# ---------------------------------------------------------------------------
# Self-check (run `julia --project=. mie_roots.jl` directly).
# ---------------------------------------------------------------------------

function _mie_roots_self_check()
    # n → 1 vacuum limit: the TE characteristic reduces to ψ_l(k R_b) = 0,
    # i.e. j_l(k R_b) = 0. First zero of j_1 is at x ≈ 4.4934.
    roots = resonance_roots(:TE, 1.0, 1, 0.5, 1.0, 1)
    @assert !isempty(roots) "vacuum-limit TE l=1 root not found"
    @assert abs(roots[1].k - 4.4934) < 1e-2 "vacuum-limit root off: $(roots[1])"

    # Catalogue extent: 2 pol × l_max × n_max entries, globally sorted.
    cat = mie_roots_catalog(1.5, 3, 3)
    @assert length(cat) == 2 * 3 * 3 "catalogue extent: $(length(cat))"
    @assert all(c -> c.k > 0.0 && isfinite(c.k), cat)
    @assert all(c -> c.multiplicity == 2 * c.l + 1, cat)
    @assert all(i -> cat[i].k <= cat[i + 1].k, 1:length(cat)-1)
    println("mie_roots.jl self-check OK")

    cat45 = mie_roots_catalog(MIE_N_INSIDE, 4, 5)
    println("full catalogue (l ≤ 4, n ≤ 5): $(length(cat45)) roots")
    for r in cat45[1:5]
        @printf("  %s l=%d n=%d  k = %.12f\n", r.pol, r.l, r.n, r.k)
    end
end

if abspath(PROGRAM_FILE) == @__FILE__
    _mie_roots_self_check()
end
