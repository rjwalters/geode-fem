"""The interpolant of method C (proposed amendment 4 of #1185): a cubic
radial basis function phi(r) = r^3 with a linear polynomial tail,

    s(u) = sum_i lambda_i |u - u_i|^3 + c_0 + c^T u,

interpolating every training evaluation of the run, with the side
conditions sum_i lambda_i p(u_i) = 0 for the linear p (Powell 1992; the
ORBIT models of Wild, Regis and Shoemaker 2008). With n + 1 affinely
independent points it is the linear interpolant. The saddle-point system
is solved directly; if it is singular (it is not for distinct, affinely
spanning points) the least-squares solution is used and the fit is
flagged.
"""
import numpy as np


class CubicRBF:
    def __init__(self, points, values):
        X = np.asarray(points, dtype=float)
        y = np.asarray(values, dtype=float)
        m, n = X.shape
        self.X = X
        self.n = n
        r = np.linalg.norm(X[:, None, :] - X[None, :, :], axis=2)
        phi = r ** 3
        P = np.hstack([np.ones((m, 1)), X])
        A = np.zeros((m + n + 1, m + n + 1))
        A[:m, :m] = phi
        A[:m, m:] = P
        A[m:, :m] = P.T
        b = np.concatenate([y, np.zeros(n + 1)])
        self.exact = True
        try:
            sol = np.linalg.solve(A, b)
            if not np.all(np.isfinite(sol)):
                raise np.linalg.LinAlgError("non-finite")
        except np.linalg.LinAlgError:
            sol = np.linalg.lstsq(A, b, rcond=None)[0]
            self.exact = False
        self.lam = sol[:m]
        self.c0 = sol[m]
        self.c = sol[m + 1:]

    def __call__(self, u):
        d = np.linalg.norm(self.X - np.asarray(u, dtype=float), axis=1)
        return float(self.lam @ d ** 3 + self.c0 + self.c @ u)

    def grad(self, u):
        u = np.asarray(u, dtype=float)
        diff = u - self.X
        d = np.linalg.norm(diff, axis=1)
        # d/du |u - u_i|^3 = 3 |u - u_i| (u - u_i)
        return (3.0 * (self.lam * d) @ diff) + self.c


class Models:
    """The four interpolants of method C: the per-current training stds
    (normalised by sqrt(F_ref)) at fractions 0, 0.5, 1, and the
    full-current training mean (normalised by mean_ref)."""

    def __init__(self, U, S, M):
        U = np.asarray(U)
        S = np.asarray(S)
        self.stds = [CubicRBF(U, S[:, c]) for c in range(S.shape[1])]
        self.mean = CubicRBF(U, np.asarray(M))
        self.exact = all(s.exact for s in self.stds) and self.mean.exact

    def f(self, u):
        return sum(s(u) ** 2 for s in self.stds)

    def f_grad(self, u):
        return sum(2.0 * s(u) * s.grad(u) for s in self.stds)

    def predict(self, u):
        return [s(u) for s in self.stds], self.mean(u)
