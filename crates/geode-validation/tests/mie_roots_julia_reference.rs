//! Root-for-root cross-check of the analytic Mie resonance catalogue
//! against the Julia/SpecialFunctions.jl reference (issue #172, Epic
//! #88 Phase J.3 — Julia sibling of `mie_roots_numpy_reference.rs`).
//!
//! Loads `reference/fixtures/mie_roots/julia_baseline.json` (generated
//! by `reference/julia/gen_mie_roots_julia_baseline.jl` from
//! `reference/julia/mie_roots.jl`) and joins the Burn-side
//! `geode_core::analytic::mie::{resonance_roots, mie_roots_catalog}` output
//! against it on the exact integer key `(pol, l, n)`.
//!
//! # Why a third catalogue matters
//!
//! The Julia side computes the spherical Bessel functions via
//! **SpecialFunctions.jl half-order `besselj`/`bessely`**
//! (openspecfun/AMOS lineage) — independent of both scipy's
//! `spherical_jn`/`spherical_yn` recurrences (J.1) and the hand-rolled
//! Burn ladder. Agreement at ≤ 1e-10 relative across three Bessel
//! lineages pins the *mathematics* of the TE/TM characteristic
//! functions, not a shared special-function implementation. (Measured
//! generation-time agreement vs the J.1 SciPy catalogue: ~1.5e-15
//! worst-case relative — pinned on disk in
//! `cross_check_max_rel_vs_scipy`.)
//!
//! # What is checked
//!
//! 1. Schema conformance + geometry constants bit-exact vs Burn.
//! 2. Root-count agreement per `(l, polarisation)` window vs Burn.
//! 3. Root-for-root `(pol, l, n) → k` agreement vs Burn's
//!    `mie_roots_catalog(1.5, 4, 5)` at ≤ 1e-10 relative.
//! 4. Fixture-to-fixture join vs the J.1 SciPy catalogue at the same
//!    contract (no Burn in the loop — pure Julia-vs-NumPy), plus the
//!    pinned `cross_check_max_rel_vs_scipy` field stays ≤ 1e-10.

use std::collections::BTreeMap;
use std::path::PathBuf;

use geode_core::analytic::mie::{MiePolarisation, MieRoot, mie_roots_catalog, resonance_roots};
use geode_core::mesh::{R_BUFFER, R_SPHERE};
use geode_validation::{Fixture, FixtureFormat};

// ---------------------------------------------------------------------------
// Catalogue parameters — mirror examples/mie_sphere.rs and the fixtures.
// ---------------------------------------------------------------------------

const N_INSIDE: f64 = 1.5;
const L_MAX: usize = 4;
const N_MAX: usize = 5;

/// Cross-check contract from issues #170 / #172: ≤ 1e-10 relative.
const REL_TOL: f64 = 1e-10;

// ---------------------------------------------------------------------------
// Fixture path resolution (same pattern as mie_roots_numpy_reference.rs).
// ---------------------------------------------------------------------------

fn repo_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for ancestor in manifest.ancestors() {
        if ancestor.join("reference").is_dir() {
            return ancestor.to_path_buf();
        }
    }
    panic!(
        "could not find a `reference/` directory walking up from {}",
        manifest.display()
    );
}

fn julia_fixture_path() -> PathBuf {
    repo_root().join("reference/fixtures/mie_roots/julia_baseline.json")
}

fn numpy_fixture_path() -> PathBuf {
    repo_root().join("reference/fixtures/mie_roots/baseline.json")
}

fn load_julia_fixture() -> Fixture {
    Fixture::load_from(&julia_fixture_path(), FixtureFormat::Json)
        .expect("mie_roots/julia_baseline.json should load")
}

// ---------------------------------------------------------------------------
// Fixture field accessors (same helpers as mie_roots_numpy_reference.rs).
// ---------------------------------------------------------------------------

fn fixture_scalar_f64(fixture: &Fixture, name: &str) -> f64 {
    let f = fixture
        .output_f64(name)
        .unwrap_or_else(|e| panic!("fixture missing scalar output `{name}`: {e}"));
    assert_eq!(
        f.data.len(),
        1,
        "fixture scalar `{name}` should be length 1, got {}",
        f.data.len()
    );
    f.data[0]
}

fn fixture_scalar_usize(fixture: &Fixture, name: &str) -> usize {
    fixture_scalar_f64(fixture, name).round() as usize
}

fn fixture_array_f64(fixture: &Fixture, name: &str) -> Vec<f64> {
    fixture
        .output_f64(name)
        .unwrap_or_else(|e| panic!("fixture missing array output `{name}`: {e}"))
        .data
        .clone()
}

fn fixture_array_usize(fixture: &Fixture, name: &str) -> Vec<usize> {
    fixture_array_f64(fixture, name)
        .iter()
        .map(|v| v.round() as usize)
        .collect()
}

/// Canonical join key: `(pol_index, l, n)` with `pol_index` 0 = TE,
/// 1 = TM — matching the fixtures' `root_pol` encoding.
type RootKey = (u8, usize, usize);

