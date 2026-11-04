"""Deterministic Latin-hypercube starts (issue #1195, of #1185).

The stream is SplitMix64 (Steele, Lea and Flood 2014) in pure Python, so
the starts do not depend on any numpy version. The arithmetic is exactly
`latin_hypercube` in crates/geode-core/tests/common/icem_harness.rs, and the
CI target `motor_experiment2_harness` re-derives the committed points from
it bit for bit.

A Latin hypercube of `m` points in [0, 1)^d: for each dimension j in turn,
a Fisher-Yates permutation `perm_j` of the m strata (i from m-1 down to 1,
swap i with `below(i + 1)`), then one uniform offset r_ij per point; point
i, dimension j is (perm_j[i] + r_ij) / m.
"""

MASK = (1 << 64) - 1


class SplitMix64:
    def __init__(self, seed):
        self.state = seed & MASK

    def next_u64(self):
        self.state = (self.state + 0x9E3779B97F4A7C15) & MASK
        z = self.state
        z = ((z ^ (z >> 30)) * 0xBF58476D1CE4E5B9) & MASK
        z = ((z ^ (z >> 27)) * 0x94D049BB133111EB) & MASK
        return z ^ (z >> 31)

    def next_f64(self):
        """Uniform in [0, 1) with 53 random bits."""
        return float(self.next_u64() >> 11) * (1.0 / float(1 << 53))

    def below(self, n):
        """Uniform integer in range(n), by rejection (no modulo bias)."""
        zone = MASK - (MASK % n)
        while True:
            r = self.next_u64()
            if r < zone:
                return r % n


def latin_hypercube(seed, m, d):
    rng = SplitMix64(seed)
    out = [[0.0] * d for _ in range(m)]
    for j in range(d):
        perm = list(range(m))
        for i in range(m - 1, 0, -1):
            k = rng.below(i + 1)
            perm[i], perm[k] = perm[k], perm[i]
        for i in range(m):
            r = rng.next_f64()
            out[i][j] = (float(perm[i]) + r) / float(m)
    return out


def to_box(u, lower, upper):
    """x_j = lower_j + u_j (upper_j - lower_j), the same expression as the
    Rust check."""
    return [lo + uj * (hi - lo) for uj, lo, hi in zip(u, lower, upper)]
