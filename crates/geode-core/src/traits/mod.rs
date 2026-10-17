//! Core abstractions for the geode-core finite-element pipeline.
//!
//! These thin, backend-parameterized traits ([`Mesh`], [`Element`],
//! [`Operator`]) establish the directional shape of the API. They remain
//! reachable at the crate root (`geode_core::{Mesh, Element, Operator}`)
//! and via [`crate::prelude`] as the conceptual entry point.

use burn::tensor::Tensor;
use burn::tensor::backend::Backend;

/// A geometric mesh: element connectivity and node coordinates.
///
/// This trait describes an *in-pipeline* mesh — backend-parameterized so a
/// concrete impl can hold device-side tensors. The raw CPU output of mesh
/// I/O lives in [`mesh::TetMesh`](crate::mesh::TetMesh), which is what
/// `MeshReader` returns; a future `Mesh`-implementing struct will typically
/// wrap one of those plus the Burn tensors it owns.
pub trait Mesh {
    type Backend: Backend;

    fn n_nodes(&self) -> usize;
    fn n_elements(&self) -> usize;
}

/// A finite element: local geometry, basis, and quadrature.
pub trait Element {
    type Backend: Backend;

    fn n_basis(&self) -> usize;
}

/// An abstract linear operator on a discretized field.
pub trait Operator {
    type Backend: Backend;

    fn apply(&self, input: Tensor<Self::Backend, 1>) -> Tensor<Self::Backend, 1>;
}