fn pol_index(pol: MiePolarisation) -> u8 {
    match pol {
        MiePolarisation::TE => 0,
        MiePolarisation::TM => 1,
    }
}

fn pol_name(idx: u8) -> &'static str {
    match idx {
        0 => "TE",
        1 => "TM",
        _ => "??",
    }
}

/// Load a roots fixture's catalogue as a `(pol, l, n) → k` map,
/// asserting the parallel-array invariants.
fn fixture_root_map(fixture: &Fixture) -> BTreeMap<RootKey, f64> {
    let n_roots = fixture_scalar_usize(fixture, "n_roots");
    let pols = fixture_array_usize(fixture, "root_pol");
    let ls = fixture_array_usize(fixture, "root_l");
    let ns = fixture_array_usize(fixture, "root_n");
    let mults = fixture_array_usize(fixture, "root_multiplicity");
    let ks = fixture_array_f64(fixture, "root_k");

    for (name, len) in [
        ("root_pol", pols.len()),
        ("root_l", ls.len()),
        ("root_n", ns.len()),
        ("root_multiplicity", mults.len()),
        ("root_k", ks.len()),
    ] {
        assert_eq!(len, n_roots, "`{name}` length {len} != n_roots {n_roots}");
    }

    let mut map = BTreeMap::new();
    for i in 0..n_roots {
        let key: RootKey = (pols[i] as u8, ls[i], ns[i]);
        assert!(pols[i] <= 1, "root_pol[{i}] = {} not in {{0, 1}}", pols[i]);
        assert_eq!(
            mults[i],
            2 * ls[i] + 1,
            "fixture multiplicity for {key:?} should be 2l+1"
        );
        assert!(
            ks[i].is_finite() && ks[i] > 0.0,
            "fixture k for {key:?} is {}, not a positive finite root",
            ks[i]
        );
        let prev = map.insert(key, ks[i]);
        assert!(prev.is_none(), "duplicate fixture key {key:?}");
    }
    map
}

/// Project a Burn-side root list into the same `(pol, l, n) → k` map.
fn burn_root_map(roots: &[MieRoot]) -> BTreeMap<RootKey, f64> {
    let mut map = BTreeMap::new();
    for r in roots {
        assert_eq!(
            r.multiplicity,
            2 * r.l + 1,
            "Burn multiplicity drift: {r:?}"
        );
        let key: RootKey = (pol_index(r.pol), r.l, r.n);
        let prev = map.insert(key, r.k);
        assert!(prev.is_none(), "duplicate Burn key {key:?}");
    }
    map
}

