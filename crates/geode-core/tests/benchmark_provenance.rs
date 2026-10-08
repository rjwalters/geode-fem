//! Benchmark-artifact provenance guard (issue #692).
//!
//! Stale f32-era benchmark artifacts went unnoticed for months (#674,
//! #687): a Burn backend without a pinned float dtype silently moved
//! committed numbers by double-digit percentages. This test is the cheap,
//! always-on read-side check that makes "every Burn-backed artifact records
//! an f64 generation backend" structurally enforced rather than true by
//! convention.
//!
//! # How it works
//!
//! [`MANIFEST`] classifies **every** committed `benchmarks/**/*.toml`. Two
//! tests run against it:
//!
//! - [`manifest_covers_every_committed_benchmark_toml`] walks `benchmarks/`
//!   and fails if a TOML exists that the manifest does not classify (so a
//!   new artifact cannot slip in unclassified), or if a manifest entry
//!   names a file that no longer exists (so the manifest cannot rot).
//! - [`every_benchmark_artifact_satisfies_its_provenance_shape`] parses each
//!   classified artifact and checks it against its [`Shape`]; every failure
//!   names the concrete regeneration / measurement command.
//!
//! # Buckets
//!
//! - **A** ([`Shape::F64Trio`]) — Burn-backed, regenerated through
//!   `geode_util::fixture::BackendInfo::of::<B>(device)`: `[meta].backend`,
//!   `[meta].float_dtype == "F64"` and `[meta].generated_at_commit` must be
//!   present.
//! - **B** ([`Shape::Exempt`]) — no Burn backend anywhere in the numeric
//!   path (pure faer / scalar assembly), or not a physics artifact at all.
//!   Listed explicitly with a reason so the coverage test still sees them.
//! - **C** ([`Shape::PerCellDtype`]) — Burn-backed with an intentionally
//!   non-uniform dtype (CPU-f64 vs GPU-f32 legs of the same pencil). A
//!   document-level `float_dtype = "F64"` would be *wrong*; instead every
//!   `[[cell]]` must carry its own `dtype`.
//! - **D** ([`Shape::PendingF64Trio`]) — Burn-backed (`TestBackend`,
//!   feature-pinned to f64) with a live generator but not yet regenerated
//!   with the provenance trio. This is the **shrinking allowlist**: once an
//!   entry is regenerated and carries a compliant trio, this test fails
//!   until it is promoted to bucket A, so the list can only shrink.
//! - **E** ([`Shape::HandProvenance`]) — hand-measured / hand-transcribed
//!   artifacts with no mechanical regeneration command. They must carry
//!   their own hand-provenance keys (e.g. `measured_date`) instead of the
//!   mechanical trio, so a completely bare `[meta]` still fails loudly.
//!
//! Adding a new benchmark artifact: add a [`MANIFEST`] entry. A Burn-backed
//! generator should write its `[meta]` through `BackendInfo` and be listed
//! as [`Shape::F64Trio`]; do not add new bucket-D entries.
//!
//! Runs in CI by name (`.github/workflows/arpack.yml`, step "Benchmark
//! provenance guard"): there is no blanket `cargo test -p geode-core` job,
//! so a new `#[test]` here is only enforced because that step names it.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Required provenance shape of one committed benchmark artifact.
#[derive(Debug, Clone, Copy)]
enum Shape {
    /// Bucket A: `[meta].backend`, `[meta].float_dtype == "F64"` and
    /// `[meta].generated_at_commit` must all be present.
    F64Trio {
        /// Command that regenerates the artifact with the trio.
        regen: &'static str,
    },
    /// Bucket B: exempt — the generator never touches a Burn backend (or the
    /// file is not a physics-accuracy artifact). Only TOML validity is checked.
    Exempt {
        /// Why this artifact needs no float-dtype provenance.
        reason: &'static str,
    },
    /// Bucket C: Burn-backed with intentionally mixed dtype; every
    /// `[[cell]]` must carry a non-empty `dtype` field.
    PerCellDtype {
        /// Command(s) that regenerate the artifact.
        regen: &'static str,
    },
    /// Bucket D: Burn-backed, trio not yet recorded — shrinking allowlist.
    /// Passes while the trio is absent; fails if a non-F64 dtype is recorded,
    /// and fails (asking for promotion to bucket A) once the trio is complete.
    PendingF64Trio {
        /// Command that regenerates the artifact (and should add the trio).
        regen: &'static str,
        /// Tracking note for the pending regeneration.
        tracking: &'static str,
    },
    /// Bucket E: hand-measured — every listed dotted key path must be present
    /// and non-empty instead of the mechanical trio.
    HandProvenance {
        /// Dotted TOML key paths that must be present (e.g. `meta.measured_date`).
        required: &'static [&'static str],
        /// How the artifact is (re)measured, printed on failure.
        how: &'static str,
    },
}

