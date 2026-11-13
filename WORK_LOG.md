# Work Log

Chronological record of merged pull requests and closed issues, newest first.
Maintained automatically by the Loom Guide role; entries are appended, never edited.

### 2026-11-13

- **PR #1589**: feat(floquet): differentiate the lumped delta-gap scan impedance
- **PR #1590**: refactor(cli): split load_parsed into named phase helpers
- **PR #1587**: refactor(examples): share the five adaptive-loop transmon helpers
- **PR #1584**: refactor: route conduction, eqs_transient and adapt sparse kernels through sparse_ops
- **PR #1585**: refactor: share the one-port spiral sweep between the two spiral benchmarks
- **PR #1575**: refactor: share scaled_matrix and conductor_summaries across static-matrix commands
- **PR #1577**: refactor(tests): share tf/tstr TOML helpers from transmon_util
- **PR #1576**: test: extract one rect_wave_port helper for the nine inline port builders
- **PR #1560**: feat: add steady heat conduction solve and Picard Joule self-heating loop
- **PR #1571**: fix: correct Korhonen citation author and add EmError::Negative for t_end
- **PR #1539**: feat(shape): Floquet element parameterization and trace/gap constraints
- **Issue #1578** (closed): [#1537 P4c-1] Lumped-feed Γ_active adjoint and Design A min-max driver
- **Issue #1583** (closed): Split load_parsed: 863-line function in geode-cli problem/mod.rs into phase helpers
- **Issue #1582** (closed): Share the adaptive-loop helpers duplicated between transmon_refined_optimize and transmon_c_sigma_bracket examples (~100 LOC)
- **Issue #1573** (closed): Route sparse kernels re-declared in conduction, eqs_transient and adapt/driver through sparse_ops; drop the second complex spmv
- **Issue #1572** (closed): Share the spiral sweep (Row, PointRow, run_sweep) between spiral_inductor and slcfet_3hp_spiral examples
- **Issue #1566** (closed): Share the scale-and-finite-check and ConductorSummary helpers across geode-cli static-matrix commands
- **Issue #1563** (closed): Finish #1552: share tf/tstr TOML helpers and drop transmon_eigenmode's local copies
- **Issue #1570** (closed): Consolidate 9 copies of the rectangular wave-port test builder into tests/common
- **Issue #1556** (closed): Heat conduction with Joule-heat source and self-heating loop
- **Issue #1564** (closed): electromigration: verify the Korhonen 1993 author list and fix a misleading t_end error
- **Issue #1535** (closed): [#1534 P4a] Wide-scan element parameterization and PCB manufacturability constraints
- **Issue #1452** (closed): Remove unreferenced 37 MB omni.ja fixture from vendored zip test-support
- **PR #1562**: feat: add vector-P1 thermo-elastic solver with stress and hydrostatic recovery
- **PR #1554**: refactor: share repo_root, vacuum_eps and toml_num helpers across geode-core tests
- **PR #1561**: feat: add Black, Blech and 1-D Korhonen electromigration models
- **PR #1559**: docs: build plan for self-heating, thermo-elastic stress and electromigration models
- **PR #1538**: feat(benchmarks): TCDA-like baseline cell, recorded |Γ_active| scan and min-max evaluator
- **PR #1553**: refactor: share tget, tarr and zero_source across tests and examples
- **PR #1550**: refactor: remove superseded cube-only p=2 shape-gradient wrappers
- **PR #1548**: refactor: share write_json, curves_json and sha256_hex across examples and tests
- **PR #1545**: refactor: split driven solve tests and surface-impedance types into sibling modules
- **PR #1541**: fix(ci): restore green main (Command import, rustdoc pipes, needless_range_loop)
- **Issue #1557** (closed): Linear thermo-elastic stress from the temperature field
- **Issue #1551** (closed): Dedupe per-file test helpers repo(), vacuum(), num() into shared test module
- **Issue #1558** (closed): Electromigration lifetime models (Black, Blech, Korhonen) as field consumers
- **Issue #1555** (closed): Self-heating, thermo-elastic stress and electromigration lifetime on top of DC conduction: design note and split
- **Issue #1536** (closed): [#1534 P4b] TCDA-like baseline element and scan-grid evaluator
- **Issue #1552** (closed): Consolidate byte-identical test/example helpers (tget, tarr, zero_source) into shared modules
- **Issue #1542** (closed): cargo doc -D warnings fails: unescaped | in rustdoc tables (port_modes.rs, icem_domain.rs)
- **Issue #1549** (closed): Remove superseded cube-only driven_shape_gradient_p2 wrappers
- **Issue #1547** (closed): Dedupe copy-pasted helpers across geode-core examples/tests (write_json, curves_json, sha256_hex)
- **Issue #1544** (closed): Split driven/solve.rs: move inline tests and surface-impedance types out (6.3k lines)
- **Issue #1540** (closed): main red: geode-cli floquet_slab_golden missing Command import, 2 rustdoc table warnings, 4 clippy needless_range_loop

### 2026-11-12

