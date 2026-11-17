//! Shared one-port spiral-inductor frequency sweep (issue #1572).
//!
//! The generic spiral benchmark (`examples/spiral_inductor`, issue #211)
//! and the SLCFET 3HP benchmark (`examples/slcfet_3hp_spiral`, issue
//! #212) are the **same** port-driven one-port sweep on two material
//! sets: assemble the ω-independent operator once, drive the fixture's
//! 50 Ω lumped port with `V_inc = 1`, and reduce `Z = V/I` at every
//! frequency to `L = Im Z/ω`, `R = Re Z`, `Q = Im Z/Re Z`, `|S₁₁|`.
//! Each example carried a private copy of that pipeline — the row type,
//! its serde view, and the sweep driver — differing only in the
//! material set and the output directory. This module is the single
//! copy; [`run_sweep`] takes the materials as an argument.
//!
//! The cost of the duplication was that any change to the row columns,
//! the `L = Im Z / ω` convention, or the port/BC setup had to be made
//! twice and kept in step by hand. That already happened once in the
//! other direction: the `f → 0` extrapolation was a private copy in the
//! SLCFET example until issue #682 promoted it to
//! [`geode_core::driven::extraction::extrapolate_l0`].
//!
//! # What stays per-example
//!
//! Only the sweep is shared. Each benchmark keeps its own `emit_results`
//! (different oracle constants, different reference frequencies, the
//! SLCFET-only `L₀` block), its own sweep frequency lists, its own CLI,
//! and its own `benchmarks/<name>/` directory — passed to
//! [`results_path`].
//!
//! # Why `geode-util` and not `geode-core`
//!
//! Issue #1572 proposed `geode_core::driven::extraction`, next to
//! `extrapolate_l0`. This landed here instead, for two reasons:
//!
//! - [`PointRow`] is a `serde` DTO feeding the
//!   [`crate::fixture::push_rows`] row seam, and `geode-core` has no
//!   `serde` dependency (not even a transitive one in `src/`). Adding
//!   one to core for a benchmark DTO works against issue #1365's
//!   slim/`tensor`-free `geode-core` build (PR #1376), which exists to
//!   *shrink* core's dependency graph.
//! - `geode-util` is documented as exactly this: shared helpers "that
//!   several consumers (`geode-validation`, the standalone example
//!   crates, future tools) would otherwise re-implement, but which are
//!   not (yet) core FEM kernels and so do not belong in `geode-core`"
//!   (see the crate root). It already hosts the row seam
//!   ([`crate::fixture::push_rows`]) and the GHz → ω conversion
//!   ([`crate::units::ghz_to_omega_um`]) this sweep uses, and both
//!   example crates already depend on it.
//!
//! The layering arrow is unchanged — this module reads `geode-core`,
//! core stays unaware of `geode-util`.

use std::path::PathBuf;

use burn::tensor::backend::Backend;
use faer::c64;
use serde::Serialize;

use geode_core::constants::ETA_0_OHM as ETA_0;
use geode_core::driven::extraction::driven_frequency_sweep;
use geode_core::driven::solve::{
    CurrentSource, DrivenBcs, DrivenError, DrivenMaterials, SurfaceImpedanceBc,
    SurfaceImpedanceModel,
};
use geode_core::mesh::{SpiralFixture, SpiralMaterials, pec_interior_mask_from_triangles};

use crate::units::ghz_to_omega_um as ghz_to_omega;

/// Port reference resistance (Ω) of both spiral benchmarks.
pub const R_PORT_OHM: f64 = 50.0;

/// Which bundled fixture a benchmark run sweeps, and therefore which
/// results file it writes.
///
/// The concrete mesh behind each variant is per-example (the generic
/// benchmark's `spiral_3p5.msh` vs the SLCFET `spiral_slcfet_3hp.msh`);
/// what is shared is the benchmark/smoke split and the output file name
/// it selects ([`results_path`]).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FixtureChoice {
    /// The full benchmark fixture → `results.toml`.
    Benchmark,
    /// The coarse smoke fixture → `results_smoke.toml`.
    Smoke,
}

impl FixtureChoice {
    /// Results file name written for this choice.
    pub fn results_file_name(self) -> &'static str {
        match self {
            FixtureChoice::Benchmark => "results.toml",
            FixtureChoice::Smoke => "results_smoke.toml",
        }
    }
}

/// Absolute path of the results TOML for a benchmark directory:
/// `<repo root>/benchmarks/<benchmark_dir>/<results file>`.
///
/// `benchmark_dir` is the per-example directory name (`spiral_inductor`,
/// `slcfet_3hp`); the file name comes from
/// [`FixtureChoice::results_file_name`].
pub fn results_path(benchmark_dir: &str, choice: FixtureChoice) -> PathBuf {
    crate::repo::repo_root()
        .join("benchmarks")
        .join(benchmark_dir)
        .join(choice.results_file_name())
}

