# Palace Electromagnetic Solver — CPU Docker build
#
# In-repo Palace oracle build recipe, kept here so the build is reproducible
# from this repository alone (issue #519). This is the CPU baseline that
# produced reference/fixtures/transmon_palace/results_p1/; the GPU variant is
# Dockerfile.cuda.
#
# Build: docker build -t palace:cpu -f Dockerfile .
# Run:   docker run --rm -v $(pwd):/work palace:cpu /work/config.json

FROM rockylinux:9 AS builder

# CPU architecture for compilation. 'native' = best perf on the build machine;
# omit -march for cross-platform portability.
ARG MARCH=native
ENV OPT_FLAGS="-O3 -ffp-contract=fast -funroll-loops"

RUN dnf install -y dnf-plugins-core && dnf config-manager --enable crb && \
    dnf install -y epel-release && \
    dnf update -y && \
    dnf groupinstall -y "Development Tools" && \
    dnf install -y \
        curl-minimal \
        gcc-gfortran \
        git \
        libunwind-devel \
        openblas-devel \
        pkg-config \
        python3 \
        wget \
        zlib \
        mpich-devel && \
    dnf clean all

ARG CMAKE_VERSION=3.31.0
RUN wget -q https://github.com/Kitware/CMake/releases/download/v${CMAKE_VERSION}/cmake-${CMAKE_VERSION}-linux-$(uname -m).sh && \
    /bin/bash cmake-${CMAKE_VERSION}-*.sh --skip-license --prefix=/usr && \
    rm -rf cmake-*

ENV LD_LIBRARY_PATH="/usr/lib64/mpich/lib"
ENV PATH="/usr/lib64/mpich/bin:${PATH}"

# PALACE_REF pins the Palace source. Default = fba6a5b, the changeset of the
# committed CPU baseline (reference/fixtures/transmon_palace/results_p1/
# palace_run_v22.log: "Git changeset ID: fba6a5b") and of Dockerfile.cuda, so a
# rebuild reproduces that solver and not whatever Palace's main is today (#927).
ARG PALACE_REF=fba6a5b9d17ef09db2907e9c4c6f92d123d131c0
WORKDIR /opt
RUN git clone https://github.com/awslabs/palace.git palace-src && \
    cd palace-src && git checkout --detach ${PALACE_REF} && git log -1 --format='%H %cI %s'

WORKDIR /opt/palace-build
RUN cmake /opt/palace-src \
    -DCMAKE_INSTALL_PREFIX=/opt/palace \
    -DCMAKE_CXX_COMPILER=g++ \
    -DCMAKE_CXX_FLAGS="${OPT_FLAGS}" \
    -DCMAKE_C_COMPILER=gcc \
    -DCMAKE_C_FLAGS="${OPT_FLAGS}" \
    -DCMAKE_Fortran_COMPILER=gfortran \
    -DCMAKE_Fortran_FLAGS="${OPT_FLAGS}" \
    -DBUILD_SHARED_LIBS=ON \
    -DPALACE_WITH_64BIT_INT=OFF \
    -DPALACE_WITH_OPENMP=ON \
    -DPALACE_WITH_CUDA=OFF \
    -DPALACE_WITH_HIP=OFF \
    -DPALACE_WITH_SUPERLU=ON \
    -DPALACE_WITH_STRUMPACK=ON \
    -DPALACE_WITH_MUMPS=ON \
    -DPALACE_WITH_SLEPC=ON \
    -DPALACE_WITH_ARPACK=ON \
    -DPALACE_WITH_LIBXSMM=OFF \
    -DPALACE_WITH_MAGMA=OFF \
    -DPALACE_WITH_GSLIB=ON && \
    make -j$(nproc) && \
    { git -C /opt/palace-src log -1 --format='palace_commit=%H %cI %s'; \
      echo "mpicxx_show=$(mpicxx -show)"; \
      cmake -LA -N /opt/palace-build \
        | grep -E '^(PALACE_|CMAKE_(C|CXX|Fortran)_(COMPILER|FLAGS)|CMAKE_BUILD_TYPE|BUILD_SHARED_LIBS)'; \
    } > /opt/palace/palace-build-info.txt

FROM rockylinux:9

# openblas-serial: the builder links libopenblas.so.0 (the serial variant),
# which on Rocky 9 is shipped by openblas-serial, not by `openblas` (#927: the
# image built without it exits 127 before main).
RUN dnf install -y epel-release && \
    dnf install -y \
        openblas \
        openblas-serial \
        mpich \
        libgomp \
        libunwind \
        time \
        which \
        zlib && \
    dnf clean all && \
    rm -rf /var/cache/dnf

COPY --from=builder /opt/palace /opt/palace

ENV LD_LIBRARY_PATH="/opt/palace/lib:/usr/lib64/mpich/lib"
ENV PATH="/opt/palace/bin:/usr/lib64/mpich/bin:${PATH}"

WORKDIR /work
ENTRYPOINT ["palace"]
