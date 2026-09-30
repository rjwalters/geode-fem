# geode CLI container

`Dockerfile` builds the `geode` CLI (default features: ndarray f64 CPU
backend, no GPU, no arpack) in a `rust:trixie` stage and ships it on
`debian:trixie-slim` with **Gmsh 4.13** preinstalled, so `geode mesh` works
out of the box (issue #709, Epic #702). Debian 13 is required for the Gmsh
version: bookworm's Gmsh 4.8 is too old for `geode mesh` (needs >= 4.11).

```sh
# from the repository root
docker build -f docker/geode-cli/Dockerfile \
  --build-arg GEODE_GIT_SHA="$(git rev-parse --short=12 HEAD)" \
  -t geode-cli .

docker run --rm geode-cli --version
docker run --rm -v "$PWD:/work" geode-cli check spec.json
docker run --rm -v "$PWD:/work" geode-cli driven spec.json -o report.json
```

- The build context is the repository root; `Dockerfile.dockerignore`
  whitelists only the Cargo workspace (`Cargo.toml`, `Cargo.lock`,
  `crates/`, `examples/`) plus `LICENSE`.
- `.git` is not in the context, so pass `GEODE_GIT_SHA` or
  `geode --version` reports `unknown`.
- The image runs as the non-root user `geode` (uid 1000) in `/work`; mount
  your spec + mesh directory there. A spec's mesh `path` resolves relative
  to the spec file, so mount a directory that contains both.
- CI (`.github/workflows/release.yml`, `container` job) builds and
  smoke-tests the image on pull requests that touch this directory or the
  release workflow. It is **not pushed anywhere**: publishing to
  `ghcr.io` is deferred until there is a consumer for it (it would need a
  `packages: write` grant this repository does not otherwise use).
