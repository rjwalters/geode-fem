# geode CLI container: the `geode` binary + Gmsh (issue #709, Epic #702).
#
# Build from the REPOSITORY ROOT (the build context must contain the Cargo
# workspace; Dockerfile.dockerignore next to this file whitelists only what
# the build needs):
#
#   docker build -f docker/geode-cli/Dockerfile \
#     --build-arg GEODE_GIT_SHA="$(git rev-parse --short=12 HEAD)" \
#     -t geode-cli .
#
# Run (mount the directory holding your spec + mesh at /work):
#
#   docker run --rm -v "$PWD:/work" geode-cli check spec.json
#   docker run --rm -v "$PWD:/work" geode-cli mesh layout.json \
#     --mesh-out m.msh --spec-out s.json
#
# The image is built (never pushed) in CI by .github/workflows/release.yml;
# publishing to ghcr.io is deferred. See docker/geode-cli/README.md.

ARG RUST_VERSION=1
# Debian 13 (trixie): its gmsh is 4.13.x. Do NOT drop to bookworm — its
# gmsh 4.8.4 predates the `Sampling` Distance-field option `geode mesh`
# emits (geode mesh needs Gmsh >= 4.11) and fails with `gmsh_failed`.
ARG DEBIAN_RELEASE=trixie

FROM rust:${RUST_VERSION}-${DEBIAN_RELEASE} AS builder

# .git is not in the build context, so build.rs falls back to this env var
# for `geode --version` / the report `git_sha`. Pass the commit you built.
ARG GEODE_GIT_SHA=unknown
ENV GEODE_GIT_SHA=${GEODE_GIT_SHA} \
    CARGO_TERM_COLOR=always

WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY examples ./examples

# Default features: ndarray f64 CPU backend + faer-parallel (no GPU, no
# arpack), identical to the prebuilt release binaries.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --locked --release -p geode-cli \
 && install -Dm755 target/release/geode /out/geode

FROM debian:${DEBIAN_RELEASE}-slim

# Gmsh is an external runtime dependency of `geode mesh` (it is shelled out
# to, never linked); every other subcommand needs only the geode binary.
RUN apt-get update \
 && apt-get install -y --no-install-recommends gmsh ca-certificates \
 && rm -rf /var/lib/apt/lists/* \
 && gmsh --version

COPY --from=builder /out/geode /usr/local/bin/geode
COPY LICENSE /usr/share/doc/geode/LICENSE

RUN useradd --create-home --uid 1000 geode
USER geode
WORKDIR /work

ENTRYPOINT ["geode"]
CMD ["--help"]
