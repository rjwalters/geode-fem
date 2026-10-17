# docs/

Project documentation that does not belong to a single crate: the roadmap,
design records and research notes, and the images the root
[README](../README.md) embeds. The `geode` CLI reference is in
[`crates/geode-cli/README.md`](../crates/geode-cli/README.md), and each crate
under [`crates/`](../crates/README.md) has its own README.

| Path | Contents |
|---|---|
| [`ROADMAP.md`](ROADMAP.md) | Where the project is going, one item per GitHub epic. Update it in the same PR as any change of epic scope |
| [`formulation_audit_reduced_vs_full_vector.md`](formulation_audit_reduced_vs_full_vector.md) | Audit of the full-vector E_t–E_z mixed pencil against the reduced E_t-only dielectric solver (Epic #339, issue #449; corrected by #791) |
| [`research/`](research/) | Dated research and decision notes; each states its own status |
| [`images/`](images/) | Benchmark tearsheets and field renders used by the root README |

## research/

| Note | Status |
|---|---|
| [`2026-07-16-strategic-direction.md`](research/2026-07-16-strategic-direction.md) | Strategic direction: become the differentiable EM design engine, not a Palace speed rival. Proposal for discussion; the raw sources are in [`2026-07-16-strategic-direction.raw-findings.md`](research/2026-07-16-strategic-direction.raw-findings.md) |
| [`driven-first-performance-strategy.md`](research/driven-first-performance-strategy.md) | Lead the performance story with the driven solve. Design decision (Epic #569) |
| [`geode-vs-palace-comparison.md`](research/geode-vs-palace-comparison.md) | Measured comparison with Palace, written for the Palace authors |
| [`gpu-eigensolve-path-design.md`](research/gpu-eigensolve-path-design.md) | Which GPU eigensolve to build, if any. Design decision (issue #503) |
| [`2026-07-20-distributed-collectives-ir-vs-ffi.md`](research/2026-07-20-distributed-collectives-ir-vs-ffi.md) | Tensor-IR vs FFI boundary for distributed collectives. Decided (Epic #546 Phase B) |
| [`transmon-paper-reframe.md`](research/transmon-paper-reframe.md) | Outline of the transmon paper: LOM optimization now, eigenmode as roadmap (Epic #476) |

The tearsheet images in `images/` are regenerated with the `geode_viz`
package in [`tools/viz/`](../tools/viz/README.md).