- **PR #1533**: refactor(tests): share geode/manifest_dir/f64_at helpers via support/cli.rs
- **PR #1531**: feat: Floquet z_lo/z_hi port groups for periodic driven geode mesh
- **PR #1532**: feat: epigraph min-max over Γ_active grid with Floquet adjoint gradient
- **PR #1527**: test: pin measured dipole-array FEM/oracle mismatch envelope
- **PR #1529**: fix(eqs): midpoint displacement work; reject t_end not a multiple of dt
- **PR #1525**: feat: fixed-k_t reduced-order model for Floquet-port adaptive sweeps
- **PR #1524**: refactor: share xorshift64* and 32-bit LCG helpers instead of hand-rolled copies
- **PR #1499**: feat: add geode eqs subcommand for EQS transient and admittance
- **PR #1522**: fix: update stale Phase 4b message in periodic mesh guard
- **PR #1523**: refactor: use geode_util::rng::Lcg in remaining test LCG copies
- **PR #1520**: feat: add delta-gap edge feed and dipole-array Floquet-sum oracle (AC2 not yet within 3%)
- **PR #1516**: feat(cli): Floquet-port driven unit cell with incidence/scan, per-order rows and per-angle Touchstone
- **PR #1517**: feat: add array-element active reflection and grating-lobe/blind-scan detection to Floquet cells
- **PR #1509**: refactor(tests): share repo_root and RNG helpers via geode-util
- **PR #1508**: chore(repo): untrack 422 ignored console logs not cited as evidence
- **PR #1507**: Bump Repo Skills 0.25.10 -> 0.26.1
- **PR #1504**: refactor(examples): share transmon example helpers via transmon_util
- **PR #1503**: chore(benchmarks): untrack ignored motor_validation console logs
- **Issue #1526** (closed): Dedupe geode-cli test helpers: 22× geode(), 11× manifest_dir(), 4× f64_at()
- **Issue #1510** (closed): Verify epic #1252 deliverable before closing
- **Issue #1530** (closed): [#837 P4b] geode mesh --periodic: generate driven cell with Floquet port groups
- **Issue #1515** (closed): [#1252 P3] Min-max |Γ_active| design objective over (θ,φ,f) with adjoint reuse
- **Issue #837** (closed): Epic: periodic / Floquet boundaries — unit-cell eigen + driven with Floquet ports (v0.9.0)
- **Issue #1513** (closed): [#1252 P2] Infinite-array active reflection Γ_active(θ,φ,f) and its analytic oracles
- **Issue #1528** (closed): eqs: closed lossless pulse reports nonzero net displacement work; non-multiple t_end silently rounded
- **Issue #1514** (closed): [#837 P4b] CLI — Floquet ports, incidence/scan sweeps, per-order report and Touchstone
- **Issue #1521** (closed): [#837 P4b] Floquet adaptive PROM at fixed k_t (core reduced-order API + CLI)
- **Issue #1518** (closed): Consolidate hand-rolled xorshift/LCG copies into shared RNG helpers
- **Issue #1494** (closed): CLI: spec-file problem type for the electro-quasi-static transient and admittance solve (#1478 is library-only)
- **Issue #1519** (closed): Consolidate 8 inline LCG copies onto geode_util::rng::Lcg
- **Issue #1511** (closed): Verify epic #837 deliverable before closing
- **Issue #1512** (closed): Verify epic #836 deliverable before closing
- **Issue #1253** (closed): Epic: GEODE-Bench — gradient-labelled unstructured-mesh EM dataset + gradient-supervised surrogates
- **Issue #1505** (closed): Consolidate duplicated repo_root() and tiny RNG helpers onto geode-util
- **Issue #1506** (closed): Untrack 541 ignored-but-tracked *.log files (follow-up to 0361da65)
- **Issue #1502** (closed): Consolidate copy-pasted helpers across examples into shared transmon_util module
- **Issue #1501** (closed): Untrack ignored motor_validation run logs (141 files, ~2 MB)

### 2026-11-11

- **PR #1500**: refactor(mesh): share hex/prism tet-split helpers between fixtures
- **PR #1498**: feat(cli): export per-element J and Joule density from geode conduction
- **Issue #1497** (closed): Deduplicate hex/prism tet-split helpers shared by electrostatic and magnetostatic fixtures
- **Issue #1495** (closed): conduction: export per-element J and Joule density, and report a ball/box-averaged peak current density (pointwise corner maximum is mesh-dependent)

### 2026-11-10

- **PR #1492**: feat: add electro-quasi-static transient and admittance solver
- **PR #1493**: feat: add geode conduction subcommand for DC terminal conductance matrices
- **PR #1490**: docs: hide solve_transmon_eigenmodes_gauged from public rustdoc
- **PR #1484**: feat(layout): round convex conductor corners via optional edge_radius
- **PR #1491**: docs: mark Mesh/Element/Operator traits as reserved and unused
- **PR #1487**: ci: raise geode-cli job timeout from 60 to 90 min
- **PR #1486**: test: use shared Scratch helper in four geode-cli golden tests
- **PR #1483**: feat(conduction): steady DC current conduction with conductance matrix, Joule power and r_ext terminals
- **PR #1474**: refactor: share one test_device helper across tests and examples
- **PR #1473**: feat: re-mesh ladder for geode optimize when the mesh-quality guard trips
- **PR #1472**: test: tune a 2-pole iris waveguide filter with geode optimize
- **PR #1311**: feat: add geode optimize, a gradient-based design loop over the sensitivity section
- **Issue #475** (closed): Epic: AWS Palace feature parity — problem-type and infrastructure roadmap
- **Issue #1478** (closed): Electro-quasi-static transient: charging of floating conductors through leaky dielectrics
- **Issue #1479** (closed): DC conduction CLI problem type and report fields (split from #1476)
- **Issue #1411** (closed): Remove solve_transmon_eigenmodes_gauged: test-only API pinning a superseded negative result
- **Issue #1482** (closed): Layout-to-mesh: conductor edge-radius parameter (#1477 slice C)
- **Issue #1469** (closed): Remove unused Mesh/Element/Operator placeholder traits (or fix their stale docs)
- **Issue #1485** (closed): test flake: geode-cli job hits 60m timeout (cancelled, no failing test), recurs after #1320
- **Issue #1475** (closed): Remove 4 hand-rolled TempDir structs in geode-cli tests: use shared support::Scratch
- **Issue #1476** (closed): DC current conduction: varying conductivity, terminal conductance matrix, current density and Joule heat
- **Issue #476** (closed): Epic: reproduce AWS DeviceLayout.jl transmon benchmarks in geode-fem + honest head-to-head vs Palace
- **Issue #569** (closed): Epic: reposition GEODE as the differentiable-by-construction complement to Palace (design sensitivities, not raw speed)
- **Issue #1470** (closed): Consolidate 94 copy-pasted test device() helpers into geode_core::testing
- **Issue #1303** (closed): [#841 P6b-B] geode optimize: re-mesh policy and waveguide iris golden
- **Issue #1471** (closed): [#841 P6b-B2] geode optimize: 2-pole inductive-iris waveguide golden (ii)
- **Issue #1261** (closed): [#841 P6b] geode optimize <spec> — gradient-based design loop (L-BFGS-B / MMA) with checkpointing, re-mesh policy, passivity tripwire

### 2026-11-09

