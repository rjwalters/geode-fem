# tools/

Developer tooling that sits outside the Cargo workspace. At present it holds
one Python package.

| Path | Contents |
|---|---|
| [`viz/`](viz/README.md) | `geode_viz`: matplotlib plots and tearsheets drawn from the `benchmarks/*/results*.toml` files, plus VTK / ParaView (`pvbatch`) render scripts for exported `.vtu` fields |

Install and run it from the repository root (Python 3.11+):

```sh
pip install -e tools/viz
python -m geode_viz.scripts.plot_benchmark spiral_inductor --tearsheet
```

Plots are written under `artifacts/viz/<benchmark>/`, which is gitignored. See
[`viz/README.md`](viz/README.md) for the package surface and each plot family.