/// One manifest row: repo-relative artifact path + its required shape.
struct Entry {
    path: &'static str,
    shape: Shape,
}

const fn a(path: &'static str, regen: &'static str) -> Entry {
    Entry {
        path,
        shape: Shape::F64Trio { regen },
    }
}

const fn b(path: &'static str, reason: &'static str) -> Entry {
    Entry {
        path,
        shape: Shape::Exempt { reason },
    }
}

const fn d(path: &'static str, regen: &'static str, tracking: &'static str) -> Entry {
    Entry {
        path,
        shape: Shape::PendingF64Trio { regen, tracking },
    }
}

const fn e(path: &'static str, required: &'static [&'static str], how: &'static str) -> Entry {
    Entry {
        path,
        shape: Shape::HandProvenance { required, how },
    }
}

/// Tracking note shared by every bucket-D entry.
const D_TRACKING: &str = "issue #692 allowlist — regenerate on an f64 backend via \
     geode_util::fixture::BackendInfo, then promote this entry to bucket A";

/// Classification of every committed `benchmarks/**/*.toml` (issue #692).
const MANIFEST: &[Entry] = &[
    // ---- Bucket A: Burn-backed, f64 provenance trio required ------------
    a(
        "benchmarks/periodic/results.toml",
        "GEODE_BLESS_PERIODIC=1 cargo test -p geode-core --release --test periodic_cavity \
         -- --ignored regenerate_periodic_results",
    ),
    a(
        "benchmarks/spiral_inductor/results.toml",
        "cargo run -p spiral_inductor --release",
    ),
    a(
        "benchmarks/spiral_inductor/results_smoke.toml",
        "cargo run -p spiral_inductor --release -- smoke",
    ),
    a(
        "benchmarks/slcfet_3hp/results.toml",
        "cargo run -p slcfet_3hp_spiral --release",
    ),
    a(
        "benchmarks/slcfet_3hp/results_smoke.toml",
        "cargo run -p slcfet_3hp_spiral --release -- smoke",
    ),
    a(
        "benchmarks/mie_sphere/results.toml",
        "cargo run -p mie_sphere --release",
    ),
    a(
        "benchmarks/patch_antenna/results.toml",
        "cargo run -p patch_antenna --release",
    ),
    a(
        "benchmarks/patch_antenna/results_smoke.toml",
        "cargo run -p patch_antenna --release -- smoke",
    ),
    a(
        "benchmarks/patch_antenna/results_matched.toml",
        "cargo run -p patch_antenna --release -- matched",
    ),
    a(
        "benchmarks/patch_antenna/pattern.toml",
        "cargo run -p patch_antenna --release -- pattern",
    ),
    a(
        "benchmarks/patch_antenna/pattern_matched.toml",
        "cargo run -p patch_antenna --release -- pattern-matched",
    ),
    // ---- Bucket B: exempt, no Burn backend in the numeric path ----------
    b(
        "benchmarks/soi_waveguide/results.toml",
        "examples/soi_waveguide has no burn dependency (#687 audit)",
    ),
    b(
        "benchmarks/step_index_fiber/results.toml",
        "examples/step_index_fiber has no burn dependency (#687 audit)",
    ),
    b(
        "benchmarks/electrostatic/results.toml",
        "examples/electrostatic_capacitance.rs: scalar f64 assembly + direct faer LU, no Backend generic",
    ),
    b(
        "benchmarks/magnetostatic_inductance/results.toml",
        "examples/magnetostatic_inductance.rs: no Backend generic",
    ),
    b(
        "benchmarks/motor/results.toml",
        "examples/motor_torque.rs: no Backend generic (direct faer solve)",
    ),
    b(
        "benchmarks/fiber_dispersion/results.toml",
        // Regenerate with GEODE_BLESS_FIBER_DISPERSION=1 cargo test -p geode-core
        // --release --test fiber_dispersion_benchmark -- --ignored (issue #823).
        "tests/fiber_dispersion_benchmark.rs: mixed E_t-E_z pencil on faer, no Backend generic",
    ),
    b(
        "benchmarks/transmon_diffopt/results.toml",
        "tests/transmon_diffopt.rs: damped Newton on faer LU, no Backend generic",
    ),
    b(
        "benchmarks/transmon_diffopt/pad_results.toml",
        "examples/transmon_pad_diffopt.rs: no Backend generic",
    ),
    b(
        "benchmarks/transmon_diffopt/harmonic_results.toml",
        "examples/transmon_pad_harmonic.rs: no Backend generic",
    ),
    b(
        // The #692 curation listed this under bucket D; the generator was
        // re-audited during implementation: it only uses the scalar
        // `assemble_electrostatic{,_tensor}` + `extract_capacitance` f64
        // path (same as benchmarks/electrostatic), no Burn backend.
        "benchmarks/transmon_quantum/results.toml",
        "examples/transmon_quantum.rs: scalar f64 electrostatic assembly + faer, no Backend generic \
         (re-audited in #692; not Burn-backed)",
    ),
    b(
        "benchmarks/perf/baseline.toml",
        "criterion wall-clock timing baseline (issue #50, `cargo run -p extract_baseline`), \
         not a physics-accuracy artifact",
    ),
    // ---- Bucket C: Burn-backed, intentionally mixed dtype per cell ------
    Entry {
        path: "benchmarks/gpu_driven_scaling/results.toml",
        shape: Shape::PerCellDtype {
            regen: "cargo test -p geode-core --release --test gpu_driven_scaling -- --ignored --nocapture \
                    (CPU-f64 legs) + the same with --features cuda (GPU-f32 leg), then hand-merge",
        },
    },
    e(
        "benchmarks/gpu_driven_scaling/results_large_a100.toml",
        &[
            "meta.measured_date",
            "meta.geode_commit",
            "palace_driven.palace_commit",
        ],
        "hand-transcribed from summarize_520.py over runs/2026-10-07_lambda_a100 (the \
         gpu_driven_scaling test with GEODE_SCALING_* knobs, CPU-f64 + Cuda-f32 legs, plus Palace \
         driven runs, Lambda A100, issue #520); record meta.measured_date, meta.geode_commit and \
         palace_driven.palace_commit",
    ),
    e(
        "benchmarks/gpu_driven_scaling/results_ams_cpu_local.toml",
        &[
            "meta.measured_date",
            "meta.geode_commit",
            "meta.scope",
            "hardware.label",
        ],
        "generated by benchmarks/gpu_driven_scaling/summarize_ams_local.py over a run tree written \
         by ams_cpu_sweep.sh (the gpu_driven_scaling test with GEODE_SCALING_CONFIGS=iterative_ams, \
         CPU f64, issue #930); rerun both rather than editing the file. It is a wall-clock record \
         of one host, so it carries meta.measured_date, meta.geode_commit, meta.scope and \
         hardware.label instead of the mechanical trio",
    ),
    // ---- Bucket D: Burn-backed, trio missing — SHRINKING allowlist ------
    d(
        // Multi-precision in *arithmetic* only; the Burn backend is always
        // ndarray-f64, so a top-level trio is accurate here (unlike bucket C).
        "benchmarks/mixed_precision_refinement/results.toml",
        "cargo test -p geode-core --release --test mixed_precision_refinement \
         mixed_precision_refinement_benchmark -- --ignored --nocapture (prints the TOML fragment)",
        D_TRACKING,
    ),
    d(
        "benchmarks/transient/results.toml",
        "GEODE_BLESS_TRANSIENT=1 cargo test -p geode-core --release --test transient_sparams \
         transient_self_oracle_broadband -- --ignored",
        D_TRACKING,
    ),
    d(
        "benchmarks/patch_antenna_diffopt/results.toml",
        "cargo run -p geode-core --example patch_diffopt --release",
        D_TRACKING,
    ),
    d(
        "benchmarks/patch_antenna_diffopt/capstone_results.toml",
        "cargo run -p geode-core --example patch_capstone_diffopt --release",
        D_TRACKING,
    ),
    d(
        "benchmarks/patch_antenna_conformal/conformal_results.toml",
        "cargo run -p geode-core --example patch_conformal_diffopt --release",
        D_TRACKING,
    ),
    // ---- Bucket E: hand-measured, own provenance shape ------------------
    e(
        "benchmarks/mie_sphere/driven_results.toml",
        &["meta.generated_at_commit"],
        "hand-recorded from crates/geode-core/tests/mie_driven_scattering.rs \
         (cargo test -p geode-core --release --test mie_driven_scattering -- --ignored --nocapture); \
         the TOML is documentation, its maxima are pinned as Rust constants",
    ),
    e(
        "benchmarks/mie_sphere/driven_results_fine.toml",
        &["meta.generated_at_commit"],
        "hand-recorded from crates/geode-core/tests/mie_driven_scattering.rs \
         (cargo test -p geode-core --release --test mie_driven_scattering -- --ignored --nocapture)",
    ),
    e(
        "benchmarks/mie_sphere/open_results.toml",
        &["meta.generated_at_commit"],
        "hand-recorded from crates/geode-core/tests/sphere_matched_upml_eigenmode.rs \
         (cargo test -p geode-core --release --test sphere_matched_upml_eigenmode -- --ignored --nocapture)",
    ),
    e(
        "benchmarks/transmon_eigen/results.toml",
        &["meta.fixture_sha256", "oracles.palace.palace_version"],
        "hand-populated cross-solver run: geode transmon eigenmode (crates/geode-core/tests/transmon_eigenmode.rs) \
         + Palace on the same mesh (reference/palace/geode_transmon_baseline); record the fixture hash and \
         Palace version",
    ),
    e(
        "benchmarks/transmon_ams_minres_133k/results.toml",
        &["meta.measured_date"],
        "recorded honest negative from the permanently #[ignore]d test in \
         crates/geode-core/tests/transmon_eigenmode.rs; record meta.measured_date",
    ),
    e(
        "benchmarks/transmon_bench_cpu/results.toml",
        &["meta.measured_date"],
        "hand-transcribed EC2 timing of cargo run -p geode-core --example transmon_bench --release \
         vs Palace; record meta.measured_date",
    ),
    e(
        "benchmarks/transmon_bench_gpu/results.toml",
        &["meta.measured_date", "software.palace_commit"],
        "hand-transcribed from summarize.py over runs/2026-10-07_lambda_a100/raw \
         (Palace-GPU vs Palace-CPU vs geode CPU f64, Lambda A100); \
         record meta.measured_date and software.palace_commit",
    ),
];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root resolves")
}

