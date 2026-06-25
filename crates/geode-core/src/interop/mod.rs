//! Interoperability with external solvers and reference oracles.
//!
//! This directory-backed module groups the crate's external-tool glue —
//! the configuration and result-ingestion code that lets third-party
//! solvers serve as gold-standard oracles for the geode-fem benchmarks:
//!
//! - [`palace`] — Palace oracle ingestion: sweep-point configuration and
//!   S-parameter result parsing for the driven-port benchmarks.

pub mod palace;
