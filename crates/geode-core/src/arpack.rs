//! Optional ARPACK-backed sparse eigensolver via the ICB C wrapper.
//!
//! Gated behind the `arpack` Cargo feature. When the feature is off this
//! module is not compiled and the default sparse path remains the
//! pure-Rust [`crate::lanczos::SparseShiftInvertLanczos`].
//!
//! # Why ARPACK at all?
//!
//! The pure-Rust shift-and-invert Lanczos satisfies the issue-#13 1e-6
//! oracle bound, but ARPACK is the canonical reference. Having both
//! solvers behind a common trait gives:
//!
//! * a cross-check oracle for the in-tree Lanczos on harder pencils
//!   (Mie sphere, Silver-Müller PML) where the Lanczos convergence story
//!   might shift with discretization changes,
//! * an opt-in fast path on systems where a tuned BLAS-integrated ARPACK
//!   is already installed (HPC environments, distros).
//!
//! ARPACK stays **opt-in** by deliberate non-goal in the issue body. The
//! Lanczos remains the default sparse path.
//!
//! # Why vendored FFI declarations?
//!
//! The `arpack-ng-sys` crate's `system` feature runs pkg-config and then
//! asks bindgen to parse `#include <arpack/arpack.h>` with no extra `-I`
//! paths beyond pkg-config's `includedir`. On macOS Homebrew the
//! `arpack.pc` file sets `includedir=${prefix}/include/arpack` (one
//! level deep, since the ICB C headers all `#include` each other as
//! `arpack/foo.h`), which means clang cannot resolve
//! `<arpack/arpack.h>` from that path and `arpack-ng-sys` fails with
//! "file not found." The crate's other feature, `static`, side-steps
//! the issue by building arpack-ng from source via cmake + gfortran,
//! which is a heavier toolchain ask than we want behind a single Cargo
//! flag.
//!
//! The ARPACK ICB ABI (`dsaupd_c`, `dseupd_c`) has been stable since
//! arpack-ng 3.7 (2019). The signatures we need are short and inline
//! `extern "C"` blocks are easier to maintain than the entire bindgen +
//! clang dependency chain.
//!
//! # Algorithm
//!
//! Same shift-and-invert mode 3 as the Lanczos path:
//!
//! 1. Factor `A = K - σ M` once via faer's sparse LU.
//! 2. Hand ARPACK an `OP = A⁻¹ M` operator via reverse communication.
//! 3. Decode the Ritz values via `dseupd_c` and return the lowest
//!    `n_modes` in ascending order.
//!
//! ARPACK handles convergence, restarts, and Ritz extraction; we only
//! supply the matrix-vector apply and the linear-system solve. The
//! routine targets *symmetric* generalized eigenproblems — for our P1
//! Laplacian (`K` SPSD, `M` SPD on Dirichlet-reduced DOFs) that's the
//! right specialization.

#![allow(unsafe_code)]

use faer::sparse::linalg::solvers::Lu;
use faer::sparse::{SparseColMat, SparseColMatRef};
use faer::{Mat, MatMut};

use crate::eigen::EigenError;
use crate::lanczos::SparseEigenSolver;

// ---------------------------------------------------------------------------
// ARPACK ICB C wrapper FFI.
//
// The upstream signatures (from `arpack/arpack.h` in arpack-ng ≥ 3.7):
//
//   void dsaupd_c(a_int* ido, char const* bmat, a_int n, char const* which,
//                 a_int nev, double tol, double* resid, a_int ncv,
//                 double* v, a_int ldv, a_int* iparam, a_int* ipntr,
//                 double* workd, double* workl, a_int lworkl, a_int* info);
//
//   void dseupd_c(a_int rvec, char const* howmny, a_int const* select,
//                 double* d, double* z, a_int ldz, double sigma,
//                 char const* bmat, a_int n, char const* which,
//                 a_int nev, double tol, double* resid, a_int ncv,
//                 double* v, a_int ldv, a_int* iparam, a_int* ipntr,
//                 double* workd, double* workl, a_int lworkl, a_int* info);
//
// `a_int` is `int` (C int) on the default INTERFACE64=0 build that every
// distro ships (Homebrew, Debian, Ubuntu, Fedora). The 64-bit-index
// variant is rare in distributions; users who built ARPACK with
// `INTERFACE64=1` will need a different FFI module.
// ---------------------------------------------------------------------------

use core::ffi::{c_char, c_int};