/// Recursively collect repo-relative `*.toml` paths under `dir`.
fn collect_tomls(root: &Path, dir: &Path, out: &mut BTreeSet<String>) {
    let entries = fs::read_dir(dir).unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display()));
    for entry in entries {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            collect_tomls(root, &path, out);
        } else if path.extension().is_some_and(|x| x == "toml") {
            let rel = path
                .strip_prefix(root)
                .expect("under repo root")
                .to_string_lossy()
                .replace('\\', "/");
            out.insert(rel);
        }
    }
}

/// Look up a dotted key path (e.g. `meta.float_dtype`) in a parsed document.
fn lookup<'a>(doc: &'a toml::Value, dotted: &str) -> Option<&'a toml::Value> {
    dotted.split('.').try_fold(doc, |v, k| v.get(k))
}

/// A present, non-empty string (or any non-string value) at `dotted`.
fn present(doc: &toml::Value, dotted: &str) -> bool {
    match lookup(doc, dotted) {
        None => false,
        Some(toml::Value::String(s)) => !s.trim().is_empty(),
        Some(_) => true,
    }
}

/// Check one artifact's parsed contents against its shape. Returns one
/// human-readable message per violation (empty = compliant).
fn check(path: &str, raw: &str, shape: Shape) -> Vec<String> {
    let doc: toml::Value = match toml::from_str(raw) {
        Ok(d) => d,
        Err(err) => return vec![format!("{path} is not valid TOML: {err}")],
    };
    let dtype = lookup(&doc, "meta.float_dtype").and_then(|v| v.as_str());
    let mut errs = Vec::new();
    match shape {
        Shape::F64Trio { regen } => {
            for key in ["meta.backend", "meta.generated_at_commit"] {
                if !present(&doc, key) {
                    errs.push(format!(
                        "{path} is missing [{key}] — regenerate with: {regen} (see issue #692)"
                    ));
                }
            }
            if dtype != Some("F64") {
                errs.push(format!(
                    "{path} [meta].float_dtype is {dtype:?}, expected \"F64\" — regenerate on an \
                     f64 backend with: {regen} (see issues #674/#692)"
                ));
            }
        }
        Shape::Exempt { .. } => {}
        Shape::PerCellDtype { regen } => match doc.get("cell").and_then(|c| c.as_array()) {
            None => errs.push(format!(
                "{path} has no [[cell]] rows to carry a per-cell dtype — regenerate with: {regen} \
                 (see issue #692)"
            )),
            Some(cells) if cells.is_empty() => errs.push(format!(
                "{path} has an empty [[cell]] array — regenerate with: {regen} (see issue #692)"
            )),
            Some(cells) => {
                for (i, cell) in cells.iter().enumerate() {
                    let ok = cell
                        .get("dtype")
                        .and_then(|v| v.as_str())
                        .is_some_and(|s| !s.trim().is_empty());
                    if !ok {
                        errs.push(format!(
                            "{path} [[cell]] #{i} is missing a `dtype` field (mixed-precision \
                             artifacts record dtype per cell) — regenerate with: {regen} \
                             (see issue #692)"
                        ));
                    }
                }
            }
        },
        Shape::PendingF64Trio { regen, tracking } => {
            if let Some(dt) = dtype
                && dt != "F64"
            {
                errs.push(format!(
                    "{path} [meta].float_dtype is {dt:?}, expected \"F64\" — regenerate on an f64 \
                     backend with: {regen} ({tracking})"
                ));
            }
            let complete = dtype == Some("F64")
                && present(&doc, "meta.backend")
                && present(&doc, "meta.generated_at_commit");
            if complete {
                errs.push(format!(
                    "{path} now carries a complete f64 provenance trio — promote its MANIFEST \
                     entry in crates/geode-core/tests/benchmark_provenance.rs from bucket D \
                     (PendingF64Trio) to bucket A (F64Trio) so the allowlist shrinks (issue #692)"
                ));
            }
        }
        Shape::HandProvenance { required, how } => {
            for key in required {
                if !present(&doc, key) {
                    errs.push(format!(
                        "{path} is missing hand-provenance key [{key}] — this artifact has no \
                         mechanical regeneration command; {how} (see issue #692)"
                    ));
                }
            }
            if let Some(dt) = dtype
                && dt != "F64"
            {
                errs.push(format!(
                    "{path} [meta].float_dtype is {dt:?}, expected \"F64\" — {how} (see issue #692)"
                ));
            }
        }
    }
    errs
}

