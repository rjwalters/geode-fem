---
marp: true
size: 16:9
theme: anvil-slides
---

## Methodology — eight steps from raw data to figures

We pipeline the raw traces through a standard set of stages.

- ingest raw event logs from the production fleet
- denormalize per-host and partition by service
- aggregate to 1-second bins for temporal alignment
- filter outliers using the trimmed-mean estimator
- attach call-graph context from the tracing service
- score each event with the bandwidth-pressure metric
- write Parquet partitions to the shared object store
- materialize per-dataset summary tables for the figures

_See the appendix for the bandwidth-pressure derivation._
