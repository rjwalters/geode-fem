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

/// Regeneration of one #1085 UPML-ladder run (`<shell>_sigma<s>.toml`): the
/// shell mesh from its YAML, then the dense sweep with σ₀ overridden.
const LADDER_REGEN: &str = "python3 reference/gmsh/generate_patch_fixture.py --max-edges 200000 \
     reference/gmsh/<shell>.yaml <dir>/<shell>.msh && PATCH_CONFORMAL_SIGMA_0=<s> \
     PATCH_CONFORMAL_SWEEP_GHZ=list:2.24,2.26,2.27,2.28,2.29,2.30,2.31,2.32,2.33,2.36,2.40,2.4835 \
     PATCH_CONFORMAL_OUT=benchmarks/patch_antenna_conformal/upml_ladder/<shell>_sigma<s>.toml \
     cargo run -p geode-core --release --example patch_conformal_2g4_baseline -- <dir>/<shell>.msh";

/// Classification of every committed `benchmarks/**/*.toml` (issue #692).
const MANIFEST: &[Entry] = &[
    // ---- Bucket A: Burn-backed, f64 provenance trio required ------------
    a(
        "benchmarks/tm_guard_955/measurement_table.toml",
        "GEODE_BLESS_955=1 cargo test -p geode-core --release --test wave_port_p2 -- --ignored \
         tm_guard_p2_measurement_table --nocapture (needs gmsh on PATH)",
    ),
    a(
        "benchmarks/tm_guard_955/long_guide_table.toml",
        "GEODE_BLESS_955=1 cargo test -p geode-core --release --test wave_port_p2 -- --ignored \
         tm_guard_p2_long_guide_table --nocapture (needs gmsh on PATH)",
    ),
    a(
        "benchmarks/tm_guard_955/excitation_table.toml",
        "GEODE_BLESS_955=1 cargo test -p geode-core --release --test wave_port_p2 -- --ignored \
         tm_guard_excitation_table --nocapture (needs gmsh on PATH)",
    ),
    a(
        "benchmarks/transmon_coupling/results.toml",
        "cargo run -p geode-core --release --example transmon_coupling_g (issue #1078; reads the \
         committed Palace cells of reference/fixtures/transmon_palace/results_coupling_1078/)",
    ),
    a(
        "benchmarks/transmon_devicelayout_crosscheck/results.toml",
        "cargo run -p geode-core --release --example transmon_devicelayout_crosscheck (issue \
         #1092, assemble mode; reads the geode_runs/ records and the committed Palace cells of \
         reference/fixtures/transmon_palace/results_crosscheck_1092/)",
    ),
    a(
        "benchmarks/transmon_devicelayout_crosscheck/geode_runs/initial_h.toml",
        "GEODE_SOLVE_MESH=<initial_o1.msh> GEODE_SOLVE_LABEL=initial_h GEODE_SIGMA_GHZ=5.0 cargo \
         run -p geode-core --release --example transmon_devicelayout_crosscheck (issue #1092; \
         mesh from reference/julia/generate_transmon_param.jl)",
    ),
    a(
        "benchmarks/transmon_devicelayout_crosscheck/geode_runs/initial_h2.toml",
        "GEODE_SOLVE_MESH=<initial_r1.msh> GEODE_SOLVE_LABEL=initial_h2 GEODE_SIGMA_GHZ=5.0 \
         cargo run -p geode-core --release --example transmon_devicelayout_crosscheck (issue \
         #1092; 94 GB peak, run on an AWS r8g.8xlarge)",
    ),
    a(
        "benchmarks/transmon_devicelayout_crosscheck/geode_runs/final_h.toml",
        "GEODE_SOLVE_MESH=<final_o1.msh> GEODE_SOLVE_LABEL=final_h GEODE_SIGMA_GHZ=4.0 cargo run \
         -p geode-core --release --example transmon_devicelayout_crosscheck (issue #1092)",
    ),
    a(
        "benchmarks/transmon_devicelayout_crosscheck/geode_runs/final_h2.toml",
        "GEODE_SOLVE_MESH=<final_r1.msh> GEODE_SOLVE_LABEL=final_h2 GEODE_SIGMA_GHZ=4.0 cargo run \
         -p geode-core --release --example transmon_devicelayout_crosscheck (issue #1092; 136 GB \
         peak, run on an AWS r8g.8xlarge)",
    ),
    a(
        "benchmarks/transmon_devicelayout_optimize/results.toml",
        "cargo run -p geode-core --release --example transmon_devicelayout_optimize (issue #1092 \
         part 2, assemble mode; reads runs/, ladder_<tag>/geode_runs/ and the Palace cells of \
         reference/fixtures/transmon_palace/results_optimize_1092/)",
    ),
    a(
        "benchmarks/transmon_devicelayout_optimize/runs/calibC_650.toml",
        "python3 -I benchmarks/transmon_devicelayout_optimize/driver.py (issue #1092 part 2; \
         examples/transmon_devicelayout_optimize.rs solve mode on the AWS worker, mesh from \
         reference/julia/generate_transmon_param.jl)",
    ),
    a(
        "benchmarks/transmon_devicelayout_optimize/runs/calibL_5000.toml",
        "python3 -I benchmarks/transmon_devicelayout_optimize/driver.py (issue #1092 part 2; \
         examples/transmon_devicelayout_optimize.rs solve mode on the AWS worker, mesh from \
         reference/julia/generate_transmon_param.jl)",
    ),
    a(
        "benchmarks/transmon_devicelayout_optimize/runs/calibL_5250.toml",
        "python3 -I benchmarks/transmon_devicelayout_optimize/driver.py (issue #1092 part 2; \
         examples/transmon_devicelayout_optimize.rs solve mode on the AWS worker, mesh from \
         reference/julia/generate_transmon_param.jl)",
    ),
    a(
        "benchmarks/transmon_devicelayout_optimize/runs/cv1_at.toml",
        "python3 -I benchmarks/transmon_devicelayout_optimize/driver.py (issue #1092 part 2; \
         examples/transmon_devicelayout_optimize.rs solve mode on the AWS worker, mesh from \
         reference/julia/generate_transmon_param.jl)",
    ),
    a(
        "benchmarks/transmon_devicelayout_optimize/runs/cv2_at.toml",
        "python3 -I benchmarks/transmon_devicelayout_optimize/driver.py (issue #1092 part 2; \
         examples/transmon_devicelayout_optimize.rs solve mode on the AWS worker, mesh from \
         reference/julia/generate_transmon_param.jl)",
    ),
    a(
        "benchmarks/transmon_devicelayout_optimize/runs/dl0.toml",
        "python3 -I benchmarks/transmon_devicelayout_optimize/driver.py (issue #1092 part 2; \
         examples/transmon_devicelayout_optimize.rs solve mode on the AWS worker, mesh from \
         reference/julia/generate_transmon_param.jl)",
    ),
    a(
        "benchmarks/transmon_devicelayout_optimize/runs/dl1.toml",
        "python3 -I benchmarks/transmon_devicelayout_optimize/driver.py (issue #1092 part 2; \
         examples/transmon_devicelayout_optimize.rs solve mode on the AWS worker, mesh from \
         reference/julia/generate_transmon_param.jl)",
    ),
    a(
        "benchmarks/transmon_devicelayout_optimize/runs/dl2.toml",
        "python3 -I benchmarks/transmon_devicelayout_optimize/driver.py (issue #1092 part 2; \
         examples/transmon_devicelayout_optimize.rs solve mode on the AWS worker, mesh from \
         reference/julia/generate_transmon_param.jl)",
    ),
    a(
        "benchmarks/transmon_devicelayout_optimize/runs/dl3.toml",
        "python3 -I benchmarks/transmon_devicelayout_optimize/driver.py (issue #1092 part 2; \
         examples/transmon_devicelayout_optimize.rs solve mode on the AWS worker, mesh from \
         reference/julia/generate_transmon_param.jl)",
    ),
    a(
        "benchmarks/transmon_devicelayout_optimize/runs/fd_initial.toml",
        "python3 -I benchmarks/transmon_devicelayout_optimize/driver.py (issue #1092 part 2; \
         examples/transmon_devicelayout_optimize.rs solve mode on the AWS worker, mesh from \
         reference/julia/generate_transmon_param.jl)",
    ),
    a(
        "benchmarks/transmon_devicelayout_optimize/ladder_cv1/geode_runs/cv1_h.toml",
        "benchmarks/transmon_devicelayout_optimize/ladder.sh cv1 (issue #1092 part 2; \
         examples/transmon_devicelayout_crosscheck.rs solve mode on the AWS worker)",
    ),
    a(
        "benchmarks/transmon_devicelayout_optimize/ladder_cv1/geode_runs/cv1_h2.toml",
        "benchmarks/transmon_devicelayout_optimize/ladder.sh cv1 (issue #1092 part 2; \
         examples/transmon_devicelayout_crosscheck.rs solve mode on the AWS worker)",
    ),
    a(
        "benchmarks/transmon_devicelayout_optimize/ladder_cv2/geode_runs/cv2_h.toml",
        "benchmarks/transmon_devicelayout_optimize/ladder.sh cv2 (issue #1092 part 2; \
         examples/transmon_devicelayout_crosscheck.rs solve mode on the AWS worker)",
    ),
    a(
        "benchmarks/transmon_devicelayout_optimize/ladder_cv2/geode_runs/cv2_h2.toml",
        "benchmarks/transmon_devicelayout_optimize/ladder.sh cv2 (issue #1092 part 2; \
         examples/transmon_devicelayout_crosscheck.rs solve mode on the AWS worker)",
    ),
    a(
        "benchmarks/transmon_loop_inductance/results.toml",
        "cargo run -p geode-core --release --example transmon_loop_inductance (issue #1079; reads \
         the committed Palace cells of reference/fixtures/transmon_palace/results_loop_1079/ and \
         results_coupling_1078/h2_L14p86)",
    ),
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
        "benchmarks/mie_sphere/open_sigma_sweep.toml",
        "cargo run -p mie_open_quasimode --release -- --sigma-sweep --sigma-step 0.25",
    ),
    a(
        "benchmarks/mie_sphere/open_sigma_sweep_fine.toml",
        "cargo run -p mie_open_quasimode --release -- --sigma-sweep --sigma-step 0.5 \
         --fixture fine",
    ),
    a(
        "benchmarks/mie_sphere/open_selfconsistent.toml",
        "cargo run -p mie_open_quasimode --release -- --self-consistent --fixture coarse \
         --sigma-start 5 --sigma-end 25 --sigma-step 0.5 --max-iters 6",
    ),
    a(
        "benchmarks/mie_sphere/open_selfconsistent_fine.toml",
        "cargo run -p mie_open_quasimode --release -- --self-consistent --fixture fine \
         --sigma-values 5,10,15,25 --max-iters 4",
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
    // Promoted from bucket D in #1084 (regenerated with the trio).
    a(
        "benchmarks/patch_antenna_conformal/conformal_results.toml",
        "cargo run -p geode-core --example patch_conformal_diffopt --release",
    ),
    a(
        "benchmarks/patch_antenna_conformal/baseline_2g4_L0.toml",
        "python3 reference/gmsh/generate_patch_fixture.py reference/gmsh/patch_conformal_L0.yaml \
         crates/geode-core/tests/fixtures/patch_conformal_L0.msh && cargo run -p geode-core \
         --release --example patch_conformal_2g4_baseline",
    ),
    a(
        "benchmarks/patch_antenna_conformal/baseline_2g4_L1.toml",
        "python3 reference/gmsh/generate_patch_fixture.py --max-edges 200000 \
         reference/gmsh/patch_conformal_L1.yaml <dir>/patch_conformal_L1.msh && cargo run -p \
         geode-core --release --example patch_conformal_2g4_baseline -- \
         <dir>/patch_conformal_L1.msh",
    ),
    // #1085 (conformal v5 E2): per-level dip / FD artifacts of the mesh-refinement study.
    // L2 / L3 are regenerated from YAML (sha256 pinned in tests/patch_conformal_convergence.rs);
    // L3 and p = 2 on L1 were run one geometry at a time (PATCH_CONFORMAL_GEOMS) and merged.
    // L0 is regenerated too (gmsh 4.15; reproduces the committed L0 sha256): the loader needs the
    // `<stem>.flat.msh` companion, which is not committed (.gitignore, #1084).
    a(
        "benchmarks/patch_antenna_conformal/convergence/dip_L0.toml",
        "python3 reference/gmsh/generate_patch_fixture.py reference/gmsh/patch_conformal_L0.yaml \
         <dir>/patch_conformal_L0.msh && cargo run -p geode-core --release --example \
         patch_conformal_convergence -- dip <dir>/patch_conformal_L0.msh",
    ),
    a(
        "benchmarks/patch_antenna_conformal/convergence/dip_L1.toml",
        "cargo run -p geode-core --release --example patch_conformal_convergence -- dip \
         <dir>/patch_conformal_L1.msh",
    ),
    a(
        "benchmarks/patch_antenna_conformal/convergence/dip_L2.toml",
        "python3 reference/gmsh/generate_patch_fixture.py --max-edges 500000 \
         reference/gmsh/patch_conformal_L2.yaml <dir>/patch_conformal_L2.msh && cargo run -p \
         geode-core --release --example patch_conformal_convergence -- dip \
         <dir>/patch_conformal_L2.msh",
    ),
    a(
        "benchmarks/patch_antenna_conformal/convergence/dip_L3.toml",
        "python3 reference/gmsh/generate_patch_fixture.py --max-edges 1000000 \
         reference/gmsh/patch_conformal_L3.yaml <dir>/patch_conformal_L3.msh && for g in flat \
         bent; do PATCH_CONFORMAL_GEOMS=$g PATCH_CONFORMAL_DIP_SCAN=2.26,2.36,6 \
         PATCH_CONFORMAL_DIP_TOL_MHZ=0.5 PATCH_CONFORMAL_FIXED_GHZ=2.40,2.4835 cargo run -p \
         geode-core --release --example patch_conformal_convergence -- dip \
         <dir>/patch_conformal_L3.msh --out <dir>/dip_L3_$g.toml; done && cargo run -p geode-core \
         --release --example patch_conformal_convergence -- merge <dir>/dip_L3_flat.toml \
         <dir>/dip_L3_bent.toml (about 68 GiB per solve)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/convergence/dip_L0_p2.toml",
        "python3 reference/gmsh/generate_patch_fixture.py reference/gmsh/patch_conformal_L0.yaml \
         <dir>/patch_conformal_L0.msh && cargo run -p geode-core --release --example \
         patch_conformal_convergence -- dip <dir>/patch_conformal_L0.msh --p2",
    ),
    a(
        "benchmarks/patch_antenna_conformal/convergence/dip_L1_p2.toml",
        "for g in flat bent; do PATCH_CONFORMAL_GEOMS=$g PATCH_CONFORMAL_DIP_SCAN=2.34,2.38,5 \
         PATCH_CONFORMAL_DIP_TOL_MHZ=0.5 PATCH_CONFORMAL_FIXED_GHZ=2.40,2.4835 cargo run -p \
         geode-core --release --example patch_conformal_convergence -- dip \
         <dir>/patch_conformal_L1.msh --p2 --out <dir>/dip_L1_p2_$g.toml; done && cargo run -p \
         geode-core --release --example patch_conformal_convergence -- merge \
         <dir>/dip_L1_p2_flat.toml <dir>/dip_L1_p2_bent.toml (about 132 GiB per solve)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/convergence/split/dip_L3_flat.toml",
        "PATCH_CONFORMAL_GEOMS=flat PATCH_CONFORMAL_DIP_SCAN=2.26,2.36,6 PATCH_CONFORMAL_DIP_TOL_MHZ=0.5 \
         PATCH_CONFORMAL_FIXED_GHZ=2.40,2.4835 cargo run -p geode-core --release --example \
         patch_conformal_convergence -- dip <dir>/patch_conformal_L3.msh (one-geometry \
         input of convergence/dip_L3.toml; about 68 GiB per solve)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/convergence/split/dip_L3_bent.toml",
        "PATCH_CONFORMAL_GEOMS=bent PATCH_CONFORMAL_DIP_SCAN=2.26,2.36,6 PATCH_CONFORMAL_DIP_TOL_MHZ=0.5 \
         PATCH_CONFORMAL_FIXED_GHZ=2.40,2.4835 cargo run -p geode-core --release --example \
         patch_conformal_convergence -- dip <dir>/patch_conformal_L3.msh (one-geometry \
         input of convergence/dip_L3.toml; about 68 GiB per solve)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/convergence/split/dip_L1_p2_flat.toml",
        "PATCH_CONFORMAL_GEOMS=flat PATCH_CONFORMAL_DIP_SCAN=2.34,2.38,5 PATCH_CONFORMAL_DIP_TOL_MHZ=0.5 \
         PATCH_CONFORMAL_FIXED_GHZ=2.40,2.4835 cargo run -p geode-core --release --example \
         patch_conformal_convergence -- dip <dir>/patch_conformal_L1.msh --p2 (one-geometry \
         input of convergence/dip_L1_p2.toml; about 132 GiB per solve)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/convergence/split/dip_L1_p2_bent.toml",
        "PATCH_CONFORMAL_GEOMS=bent PATCH_CONFORMAL_DIP_SCAN=2.34,2.38,5 PATCH_CONFORMAL_DIP_TOL_MHZ=0.5 \
         PATCH_CONFORMAL_FIXED_GHZ=2.40,2.4835 cargo run -p geode-core --release --example \
         patch_conformal_convergence -- dip <dir>/patch_conformal_L1.msh --p2 (one-geometry \
         input of convergence/dip_L1_p2.toml; about 132 GiB per solve)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/convergence/fd_L0.toml",
        "python3 reference/gmsh/generate_patch_fixture.py reference/gmsh/patch_conformal_L0.yaml \
         <dir>/patch_conformal_L0.msh && cargo run -p geode-core --release --example \
         patch_conformal_convergence -- fd <dir>/patch_conformal_L0.msh",
    ),
    a(
        "benchmarks/patch_antenna_conformal/convergence/fd_L1.toml",
        "cargo run -p geode-core --release --example patch_conformal_convergence -- fd \
         <dir>/patch_conformal_L1.msh",
    ),
    a(
        "benchmarks/patch_antenna_conformal/convergence/fd_L2.toml",
        "cargo run -p geode-core --release --example patch_conformal_convergence -- fd \
         <dir>/patch_conformal_L2.msh",
    ),
    // #1085: the UPML sigma_0 x shell ladder on L1 — one dense flat / bent sweep of
    // patch_conformal_2g4_baseline per (shell, sigma_0) on the 12-point list grid.
    a(
        "benchmarks/patch_antenna_conformal/upml_ladder/patch_conformal_L1_sigma0.5.toml",
        LADDER_REGEN,
    ),
    a(
        "benchmarks/patch_antenna_conformal/upml_ladder/patch_conformal_L1_sigma1.toml",
        LADDER_REGEN,
    ),
    a(
        "benchmarks/patch_antenna_conformal/upml_ladder/patch_conformal_L1_sigma2.toml",
        LADDER_REGEN,
    ),
    a(
        "benchmarks/patch_antenna_conformal/upml_ladder/patch_conformal_L1_sigma4.toml",
        LADDER_REGEN,
    ),
    a(
        "benchmarks/patch_antenna_conformal/upml_ladder/patch_conformal_L1_sigma8.toml",
        LADDER_REGEN,
    ),
    a(
        "benchmarks/patch_antenna_conformal/upml_ladder/patch_conformal_L1_sigma25.toml",
        LADDER_REGEN,
    ),
    a(
        "benchmarks/patch_antenna_conformal/upml_ladder/patch_conformal_L1_pml5_sigma0.5.toml",
        LADDER_REGEN,
    ),
    a(
        "benchmarks/patch_antenna_conformal/upml_ladder/patch_conformal_L1_pml5_sigma1.toml",
        LADDER_REGEN,
    ),
    a(
        "benchmarks/patch_antenna_conformal/upml_ladder/patch_conformal_L1_pml5_sigma2.toml",
        LADDER_REGEN,
    ),
    a(
        "benchmarks/patch_antenna_conformal/upml_ladder/patch_conformal_L1_pml5_sigma4.toml",
        LADDER_REGEN,
    ),
    a(
        "benchmarks/patch_antenna_conformal/upml_ladder/patch_conformal_L1_pml5_sigma8.toml",
        LADDER_REGEN,
    ),
    a(
        "benchmarks/patch_antenna_conformal/upml_ladder/patch_conformal_L1_pml5_sigma25.toml",
        LADDER_REGEN,
    ),
    a(
        "benchmarks/patch_antenna_conformal/upml_ladder/patch_conformal_L1_pml40_sigma0.5.toml",
        LADDER_REGEN,
    ),
    a(
        "benchmarks/patch_antenna_conformal/upml_ladder/patch_conformal_L1_pml40_sigma1.toml",
        LADDER_REGEN,
    ),
    a(
        "benchmarks/patch_antenna_conformal/upml_ladder/patch_conformal_L1_pml40_sigma2.toml",
        LADDER_REGEN,
    ),
    a(
        "benchmarks/patch_antenna_conformal/upml_ladder/patch_conformal_L1_pml40_sigma4.toml",
        LADDER_REGEN,
    ),
    // Nested-DOF ladder of the curved conformal 2.4 GHz patch (#1087).
    a(
        "benchmarks/patch_antenna_conformal/nested_dof/L0_gap_1.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture \
         crates/geode-core/tests/fixtures/patch_conformal_L0.msh --radial 1x1 --outline 0 --n-band 5 --verify-n 9 \
         --max-steps 60 --label gap_1 (issue #1087 nested-DOF ladder)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/nested_dof/L0_gap_4.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture \
         crates/geode-core/tests/fixtures/patch_conformal_L0.msh --radial 2x2 --outline 0 --n-band 5 --verify-n 9 \
         --max-steps 60 --label gap_4 (issue #1087 nested-DOF ladder)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/nested_dof/L0_gap_16.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture \
         crates/geode-core/tests/fixtures/patch_conformal_L0.msh --radial 4x4 --outline 0 --n-band 5 --verify-n 9 \
         --max-steps 60 --label gap_16 (issue #1087 nested-DOF ladder)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/nested_dof/L0_gap_48.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture \
         crates/geode-core/tests/fixtures/patch_conformal_L0.msh --radial 8x6 --outline 0 --n-band 5 --verify-n 9 \
         --max-steps 60 --label gap_48 (issue #1087 nested-DOF ladder)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/nested_dof/L0_outline_16.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture \
         crates/geode-core/tests/fixtures/patch_conformal_L0.msh --radial 0 --outline 4 --n-band 5 --verify-n 9 \
         --max-steps 60 --label outline_16 (issue #1087 nested-DOF ladder)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/nested_dof/L0_combined_64.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture \
         crates/geode-core/tests/fixtures/patch_conformal_L0.msh --radial 8x6 --outline 4 --n-band 5 --verify-n 9 \
         --max-steps 60 --label combined_64 (issue #1087 nested-DOF ladder)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/nested_dof/L1_gap_1.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture \
         <dir>/patch_conformal_L1.msh --radial 1x1 --outline 0 --n-band 5 --verify-n 9 \
         --max-steps 60 --label gap_1 (issue #1087 nested-DOF ladder)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/nested_dof/L1_gap_4.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture \
         <dir>/patch_conformal_L1.msh --radial 2x2 --outline 0 --n-band 5 --verify-n 9 \
         --max-steps 60 --label gap_4 (issue #1087 nested-DOF ladder)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/nested_dof/L1_gap_16.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture \
         <dir>/patch_conformal_L1.msh --radial 4x4 --outline 0 --n-band 5 --verify-n 9 \
         --max-steps 60 --label gap_16 (issue #1087 nested-DOF ladder)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/nested_dof/L1_gap_48.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture \
         <dir>/patch_conformal_L1.msh --radial 8x6 --outline 0 --n-band 5 --verify-n 9 \
         --max-steps 60 --label gap_48 (issue #1087 nested-DOF ladder)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/nested_dof/L1_outline_16.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture \
         <dir>/patch_conformal_L1.msh --radial 0 --outline 4 --n-band 5 --verify-n 9 \
         --max-steps 60 --label outline_16 (issue #1087 nested-DOF ladder)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/nested_dof/L1_combined_64.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture \
         <dir>/patch_conformal_L1.msh --radial 8x6 --outline 4 --n-band 5 --verify-n 9 \
         --max-steps 60 --label combined_64 (issue #1087 nested-DOF ladder)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/nested_dof/L2_combined_64_cold.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture \
         <dir>/patch_conformal_L2.msh --radial 8x6 --outline 4 --label combined_64_cold (issue #1103 L2 re-optimization)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/nested_dof/L2_combined_64_warm.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture \
         <dir>/patch_conformal_L2.msh --radial 8x6 --outline 4 --label combined_64_warm --design \
         benchmarks/patch_antenna_conformal/nested_dof/L1_combined_64.toml (issue #1103 L2 re-optimization)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/nested_dof/L2_gap_48_warm.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture \
         <dir>/patch_conformal_L2.msh --radial 8x6 --outline 0 --label gap_48_warm --design \
         benchmarks/patch_antenna_conformal/nested_dof/L1_gap_48.toml (issue #1103 L2 re-optimization)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/nested_dof/verify/initial_on_L1.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture \
         <dir>/patch_conformal_L1.msh --radial 1x1 --outline 0 --wide-step 0 --verify-only \
         (issue #1103 gate G2 verification)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/nested_dof/verify/initial_on_L2.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture \
         <dir>/patch_conformal_L2.msh --radial 1x1 --outline 0 --wide-step 0 --verify-only \
         (issue #1103 gate G2 verification)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/nested_dof/verify/initial_on_L3.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture \
         <dir>/patch_conformal_L3.msh --radial 1x1 --outline 0 --wide-step 0 --verify-only \
         (issue #1103 gate G2 verification)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/nested_dof/verify/initial_on_L1_p2.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture \
         <dir>/patch_conformal_L1.msh --radial 1x1 --outline 0 --p2 --wide-step 0 --verify-only \
         (issue #1103 gate G2 verification)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/nested_dof/verify/L1_combined_64_on_L0.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture \
         <dir>/patch_conformal_L0.msh --radial 8x6 --outline 4 --wide-step 0 --verify-only --design \
         benchmarks/patch_antenna_conformal/nested_dof/L1_combined_64.toml \
         (issue #1103 gate G2 verification)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/nested_dof/verify/L1_combined_64_on_L1.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture \
         <dir>/patch_conformal_L1.msh --radial 8x6 --outline 4 --wide-step 0 --verify-only --design \
         benchmarks/patch_antenna_conformal/nested_dof/L1_combined_64.toml \
         (issue #1103 gate G2 verification)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/nested_dof/verify/L1_combined_64_on_L2.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture \
         <dir>/patch_conformal_L2.msh --radial 8x6 --outline 4 --wide-step 0 --verify-only --design \
         benchmarks/patch_antenna_conformal/nested_dof/L1_combined_64.toml \
         (issue #1103 gate G2 verification)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/nested_dof/verify/L1_combined_64_on_L3.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture \
         <dir>/patch_conformal_L3.msh --radial 8x6 --outline 4 --wide-step 0 --verify-only --design \
         benchmarks/patch_antenna_conformal/nested_dof/L1_combined_64.toml \
         (issue #1103 gate G2 verification)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/nested_dof/verify/L1_combined_64_on_L1_p2.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture \
         <dir>/patch_conformal_L1.msh --radial 8x6 --outline 4 --p2 --wide-step 0 --verify-only --design \
         benchmarks/patch_antenna_conformal/nested_dof/L1_combined_64.toml \
         (issue #1103 gate G2 verification)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/nested_dof/verify/L1_gap_48_on_L1.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture \
         <dir>/patch_conformal_L1.msh --radial 8x6 --outline 0 --wide-step 0 --verify-only --design \
         benchmarks/patch_antenna_conformal/nested_dof/L1_gap_48.toml \
         (issue #1103 gate G2 verification)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/nested_dof/verify/L2_combined_64_cold_on_L3.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture \
         <dir>/patch_conformal_L3.msh --radial 8x6 --outline 4 --wide-step 0.05 --verify-only --design \
         benchmarks/patch_antenna_conformal/nested_dof/L2_combined_64_cold.toml \
         (issue #1103 gate G2 verification)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/nested_dof/verify/L2_combined_64_cold_on_L1_p2.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture \
         <dir>/patch_conformal_L1.msh --radial 8x6 --outline 4 --p2 --wide-step 0.05 --verify-only --design \
         benchmarks/patch_antenna_conformal/nested_dof/L2_combined_64_cold.toml \
         (issue #1103 gate G2 verification)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/nested_dof/verify/L2_combined_64_warm_on_L3.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture \
         <dir>/patch_conformal_L3.msh --radial 8x6 --outline 4 --wide-step 0.05 --verify-only --design \
         benchmarks/patch_antenna_conformal/nested_dof/L2_combined_64_warm.toml \
         (issue #1103 gate G2 verification)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/nested_dof/verify/L2_combined_64_warm_on_L1_p2.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture \
         <dir>/patch_conformal_L1.msh --radial 8x6 --outline 4 --p2 --wide-step 0 --verify-only --design \
         benchmarks/patch_antenna_conformal/nested_dof/L2_combined_64_warm.toml \
         (issue #1103 gate G2 verification)",
    ),
    // ---- Bucket B: exempt, no Burn backend in the numeric path ----------
    b(
        "benchmarks/soi_waveguide/results.toml",
        "examples/soi_waveguide has no burn dependency (#687 audit)",
    ),
    b(
        "benchmarks/patch_antenna_conformal/upml_ladder_L1.toml",
        "patch_conformal_convergence ladder: post-processing of the bucket-A upml_ladder/*.toml \
         runs (no solve; carries generated_at_commit)",
    ),
    b(
        "benchmarks/patch_antenna_conformal/convergence.toml",
        "patch_conformal_convergence aggregate: post-processing of the bucket-A convergence/*.toml \
         and baseline artifacts (no solve; carries generated_at_commit)",
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
        "benchmarks/transmon_diffopt/multiparam_gradient.toml",
        "examples/transmon_multiparam_diffopt.rs: scalar f64 electrostatic assembly + faer LU \
         (shape::capacitance_matrix_shape_gradient), no Backend generic",
    ),
    b(
        "benchmarks/transmon_diffopt/multiparam_results.toml",
        "examples/transmon_multiparam_optimize.rs: scalar/tensor f64 electrostatic assembly + \
         faer LU (shape::transmon_remorph, quantum::diffopt::optimize_multiparam_bounded), no \
         Backend generic",
    ),
    b(
        "benchmarks/transmon_diffopt/c_sigma_convergence.toml",
        "examples/transmon_c_sigma_convergence.rs: scalar/tensor f64 P1 and P2 electrostatic \
         assembly + faer LU / AMG-PCG on host CSR (shape::transmon_remorph::\
         red_refined_c_sigma_levels, issue #1051), no Backend generic",
    ),
    b(
        "benchmarks/transmon_diffopt/c_sigma_bracket.toml",
        "examples/transmon_c_sigma_bracket.rs: scalar/tensor f64 P1 and P2 electrostatic \
         assembly + AMG-PCG on host CSR, f64 Raviart-Thomas patch equilibration (faer dense) and \
         a Crouzeix-Raviart solve (assembly::electrostatic_dual, issue #1062), no Backend generic",
    ),
    b(
        "benchmarks/transmon_diffopt/c_sigma_bracket_anchor_93p95.toml",
        "examples/transmon_c_sigma_bracket.rs (BRACKET_PART=anchor_93p95, issue #1075): scalar f64 \
         P1 and P2 electrostatic assembly + AMG-PCG on host CSR, f64 Raviart-Thomas patch \
         equilibration (faer dense) and a Crouzeix-Raviart solve (assembly::electrostatic_dual), \
         no Backend generic",
    ),
    b(
        "benchmarks/transmon_diffopt/c_sigma_bracket_tensor.toml",
        "examples/transmon_c_sigma_bracket.rs (BRACKET_PART=tensor, issue #1091): scalar and \
         sapphire-tensor f64 P1 and P2 electrostatic assembly + AMG-PCG on host CSR, f64 \
         Raviart-Thomas patch equilibration (faer dense) and a Crouzeix-Raviart solve \
         (assembly::electrostatic_dual), no Backend generic",
    ),
    b(
        "benchmarks/transmon_diffopt/refined_optimization/p2_cycles10/summary.toml",
        "examples/transmon_refined_optimize.rs: scalar f64 P1/P2 electrostatic assembly + AMG-PCG \
         on host CSR and the f64 energy shape derivative (shape::refined_capacitance, issue \
         #1056), no Backend generic",
    ),
    b(
        "benchmarks/transmon_diffopt/refined_optimization/p2_cycles10_resolution_corrected/\
         summary.toml",
        "examples/transmon_refined_optimize.rs with REFINED_OPT_TARGET set (issue #1056): same \
         f64 path as p2_cycles10/summary.toml, no Backend generic",
    ),
    b(
        "benchmarks/transmon_diffopt/refined_optimization/p2_cycles10_anchor_93p95/summary.toml",
        "examples/transmon_refined_optimize.rs at the DeviceLayout anchor (issue #1066): same f64 \
         P2 AMG-PCG path as p2_cycles10/summary.toml, no Backend generic",
    ),
    b(
        "benchmarks/transmon_diffopt/refined_optimization/\
         p2_cycles10_anchor_93p95_resolution_corrected/summary.toml",
        "examples/transmon_refined_optimize.rs with REFINED_OPT_TARGET set (issue #1066): same \
         f64 path as p2_cycles10_anchor_93p95/summary.toml, no Backend generic",
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
    e(
        "benchmarks/gpu_driven_scaling/results_threading_956.toml",
        &[
            "meta.measured_date",
            "meta.main_commit",
            "meta.fix_commit",
            "meta.faer",
            "hardware.cpu",
        ],
        "generated by benchmarks/gpu_driven_scaling/summarize_threading_956.py over \
         runs/2026-10-09_local_threading_956 (the archived diag956_threading.rs harness run by \
         sweep956.sh, refine956.sh, gated956.sh and spotcheck956.sh, issue #956); rerun the \
         summarizer rather than editing the file. It is a wall-clock record of one host comparing \
         several builds, so it carries meta.measured_date, meta.main_commit, meta.fix_commit, \
         meta.faer and hardware.cpu instead of the mechanical trio",
    ),
    e(
        "benchmarks/gpu_driven_scaling/results_ams_cpu_r6i.toml",
        &[
            "meta.measured_date",
            "meta.geode_commit",
            "meta.scope",
            "hardware.label",
            "palace.palace_commit",
        ],
        "generated by benchmarks/gpu_driven_scaling/summarize_ams_r6i.py over \
         runs/2026-10-08_r6i_ams_scale (ams_cpu_sweep.sh trees pinned with TASKSET_CPUS at 1 and \
         8 threads, plus box_palace_cpu.sh runs of Palace on the same host; AWS r6i.4xlarge, \
         issue #930); rerun the summarizer rather than editing the file. A wall-clock record of \
         one host, so it carries meta.measured_date, meta.geode_commit, meta.scope, \
         hardware.label and palace.palace_commit instead of the mechanical trio",
    ),
    e(
        "benchmarks/gpu_driven_scaling/results_mf_ams_cpu_local.toml",
        &[
            "meta.measured_date",
            "meta.geode_commit",
            "meta.scope",
            "hardware.label",
        ],
        "generated by benchmarks/gpu_driven_scaling/summarize_mf_ams_local.py over \
         runs/2026-10-09_local_mf_ams (ams_cpu_sweep.sh trees with the matrix-free legs \
         LEGS=mfjacobi,mfams,mfxcheck plus the assembled jacobi / ams legs, CPU ndarray f64, \
         issue #966; NOTE.txt there has the invocations); rerun the summarizer rather than \
         editing the file. A wall-clock record of one host, so it carries meta.measured_date, \
         meta.geode_commit, meta.scope and hardware.label instead of the mechanical trio",
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
        "benchmarks/transmon_bench_cpu/results_like_for_like_local.toml",
        &[
            "meta.measured_date",
            "meta.geode_commit",
            "meta.scope",
            "hardware.label",
        ],
        "generated by benchmarks/transmon_bench_cpu/summarize_like_for_like.py over a run tree \
         written by like_for_like_local.sh (the transmon_bench example, geode only, CPU f64, \
         issue #927); rerun both rather than editing the file. It is a mode-content and \
         wall-clock record of one loaded developer machine, so it carries meta.measured_date, \
         meta.geode_commit, meta.scope and hardware.label instead of the mechanical trio",
    ),
    e(
        "benchmarks/transmon_bench_cpu/results_like_for_like_m6i.toml",
        &[
            "meta.measured_date",
            "meta.geode_commit",
            "meta.scope",
            "hardware.instance",
            "hardware.label",
            "software.palace_commit",
        ],
        "generated by benchmarks/transmon_bench_cpu/summarize_like_for_like_m6i.py over a run \
         tree written on the benchmark host by m6i_box_setup.sh and m6i_box_bench.sh (the \
         transmon_bench example, CPU f64, and Palace from reference/palace/docker/Dockerfile, \
         issues #927 and #763); rerun those rather than editing the file. It is a mode-content, \
         wall-clock and core-seconds record of one AWS m6i.4xlarge session, so it carries \
         meta.measured_date, meta.geode_commit, meta.scope, hardware.instance, hardware.label \
         and software.palace_commit instead of the mechanical trio",
    ),
    e(
        "benchmarks/transmon_bench_cpu/results_tuned_palace_m6i.toml",
        &[
            "meta.measured_date",
            "meta.geode_commit",
            "meta.scope",
            "hardware.instance",
            "hardware.label",
            "palace_build.palace_commit",
            "palace_build.march",
        ],
        "generated by benchmarks/transmon_bench_cpu/summarize_tuned_palace_m6i.py over the run \
         tree written on the benchmark host by m6i_tuned_box_setup.sh and m6i_tuned_box_bench.sh \
         (Palace from reference/palace/docker/Dockerfile.tuned and the transmon_bench example, \
         CPU f64, issue #927) and the committed PR #964 tree; rerun those rather than editing \
         the file. It is a wall-clock and core-seconds record of one AWS m6i.4xlarge session, so \
         it carries meta.measured_date, meta.geode_commit, meta.scope, hardware.instance, \
         hardware.label, palace_build.palace_commit and palace_build.march instead of the \
         mechanical trio",
    ),
    e(
        "benchmarks/transmon_bench_gpu/results.toml",
        &["meta.measured_date", "software.palace_commit"],
        "hand-transcribed from summarize.py over runs/2026-10-07_lambda_a100/raw \
         (Palace-GPU vs Palace-CPU vs geode CPU f64, Lambda A100); \
         record meta.measured_date and software.palace_commit",
    ),
    e(
        "benchmarks/transmon_devicelayout_controls/results.toml",
        &["meta.measured_date", "meta.palace_commit"],
        "python3 -I benchmarks/transmon_devicelayout_controls/assemble.py (issues #1123, #1129; reads the \
         Palace cells of reference/fixtures/transmon_palace/results_controls_1123/ and \
         results_controls_1129/ written by driver.py, prima_driver.jl, ladder_palace.sh and \
         worker_amr.sh on AWS, plus the committed #1079/#1092 artifacts). Palace runs only, no Burn backend and no new geode solve (the one \
         geode-derived input is #1111's transmission-line delta), so it carries \
         meta.measured_date and meta.palace_commit instead of the mechanical trio",
    ),
    // Issue #1120: copper-only design on the fixed 1.6 mm board.
    a(
        "benchmarks/patch_antenna_conformal/copper/L0_copper_38.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture <dir>/patch_conformal_L0.msh --radial 0 --outline 8 --filter-radius 0 --copper --corners 6 --feed --n-band 5 --beta 25 --max-steps 60 --verify-n 9 --wide-step 0.025 --label copper_38 (issue #1120 copper optimization)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/copper/L0_copper_38_fd.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture <dir>/patch_conformal_L0.msh --radial 0 --outline 8 --filter-radius 0 --copper --corners 6 --feed --n-band 5 --beta 25 --max-steps 60 --verify-n 9 --wide-step 0.05 --label copper_38_fd --fd-only (issue #1120 FD gate)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/copper/L1_copper_38.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture <dir>/patch_conformal_L1.msh --radial 0 --outline 8 --filter-radius 0 --copper --corners 6 --feed --n-band 5 --beta 25 --max-steps 60 --verify-n 9 --wide-step 0.025 --label copper_38 (issue #1120 copper optimization)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/copper/L1_copper_38_fd.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture <dir>/patch_conformal_L1.msh --radial 0 --outline 8 --filter-radius 0 --copper --corners 6 --feed --n-band 5 --beta 25 --max-steps 60 --verify-n 9 --wide-step 0.05 --label copper_38_fd --fd-only (issue #1120 FD gate)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/copper/L2_copper_38.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture <dir>/patch_conformal_L2.msh --radial 0 --outline 8 --filter-radius 0 --copper --corners 6 --feed --n-band 5 --beta 25 --max-steps 60 --verify-n 9 --wide-step 0.025 --label copper_38 (issue #1120 copper optimization)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/copper/L2_copper_38_s30.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture <dir>/patch_conformal_L2.msh --radial 0 --outline 8 --filter-radius 0 --copper --corners 6 --feed --band-shift 0.03 --n-band 5 --beta 25 --max-steps 60 --verify-n 9 --wide-step 0.025 --label copper_38_s30 (issue #1120 copper optimization)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/copper/rbA_L0_copper_38_warm.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture <dir>/patch_conformal_rbA_L0.msh --radial 0 --outline 8 --filter-radius 0 --copper --corners 6 --feed --feed-support-legacy --rebase-feed --n-band 5 --beta 25 --max-steps 60 --verify-n 9 --wide-step 0.025 --label copper_38_warm --design benchmarks/patch_antenna_conformal/copper/L0_copper_38.toml (issue #1120 copper optimization)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/copper/rbB_L1_copper_38_sm48.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture <dir>/patch_conformal_rbB_L1.msh --radial 0 --outline 8 --filter-radius 0 --copper --corners 6 --feed --feed-support-legacy --rebase-feed --band-shift 0.04785 --n-band 5 --beta 25 --max-steps 60 --verify-n 9 --wide-step 0.025 --label copper_38_sm48 --design benchmarks/patch_antenna_conformal/copper/rbB_L1_copper_38_warm.toml (issue #1120 copper optimization)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/copper/rbB_L1_copper_38_warm.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture <dir>/patch_conformal_rbB_L1.msh --radial 0 --outline 8 --filter-radius 0 --copper --corners 6 --feed --feed-support-legacy --rebase-feed --n-band 5 --beta 25 --max-steps 60 --verify-n 9 --wide-step 0.025 --label copper_38_warm --design benchmarks/patch_antenna_conformal/copper/L1_copper_38.toml (issue #1120 copper optimization)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/copper/rbC_L2_copper_38_sm29.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture <dir>/patch_conformal_rbC_L2.msh --radial 0 --outline 8 --filter-radius 0 --copper --corners 6 --feed --feed-support-legacy --rebase-feed --band-shift 0.02933 --n-band 5 --beta 25 --max-steps 60 --verify-n 9 --wide-step 0.025 --label copper_38_sm29 --design benchmarks/patch_antenna_conformal/copper/L2_copper_38_s30.toml (issue #1120 copper optimization)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/copper/rbD_L1_copper_38_sm45.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture <dir>/patch_conformal_rbD_L1.msh --radial 0 --outline 8 --filter-radius 0 --copper --corners 6 --feed --feed-support-legacy --rebase-feed --band-shift 0.04494 --n-band 5 --beta 25 --max-steps 60 --verify-n 9 --wide-step 0.025 --label copper_38_sm45 --design benchmarks/patch_antenna_conformal/copper/rbB_L1_copper_38_sm48.toml (issue #1120 copper optimization)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/copper/verify/L0_copper_38_on_rbA_L0.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture <dir>/patch_conformal_rbA_L0.msh --radial 0 --outline 8 --filter-radius 0 --copper --corners 6 --feed --feed-support-legacy --rebase-feed --verify-only --verify-n 9 --wide-step 0.025 --design benchmarks/patch_antenna_conformal/copper/L0_copper_38.toml (issue #1120 verification)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/copper/verify/L1_copper_38_on_L1_dense0of1.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture <dir>/patch_conformal_L1.msh --radial 0 --outline 8 --filter-radius 0 --copper --corners 6 --feed --verify-only --verify-n 0 --wide-step 0 --dense 2.3:2.6:61 --design benchmarks/patch_antenna_conformal/copper/L1_copper_38.toml (issue #1120 verification)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/copper/verify/L1_copper_38_on_L1_p2.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture <dir>/patch_conformal_L1.msh --radial 0 --outline 8 --filter-radius 0 --copper --corners 6 --feed --verify-only --p2 --verify-n 9 --wide-step 0 --design benchmarks/patch_antenna_conformal/copper/L1_copper_38.toml (issue #1120 verification)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/copper/verify/L2_copper_38_s30_on_L1_p2.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture <dir>/patch_conformal_L1.msh --radial 0 --outline 8 --filter-radius 0 --copper --corners 6 --feed --verify-only --p2 --verify-n 9 --wide-step 0 --design benchmarks/patch_antenna_conformal/copper/L2_copper_38_s30.toml (issue #1120 verification)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/copper/verify/L2_copper_38_s30_on_L2_dense0of1.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture <dir>/patch_conformal_L2.msh --radial 0 --outline 8 --filter-radius 0 --copper --corners 6 --feed --verify-only --verify-n 0 --wide-step 0 --dense 2.3:2.6:31 --design benchmarks/patch_antenna_conformal/copper/L2_copper_38_s30.toml (issue #1120 verification)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/copper/verify/L2_copper_38_s30_on_L3.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture <dir>/patch_conformal_L3.msh --radial 0 --outline 8 --filter-radius 0 --copper --corners 6 --feed --verify-only --verify-n 9 --wide-step 0 --design benchmarks/patch_antenna_conformal/copper/L2_copper_38_s30.toml (issue #1120 verification)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/copper/verify/L2_copper_38_s30_on_rbC_L1_p2.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture <dir>/patch_conformal_rbC_L1.msh --radial 0 --outline 8 --filter-radius 0 --copper --corners 6 --feed --feed-support-legacy --rebase-feed --verify-only --p2 --verify-n 9 --wide-step 0 --design benchmarks/patch_antenna_conformal/copper/L2_copper_38_s30.toml (issue #1120 verification)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/copper/verify/rbB_L1_copper_38_sm48_on_rbB_L1_dense0of1.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture <dir>/patch_conformal_rbB_L1.msh --radial 0 --outline 8 --filter-radius 0 --copper --corners 6 --feed --feed-support-legacy --rebase-feed --verify-only --verify-n 0 --wide-step 0 --dense 2.3:2.6:61 --design benchmarks/patch_antenna_conformal/copper/rbB_L1_copper_38_sm48.toml (issue #1120 verification)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/copper/verify/rbB_L1_copper_38_sm48_on_rbB_L1_p2.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture <dir>/patch_conformal_rbB_L1.msh --radial 0 --outline 8 --filter-radius 0 --copper --corners 6 --feed --feed-support-legacy --rebase-feed --verify-only --p2 --verify-n 9 --wide-step 0 --design benchmarks/patch_antenna_conformal/copper/rbB_L1_copper_38_sm48.toml (issue #1120 verification)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/copper/verify/rbC_L2_copper_38_sm29_on_rbE_L1_p2.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture <dir>/patch_conformal_rbE_L1.msh --radial 0 --outline 8 --filter-radius 0 --copper --corners 6 --feed --feed-support-legacy --rebase-feed --verify-only --p2 --verify-n 9 --wide-step 0 --design benchmarks/patch_antenna_conformal/copper/rbC_L2_copper_38_sm29.toml (issue #1120 verification)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/copper/verify/rbC_L2_copper_38_sm29_on_rbE_L1_p2_dense0of2.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture <dir>/patch_conformal_rbE_L1.msh --radial 0 --outline 8 --filter-radius 0 --copper --corners 6 --feed --feed-support-legacy --rebase-feed --verify-only --p2 --verify-n 0 --wide-step 0 --dense 2.35:2.55:21 --shard 0/2 --design benchmarks/patch_antenna_conformal/copper/rbC_L2_copper_38_sm29.toml (issue #1120 verification)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/copper/verify/rbC_L2_copper_38_sm29_on_rbE_L1_p2_dense1of2.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture <dir>/patch_conformal_rbE_L1.msh --radial 0 --outline 8 --filter-radius 0 --copper --corners 6 --feed --feed-support-legacy --rebase-feed --verify-only --p2 --verify-n 0 --wide-step 0 --dense 2.35:2.55:21 --shard 1/2 --design benchmarks/patch_antenna_conformal/copper/rbC_L2_copper_38_sm29.toml (issue #1120 verification)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/copper/verify/rbC_L2_copper_38_sm29_on_rbE_L3.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture <dir>/patch_conformal_rbE_L3.msh --radial 0 --outline 8 --filter-radius 0 --copper --corners 6 --feed --feed-support-legacy --rebase-feed --verify-only --verify-n 9 --wide-step 0 --design benchmarks/patch_antenna_conformal/copper/rbC_L2_copper_38_sm29.toml (issue #1120 verification)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/copper/verify/rbD_L1_copper_38_sm45_on_rbD_L1_p2.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture <dir>/patch_conformal_rbD_L1.msh --radial 0 --outline 8 --filter-radius 0 --copper --corners 6 --feed --feed-support-legacy --rebase-feed --verify-only --p2 --verify-n 9 --wide-step 0 --design benchmarks/patch_antenna_conformal/copper/rbD_L1_copper_38_sm45.toml (issue #1120 verification)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/copper/verify/rbD_L1_copper_38_sm45_on_rbD_L1_p2_dense0of3.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture <dir>/patch_conformal_rbD_L1.msh --radial 0 --outline 8 --filter-radius 0 --copper --corners 6 --feed --feed-support-legacy --rebase-feed --verify-only --p2 --verify-n 0 --wide-step 0 --dense 2.3:2.6:41 --shard 0/3 --design benchmarks/patch_antenna_conformal/copper/rbD_L1_copper_38_sm45.toml (issue #1120 verification)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/copper/verify/rbD_L1_copper_38_sm45_on_rbD_L1_p2_dense1of3.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture <dir>/patch_conformal_rbD_L1.msh --radial 0 --outline 8 --filter-radius 0 --copper --corners 6 --feed --feed-support-legacy --rebase-feed --verify-only --p2 --verify-n 0 --wide-step 0 --dense 2.3:2.6:41 --shard 1/3 --design benchmarks/patch_antenna_conformal/copper/rbD_L1_copper_38_sm45.toml (issue #1120 verification)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/copper/verify/rbD_L1_copper_38_sm45_on_rbD_L1_p2_dense2of3.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture <dir>/patch_conformal_rbD_L1.msh --radial 0 --outline 8 --filter-radius 0 --copper --corners 6 --feed --feed-support-legacy --rebase-feed --verify-only --p2 --verify-n 0 --wide-step 0 --dense 2.3:2.6:41 --shard 2/3 --design benchmarks/patch_antenna_conformal/copper/rbD_L1_copper_38_sm45.toml (issue #1120 verification)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/copper/verify/rbD_L1_copper_38_sm45_on_rbD_L3.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture <dir>/patch_conformal_rbD_L3.msh --radial 0 --outline 8 --filter-radius 0 --copper --corners 6 --feed --feed-support-legacy --rebase-feed --verify-only --verify-n 9 --wide-step 0 --design benchmarks/patch_antenna_conformal/copper/rbD_L1_copper_38_sm45.toml (issue #1120 verification)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/copper/verify/rbD_L1_copper_38_sm45_on_rbD_L3_cyl.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture <dir>/patch_conformal_rbD_L3.msh --radial 0 --outline 8 --filter-radius 0 --copper --corners 6 --feed --feed-support-legacy --rebase-feed --air-on-cylinder --verify-only --verify-n 9 --wide-step 0 --design benchmarks/patch_antenna_conformal/copper/rbD_L1_copper_38_sm45.toml (issue #1120 verification)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/copper/verify/rbD_L1_copper_38_sm45_on_rbD_L3_cyl_dense0of3.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture <dir>/patch_conformal_rbD_L3.msh --radial 0 --outline 8 --filter-radius 0 --copper --corners 6 --feed --feed-support-legacy --rebase-feed --air-on-cylinder --verify-only --verify-n 0 --wide-step 0 --dense 2.3:2.6:41 --shard 0/3 --design benchmarks/patch_antenna_conformal/copper/rbD_L1_copper_38_sm45.toml (issue #1120 verification)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/copper/verify/rbD_L1_copper_38_sm45_on_rbD_L3_cyl_dense1of3.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture <dir>/patch_conformal_rbD_L3.msh --radial 0 --outline 8 --filter-radius 0 --copper --corners 6 --feed --feed-support-legacy --rebase-feed --air-on-cylinder --verify-only --verify-n 0 --wide-step 0 --dense 2.3:2.6:41 --shard 1/3 --design benchmarks/patch_antenna_conformal/copper/rbD_L1_copper_38_sm45.toml (issue #1120 verification)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/copper/verify/rbD_L1_copper_38_sm45_on_rbD_L3_cyl_dense2of3.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture <dir>/patch_conformal_rbD_L3.msh --radial 0 --outline 8 --filter-radius 0 --copper --corners 6 --feed --feed-support-legacy --rebase-feed --air-on-cylinder --verify-only --verify-n 0 --wide-step 0 --dense 2.3:2.6:41 --shard 2/3 --design benchmarks/patch_antenna_conformal/copper/rbD_L1_copper_38_sm45.toml (issue #1120 verification)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/copper/verify/rbD_L1_copper_38_sm45_on_rbF_L1_p2.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture <dir>/patch_conformal_rbF_L1.msh --radial 0 --outline 8 --filter-radius 0 --copper --corners 6 --feed --feed-support-legacy --rebase-feed --verify-only --p2 --verify-n 9 --wide-step 0 --design benchmarks/patch_antenna_conformal/copper/rbD_L1_copper_38_sm45.toml (issue #1120 verification)",
    ),
    b(
        "benchmarks/patch_antenna_conformal/copper/g2_summary.toml",
        "copper_g2.py: post-processing of the bucket-A copper/verify/*.toml (dense-sweep \
         bandwidth / ISM coverage of issue #1120); no Burn backend",
    ),
    // Issue #1126: antenna v6 experiments (X1-X5).
    a(
        "benchmarks/patch_antenna_conformal/v6/x1/dip_L1.toml",
        "PATCH_CONFORMAL_GEOMS=bent PATCH_CONFORMAL_FIXED_GHZ=2.4,2.4835 PATCH_CONFORMAL_DIP_SCAN=2.2,2.38,7 PATCH_CONFORMAL_DIP_TOL_MHZ=0.2 cargo run -p geode-core --release --example patch_conformal_convergence -- dip <dir>/patch_conformal_L1.msh (issue #1126)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/v6/x1/dip_x1comb_L1.toml",
        "PATCH_CONFORMAL_GEOMS=bent PATCH_CONFORMAL_FIXED_GHZ=2.4 PATCH_CONFORMAL_DIP_SCAN=2.33,2.43,6 PATCH_CONFORMAL_DIP_TOL_MHZ=0.5 cargo run -p geode-core --release --example patch_conformal_convergence -- dip <dir>/patch_conformal_x1comb_L1.msh (issue #1126)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/v6/x1/dip_x1rim020_L1.toml",
        "PATCH_CONFORMAL_GEOMS=bent PATCH_CONFORMAL_FIXED_GHZ=2.4 PATCH_CONFORMAL_DIP_SCAN=2.31,2.41,6 PATCH_CONFORMAL_DIP_TOL_MHZ=0.5 cargo run -p geode-core --release --example patch_conformal_convergence -- dip <dir>/patch_conformal_x1rim020_L1.msh (issue #1126)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/v6/x1/dip_x1rim040_L1.toml",
        "PATCH_CONFORMAL_GEOMS=bent PATCH_CONFORMAL_FIXED_GHZ=2.4,2.4835 PATCH_CONFORMAL_DIP_SCAN=2.2,2.38,7 PATCH_CONFORMAL_DIP_TOL_MHZ=0.2 cargo run -p geode-core --release --example patch_conformal_convergence -- dip <dir>/patch_conformal_x1rim040_L1.msh (issue #1126)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/v6/x1/dip_x1sub030_L1.toml",
        "PATCH_CONFORMAL_GEOMS=bent PATCH_CONFORMAL_FIXED_GHZ=2.4 PATCH_CONFORMAL_DIP_SCAN=2.31,2.41,6 PATCH_CONFORMAL_DIP_TOL_MHZ=0.5 cargo run -p geode-core --release --example patch_conformal_convergence -- dip <dir>/patch_conformal_x1sub030_L1.msh (issue #1126)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/v6/x1/dip_x1sub040_L1.toml",
        "PATCH_CONFORMAL_GEOMS=bent PATCH_CONFORMAL_FIXED_GHZ=2.4,2.4835 PATCH_CONFORMAL_DIP_SCAN=2.2,2.38,7 PATCH_CONFORMAL_DIP_TOL_MHZ=0.2 cargo run -p geode-core --release --example patch_conformal_convergence -- dip <dir>/patch_conformal_x1sub040_L1.msh (issue #1126)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/v6/x1/dip_x1sub053_L1.toml",
        "PATCH_CONFORMAL_GEOMS=bent PATCH_CONFORMAL_FIXED_GHZ=2.4,2.4835 PATCH_CONFORMAL_DIP_SCAN=2.2,2.38,7 PATCH_CONFORMAL_DIP_TOL_MHZ=0.2 cargo run -p geode-core --release --example patch_conformal_convergence -- dip <dir>/patch_conformal_x1sub053_L1.msh (issue #1126)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/v6/x1/palace/initial_on_L1_mesh.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture <dir>/patch_conformal_L1.msh --radial 0 --outline 8 --filter-radius 0 --copper --corners 6 --feed --verify-only --verify-n 0 --wide-step 0 --write-mesh <out>.msh (issue #1126)",
    ),
    b(
        "benchmarks/patch_antenna_conformal/v6/x1/palace_summary.toml",
        "palace_ingest.py: Palace (MFEM, no Burn backend) driven sweeps of the conformal patch, \
         issue #1126 X1",
    ),
    a(
        "benchmarks/patch_antenna_conformal/v6/x2/rbD_L1_copper_38_sm45_on_rbD_L1_dense.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture <dir>/patch_conformal_rbD_L1.msh --radial 0 --outline 8 --filter-radius 0 --copper --corners 6 --feed --rebase-feed --feed-support-legacy --verify-only --verify-n 9 --wide-step 0 --dense 2.3:2.6:41 --design benchmarks/patch_antenna_conformal/copper/rbD_L1_copper_38_sm45.toml (issue #1126)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/v6/x2/rbD_L1_copper_38_sm45_on_rbDflat_L1_dense.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture <dir>/patch_conformal_rbDflat_L1.msh --radial 0 --outline 8 --filter-radius 0 --copper --corners 6 --feed --rebase-feed --feed-support-legacy --verify-only --verify-n 9 --wide-step 0 --dense 2.3:2.6:41 --design benchmarks/patch_antenna_conformal/copper/rbD_L1_copper_38_sm45.toml (issue #1126)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/v6/x2/rbD_L1_copper_38_sm45_on_rbDflat_L1_p2.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture <dir>/patch_conformal_rbDflat_L1.msh --radial 0 --outline 8 --filter-radius 0 --copper --corners 6 --feed --rebase-feed --feed-support-legacy --verify-only --p2 --verify-n 9 --wide-step 0 --design benchmarks/patch_antenna_conformal/copper/rbD_L1_copper_38_sm45.toml (issue #1126)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/v6/x3/initial_on_L1_radiation.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture <dir>/patch_conformal_L1.msh --radial 0 --outline 8 --filter-radius 0 --copper --corners 6 --feed --verify-only --verify-n 0 --wide-step 0 --radiation 2.29,2.3,2.31,2.32,2.33 (issue #1126)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/v6/x3/rbD_L1_copper_38_sm45_on_rbD_L1_radiation.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture <dir>/patch_conformal_rbD_L1.msh --radial 0 --outline 8 --filter-radius 0 --copper --corners 6 --feed --rebase-feed --feed-support-legacy --verify-only --verify-n 0 --wide-step 0 --radiation 2.38,2.4,2.42,2.44,2.46,2.4835,2.5 --design benchmarks/patch_antenna_conformal/copper/rbD_L1_copper_38_sm45.toml (issue #1126)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/v6/x3/rbD_L1_copper_38_sm45_on_rbD_L3_radiation.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture <dir>/patch_conformal_rbD_L3.msh --radial 0 --outline 8 --filter-radius 0 --copper --corners 6 --feed --rebase-feed --feed-support-legacy --air-on-cylinder --verify-only --verify-n 0 --wide-step 0 --radiation 2.4,2.44,2.4835 --design benchmarks/patch_antenna_conformal/copper/rbD_L1_copper_38_sm45.toml (issue #1126)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/v6/x3/rbD_L1_copper_38_sm45_on_rbDair60_L1_dense.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture <dir>/patch_conformal_rbDair60_L1.msh --radial 0 --outline 8 --filter-radius 0 --copper --corners 6 --feed --rebase-feed --feed-support-legacy --verify-only --verify-n 9 --wide-step 0 --radiation 2.4,2.44,2.4835 --dense 2.3:2.6:41 --design benchmarks/patch_antenna_conformal/copper/rbD_L1_copper_38_sm45.toml (issue #1126)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/v6/x3/rbD_L1_copper_38_sm45_on_rbDair80_L1_dense.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture <dir>/patch_conformal_rbDair80_L1.msh --radial 0 --outline 8 --filter-radius 0 --copper --corners 6 --feed --rebase-feed --feed-support-legacy --verify-only --verify-n 9 --wide-step 0 --radiation 2.4,2.44,2.4835 --dense 2.3:2.6:41 --design benchmarks/patch_antenna_conformal/copper/rbD_L1_copper_38_sm45.toml (issue #1126)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/v6/x4/L1_copper_38_cma114.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture <dir>/patch_conformal_L1.msh --radial 0 --outline 8 --filter-radius 0 --copper --corners 6 --feed --gradient-free 114 --cma-sigma0 0.5 --band-shift 0 --n-band 5 --beta 25 --verify-n 9 --label copper_38_cma114 (issue #1126)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/v6/x4/L1_copper_38_cma190_s2.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture <dir>/patch_conformal_L1.msh --radial 0 --outline 8 --filter-radius 0 --copper --corners 6 --feed --gradient-free 190 --cma-sigma0 0.5 --cma-seed 2 --band-shift 0 --n-band 5 --beta 25 --verify-n 9 --label copper_38_cma190_s2 (issue #1126)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/v6/x4/L1_copper_38_cma190_s3.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture <dir>/patch_conformal_L1.msh --radial 0 --outline 8 --filter-radius 0 --copper --corners 6 --feed --gradient-free 190 --cma-sigma0 0.5 --cma-seed 3 --band-shift 0 --n-band 5 --beta 25 --verify-n 9 --label copper_38_cma190_s3 (issue #1126)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/v6/x4/L1_copper_rect4_sm45.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture <dir>/patch_conformal_L1.msh --radial 0 --outline 8 --filter-radius 0 --copper --feed --rect4 --band-shift 0.04494 --n-band 5 --beta 25 --max-steps 60 --verify-n 9 --wide-step 0.025 --label copper_rect4_sm45 (issue #1126)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/v6/x4/rbR_L1_copper_rect4_sm45_rb.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture <dir>/patch_conformal_rbR_L1.msh --radial 0 --outline 8 --filter-radius 0 --copper --feed --rebase-feed --rect4 --band-shift 0.04494 --n-band 5 --beta 25 --max-steps 60 --verify-n 9 --wide-step 0.025 --label copper_rect4_sm45_rb --design benchmarks/patch_antenna_conformal/v6/x4/L1_copper_rect4_sm45.toml (issue #1126)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/v6/x4/rbR_L1_copper_rect4_sm45_rb_on_rbR_L1_dense.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture <dir>/patch_conformal_rbR_L1.msh --radial 0 --outline 8 --filter-radius 0 --copper --feed --rebase-feed --rect4 --verify-only --verify-n 9 --wide-step 0 --dense 2.3:2.6:41 --design benchmarks/patch_antenna_conformal/v6/x4/rbR_L1_copper_rect4_sm45_rb.toml (issue #1126)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/v6/x4/rbR_L1_copper_rect4_sm45_rb_on_rbR_L1_p2.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture <dir>/patch_conformal_rbR_L1.msh --radial 0 --outline 8 --filter-radius 0 --copper --feed --rebase-feed --rect4 --verify-only --p2 --verify-n 9 --wide-step 0 --design benchmarks/patch_antenna_conformal/v6/x4/rbR_L1_copper_rect4_sm45_rb.toml (issue #1126)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/v6/x5/rbD_L1_copper_38_sm45_on_rbG_L1_frozen.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture <dir>/patch_conformal_rbG_L1.msh --radial 0 --outline 8 --filter-radius 0 --copper --corners 6 --feed --rebase-feed --feed-support-legacy --freeze-feed-width --verify-only --verify-n 9 --wide-step 0 --dense 2.3:2.6:41 --design benchmarks/patch_antenna_conformal/copper/rbD_L1_copper_38_sm45.toml (issue #1126)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/v6/x5/rbD_L1_copper_38_sm45_on_rbH_L1_frozen.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture <dir>/patch_conformal_rbH_L1.msh --radial 0 --outline 8 --filter-radius 0 --copper --corners 6 --feed --rebase-feed --feed-support-legacy --freeze-feed-width --verify-only --verify-n 9 --wide-step 0 --dense 2.3:2.6:41 --design benchmarks/patch_antenna_conformal/copper/rbD_L1_copper_38_sm45.toml (issue #1126)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/v6/x5/rbG_L1_copper_38_sm45_w16.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture <dir>/patch_conformal_rbG_L1.msh --radial 0 --outline 8 --filter-radius 0 --copper --corners 6 --feed --rebase-feed --feed-support-legacy --freeze-feed-width --band-shift 0.04494 --n-band 5 --beta 25 --max-steps 60 --verify-n 9 --wide-step 0.025 --label copper_38_sm45_w16 --design benchmarks/patch_antenna_conformal/copper/rbD_L1_copper_38_sm45.toml (issue #1126)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/v6/x5/rbG_L1_copper_38_sm45_w16_on_rbG_L1_dense.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture <dir>/patch_conformal_rbG_L1.msh --radial 0 --outline 8 --filter-radius 0 --copper --corners 6 --feed --rebase-feed --feed-support-legacy --freeze-feed-width --verify-only --verify-n 9 --wide-step 0 --radiation 2.4,2.44,2.4835 --dense 2.3:2.6:41 --design benchmarks/patch_antenna_conformal/v6/x5/rbG_L1_copper_38_sm45_w16.toml (issue #1126)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/v6/x5/rbG_L1_copper_38_sm45_w16_on_rbG_L1_p2.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture <dir>/patch_conformal_rbG_L1.msh --radial 0 --outline 8 --filter-radius 0 --copper --corners 6 --feed --rebase-feed --feed-support-legacy --freeze-feed-width --verify-only --p2 --verify-n 9 --wide-step 0 --design benchmarks/patch_antenna_conformal/v6/x5/rbG_L1_copper_38_sm45_w16.toml (issue #1126)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/v6/x5/rbH_L1_copper_38_sm45_w127.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture <dir>/patch_conformal_rbH_L1.msh --radial 0 --outline 8 --filter-radius 0 --copper --corners 6 --feed --rebase-feed --feed-support-legacy --freeze-feed-width --band-shift 0.04494 --n-band 5 --beta 25 --max-steps 60 --verify-n 9 --wide-step 0.025 --label copper_38_sm45_w127 --design benchmarks/patch_antenna_conformal/copper/rbD_L1_copper_38_sm45.toml (issue #1126)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/v6/x5/rbH_L1_copper_38_sm45_w127_on_rbH_L1_p2.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture <dir>/patch_conformal_rbH_L1.msh --radial 0 --outline 8 --filter-radius 0 --copper --corners 6 --feed --rebase-feed --feed-support-legacy --freeze-feed-width --verify-only --p2 --verify-n 9 --wide-step 0 --design benchmarks/patch_antenna_conformal/v6/x5/rbH_L1_copper_38_sm45_w127.toml (issue #1126)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/v6/palace_loop/L1_copper_38_pcma190_sd.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture <dir>/patch_conformal_L1.msh --radial 0 --outline 8 --filter-radius 0 --copper --corners 6 --feed --band-shift 0 --n-band 5 --beta 25 --verify-n 9 --gradient-free 190 --cma-sigma0 0.5 --cma-seed 18858188808230 --label copper_38_pcma190_sd --palace-eval benchmarks/patch_antenna_conformal/scripts/palace_band_eval.py --palace-workdir <dir> (issue #1130; needs the palace:cpu image of reference/palace/docker/)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/v6/palace_loop/L1_copper_38_pcma190_s2.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture <dir>/patch_conformal_L1.msh --radial 0 --outline 8 --filter-radius 0 --copper --corners 6 --feed --band-shift 0 --n-band 5 --beta 25 --verify-n 9 --gradient-free 190 --cma-sigma0 0.5 --cma-seed 2 --label copper_38_pcma190_s2 --palace-eval benchmarks/patch_antenna_conformal/scripts/palace_band_eval.py --palace-workdir <dir> (issue #1130; needs the palace:cpu image of reference/palace/docker/)",
    ),
    a(
        "benchmarks/patch_antenna_conformal/v6/palace_loop/L1_copper_38_pcma190_s3.toml",
        "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture <dir>/patch_conformal_L1.msh --radial 0 --outline 8 --filter-radius 0 --copper --corners 6 --feed --band-shift 0 --n-band 5 --beta 25 --verify-n 9 --gradient-free 190 --cma-sigma0 0.5 --cma-seed 3 --label copper_38_pcma190_s3 --palace-eval benchmarks/patch_antenna_conformal/scripts/palace_band_eval.py --palace-workdir <dir> (issue #1130; needs the palace:cpu image of reference/palace/docker/)",
    ),
    b(
        "benchmarks/patch_antenna_conformal/v6/palace_loop/L1_copper_38_pnewuoa190.toml",
        "palace_loop_prima.py: PRIMA NEWUOA (Fortran, no Burn backend) driving Palace (MFEM, \
         no Burn backend) through patch_conformal_nested_dof --eval-server --palace-eval, which \
         supplies only the mesh morph and the volume guard; issue #1130",
    ),
    a(
        "benchmarks/patch_antenna_conformal/v6/palace_loop/costs_geode_r6i.toml",
        "python3 benchmarks/patch_antenna_conformal/scripts/palace_loop_xeval.py --exe <target>/release/examples/patch_conformal_nested_dof --out <out> --solver geode --repeat-x0 3 --grad --design NAME=TOML ... -- --fixture <dir>/patch_conformal_L1.msh --radial 0 --outline 8 --filter-radius 0 --copper --corners 6 --feed --band-shift 0 --n-band 5 --beta 25 --verify-n 9 (issue #1130)",
    ),
    b(
        "benchmarks/patch_antenna_conformal/v6/palace_loop/costs_palace_r6i.toml",
        "palace_loop_xeval.py --solver palace: Palace (MFEM, no Burn backend) band evaluations \
         and their wall time; geode supplies only the mesh morph; issue #1130",
    ),
    a(
        "benchmarks/patch_antenna_conformal/v6/palace_loop/cross_geode_on_palace_designs.toml",
        "python3 benchmarks/patch_antenna_conformal/scripts/palace_loop_xeval.py --exe <target>/release/examples/patch_conformal_nested_dof --out <out> --solver geode --design NAME=v6/palace_loop/L1_*.toml ... -- --fixture <dir>/patch_conformal_L1.msh --radial 0 --outline 8 --filter-radius 0 --copper --corners 6 --feed --band-shift 0 --n-band 5 --beta 25 --verify-n 9 (issue #1130)",
    ),
    b(
        "benchmarks/patch_antenna_conformal/v6/palace_loop/palace_loop_summary.toml",
        "palace_loop_summary.py: post-processing of v6/palace_loop/*.toml, v6/x4/*.toml and \
         copper/L1_copper_38.toml (issue #1130); no Burn backend",
    ),
    b(
        "benchmarks/patch_antenna_conformal/v6/v6_summary.toml",
        "v6_summary.py: post-processing of the bucket-A v6/**/*.toml and copper/verify/*.toml \
         (issue #1126); no Burn backend",
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