#[test]
fn manifest_covers_every_committed_benchmark_toml() {
    let root = repo_root();
    let mut on_disk = BTreeSet::new();
    collect_tomls(&root, &root.join("benchmarks"), &mut on_disk);

    let mut listed = BTreeSet::new();
    let mut dups = Vec::new();
    for entry in MANIFEST {
        if !listed.insert(entry.path.to_string()) {
            dups.push(entry.path);
        }
    }
    assert!(dups.is_empty(), "duplicate MANIFEST entries: {dups:?}");

    let unclassified: Vec<_> = on_disk.difference(&listed).collect();
    let stale: Vec<_> = listed.difference(&on_disk).collect();
    assert!(
        unclassified.is_empty(),
        "unclassified benchmark artifact(s) {unclassified:?}: add each to MANIFEST in \
         crates/geode-core/tests/benchmark_provenance.rs. A Burn-backed generator must write \
         [meta].backend/float_dtype/generated_at_commit via \
         geode_util::fixture::BackendInfo::of::<B>(device) and be listed as F64Trio; \
         pure-faer artifacts go in Exempt with a reason (issue #692)"
    );
    assert!(
        stale.is_empty(),
        "MANIFEST lists artifact(s) that no longer exist {stale:?}: remove them from \
         crates/geode-core/tests/benchmark_provenance.rs (issue #692)"
    );
}

