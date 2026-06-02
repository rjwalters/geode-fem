# TF-Java reference implementations

L4-shaped reference backend per **Epic #88**. Adds static
typechecking + IDE tooling against an L4-shaped object graph
(no other backend exposes the graph as a first-class typed value).

## Status

Stub — concrete impls deferred to **#88 Phase D**, sequenced with
JAX (Phase C) since both go through XLA. Agreement validates the
L4 → XLA lowering; disagreement isolates DX-surface vs
compiler-semantics.

## Planned layout

Mirrors `reference/numpy/` with a Maven `pom.xml` driving
dependency resolution.