- **PR #1464**: Bump nix from 0.29.0 to 0.31.3
- **PR #1465**: Bump the github-actions group across 1 directory with 2 updates
- **PR #1466**: Bump toml from 1.1.2+spec-1.1.0 to 1.1.8+spec-1.1.0 in the cargo-minor-patch group across 1 directory
- **PR #1467**: Bump bunsen from 0.36.1 to 0.37.0
- **PR #1468**: Bump jsonschema from 0.58.3 to 0.60.1
- **PR #1463**: refactor(tests): dedupe Gmsh MSH writers onto support/msh.rs
- **PR #1461**: refactor(tests): share Lcg/spmv/norm helpers via tests/common
- **PR #1460**: refactor: consolidate remaining CSC mat-vec helpers into sparse_ops
- **PR #1458**: fix: restore relative-pivot singularity guard in diffopt solve_small
- **PR #1455**: refactor: consolidate dense solvers onto faer partial_piv_lu
- **PR #1454**: refactor: share spmv_transpose in sparse_ops and drop duplicate spmv_vec
- **PR #1453**: refactor: share slice norm2 and complex spmv via sparse_ops
- **Issue #1462** (closed): Dedupe test Gmsh MSH writers onto tests/support/msh.rs
- **Issue #1457** (closed): Dedupe copy-pasted Lcg/spmv/norm test helpers into tests/common
- **Issue #1459** (closed): Consolidate nine remaining CSC spmv/matvec helpers in geode-core into sparse_ops
- **Issue #1456** (closed): Restore relative pivot tolerance in diffopt::solve_small / mode_character::solve_small after faer LU migration
- **Issue #1451** (closed): Consolidate 9 hand-rolled dense Gaussian-elimination solvers in geode-core onto faer partial_piv_lu
- **Issue #1448** (closed): Consolidate duplicated sparse spmv helpers left in eigen/projection.rs and eigen/ams.rs into sparse_ops
- **Issue #1450** (closed): Consolidate six duplicated slice norm2/vec_norm helpers and complex spmv in geode-core

### 2026-11-08

- **PR #1446**: refactor: replace duplicated norm/scale/add helpers with vec3 imports
- **PR #1449**: refactor: consolidate tet signed-volume helpers into vec3
- **PR #1447**: refactor: consolidate LU solve helpers into sparse_ops::lu_solve
- **PR #1380**: test(cli): gmsh-gated periodic mesh-to-check test, live-verified Translate convention
- **PR #1407**: refactor: share gpu_driven_scaling summarize helpers in one module
- **PR #1399**: refactor(geode-app): remove no-op -v/-q verbosity plumbing
- **PR #1442**: refactor: consolidate duplicated sparse spmv/quad_form helpers into sparse_ops
- **PR #1440**: refactor: remove unreachable coordinate nested-dissection ordering
- **PR #1437**: refactor: remove nine unreferenced pub accessors from geode-core
- **Issue #1445** (closed): Finish vec3 consolidation: remove 8 local norm/scale/add copies in geode-core
- **Issue #1443** (closed): Consolidate five duplicated tet signed-volume helpers into vec3.rs
- **Issue #1444** (closed): Consolidate six duplicated LU solve helpers (solve_with_lu / lu_solve / lu_solve_c) into sparse_ops
- **Issue #1315** (closed): Live-verify geode mesh periodic layouts against real gmsh (Epic #837 4a follow-up)
- **Issue #1405** (closed): Dedupe copy-pasted parsing helpers across gpu_driven_scaling summarize scripts
- **Issue #1357** (closed): Remove or finish no-op -v/-q verbosity plumbing in geode-app
- **Issue #647** (closed): Epic: freeform curved-metal conformal-radiator inverse-design paper — high-DOF shape adjoint + FDTD-density head-to-head
- **Issue #1441** (closed): Consolidate duplicated sparse spmv/quad_form helpers in geode-core into one module
- **Issue #1439** (closed): Remove unreachable coordinate-nested-dissection ordering path and *_with_coords Lanczos methods
- **Issue #1438** (closed): Remove nine unreferenced pub methods from geode-core
- **Issue #1436** (closed): Remove 9 more unreferenced pub accessors in geode-core (~45 LOC)

### 2026-11-07

- **PR #1205**: fix(geode-core): resolve clippy warnings in transmon_multiparam examples (#1102)
- **PR #1435**: refactor: remove seven unreferenced pub functions from geode-core
- **PR #1433**: refactor: share transmon scaled_mesh/git_head helpers in one module
- **PR #1406**: refactor(assembly): remove unused p2_interior_km_complex
- **PR #1403**: refactor(validation): share BackendTolerances across julia/onnx/tfjava harnesses
- **PR #1400**: docs(zip-compat): note retained-but-unrun fuzz, benches, .amazonq and .typos.toml
- **PR #1402**: refactor(geode-cli): remove test-only Zarr reader and dead_code allow
- **PR #1431**: refactor(geode-core): split analytic/waveguide.rs into section submodules
- **PR #1404**: refactor: remove unused smallest_eigenvalues_with_gradient and Scaled::bounds_to_design
- **PR #1430**: feat: add Meep harness v3 outline loader with verifier and mutation tests
- **PR #1386**: refactor(geode-util): remove unwired FixtureFormat::Hdf5 stub
- **PR #1426**: Bump Repo Skills 0.21.5 -> 0.25.10
- **PR #1425**: feat: report same-mesh p=1 vs p=2 S-matrix difference for order-2 driven
- **PR #1423**: feat: report same-mesh p=1 vs p=2 frequency difference for order-2 eigen
- **PR #1424**: refactor: split sensitivity, ports and capacitance out of problem/mod.rs
- **PR #1422**: test: Golden 2 gate with a driven pocket decoupled from the measured S21
- **Issue #1102** (closed): clippy --examples fails under stable 1.99 in transmon_multiparam_{diffopt,optimize}
- **Issue #1434** (closed): Remove 8 unreferenced pub fns in geode-core (~100 LOC dead code)
- **Issue #1432** (closed): Dedupe copy-pasted scaled_mesh/git_head helpers across geode-core transmon examples and tests
- **Issue #1351** (closed): Remove p2_interior_km_complex: dead after order-generic refactor
- **Issue #1401** (closed): Consolidate identical BackendTolerances copied across julia/onnx/tfjava sphere-PEC harnesses
- **Issue #1347** (closed): Remove unexercised fuzz corpus and bench/IDE files from zip-compat test support
- **Issue #1361** (closed): Remove test-only Zarr reader from geode-cli dataset.rs (~85 LOC, hides behind allow(dead_code))
- **Issue #1429** (closed): Split waveguide.rs production code into submodules (8.6k lines after test move)
- **Issue #1394** (closed): Remove unused p2_interior_km_complex and two other caller-less pub fns (~85 LOC)
- **Issue #1427** (closed): Meep vs GEODE on the optimized E4 geometry: harness v3 polygon-outline loader, R2.5/R5 (split from #1088)
- **Issue #1378** (closed): Remove unwired FixtureFormat::Hdf5 stub from fixture loader
- **Issue #1088** (closed): [conformal v5] E5: Matched-geometry Meep S11 vs resolution and same-host cost to accuracy
- **Issue #1265** (closed): [#836 P5a] CLI element_order: 1|2 — report, geode check p=2 DOF/memory estimates, optional p=1-vs-p=2 order estimate
- **Issue #1398** (closed): Split problem.rs: 6.7k-line geode-cli file into problem/ submodules
- **Issue #547** (closed): Epic: tensor-native matrix-free solver — wall-clock-competitive at ≥1M DOF, then GPU-accelerate (the honest scale story)
- **Issue #1413** (closed): [#1266 Golden 2] Better-posed local-error case: goal marking beats energy marking on DOFs for |ΔS|