#[test]
fn every_benchmark_artifact_satisfies_its_provenance_shape() {
    let root = repo_root();
    let mut failures = Vec::new();
    for entry in MANIFEST {
        let path = root.join(entry.path);
        match fs::read_to_string(&path) {
            Ok(raw) => failures.extend(check(entry.path, &raw, entry.shape)),
            Err(err) => failures.push(format!("read {}: {err}", entry.path)),
        }
    }
    assert!(
        failures.is_empty(),
        "{} benchmark provenance violation(s):\n  - {}",
        failures.len(),
        failures.join("\n  - ")
    );
}

#[test]
fn exempt_entries_all_state_a_reason() {
    for entry in MANIFEST {
        if let Shape::Exempt { reason } = entry.shape {
            assert!(
                !reason.trim().is_empty(),
                "{} is exempt without a reason",
                entry.path
            );
        }
    }
}

// ---- Self-tests of the checker on synthetic documents ---------------------

const TRIO_OK: &str = "[meta]\nbackend = \"ndarray\"\nfloat_dtype = \"F64\"\n\
                       generated_at_commit = \"abc1234\"\n";

#[test]
fn checker_a_accepts_trio_and_rejects_f32_or_missing_keys() {
    let regen = "cargo run -p demo --release";
    let a = Shape::F64Trio { regen };
    assert!(check("x.toml", TRIO_OK, a).is_empty());

    let f32 = TRIO_OK.replace("F64", "F32");
    let errs = check("x.toml", &f32, a);
    assert_eq!(errs.len(), 1, "{errs:?}");
    assert!(errs[0].contains("\"F32\"") && errs[0].contains(regen));

    let bare = "[meta]\ndescription = \"x\"\n";
    let errs = check("x.toml", bare, a);
    assert_eq!(errs.len(), 3, "{errs:?}");
    assert!(errs.iter().all(|m| m.contains(regen)));
}