unsafe extern "C" {
    fn dsaupd_c(
        ido: *mut c_int,
        bmat: *const c_char,
        n: c_int,
        which: *const c_char,
        nev: c_int,
        tol: f64,
        resid: *mut f64,
        ncv: c_int,
        v: *mut f64,
        ldv: c_int,
        iparam: *mut c_int,
        ipntr: *mut c_int,
        workd: *mut f64,
        workl: *mut f64,
        lworkl: c_int,
        info: *mut c_int,
    );

    fn dseupd_c(
        rvec: c_int,
        howmny: *const c_char,
        select: *const c_int,
        d: *mut f64,
        z: *mut f64,
        ldz: c_int,
        sigma: f64,
        bmat: *const c_char,
        n: c_int,
        which: *const c_char,
        nev: c_int,
        tol: f64,
        resid: *mut f64,
        ncv: c_int,
        v: *mut f64,
        ldv: c_int,
        iparam: *mut c_int,
        ipntr: *mut c_int,
        workd: *mut f64,
        workl: *mut f64,
        lworkl: c_int,
        info: *mut c_int,
    );
}

// ---------------------------------------------------------------------------
// Public solver type.
// ---------------------------------------------------------------------------

/// ARPACK-driven sparse generalized-symmetric eigensolver
/// (shift-and-invert, ARPACK mode 3).
///
/// Disabled in the default build; activate with `--features arpack` plus
/// a system `libarpack` (Homebrew `arpack` on macOS, apt
/// `libarpack2-dev` on Debian/Ubuntu). See the module docs for the
/// rationale and the linker story.
///
/// Tuning knobs:
/// - `sigma`: shift; ARPACK targets the eigenvalues of the pencil
///   closest to `sigma` first. `0.0` is the natural choice for the
///   smallest-magnitude end of the spectrum (FEM ground modes).
/// - `max_iters`: cap on ARPACK Arnoldi iterations. Maps directly onto
///   `iparam(3)` (maxiter). ARPACK's internal default is 300.
/// - `tol`: relative convergence tolerance per Ritz pair. `0.0` lets
///   ARPACK pick the machine-precision default (`dlamch('E')`); set a
///   tighter value for stricter convergence.
/// - `ncv_factor`: multiplier in `ncv ≈ ncv_factor * nev`. ARPACK
///   requires `nev + 2 ≤ ncv ≤ n`; the recommended value is `2`.
#[derive(Debug, Clone, Copy)]
pub struct ArpackEigensolver {
    pub sigma: f64,
    pub max_iters: usize,
    pub tol: f64,
    pub ncv_factor: usize,
}

impl Default for ArpackEigensolver {
    fn default() -> Self {
        Self {
            sigma: 0.0,
            max_iters: 300,
            tol: 0.0, // 0 ⇒ ARPACK uses dlamch('E').
            ncv_factor: 2,
        }
    }
}

// `K - σM` as a fresh sparse matrix, used to build the LU factorization.
//
// Iterates the union of both patterns. K and M typically share sparsity
// in this crate's assembler (same P1 stencil), but we don't assume that.
fn shifted_pencil(
    k: SparseColMatRef<'_, usize, f64>,
    m: SparseColMatRef<'_, usize, f64>,
    sigma: f64,
) -> Result<SparseColMat<usize, f64>, EigenError> {
    use faer::sparse::Triplet;
    let n = k.nrows();
    assert_eq!(k.ncols(), n);
    assert_eq!(m.nrows(), n);
    assert_eq!(m.ncols(), n);

    let nnz = k.col_ptr()[n] + m.col_ptr()[n];
    let mut trips: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(nnz);

    let push = |trips: &mut Vec<Triplet<usize, usize, f64>>,
                a: SparseColMatRef<'_, usize, f64>,
                scale: f64| {
        let cp = a.col_ptr();
        let ri = a.row_idx();
        let v = a.val();
        for j in 0..a.ncols() {
            for k in cp[j]..cp[j + 1] {
                trips.push(Triplet::new(ri[k], j, scale * v[k]));
            }
        }
    };
    push(&mut trips, k, 1.0);
    if sigma != 0.0 {
        push(&mut trips, m, -sigma);
    }

    SparseColMat::<usize, f64>::try_new_from_triplets(n, n, &trips)
        .map_err(|e| EigenError::FaerGevd(format!("shifted pencil assembly: {e:?}")))
}

/// Compute `y = A · x` (overwrite) for a CSC sparse matrix.
fn spmv(a: SparseColMatRef<'_, usize, f64>, x: &[f64], y: &mut [f64]) {
    y.iter_mut().for_each(|v| *v = 0.0);
    let col_ptr = a.col_ptr();
    let row_idx = a.row_idx();
    let val = a.val();
    for j in 0..a.ncols() {
        let xj = x[j];
        if xj == 0.0 {
            continue;
        }
        for k in col_ptr[j]..col_ptr[j + 1] {
            y[row_idx[k]] += val[k] * xj;
        }
    }
}

