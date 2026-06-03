package dev.geodefem.refcubecavity;

import java.util.ArrayList;
import java.util.List;

/**
 * Programmatic cube tetrahedral mesh — JVM-side mirror of
 * {@code geode_core::mesh::cube_tet_mesh} and
 * {@code reference/numpy/cube_cavity_minimal.py::cube_tet_mesh}.
 *
 * <p>This is pure CPU/JVM code. The mesh construction does not go through
 * TF-Java — the symbolic-graph assembly path consumes the resulting
 * {@code nodes} and {@code tets} arrays as graph constants.
 *
 * <p>Decision rationale: building integer connectivity inside a TF-Java
 * graph would force {@code tf.constant(...)} on every per-cell index, which
 * is awkward and adds zero validation value (the math being checked is
 * the assembly, not the mesh I/O). The JAX reference makes the same
 * choice: mesh is NumPy/CPU, assembly is JAX-traced.
 */
public final class CubeMesh {

    private CubeMesh() {
        // utility class — no instances
    }

    /** Tuple of mesh outputs. */
    public static final class Mesh {
        /** Node coordinates, shape [nNodes][3]. */
        public final double[][] nodes;
        /** Tet connectivity, shape [nTets][4], referencing rows of {@code nodes}. */
        public final int[][] tets;

        public Mesh(double[][] nodes, int[][] tets) {
            this.nodes = nodes;
            this.tets = tets;
        }
    }

    /**
     * Build the unit-cube tetrahedralization with {@code n} cells per side.
     *
     * <p>Produces {@code (n+1)^3} nodes and {@code 6 * n^3} tets. Vertex
     * ordering and the 6-tet split match
     * {@code geode_core::mesh::cube_tet_mesh}.
     *
     * @param n    cells per side
     * @param side cube edge length
     */
    public static Mesh build(int n, double side) {
        int nps = n + 1;
        double h = side / n;

        double[][] nodes = new double[nps * nps * nps][3];
        for (int k = 0; k < nps; k++) {
            for (int j = 0; j < nps; j++) {
                for (int i = 0; i < nps; i++) {
                    int lin = nodeIdx(i, j, k, nps);
                    nodes[lin][0] = i * h;
                    nodes[lin][1] = j * h;
                    nodes[lin][2] = k * h;
                }
            }
        }

        List<int[]> tets = new ArrayList<>(6 * n * n * n);
        for (int k = 0; k < n; k++) {
            for (int j = 0; j < n; j++) {
                for (int i = 0; i < n; i++) {
                    int[] c = new int[] {
                        nodeIdx(i,     j,     k,     nps),
                        nodeIdx(i + 1, j,     k,     nps),
                        nodeIdx(i + 1, j + 1, k,     nps),
                        nodeIdx(i,     j + 1, k,     nps),
                        nodeIdx(i,     j,     k + 1, nps),
                        nodeIdx(i + 1, j,     k + 1, nps),
                        nodeIdx(i + 1, j + 1, k + 1, nps),
                        nodeIdx(i,     j + 1, k + 1, nps),
                    };
                    // 6-tet split sharing diagonal c[0]→c[6]; all right-handed.
                    tets.add(new int[] {c[0], c[1], c[2], c[6]});
                    tets.add(new int[] {c[0], c[2], c[3], c[6]});
                    tets.add(new int[] {c[0], c[3], c[7], c[6]});
                    tets.add(new int[] {c[0], c[7], c[4], c[6]});
                    tets.add(new int[] {c[0], c[4], c[5], c[6]});
                    tets.add(new int[] {c[0], c[5], c[1], c[6]});
                }
            }
        }

        int[][] tetsArr = tets.toArray(new int[0][]);
        return new Mesh(nodes, tetsArr);
    }

    /**
     * Build the boolean interior mask for the unit-cube mesh — true for
     * nodes strictly inside the open cube [0, side]^3.
     */
    public static boolean[] interiorMask(double[][] nodes, double side) {
        double tol = 1e-9 * Math.max(side, 1.0);
        boolean[] mask = new boolean[nodes.length];
        for (int i = 0; i < nodes.length; i++) {
            double x = nodes[i][0], y = nodes[i][1], z = nodes[i][2];
            boolean onBoundary =
                x < tol || Math.abs(x - side) < tol
                || y < tol || Math.abs(y - side) < tol
                || z < tol || Math.abs(z - side) < tol;
            mask[i] = !onBoundary;
        }
        return mask;
    }

    private static int nodeIdx(int i, int j, int k, int nps) {
        return i + j * nps + k * nps * nps;
    }
}
