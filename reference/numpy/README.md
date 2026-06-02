# NumPy reference implementations

Canonical reference backend per **Epic #88**. NumPy goes first because
it has the largest training cohort, the thinnest abstraction between
math and code, and the most mature sparse eigensolvers
(`scipy.sparse.linalg.eigsh`). When backends disagree, NumPy is the
default tiebreaker.

## Status

Stub — first concrete impl lands with **#90** (NumPy P1 local
matrices) and **#92** (cube cavity end-to-end). Until then this
directory is intentionally empty.

## Planned layout

```
reference/numpy/
├── README.md                       — this file
├── pyproject.toml                  — pinned NumPy/SciPy versions (lands with #90)
├── p1_local_matrices.py            — element-local K and M for the P1 reference tet (#90)
├── cube_cavity.py                  — end-to-end cube-cavity eigenmode driver (#92)
└── _harness.py                     — fixture I/O helper shared across slices
```

## Invocation convention

Reference impls are invoked by the Rust harness as subprocesses:

```bash
python reference/numpy/<slice>.py <fixture-path> <output-path>
```

The script reads inputs from the fixture (JSON v1, see
`reference/SCHEMA.md`), produces a results file in the same schema,
and exits 0 on success / nonzero on internal error. The Rust harness
diffs the output against the fixture's golden values per the standard
`Fixture::compare_against` flow.

## Toolchain bootstrap (forward-looking, defined in #90)

Recommended pattern when the first impl lands:

```bash
cd reference/numpy
python3.12 -m venv .venv
source .venv/bin/activate
pip install -r requirements.txt   # or `pip install -e .` once pyproject.toml lands
```

The Rust harness should *not* assume a particular venv path — every
backend script's interpreter is configurable via an env var like
`GEODE_VALIDATION_NUMPY=python3` (default `python3`). See #90 for
the exact wiring.