### 2026-11-06

- **PR #1421**: docs: label port model on remaining geode-vs-Palace tables
- **PR #1419**: refactor: move problem.rs inline tests into problem/tests.rs
- **PR #1418**: docs: add p=2 box-cavity cookbook pair with measured equal-accuracy table
- **PR #1415**: test(dwr): record narrow-slit pocket Golden 2 measurement (negative result)
- **PR #1417**: docs: document S_Γ default and A2 opt-in junction port models
- **PR #1384**: fix(cli): cell-scaled k0 shift in the periodic eigen starter spec
- **PR #1389**: refactor: consolidate duplicated vec3 and sorted3 helpers into crate::vec3
- **PR #1388**: test: dedupe geode-core test helpers into tests/common
- **PR #1353**: test(cli): cover order-2 will-not-fit verdict, library parity and rejection messages
- **PR #1370**: feat: add public 2-D electrostatic path on TriMesh
- **PR #1397**: refactor(geode-core): remove deprecated pec_interior_edge_mask and PHYS_VACUUM_BUFFER
- **PR #1385**: refactor(validation): share Burn Mie and cube-cavity pipelines via tests/common
- **PR #1414**: feat(adapt): eigenfrequency DWR with p=2 enrichment (Golden 3)
- **PR #1367**: docs: state geode-core SemVer intent and git-tag consumption
- **PR #1371**: feat(cli): differentiate dispersion-model parameters in N-port sensitivities
- **PR #1409**: refactor: share one tet_centroid helper instead of nine local copies
- **PR #1412**: build: untrack vendored crate Cargo.lock files
- **PR #1374**: feat: add ElectrostaticFactor to reuse the sparse LU across electrostatic solves
- **PR #1375**: feat: goal-oriented Dörfler marking in the adapt loop
- **PR #1373**: fix(cli): hex-format sha2 0.11 digests and add missing conductivity_s_m
- **PR #1336**: fix(benches): use std::hint::black_box instead of deprecated criterion::black_box
- **PR #1073**: Update Rust crate bunsen to 0.36.0
- **Issue #1420** (closed): Add per-row port-model label to remaining geode-vs-Palace tables (follow-up to #1046)
- **Issue #1046** (closed): Transmon port P4: eigen default switch and driven-port consistency (operator decision)
- **Issue #1381** (closed): geode mesh --analysis eigen emits a default eigen shift that does not converge for micron-scale periodic cells
- **Issue #1379** (closed): Consolidate ~44 duplicated vec3 sub/dot/cross + 8 sorted3 helpers in geode-core (shared copy already exists)
- **Issue #1359** (closed): Consolidate copy-pasted geode-core test helpers (plane_faces, edge_tables, pec_mask_for_planes, repo_root)
- **Issue #1362** (closed): electrostatics: public 2-D (TriMesh) Poisson solve with charge density and field recovery
- **Issue #1350** (closed): Remove deprecated pec_interior_edge_mask and PHYS_VACUUM_BUFFER (no in-tree callers)
- **Issue #1360** (closed): Dedup byte-identical Burn Mie/cube-cavity pipelines in geode-validation reference tests
- **Issue #1390** (closed): Move test-only dataset reader out of geode binary; drop blanket allow(dead_code)
- **Issue #1366** (closed): geode-core: state API stability for assembly::electrostatic and publish or document how to depend on it
- **Issue #1408** (closed): Deduplicate tet-centroid helper: 9 local copies vs existing tet_centroids
- **Issue #1410** (closed): Untrack vendor/*/Cargo.lock: cargo rewrites them and the dirty tree froze w2's primary checkout
- **Issue #1364** (closed): electrostatics: reuse the LU factorization across solves with changing rho or voltages
- **Issue #1377** (closed): geode-cli does not compile on main: MaterialSpec.conductivity_s_m missing in mesh_cmd, sha2 LowerHex in adaptive/dataset
- **Issue #1369** (closed): geode-cli does not compile on main: sha2 0.11 LowerHex and missing MaterialSpec.conductivity_s_m
- **Issue #1396** (closed): Consolidate duplicated BackendTolerances in geode-validation reference tests
- **Issue #1333** (closed): Replace deprecated criterion::black_box with std::hint::black_box in benches
- **Issue #1346** (closed): main does not compile: SParam::Wall not handled in floquet_sensitivity.rs match
- **Issue #1383** (closed): Remove test-only Zarr reader from geode-cli dataset.rs and drop blanket allow(dead_code)

### 2026-11-05