/// One frequency point of a spiral one-port sweep, in the benchmarks'
/// reporting units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Row {
    /// Sweep frequency (GHz).
    pub f_ghz: f64,
    /// Natural-unit angular frequency `ω ≡ k₀` of the solve.
    pub omega: f64,
    /// Port input impedance `Z = V/I` (Ω).
    pub z_ohm: c64,
    /// Series inductance `L = Im Z / ω` (nH).
    pub l_nh: f64,
    /// Series resistance `R = Re Z` (Ω).
    pub r_ohm: f64,
    /// Quality factor `Q = Im Z / Re Z`.
    pub q: f64,
    /// Reflection magnitude `|S₁₁|` vs the 50 Ω reference.
    pub s11_mag: f64,
    /// Post-solve relative residual at this frequency.
    pub residual_rel: f64,
}

/// Serde view of a [`Row`] matching the emitted `[point_<i>]` TOML
/// columns: the `c64` impedance is split into its real/imaginary parts
/// and the natural-frequency / residual columns are renamed. Serialized
/// through the shared [`crate::fixture::push_rows`] seam.
///
/// The field names are the committed artifact's column names — changing
/// one changes every `benchmarks/*/results*.toml`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct PointRow {
    /// `f_ghz` column.
    pub f_ghz: f64,
    /// `omega_natural` column ([`Row::omega`]).
    pub omega_natural: f64,
    /// `z_re_ohm` column.
    pub z_re_ohm: f64,
    /// `z_im_ohm` column.
    pub z_im_ohm: f64,
    /// `l_nh` column.
    pub l_nh: f64,
    /// `r_ohm` column.
    pub r_ohm: f64,
    /// `q` column.
    pub q: f64,
    /// `s11_mag` column.
    pub s11_mag: f64,
    /// `solve_residual_rel` column ([`Row::residual_rel`]).
    pub solve_residual_rel: f64,
}

impl From<&Row> for PointRow {
    fn from(r: &Row) -> Self {
        PointRow {
            f_ghz: r.f_ghz,
            omega_natural: r.omega,
            z_re_ohm: r.z_ohm.re,
            z_im_ohm: r.z_ohm.im,
            l_nh: r.l_nh,
            r_ohm: r.r_ohm,
            q: r.q,
            s11_mag: r.s11_mag,
            solve_residual_rel: r.residual_rel,
        }
    }
}