/// Join two `(pol, l, n) → k` maps: identical key sets and per-root
/// relative error ≤ `REL_TOL`. Returns the worst relative error.
fn assert_root_maps_agree(
    left: &BTreeMap<RootKey, f64>,
    right: &BTreeMap<RootKey, f64>,
    left_name: &str,
    right_name: &str,
    context: &str,
) -> f64 {
    for key in left.keys() {
        assert!(
            right.contains_key(key),
            "{context}: {left_name} has root {}({}, n={}) that {right_name} lacks \
             (bracket / pole-rejection drift?)",
            pol_name(key.0),
            key.1,
            key.2
        );
    }
    for key in right.keys() {
        assert!(
            left.contains_key(key),
            "{context}: {right_name} has root {}({}, n={}) that {left_name} lacks \
             (root-window edge effect?)",
            pol_name(key.0),
            key.1,
            key.2
        );
    }

    let mut worst_rel: f64 = 0.0;
    let mut worst_key: RootKey = (0, 0, 0);
    for (key, &k_left) in left {
        let k_right = right[key];
        let rel = (k_left - k_right).abs() / k_right.abs();
        if rel > worst_rel {
            worst_rel = rel;
            worst_key = *key;
        }
        assert!(
            rel <= REL_TOL,
            "{context}: root {}(l={}, n={}) disagrees: {left_name} k = {k_left:.15}, \
             {right_name} k = {k_right:.15}, relative error {rel:.3e} > {REL_TOL:.0e}",
            pol_name(key.0),
            key.1,
            key.2
        );
    }
    println!(
        "{context}: {} roots agree ({left_name} vs {right_name}); worst relative \
         error {:.3e} at {}(l={}, n={})",
        left.len(),
        worst_rel,
        pol_name(worst_key.0),
        worst_key.1,
        worst_key.2
    );
    worst_rel
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn julia_fixture_loads_with_canonical_schema() {
    let fixture = load_julia_fixture();
    assert_eq!(fixture.fixture_id, "mie_roots/n15_pec_cavity_l4_n5_julia");
    assert_eq!(fixture.schema_version, "1");

    for expected in [
        "l_max",
        "n_max",
        "n_roots",
        "n_inside",
        "r_sphere",
        "r_buffer",
        "root_pol",
        "root_l",
        "root_n",
        "root_multiplicity",
        "root_k",
        "root_count_te",
        "root_count_tm",
        "cross_check_max_rel_vs_scipy",
    ] {
        assert!(
            fixture.outputs.contains_key(expected),
            "Julia fixture missing required output `{expected}`"
        );
    }

    assert_eq!(fixture_scalar_usize(&fixture, "l_max"), L_MAX);
    assert_eq!(fixture_scalar_usize(&fixture, "n_max"), N_MAX);
    assert_eq!(
        fixture_scalar_usize(&fixture, "n_roots"),
        2 * L_MAX * N_MAX,
        "full catalogue extent (2 pol × l_max × n_max)"
    );
}

#[test]
fn julia_geometry_constants_match_burn_bit_exact() {
    let fixture = load_julia_fixture();
    assert_eq!(fixture_scalar_f64(&fixture, "n_inside"), N_INSIDE);
    assert_eq!(fixture_scalar_f64(&fixture, "r_sphere"), R_SPHERE);
    assert_eq!(fixture_scalar_f64(&fixture, "r_buffer"), R_BUFFER);
}

#[test]
fn julia_pinned_scipy_cross_check_is_within_contract() {
    // The generator joins the Julia catalogue against the J.1 SciPy
    // catalogue at generation time and pins the worst observed relative
    // error on disk. It must sit (far) inside the 1e-10 contract —
    // measured ~1.5e-15 (three independent Bessel lineages, identical
    // bracket walk).
    let fixture = load_julia_fixture();
    let pinned = fixture_scalar_f64(&fixture, "cross_check_max_rel_vs_scipy");
    assert!(
        pinned.is_finite() && pinned >= 0.0,
        "pinned cross-check residual should be a finite non-negative number, got {pinned}"
    );
    assert!(
        pinned <= REL_TOL,
        "pinned Julia-vs-SciPy worst relative error {pinned:.3e} exceeds the \
         1e-10 cross-check contract — the generator gate should have caught this"
    );
}

#[test]
fn julia_root_counts_agree_per_l_and_polarisation() {
    let fixture = load_julia_fixture();
    let count_te = fixture_array_usize(&fixture, "root_count_te");
    let count_tm = fixture_array_usize(&fixture, "root_count_tm");
    assert_eq!(count_te.len(), L_MAX);
    assert_eq!(count_tm.len(), L_MAX);

    for l in 1..=L_MAX {
        for (pol, counts) in [
            (MiePolarisation::TE, &count_te),
            (MiePolarisation::TM, &count_tm),
        ] {
            let burn = resonance_roots(pol, N_INSIDE, l, R_SPHERE, R_BUFFER, N_MAX);
            assert_eq!(
                burn.len(),
                counts[l - 1],
                "root count mismatch for {pol:?} l = {l}: Burn found {}, \
                 Julia catalogued {}",
                burn.len(),
                counts[l - 1]
            );
        }
    }
}

#[test]
fn julia_catalog_roots_match_burn_root_for_root() {
    let fixture = load_julia_fixture();
    let julia = fixture_root_map(&fixture);

    let catalog = mie_roots_catalog(N_INSIDE, L_MAX, N_MAX);
    let burn = burn_root_map(&catalog);

    assert_eq!(
        burn.len(),
        fixture_scalar_usize(&fixture, "n_roots"),
        "total root count: Burn vs Julia fixture n_roots"
    );
    assert_root_maps_agree(
        &burn,
        &julia,
        "Burn",
        "Julia",
        "mie_roots_catalog(1.5, 4, 5) vs SpecialFunctions.jl",
    );
}

#[test]
fn julia_and_numpy_catalogues_agree_root_for_root() {
    // Pure fixture-to-fixture join (no Burn in the loop): the Julia
    // (SpecialFunctions.jl / AMOS) and NumPy (scipy.special) catalogues
    // must agree at the same ≤ 1e-10 relative contract. Together with
    // `julia_catalog_roots_match_burn_root_for_root` and the J.1 test,
    // this closes the three-way Burn / SciPy / Julia triangle.
    let julia_fixture = load_julia_fixture();
    let numpy_fixture = Fixture::load_from(&numpy_fixture_path(), FixtureFormat::Json)
        .expect("mie_roots/baseline.json should load");

    let julia = fixture_root_map(&julia_fixture);
    let numpy = fixture_root_map(&numpy_fixture);

    let worst = assert_root_maps_agree(
        &julia,
        &numpy,
        "Julia",
        "NumPy",
        "mie_roots fixture-to-fixture (J.3 vs J.1)",
    );

    // The live join must not be looser than the generation-time pin
    // (both compare the same on-disk arrays, so equality is expected;
    // a drift here means one fixture was regenerated without the other).
    let pinned = fixture_scalar_f64(&julia_fixture, "cross_check_max_rel_vs_scipy");
    assert!(
        worst <= pinned + 1e-15,
        "live Julia-vs-NumPy worst relative error {worst:.3e} exceeds the \
         generation-time pin {pinned:.3e} — one catalogue was regenerated \
         without rerunning the Julia generator's cross-check"
    );
}