#[test]
fn checker_c_requires_dtype_on_every_cell() {
    let c = Shape::PerCellDtype { regen: "regen-c" };
    let ok = "[meta]\n[[cell]]\ndtype = \"f64\"\n[[cell]]\ndtype = \"f32\"\n";
    assert!(check("c.toml", ok, c).is_empty());
    let bad = "[meta]\n[[cell]]\ndtype = \"f64\"\n[[cell]]\nn = 1\n";
    let errs = check("c.toml", bad, c);
    assert_eq!(errs.len(), 1, "{errs:?}");
    assert!(errs[0].contains("#1") && errs[0].contains("regen-c"));
    assert_eq!(check("c.toml", "[meta]\n", c).len(), 1);
}

#[test]
fn checker_d_passes_while_pending_and_demands_promotion_when_done() {
    let d = Shape::PendingF64Trio {
        regen: "regen-d",
        tracking: D_TRACKING,
    };
    assert!(check("d.toml", "[meta]\ndescription = \"x\"\n", d).is_empty());
    let done = check("d.toml", TRIO_OK, d);
    assert_eq!(done.len(), 1, "{done:?}");
    assert!(done[0].contains("promote"));
    let f32 = check("d.toml", "[meta]\nfloat_dtype = \"F32\"\n", d);
    assert_eq!(f32.len(), 1, "{f32:?}");
    assert!(f32[0].contains("regen-d"));
}

#[test]
fn checker_e_requires_hand_provenance_keys() {
    let e = Shape::HandProvenance {
        required: &["meta.measured_date", "oracles.palace.palace_version"],
        how: "measure-e",
    };
    let ok =
        "[meta]\nmeasured_date = \"2026-07-14\"\n[oracles.palace]\npalace_version = \"fba6a5b\"\n";
    assert!(check("e.toml", ok, e).is_empty());
    let errs = check("e.toml", "[meta]\ndescription = \"x\"\n", e);
    assert_eq!(errs.len(), 2, "{errs:?}");
    assert!(errs.iter().all(|m| m.contains("measure-e")));
    let blank = "[meta]\nmeasured_date = \"  \"\n[oracles.palace]\npalace_version = \"v\"\n";
    assert_eq!(check("e.toml", blank, e).len(), 1);
}
