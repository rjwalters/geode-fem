# docker/

Container images for running GEODE-FEM itself. Each subdirectory holds one
image, built with the repository root as the build context.

| Image | Contents |
|---|---|
| [`geode-cli/`](geode-cli/README.md) | The `geode` CLI (default features: `ndarray` f64 CPU backend, no GPU, no ARPACK) on `debian:trixie-slim` with Gmsh 4.13, so `geode mesh` works out of the box (issue #709) |

```sh
docker build -f docker/geode-cli/Dockerfile \
  --build-arg GEODE_GIT_SHA="$(git rev-parse --short=12 HEAD)" -t geode-cli .
docker run --rm -v "$PWD:/work" geode-cli check spec.json
```

The release workflow (`.github/workflows/release.yml`) builds and smoke-tests
the image but does not push it to a registry. The containers for the
reference solvers are kept next to their reference code, not here:
Palace in [`reference/palace/docker/`](../reference/palace/docker/README.md)
and Meep in [`reference/meep/docker/`](../reference/meep/docker/README.md).