- **PR #1358**: feat(adapt): dwr-p2 for wave-port S-parameters and Golden 1 height-step effectivity
- **PR #1356**: feat: differentiate DiagTensor volumes (ε_xx/yy/zz columns, shape gradient)
- **PR #1316**: feat(cli): periodic boundaries and Bloch band solves in the spec, check and mesh
- **PR #1314**: feat: add opt-in --dataset gradient-labelled sample export to driven and eigen
- **PR #1319**: feat: volumetric conductivity in the spec with a sigma adjoint gradient
- **PR #1338**: chore: drop unused thiserror dependency from geode-validation
- **PR #1281**: ci: move aarch64-apple-darwin release leg to macos-15
- **PR #1345**: ci: reclaim runner disk before zip-compat native archive tests
- **PR #1074**: Update Rust crate criterion to 0.8
- **PR #1117**: build(deps): bump toml from 0.8.23 to 1.1.2+spec-1.1.0
- **PR #1118**: build(deps): bump sha2 from 0.10.9 to 0.11.1
- **PR #1273**: build(deps): bump the github-actions group across 1 directory with 9 updates
- **PR #1354**: refactor: move waveguide inline tests into waveguide/tests.rs
- **PR #1348**: feat: add geode mesh --stackup to fill layout gaps from an XML stackup
- **PR #1355**: feat: lumped-port S_ij goal and GoalFunctional bridge to the adjoint gradient
- **PR #1337**: feat: expose Leontovich wall parameters as CLI sensitivity kinds
- **PR #1301**: feat: complex-direction material parameters (tan δ, dispersive ∂ε) for S sensitivities
- **PR #1339**: refactor: share Burn sphere-PEC pipeline across reference harnesses
- **PR #1334**: feat(adapt): add dwr-p2 weights from a p=2 enrichment of the dual
- **PR #1335**: feat(eigen): unconjugated Hellmann–Feynman ∂f/∂θ and ∂Q/∂θ for lossy/open pencils
- **PR #1342**: feat: adapt_driven accepts a lumped port on an interior sheet
- **PR #1307**: fix(mshio): reject unsupported binary data sizes in header instead of panicking
- **PR #1309**: feat(cli): add element_order spec field with order-2 eigen/driven paths and check estimates
- **PR #1302**: feat: add XML stackup reader and KLayout GDS-to-layout script
- **PR #1332**: test: cover adjoint refinement fallback to fresh LU
- **PR #1293**: feat: add Floquet-port S adjoint for shape and material gradients
- **PR #1331**: feat(sensitivity): differentiate shape columns that move impedance walls
- **PR #1321**: feat: far-field realized-gain objective and shape gradient on the conformal radiator
- **PR #1318**: feat(mesh): polygon holes and 45-degree edges in layout geometry
- **PR #1322**: ci: raise geode-cli job timeout to 60 min
- **PR #1324**: docs: mark unreproduced 3e-8 m stator-assignment misfit as uncited
- **PR #1283**: docs: drop closed #593/#659 from ROADMAP, point to #1231
- **PR #1306**: feat: lossy-eigen ∂λ/∂ε and ∂k₀/∂ε via bilinear left eigenvector
- **PR #1305**: feat: order-generic p=2 material and shape sensitivities with ports, walls and UPML
- **PR #1325**: build(deps): bump the cargo-minor-patch group across 1 directory with 2 updates
- **PR #1297**: feat(cli): adaptive_mesh spec block with per-iteration report, check bound and MSH 4.1 writer
- **PR #1116**: build(deps): bump criterion from 0.5.1 to 0.8.2
- **PR #1300**: docs: triage inherited mshio TODOs and record reproducers
- **PR #1292**: perf: store harmonic reduced constraints as sparse columns
- **PR #1310**: feat: report propagating Floquet diffraction orders as channels
- **Issue #1260** (closed): [#841 P2b] Conductor and wall physics gradients — σ_wall, roughness, Huray, London; moving impedance walls; lossy-eigen f and Q
- **Issue #1263** (closed): [#837 P4a] CLI — periodic BCs and Bloch eigen in the spec, geode check pairing report, geode mesh --periodic
- **Issue #1257** (closed): [#1253 P1] Dataset exporter + schema — mesh, field, observables, gradients, provenance
- **Issue #1254** (closed): [#1251 P1a] Volumetric conductivity in the spec + ∂/∂σ adjoint (delivers #841 P2a σ item)
- **Issue #1327** (closed): Remove unused thiserror dependency from geode-validation
- **Issue #1272** (closed): CI: move macos-14 runner pins to macos-15 before the 2026-11-02 image retirement
- **Issue #1344** (closed): CI: zip-compat native-archive-tests intermittently fails (zip-tests exit 101)
- **Issue #1343** (closed): Split waveguide.rs: move 5.6k-line inline test module out of a 14k-line file
- **Issue #1330** (closed): Expose impedance-wall parameter columns (SDesign::wall_params) in the CLI sensitivity spec and report
- **Issue #1317** (closed): Dedupe Burn pipeline scaffolding across sphere_pec reference harnesses
- **Issue #1329** (closed): [#841 P2b-3] Lossy/open eigen ∂f/∂θ and ∂Q/∂θ: complex-symmetric Hellmann–Feynman
- **Issue #1298** (closed): mshio: unsupported binary data size hits unimplemented! panic
- **Issue #1352** (closed): main fails to compile: non-exhaustive SParam::Wall match in floquet_sensitivity.rs
- **Issue #1326** (closed): test: cover non-convergence fallback of refined shape adjoint (refine_transpose)
- **Issue #1256** (closed): [#1252 P1] Floquet-port S adjoint — shape + material gradients via the −k_t transpose solve
- **Issue #1328** (closed): [#841 P2b-2] Moving impedance walls: surface Dual twin of S_Γ (microstrip width as Leontovich wall)
- **Issue #652** (closed): [#647] Phase 5 (deferred v1.5) — radiation pattern & gain objective via near-to-far-field adjoint
- **Issue #1287** (closed): layout v2: polygons with holes and 45° edges (non-rectangle GroupPart + geo.rs extrusion)
- **Issue #1320** (closed): CI: cargo test -p geode-cli (linux) hit 45m timeout (cancelled, no failing test)
- **Issue #1323** (closed): parametric_cp: add log for 3.0e-8 m alternative stator-assignment misfit
- **Issue #1276** (closed): docs(roadmap): #593/#659 are already closed; fix stale 'close once published' wording
- **Issue #1277** (closed): [#836 P4b] Lossy-eigen ∂ω/∂ε at p=2 (split from #1264)
- **Issue #1264** (closed): [#836 P4] Sensitivities retained at p=2 on the order-generic operator — material + shape, lumped ports, Leontovich, UPML, lossy eigen
- **Issue #1241** (closed): Triage inherited mshio fork maintenance TODOs after parser remediation
- **Issue #1262** (closed): [#837 P3b] Floquet ports — higher orders and diffraction, evanescent termination, Wood-anomaly handling