/// Port-driven frequency sweep of a spiral fixture with the given
/// material set.
///
/// Setup (identical for both benchmarks, the material set aside):
///
/// - per-tet permittivities from `materials`
///   ([`SpiralFixture::epsilon_r_for`]);
/// - PEC outer walls (edge-exact mask);
/// - the conductor cavity walls carry the Leontovich good-conductor
///   surface impedance at `materials.conductor_sigma_natural()`;
/// - a [`R_PORT_OHM`] lumped port driven with `V_inc = 1`.
///
/// The ω-independent operator assembles **once**
/// ([`driven_frequency_sweep`]); per frequency only the complex
/// recombination, the sparse LU and the port readback run. Assembly and
/// sweep-timing progress go to stderr.
///
/// Pass [`geode_core::mesh::GENERIC_MATERIALS`] for the generic spiral
/// benchmark and [`geode_core::mesh::SLCFET_3HP_MATERIALS`] for the
/// SLCFET 3HP one. `GENERIC_MATERIALS.conductor_sigma_natural()` is
/// exactly (`assert_eq!`-level) the legacy
/// `geode_core::mesh::spiral::CONDUCTOR_SIGMA_NATURAL` constant the
/// generic example used to pass — see `conductor_sigma_natural_units`
/// in `geode_core::mesh::spiral` — so routing through `materials` does
/// not move a single bit of the generic benchmark's output.
///
/// # Errors
///
/// Propagates any [`DrivenError`] from [`driven_frequency_sweep`]; the
/// callers attach their own fixture-specific context.
pub fn run_sweep<B: Backend>(
    device: &B::Device,
    fixture: &SpiralFixture,
    materials: &SpiralMaterials,
    freqs_ghz: &[f64],
) -> Result<Vec<Row>, DrivenError> {
    let edges = fixture.mesh.edges();
    let eps = fixture.epsilon_r_for(materials);

    // PEC outer walls; the conductor surface carries the Leontovich BC.
    let outer = fixture.outer_boundary_triangles();
    let mask = pec_interior_mask_from_triangles(&edges, &[outer.as_slice()]);

    let cond = fixture.conductor_triangles();
    let surface = SurfaceImpedanceBc {
        triangles: &cond,
        model: SurfaceImpedanceModel::GoodConductor {
            sigma: materials.conductor_sigma_natural(),
        },
    };

    let port = fixture.port();
    let r_nat = R_PORT_OHM / ETA_0;
    let lp = port.lumped_port(r_nat, c64::new(1.0, 0.0));

    let source = CurrentSource {
        j_tet: vec![[c64::new(0.0, 0.0); 3]; fixture.mesh.n_tets()],
    };

    let omegas: Vec<f64> = freqs_ghz.iter().map(|&f| ghz_to_omega(f)).collect();

    eprintln!(
        "assembling DrivenOperator: {} edges, {} tets, {} conductor faces, {} port faces",
        edges.len(),
        fixture.mesh.n_tets(),
        cond.len(),
        port.faces.len()
    );
    let t0 = std::time::Instant::now();
    let points = driven_frequency_sweep::<B>(
        &fixture.mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &DrivenBcs {
            pec_interior_mask: &mask,
        },
        std::slice::from_ref(&lp),
        std::slice::from_ref(&surface),
        &omegas,
        &source,
        device,
    )?;
    eprintln!(
        "sweep of {} points done in {:.1} s",
        points.len(),
        t0.elapsed().as_secs_f64()
    );

    Ok(points
        .iter()
        .zip(freqs_ghz.iter())
        .map(|(pt, &f_ghz)| {
            let pc = pt.ports[0];
            let z_ohm = pc.z * ETA_0;
            // L = Im Z / ω in SI: Im(Z_Ω) / (2π f) — in nH the 1e9
            // cancels against f in GHz.
            let l_nh = z_ohm.im / (2.0 * std::f64::consts::PI * f_ghz);
            Row {
                f_ghz,
                omega: pt.omega,
                z_ohm,
                l_nh,
                r_ohm: z_ohm.re,
                q: pc.quality_factor(),
                s11_mag: pc.s11(r_nat).norm(),
                residual_rel: pt.residual_rel,
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use geode_core::mesh::{GENERIC_MATERIALS, SLCFET_3HP_MATERIALS};

    /// The committed `[point_<i>]` column names are the artifact's
    /// schema: this pins them (and their order) so a rename of a `Row`
    /// field cannot silently rewrite every `benchmarks/*/results*.toml`.
    #[test]
    fn point_row_columns_match_the_committed_artifact_schema() {
        let row = Row {
            f_ghz: 1.0,
            omega: 2.0,
            z_ohm: c64::new(3.0, 4.0),
            l_nh: 5.0,
            r_ohm: 3.0,
            q: 6.0,
            s11_mag: 0.5,
            residual_rel: 1e-12,
        };
        let mut s = String::new();
        crate::fixture::push_rows(&mut s, "point", &[PointRow::from(&row)]);
        let names: Vec<&str> = s
            .lines()
            .filter_map(|l| l.split_once(" = ").map(|(k, _)| k))
            .collect();
        assert_eq!(
            names,
            [
                "f_ghz",
                "omega_natural",
                "z_re_ohm",
                "z_im_ohm",
                "l_nh",
                "r_ohm",
                "q",
                "s11_mag",
                "solve_residual_rel",
            ]
        );
        assert!(s.starts_with("[point_0]\n"), "table header: {s}");
    }

    /// The `c64` impedance splits into the two real columns, and the
    /// renamed columns carry the right source fields.
    #[test]
    fn point_row_splits_impedance_and_renames_columns() {
        let row = Row {
            f_ghz: 2.5,
            omega: 0.25,
            z_ohm: c64::new(-1.5, 7.25),
            l_nh: 1.0,
            r_ohm: -1.5,
            q: -4.0,
            s11_mag: 0.125,
            residual_rel: 3e-13,
        };
        let p = PointRow::from(&row);
        assert_eq!(p.z_re_ohm, -1.5);
        assert_eq!(p.z_im_ohm, 7.25);
        assert_eq!(p.omega_natural, row.omega);
        assert_eq!(p.solve_residual_rel, row.residual_rel);
    }

    /// `results_path` keeps the per-example directory and picks the file
    /// name from the fixture choice.
    #[test]
    fn results_path_is_per_example_directory_and_choice() {
        let bench = results_path("spiral_inductor", FixtureChoice::Benchmark);
        assert!(
            bench.ends_with("benchmarks/spiral_inductor/results.toml"),
            "{bench:?}"
        );
        let smoke = results_path("slcfet_3hp", FixtureChoice::Smoke);
        assert!(
            smoke.ends_with("benchmarks/slcfet_3hp/results_smoke.toml"),
            "{smoke:?}"
        );
    }

    /// Routing the generic example's conductivity through
    /// `SpiralMaterials` is bit-identical to the legacy constant it used
    /// to pass directly — the issue-#1572 acceptance condition for "no
    /// numeric diff" on `benchmarks/spiral_inductor/results_smoke.toml`.
    #[test]
    fn generic_materials_reproduce_the_legacy_conductivity_exactly() {
        assert_eq!(
            GENERIC_MATERIALS.conductor_sigma_natural(),
            geode_core::mesh::spiral::CONDUCTOR_SIGMA_NATURAL
        );
        // And the two material sets are genuinely different, so the
        // parameterization is load-bearing rather than cosmetic.
        assert_ne!(
            SLCFET_3HP_MATERIALS.conductor_sigma_natural(),
            GENERIC_MATERIALS.conductor_sigma_natural()
        );
    }
}
