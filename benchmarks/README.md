# benchmarks/

Committed results for the GEODE-FEM benchmark suite, one subdirectory per
benchmark family. Most files are `results*.toml` written by an example binary
or a `--ignored` release-tier test, and the header of each file names the
command that regenerates it and the test that reads it. The tests compare a
fresh solve against these numbers within documented tolerance bands, or skip
with a note when an external oracle such as Palace has not been run. Files
marked "Auto-generated" must not be edited by hand: regenerate them after an
intentional change and commit the new file with the code change.

| Directory | What it records | Regenerate with |
|---|---|---|
| [`electrostatic/`](electrostatic/) | Maxwell capacitance matrix from the P1 electrostatic solve (Epic #475) | `cargo run -p geode-core --release --example electrostatic_capacitance` |
| [`magnetostatic_inductance/`](magnetostatic_inductance/) | Maxwell inductance matrix from the Nédélec magnetostatic solve (Epic #475) | `cargo run -p geode-core --release --example magnetostatic_inductance` |
| [`mie_sphere/`](mie_sphere/) | Mie sphere eigenmodes, matched-UPML quasi-modes and driven scattering `Q_ext` / `Q_sca` | `cargo run -p mie_sphere --release`, `-p mie_open_quasimode`, `-p mie_driven_scattering` |
| [`patch_antenna/`](patch_antenna/) | Probe-fed FR-4 patch: S11, bandwidth, efficiency, NTFF pattern (Epic #226) | `cargo run -p patch_antenna --release [-- smoke\|matched\|pattern\|pattern-matched]` |
| [`patch_antenna_diffopt/`](patch_antenna_diffopt/) | `\|S11\|²` shape-optimization loops on the H(curl) adjoint (#626, #636) | `cargo run -p geode-core --release --example patch_diffopt` / `patch_capstone_diffopt` |
| [`patch_antenna_conformal/`](patch_antenna_conformal/) | Band shape optimization on curved conformal metal (Epic #647) | `cargo run -p geode-core --release --example patch_conformal_diffopt` |
| [`fdtd_density_baseline/`](fdtd_density_baseline/) | Meep / Ceviche density-method baselines for the conformal-antenna paper (#651); has its own [README](fdtd_density_baseline/README.md) | Python scripts in the directory |
| [`spiral_inductor/`](spiral_inductor/) | 3.5-turn spiral L / R / Q / S11 vs Mohan and MoM PEEC (#211) | `cargo run -p spiral_inductor --release [-- smoke]` |
| [`slcfet_3hp/`](slcfet_3hp/) | 3-turn Au-on-SiC spiral capstone, quasi-static L₀ (#212) | `cargo run -p slcfet_3hp_spiral --release [-- smoke]` |
| [`soi_waveguide/`](soi_waveguide/) | SOI strip-waveguide quasi-TE `n_eff` vs the effective-index oracle (#306) | `cargo run -p soi_waveguide --release` |
| [`step_index_fiber/`](step_index_fiber/) | SMF-28 fundamental mode vs the exact LP-mode oracle (#333) | `cargo run -p step_index_fiber --release` |
| [`fiber_dispersion/`](fiber_dispersion/) | SMF-28 chromatic dispersion D(λ) and zero-dispersion wavelength (#479) | `GEODE_BLESS_FIBER_DISPERSION=1 cargo test -p geode-core --release --test fiber_dispersion_benchmark -- --ignored` |
| [`motor/`](motor/) | Slotless-PM motor locked-rotor torque vs angle (Epic #448) | `cargo run -p geode-core --release --example motor_torque` |
| [`periodic/`](periodic/) | Zero-phase periodic BCs on p=1 Nédélec (#839) | `GEODE_BLESS_PERIODIC=1 cargo test -p geode-core --release --test periodic_cavity -- --ignored regenerate_periodic_results` |
| [`transient/`](transient/) | Generalized-α transient solve with a lumped-port drive (Epic #475) | `GEODE_BLESS_TRANSIENT=1 cargo test -p geode-core --release --test transient_sparams transient_self_oracle_broadband -- --ignored` |
| [`transmon_eigen/`](transmon_eigen/) | Transmon + readout-resonator eigenmodes, cross-validated against Palace (#492) | `transmon_eigenmode.rs` release test (`--ignored`); Palace block from [`reference/fixtures/transmon_palace/`](../reference/fixtures/transmon_palace/) |
| [`transmon_quantum/`](transmon_quantum/) | Transmon `E_C` from the capacitance matrix (Epic #476) | `cargo run -p geode-core --release --example transmon_quantum` |
| [`transmon_diffopt/`](transmon_diffopt/) | Gradient-based transmon geometry optimization (#589, #594) | `cargo run -p geode-core --release --example transmon_diffopt` / `transmon_pad_diffopt` / `transmon_pad_harmonic` |
| [`transmon_bench_cpu/`](transmon_bench_cpu/) | Measured geode-vs-Palace CPU timing and memory on the same transmon mesh; performance record only, not a CI gate | measured on EC2; see the file header |
| [`transmon_ams_minres_133k/`](transmon_ams_minres_133k/) | AMS-preconditioned MINRES at the 4.5 GHz interior shift, 133k DOF (#531) | measured; see the file header |
| [`gpu_driven_scaling/`](gpu_driven_scaling/) | GPU vs CPU driven-solve wall-clock scaling (#501) | `crates/geode-core/tests/gpu_driven_scaling.rs` (`--ignored`) |
| [`mixed_precision_refinement/`](mixed_precision_refinement/) | f64 iterative refinement over an f32 inner matvec (#534) | `cargo test -p geode-core --release --test mixed_precision_refinement -- --ignored --nocapture` |
| [`perf/`](perf/) | Criterion wall-clock baseline for assembly and eigensolves (#50) | `cargo bench -p geode-core`, then `cargo run -p extract_baseline` |

The root [README](../README.md) quotes several of these numbers. The plotting
package in [`tools/viz/`](../tools/viz/) reads them to draw the benchmark
tearsheets.