### 2026-11-04

- **PR #1308**: fix(mshio): return Err on overflowing MSH header data size instead of panicking
- **PR #1312**: feat: claw-split coupling capacitance C_g, claw-gap theta_C, and a two-target (E_C, beta) design run
- **PR #1304**: perf: precondition the shape adjoint with the forward's last LU
- **PR #1291**: feat(adapt): add DWR core with labelled dwr-product fallback
- **PR #1286**: docs(papers): v2 text revisions of the antenna, motor and transmon papers
- **PR #1294**: feat: add impedance-wall parameter columns to S-matrix sensitivity
- **PR #1279**: fix: require start_preflight binding in FD-ablation loader and test duplicate trace refusal
- **PR #1289**: feat(mesh): per-layer conductivity_s_m routes layout conductors to Leontovich walls
- **PR #1295**: test: cover AMG-PCG true-residual gate and add r.z breakdown check
- **PR #1290**: docs: fix motor benchmark README nits and commit missing logs
- **PR #1285**: bench(meep): index Meep S11 curves, add cost-to-accuracy table and forward-solve verdict
- **PR #1284**: feat(patch-conformal): add dense radiation list and tan-delta override
- **PR #1282**: fix(docker): include vendor/ in geode-cli container build context
- **PR #1280**: docs(bench): add 3-way head-to-head table built from committed artifacts
- **PR #1069**: Update rust minor/patch dependencies
- **PR #1269**: Papers: backfill the published Zenodo DOIs
- **PR #1213**: chore(deps): bump com.fasterxml.jackson.core:jackson-databind from 2.17.2 to 2.18.11 in /reference/tf_java/sphere_mie
- **PR #1275**: build(deps): bump onnx from 1.21.0 to 1.22.0 in /reference/onnx
- **PR #1110**: Bump com.fasterxml.jackson.core:jackson-databind from 2.17.2 to 2.18.11 in /reference/tf_java/sphere_pml
- **PR #1258**: docs(roadmap): design programs (#1251-#1253), status refresh, v0.10 renumber
- **PR #1214**: chore(deps): bump com.fasterxml.jackson.core:jackson-databind from 2.17.2 to 2.18.11 in /reference/tf_java/cube_cavity
- **PR #1070**: Update github-actions
- **PR #1115**: Bump bunsen from 0.28.0 to 0.32.0
- **Issue #1034** (closed): [#569] Multi-parameter gradient optimization of the real transmon island to the 89.9 fF anchor (beyond #594's 3.5× shortfall)
- **Issue #1296** (closed): mshio: header data size overflow panics instead of returning an error
- **Issue #1037** (closed): [#1034] Phase C: second design target, island-claw coupling C_g via a region split of the ground conductor (needs operator decision)
- **Issue #1186** (closed): Motor adjoint: reuse the forward tangent factorization instead of refactoring
- **Issue #1221** (closed): Ablation selection adapter: three non-blocking hardening follow-ups from #1220 review
- **Issue #1065** (closed): AMG-PCG: test the true-residual gate and add an r·z preconditioner-breakdown check (PR #1064 follow-ups)
- **Issue #1188** (closed): Motor benchmark READMEs: precision/wording nits and uncommitted logs
- **Issue #1278** (closed): ci: 'build container (no push)' fails — Dockerfile does not COPY vendor/
- **Issue #651** (closed): [#647] Phase 4 — 3-way head-to-head: GEODE freeform vs FDTD-density vs low-DOF parametric
- **Issue #900** (closed): CI: post-merge live check of main-batch.yml dispatch and skip paths (follow-up to #899)
- **Issue #1224** (closed): Motor exp 2c: warm-started finite-difference SLSQP arm (method F_w) — strongest FD baseline
- **Issue #1218** (closed): Motor exp 2b: finite-difference SLSQP ablation (method F) to isolate the adjoint-gradient benefit

### 2026-10-25

- **PR #1268**: Papers: corresponding author (antenna v13, transmon v24, motor v23)
- **Issue #659** (closed): ops(paper): publish conformal-antenna-diffopt on Zenodo — v5 revision, human read-through, then Publish (draft deposit 21480892)
- **Issue #593** (closed): ops(paper): publish transmon-benchmark on Zenodo — fix #763 claim, resolve TODO(operator) markers, coauthor sign-off, then Publish

### 2026-10-24

- **PR #1250**: fix(security): replace legacy ZIP backends and update Tokio
- **Issue #1249** (closed): fix(security): integrate ZIP compatibility fixes and Tokio update

### 2026-10-23

- **PR #1248**: fix: update Ratatui core to remove vulnerable LRU
- **PR #1246**: papers/README: rows for the flow-revised versions
- **PR #1244**: Antenna paper v12: reorder and gloss for reading flow
- **PR #1245**: Transmon paper v23: reading-flow revision
- **PR #1243**: Motor paper v22: reading-flow revision of v21
- **PR #1242**: Motor paper v21: coarse/validation simulation terminology and future work
- **PR #1240**: fix(security): remove lexical backend from Gmsh parser
- **PR #1238**: fix(security): patch compatible Rust dependencies
- **PR #1235**: Papers: short titles (antenna v11, transmon v22, motor v20)
- **Issue #1247** (closed): fix(security): update Ratatui core to remove vulnerable LRU
- **Issue #1239** (closed): Replace the Gmsh lexical backend while preserving numeric input compatibility
- **Issue #1236** (closed): Patch current TLS and RustSec lockfile findings

### 2026-10-22

- **PR #1234**: Papers: v1 Zenodo release versions (antenna v10, transmon v21, motor v19)
- **PR #1233**: Transmon paper v20: coauthor clarity revision
- **PR #1232**: Cargo.lock: bump rustls-webpki and webpki-roots (patch)
- **PR #1230**: Transmon paper v19: reader-clarity revision and coauthor
- **PR #1229**: Motor paper v18: reader-clarity revision and coauthor
- **PR #1228**: Antenna paper v9: address reader feedback (terminology, geometry diagram, ISM claim)
- **PR #1227**: Motor paper v17: v16 audit fixes, complete raw-trace archive (audit 0 critical)
- **PR #1226**: Motor paper v11–v16: FD-SLSQP ablations (cold + warm), reviews 42/40

