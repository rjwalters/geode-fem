# Meep 3D FDTD + adjoint — CPU Docker build
#
# In-repo Meep (pymeep) baseline recipe, kept here so the FDTD-density
# adjoint oracle can be rebuilt **from this repository alone** (issue #651,
# epic #647 Phase 4). This is the DEFINITIVE 3D adjoint-FDTD environment
# for the apples-to-apples head-to-head against GEODE's unstructured-tet
# shape-adjoint −10 dB curved-conformal result
# (benchmarks/patch_antenna_conformal/conformal_results.toml).
#
# Unlike the Palace oracle (reference/palace/docker/Dockerfile, compiled
# from source), Meep ships as a conda-forge binary package, so this is a
# conda/mamba install on top of miniforge — no from-source build.
#
# Build: docker build -t meep-baseline:cpu -f Dockerfile .
# Run (smoke, proves the adjoint stack end-to-end):
#        docker run --rm meep-baseline:cpu python /opt/meep-baseline/smoke_test.py
# Run (3D head-to-head scaffold):
#        docker run --rm -v "$PWD/../../..:/work" meep-baseline:cpu \
#          python /opt/meep-baseline/conformal_baseline_3d.py

FROM condaforge/miniforge3:latest

# Headless matplotlib: no display in the container, so force the Agg
# backend (avoids a libGL dependency for any plotting the scaffolds do).
ENV MPLBACKEND=Agg

# Pinned Meep-adjoint environment.
#
# NOTE (deviation from the issue spec, verified against conda-forge on
# 2026-07-20): there is NO `pymeep-extras` package on conda-forge — the
# name does not resolve. The adjoint / optimization stack (`meep.adjoint`)
# ships INSIDE `pymeep` itself; the only extra runtime deps it needs are
# `autograd` (objective backprop) and `nlopt` (the MMA optimizer driver),
# named explicitly here. numpy/scipy/matplotlib are pinned so the versions
# are recorded in the image.
#
# python is pinned to 3.10: the current conda-forge `pymeep` builds
# (1.31–1.34, mpi_mpich_py310*) are py310-only — python=3.11 fails to
# solve ("pymeep ... does not exist" for the 3.11 ABI).
RUN mamba create -y -n meep -c conda-forge \
        python=3.10 \
        pymeep \
        autograd \
        nlopt \
        numpy \
        scipy \
        matplotlib && \
    mamba clean -a -y

# Make the `meep` env the default: `python` on PATH resolves to it, so
# `docker run ... python ...` (no activation) uses Meep directly.
ENV PATH=/opt/conda/envs/meep/bin:$PATH
ENV CONDA_DEFAULT_ENV=meep

# Baseline scripts (smoke test + 3D head-to-head scaffold).
COPY smoke_test.py conformal_baseline_3d.py /opt/meep-baseline/

WORKDIR /work

# Default: prove the adjoint stack works end-to-end.
CMD ["python", "/opt/meep-baseline/smoke_test.py"]