/// Solve `A y = b` via a precomputed sparse LU factorization.
fn solve_with_lu(lu: &Lu<usize, f64>, rhs: &[f64], out: &mut [f64]) {
    use faer::linalg::solvers::Solve;
    let n = rhs.len();
    let mut work: Mat<f64> = Mat::from_fn(n, 1, |i, _| rhs[i]);
    let work_mut: MatMut<'_, f64> = work.as_mut();
    lu.solve_in_place(work_mut);
    for i in 0..n {
        out[i] = work[(i, 0)];
    }
}

impl SparseEigenSolver for ArpackEigensolver {
    fn smallest_eigenvalues(
        &self,
        k: SparseColMatRef<'_, usize, f64>,
        m: SparseColMatRef<'_, usize, f64>,
        n_modes: usize,
    ) -> Result<Vec<f64>, EigenError> {
        let n = k.nrows();
        assert_eq!(k.ncols(), n, "K must be square");
        assert_eq!(m.nrows(), n, "M and K must agree in size");
        assert_eq!(m.ncols(), n);
        if n_modes == 0 {
            return Ok(Vec::new());
        }
        if n_modes >= n {
            return Err(EigenError::FaerGevd(format!(
                "ARPACK requires n_modes ({n_modes}) strictly less than dimension ({n})"
            )));
        }

        // ARPACK parameters.
        //
        // - bmat   = 'G'  generalized eigenproblem K x = λ M x
        // - which  = 'LM' largest-magnitude of ν = 1/(λ-σ), i.e. λ closest
        //                 to σ; with σ=0 this is the smallest-magnitude end
        // - mode 3 (shift-and-invert, generalized)
        let nev = n_modes as c_int;
        let n_c = n as c_int;

        // ncv: number of Lanczos basis vectors. ARPACK requires
        // ncv - nev >= 2 and ncv <= n. The standard recommendation is
        // 2*nev, clamped into the valid range.
        let ncv_request = (self.ncv_factor.max(2) * n_modes).max(2 * n_modes + 1);
        let ncv = ncv_request.min(n).max(n_modes + 2);
        let ncv_c = ncv as c_int;

        let mut ido: c_int = 0;
        let bmat = b"G\0";
        let which = b"LM\0";
        let mut info: c_int = 0; // 0 = use random starting vector
        let mut iparam: [c_int; 11] = [0; 11];
        iparam[0] = 1; // ishift = 1 (exact shifts)
        iparam[2] = self.max_iters.max(1) as c_int; // maxiter
        iparam[3] = 1; // nb (block size, always 1 for IRA)
        iparam[6] = 3; // mode 3 (shift-and-invert, generalized)

        let mut ipntr: [c_int; 11] = [0; 11];
        let mut resid = vec![0.0_f64; n];
        let mut v = vec![0.0_f64; n * ncv];
        // workd is the 3*n-length reverse-communication workspace.
        let mut workd = vec![0.0_f64; 3 * n];
        // workl size: ncv*(ncv+8) for the symmetric driver.
        let lworkl_c = (ncv * (ncv + 8)) as c_int;
        let mut workl = vec![0.0_f64; ncv * (ncv + 8)];

        // Pre-factor A = K - σM once. Mode-3 reverse communication will
        // re-use the LU at every ido=-1 and ido=1 call.
        let a = shifted_pencil(k, m, self.sigma)?;
        let lu = a
            .as_ref()
            .sp_lu()
            .map_err(|e| EigenError::FaerGevd(format!("ARPACK shift-invert LU: {e:?}")))?;

        // Reverse-communication loop. Safety bound: ARPACK should
        // terminate within ~maxiter * ncv matvec calls. We cap an
        // absolute number of round-trips to avoid an infinite loop if
        // the FFI is misconfigured.
        let max_round_trips = (20 * self.max_iters * ncv).max(10_000);
        let mut trips = 0_usize;

        loop {
            // SAFETY: we pass valid `*mut`/`*const` pointers to buffers we
            // own for the duration of the call; ARPACK reads/writes within
            // the documented sizes set by the parameters above.
            unsafe {
                dsaupd_c(
                    &mut ido,
                    bmat.as_ptr() as *const c_char,
                    n_c,
                    which.as_ptr() as *const c_char,
                    nev,
                    self.tol,
                    resid.as_mut_ptr(),
                    ncv_c,
                    v.as_mut_ptr(),
                    n_c,
                    iparam.as_mut_ptr(),
                    ipntr.as_mut_ptr(),
                    workd.as_mut_ptr(),
                    workl.as_mut_ptr(),
                    lworkl_c,
                    &mut info,
                );
            }

            if info < 0 {
                return Err(EigenError::FaerGevd(format!(
                    "dsaupd_c returned info={info} (negative ⇒ usage error). \
                     See arpack-ng `dsaupd` docstring for the code table."
                )));
            }

            // ARPACK reports 1-based offsets into `workd`. Convert to 0-based
            // slice indices.
            let off_in = (ipntr[0] as usize).saturating_sub(1);
            let off_out = (ipntr[1] as usize).saturating_sub(1);

            match ido {
                -1 => {
                    // y = OP * x = (K - σM)^{-1} M x
                    // x at workd[off_in..off_in+n], y at workd[off_out..off_out+n].
                    // Use a temp buffer for the intermediate `M x` so the
                    // borrows of `workd` don't conflict with the LU solve.
                    let mut mx = vec![0.0_f64; n];
                    spmv(m, &workd[off_in..off_in + n], &mut mx);
                    let mut y_local = vec![0.0_f64; n];
                    solve_with_lu(&lu, &mx, &mut y_local);
                    workd[off_out..off_out + n].copy_from_slice(&y_local);
                }
                1 => {
                    // y = (K - σM)^{-1} (M x). ARPACK has already placed
                    // M*x at workd[ipntr[2]-1..], so we just need to solve.
                    let off_mx = (ipntr[2] as usize).saturating_sub(1);
                    let mx_slice = workd[off_mx..off_mx + n].to_vec();
                    let mut y_local = vec![0.0_f64; n];
                    solve_with_lu(&lu, &mx_slice, &mut y_local);
                    workd[off_out..off_out + n].copy_from_slice(&y_local);
                }
                2 => {
                    // y = B * x = M * x
                    let x_local = workd[off_in..off_in + n].to_vec();
                    let mut y_local = vec![0.0_f64; n];
                    spmv(m, &x_local, &mut y_local);
                    workd[off_out..off_out + n].copy_from_slice(&y_local);
                }
                99 => break,
                other => {
                    return Err(EigenError::FaerGevd(format!(
                        "dsaupd_c returned unexpected ido={other}"
                    )));
                }
            }

            trips += 1;
            if trips > max_round_trips {
                return Err(EigenError::FaerGevd(format!(
                    "ARPACK reverse-communication did not converge after {trips} \
                     round-trips (maxiter={}, ncv={ncv}). info={info}",
                    self.max_iters
                )));
            }
        }

        if info > 0 {
            // info > 0 from dsaupd_c is a soft convergence diagnostic
            // (e.g. 1 = max iter reached). We still proceed to dseupd_c
            // to extract whatever converged Ritz pairs we have, then
            // validate the count below.
            eprintln!(
                "geode-core/arpack: dsaupd_c finished with info={info} \
                 (soft warning); attempting Ritz extraction anyway."
            );
        }

        let nconv = iparam[4] as usize;
        if nconv < n_modes {
            return Err(EigenError::FaerGevd(format!(
                "ARPACK converged {nconv} Ritz values, but {n_modes} were requested. \
                 Increase max_iters (currently {}) or ncv_factor (currently {}).",
                self.max_iters, self.ncv_factor
            )));
        }

        // dseupd_c — extract Ritz values (we don't need eigenvectors here).
        // dseupd writes nev values into the first nev slots of d.
        let mut d = vec![0.0_f64; nev as usize];
        let select: Vec<c_int> = vec![0; ncv];
        let howmny = b"A\0";
        let rvec: c_int = 0; // 0 = eigenvalues only
                             // z and ldz are unused when rvec=0; pass a valid but trivial buffer.
        let mut z_dummy = vec![0.0_f64; n.max(1)];
        let ldz: c_int = n_c.max(1);
        let mut info_eup: c_int = 0;

        // SAFETY: same lifetime/size invariants as the dsaupd_c call above.
        unsafe {
            dseupd_c(
                rvec,
                howmny.as_ptr() as *const c_char,
                select.as_ptr(),
                d.as_mut_ptr(),
                z_dummy.as_mut_ptr(),
                ldz,
                self.sigma,
                bmat.as_ptr() as *const c_char,
                n_c,
                which.as_ptr() as *const c_char,
                nev,
                self.tol,
                resid.as_mut_ptr(),
                ncv_c,
                v.as_mut_ptr(),
                n_c,
                iparam.as_mut_ptr(),
                ipntr.as_mut_ptr(),
                workd.as_mut_ptr(),
                workl.as_mut_ptr(),
                lworkl_c,
                &mut info_eup,
            );
        }

        if info_eup != 0 {
            return Err(EigenError::FaerGevd(format!(
                "dseupd_c returned info={info_eup}. See arpack-ng dseupd docstring."
            )));
        }

        // ARPACK returns the *original* eigenvalues λ of K x = λ M x
        // (it inverts the shift-invert mapping internally). Sort ascending
        // and clip to n_modes; we requested exactly nev so the truncation
        // is a no-op but we guard anyway.
        let mut eigs: Vec<f64> = d;
        eigs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(core::cmp::Ordering::Equal));
        eigs.truncate(n_modes);
        Ok(eigs)
    }
}