### 2026-10-21

- **PR #1225**: Motor exp 2c: method F_w (slsqp_fd_warm), base-started FD stencils + L1 accuracy gate (#1224)
- **PR #1223**: Motor exp 2b: FD step sweep diagnostic, the L1 SW3 miss is truncation (#1218)
- **PR #1222**: Motor exp 2b: fd_gradient_check --level 1 and the L1 F-gradient record (#1218)
- **PR #1220**: Motor exp 2b: FD-ablation selection adapter over the verified copy (#1218)
- **PR #1219**: Motor exp 2b: method F slsqp_fd and the {A', F} ablation session (#1218, code)

### 2026-10-20

- **PR #1216**: Motor paper v10: figure-vision and citation-source fixes
- **PR #1171**: Motor paper: interface truncation, candidate validation and verified gradients (v6)
- **PR #1215**: Motor paper v7–v9: matched optimization study and final refinement
- **PR #1211**: Four-boot closed capture after the refused B2 (systemd 255 journalctl boot ID fix)
- **Issue #1185** (closed): Motor exp 2: matched optimizer comparison (scalar vs scalar+groups) with frozen validation margins

### 2026-10-19

- **PR #1209**: Capture: attest a closed motor study across documented reboots
- **PR #1207**: Motor optimization: execute and audit frozen final refinements
- **Issue #1203** (closed): Motor optimization: select and refine final representatives and target crossings

### 2026-10-18

- **PR #1206**: Add L0 motor baselines and durable main-study execution
- **PR #1204**: Motor exp 2 amendment 7: geometry domain, method safeguards, mesh repair, 120-FE budget (#1201, #1197)
- **PR #1200**: Motor exp 2 pilot: 36 runs, feasibility/budget only (#1198)
- **PR #1196**: Motor exp 2: optimizer harness (SLSQP / COBYQA / calibrated RBF surrogate) with charged validation (#1195)
- **PR #1194**: Motor exp 2 prerequisites: h93 validation mode, frozen spaces, acceptance evaluator, cost ledger, θ0 anchor (#1193)
- **Issue #1202** (closed): Exp 2 amendment 8: paired L0 physical comparators and common-validation diagnostics
- **Issue #1197** (closed): Exp 2 harness: bind the initial-set gate edge and aborted_request recording in tests (K2/K4 from #1196 review)
- **Issue #1201** (closed): Exp 2 amendment 7: geometry-domain constraints, method safeguards, mesh repair, 120-FE budget (motor paper)
- **Issue #1198** (closed): Experiment 2 pilot: 36 runs, feasibility/budget only (motor paper, #1185)
- **Issue #1195** (closed): Experiment 2 optimizer harness: SLSQP / COBYQA / calibrated surrogate with charged validation (motor paper)
- **Issue #1193** (closed): Motor exp 2 prerequisites: h93 validation mode, frozen 8 / 8+10 design spaces, acceptance evaluator and cost ledger

### 2026-10-17

- **PR #1190**: Shape adjoint: guard the Arkkio torque against bands that left their annulus (#1183)
- **PR #1192**: Motor design loop: quality-triggered remesh-and-transfer (#1184)
- **PR #1191**: Motor exp 3: ICEM reference IGA refinement under Octave resolves the cogging-amplitude gap
- **PR #1182**: Motor exp 1: validate the optimization target (refinement, coupling, angle grid, withheld currents) at θ0 and Opt120
- **PR #1180**: M4 prep: ICEM control-point design space (18 rotor + 37 stator) with gradients; Opt120 on a moved mesh
- **PR #1178**: ICEM design gradients dJ/dθ for all 12 parameters: FD/Taylor validation at both designs (#1177)
- **PR #1176**: ICEM design parametrization as fixed-topology node motion + optimized-design forward check (#1173)
- **PR #1175**: M3b: discrete shape adjoint of nonlinear sector torque (mean and ripple) w.r.t. node coordinates
- **PR #1172**: M2: harmonic-truncated rotor-stator coupling explains the ICEM load-ripple gap (#1170)
- **PR #1169**: ICEM initial design forward torque reproduction: L0-L3 vs saved reference (#1168)
- **PR #1167**: Orientation-independent P1 magnetization load; accept clockwise triangles (#1166)
- **PR #1164**: [motor paper M2] ICEM initial geometry: NURBS export to tagged gmsh 60-degree sector mesh family
- **Issue #1183** (closed): Shape adjoint: guard both-band Arkkio torque when rotor offsets change the inner gap band
- **Issue #1184** (closed): Motor: quality-triggered remesh-and-transfer when moved meshes degrade (M4/M5 prerequisite)
- **Issue #1189** (closed): Motor exp 3: reference IGA refinement to resolve the 12.9% cogging-amplitude gap (Octave + GeoPDEs)
- **Issue #1181** (closed): Motor exp 1: validate the optimization target — angular/spatial/harmonic refinement at θ0 and Opt120, withheld currents
- **Issue #1179** (closed): M4 prep: ICEM free-form control-point design space (18 rotor + 37 stator) with gradients; reproduce Opt120 on a moved mesh
- **Issue #1177** (closed): M3c: ICEM design gradients dJ/dθ for all parameters — FD/Taylor validation at initial and perturbed designs
- **Issue #1173** (closed): M3a: ICEM design parametrization as fixed-topology node motion + optimized-design forward check
- **Issue #1174** (closed): M3b: discrete shape adjoint of nonlinear sector torque (mean and ripple) w.r.t. node coordinates
- **Issue #1170** (closed): M2: harmonic-truncated rotor-stator coupling to explain the ICEM ripple gap
- **Issue #1168** (closed): [motor paper M2] ICEM initial design forward reproduction: torque waveforms at 0/0.5/1 current vs saved reference, L0-L3 convergence
- **Issue #1166** (closed): P1 assemble_magnetization_rhs uses signed area: clockwise triangles silently flip the PM load in the nonlinear path
- **Issue #1162** (closed): [motor paper M2] ICEM reference motor initial geometry: NURBS export → tagged gmsh 60° sector mesh family

### 2026-10-16

- **PR #1165**: [motor paper M2] Antiperiodic sector constraints + conforming sliding band (#1161)
- **PR #1163**: M27 table in TabulatedBhLaw: integration test + measured diff vs MATLAB pchip (#1160)
- **PR #1159**: [motor paper M1] Nonlinear B-H scalar magnetostatics: consistent-tangent Newton, Brauer + monotone-table laws
- **PR #1158**: Establish third-paper motor study and reproducible ICEM reference data
- **PR #1156**: Paper: conformal-antenna v8, text revision against the v7 reviews
- **PR #1155**: Draft transmon paper v18 against the v17 Opus and Fable reviews
- **PR #1154**: Paper: conformal-antenna v7, tuned baselines, scaling and the design-C checks
- **PR #1153**: Meep harness v2 and the corrected R2.5/R5 matched ladder (#1146)
- **PR #1152**: Draft transmon paper v17 against the v16 Opus, Fable and Codex reviews
- **PR #1150**: Antenna: tuned gradient-free baselines and solve-count scaling vs parameter count (#1147)
- **PR #1151**: Transmon: grounded-box C_Σ bound (T6) and AMR on the final design (T7)
- **PR #1149**: Antenna design C: port-free eigenmode, efficiency, p = 2 rim refinement and an FD-gradient control (#1148)
- **Issue #1161** (closed): [motor paper M2] Antiperiodic sector constraints + conforming sliding-band rotation for 2-D nonlinear magnetostatics
- **Issue #1160** (closed): [motor paper] M27 B-H table in TabulatedBhLaw: integration test + measured difference vs reference MATLAB pchip
- **Issue #1157** (closed): [motor paper M1] Nonlinear B-H scalar magnetostatics: consistent-tangent Newton, Brauer + monotone-table laws, Ampère saturated-iron oracle
- **Issue #1146** (closed): Meep: fix the harness metal/gap/port discretization and run a corrected R2.5/R5 matched ladder
- **Issue #1147** (closed): Antenna: tuned gradient-free baselines (NEWUOA npt, CMA-ES sigma/full) and solve-count scaling vs parameter count
- **Issue #1145** (closed): Transmon: grounded-box upper bound (T6) and adaptive refinement on the final geometry (T7)
- **Issue #1148** (closed): Antenna: design C eigenmode/efficiency/p=2 refinement and a finite-difference-gradient control

### 2026-10-15

- **PR #1144**: Diagnose the Meep/GEODE bent-patch disagreement (#1088)
- **PR #1143**: Paper: conformal-antenna v6, the gradient advantage and the re-centred design
- **PR #1142**: Draft transmon paper v16 against the v15 Opus and Fable reviews
- **PR #1141**: Antenna: re-centre the 1.6 mm-probe copper design on the refined model; Palace p = 3 verified (#1134)
- **PR #1140**: Transmon paper v15: C_isl/C_Σ convention, h/2 mechanism, #1135 results
- **PR #1139**: Transmon: second refinement family on the final design; PRIMA trust-radius sensitivity (#1135)
- **PR #1138**: Palace-in-the-loop gradient-free baseline for the conformal antenna (#1130)
- **PR #1137**: Record matched-geometry Meep R4 run and R8 requirement (#1088)
- **PR #1136**: Draft transmon-benchmark v14: one run-count rule, h/2-only same-problem claim, remesh reconciliation
- **PR #1133**: Antenna v6 experiments: forward-model accuracy, curvature, efficiency, baselines, realizable feed (#1126)
- **PR #1132**: Draft transmon-benchmark v13: cross-check framing, controls in one environment, 17 pages
- **PR #1131**: Transmon v13 experiments: PRIMA on our Palace build, Palace-only Newton, second family on cv2 (#1129)
- **Issue #1134** (closed): Antenna: re-centre the copper design on the refined forward model (Palace p=3 verified)
- **Issue #1135** (closed): Transmon: second refinement family on the final geometry; PRIMA trust-radius sensitivity
- **Issue #1130** (closed): Antenna: Palace-in-the-loop gradient-free baseline (CMA-ES, BOBYQA) vs geode adjoint at matched budget
- **Issue #1126** (closed): Antenna v6 experiments: forward-model accuracy, curvature, efficiency, baselines, realizable feed
- **Issue #1129** (closed): Transmon v13 experiments: PRIMA on our Palace build, Palace-only Newton, second convergence check on final design

### 2026-10-14

- **PR #1128**: Draft transmon-benchmark v12: gradient-free controls, AMR and family checks, model-dependent cap
- **PR #1127**: Transmon v12 controls: gradient-free Newton ties the geode-gradient 3 runs; Palace AMR extrapolates to 4.455 GHz below the cap; h/2 spread is refinement scheme (#1123)
- **PR #1125**: Draft conformal-antenna-diffopt v5: copper-only 2.4 GHz patch on a fixed 1.6 mm board
- **PR #1124**: [conformal v5] E4c gate G2: copper-only patch on a fixed 1.6 mm board, -10 dB across the ISM band with p = 2 on L1 (94.9 MHz)
- **PR #1122**: [conformal v5] E4b gate G2: L2 re-optimization of the nested-DOF bent patch, verified on L3 and P2/L1 (non-uniform-thickness ablation)
- **PR #1121**: Draft transmon-benchmark v11: operator rulings of 2026-10-13
- **PR #1119**: Draft transmon-benchmark v10 around the converged DeviceLayout cross-check
- **PR #1111**: DeviceLayout.jl SingleTransmon re-optimization with geode's tensor-eps shape gradient: DL's criterion in 3 Palace runs (PRIMA 9), converged 3.0/4.0 GHz design (#1092 part 2)
- **Issue #1123** (closed): [transmon v12] Gradient-free Newton control, Palace adaptive-refinement check, mesh-family spread, cap tier data
- **Issue #1108** (closed): Flaky test: empty_ports_equals_portless_matched_upml_gradient compares floats bit-exactly
- **Issue #1120** (closed): [conformal v5] E4c: Fixed 1.6 mm board — optimize only the etched copper (outline, feed inset, in-plane slots), verify on L3/P2
- **Issue #1103** (closed): [conformal v5] E4b: Re-optimize the nested-DOF design on L2, verify at L3 and P2 (gate G2)
- **Issue #1092** (closed): [transmon v10] Cross-check DeviceLayout's initial geometry (4.14 GHz) and reproduce its published two-parameter optimization (3.005/3.982 GHz)
