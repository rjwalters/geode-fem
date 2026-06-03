package dev.geodefem.refcubecavity;

import org.tensorflow.Graph;
import org.tensorflow.Operand;
import org.tensorflow.Session;
import org.tensorflow.ndarray.NdArrays;
import org.tensorflow.ndarray.Shape;
import org.tensorflow.ndarray.StdArrays;
import org.tensorflow.op.Ops;
import org.tensorflow.op.core.Constant;
import org.tensorflow.op.core.Placeholder;
import org.tensorflow.op.linalg.MatMul;
import org.tensorflow.types.TFloat64;
import org.tensorflow.types.TInt32;
import org.tensorflow.types.TInt64;

/**
 * TF-Java static-graph assembly of the global P1 stiffness/mass matrices
 * for the unit cube. Mirrors {@code reference/jax/cube_cavity.py}'s
 * {@code _assemble_dense_jax} routine, but expressed against the
 * TF-Java {@code Ops} symbolic-graph API rather than JAX tracing.
 *
 * <p><b>Why static graph (not eager)?</b> The TF-Java framing on Epic
 * #88 explicitly singles out the static-graph surface as the validation
 * target — it is the L4-shaped object graph that no other backend
 * exposes as a first-class typed value. Running TF-Java in eager mode
 * would defeat that. So: {@link Graph} + {@link Session}, with a
 * {@link Placeholder} node input and {@code ScatterNdAdd} as the
 * scatter-add primitive.
 *
 * <p><b>What gets built once vs per-call</b>: the graph is built once
 * for a given connectivity (since tet indices become constants in the
 * graph). Re-running on different node coordinates re-feeds the
 * placeholder; the graph is reused. This matches the JAX path's
 * {@code jit}-with-static-connectivity factory.
 *
 * <p><b>What does NOT go through the graph</b>: the eigensolve. TF-Java
 * has no native sparse generalized eigensolver. We materialize the
 * reduced K, M (after Dirichlet boundary elimination) on the JVM side
 * and emit them for downstream SciPy consumption. See the {@code
 * CubeCavityMain} driver and {@code reference/driver/eigensolve_from_tfjava.py}.
 */
public final class AssemblyGraph implements AutoCloseable {

    private final Graph graph;
    private final Session session;
    private final Placeholder<TFloat64> nodesPlaceholder;
    private final Operand<TFloat64> kGlobalOp;
    private final Operand<TFloat64> mGlobalOp;
    private final int nNodes;
    private final int nElem;

    /**
     * Build the assembly graph for a fixed tet connectivity.
     *
     * @param tets   shape [nElem][4] connectivity (constant in the graph)
     * @param nNodes total node count
     */
    public AssemblyGraph(int[][] tets, int nNodes) {
        this.nNodes = nNodes;
        this.nElem = tets.length;
        this.graph = new Graph();
        Ops tf = Ops.create(graph);

        // ----- Inputs -----
        // Nodes are the runtime input: shape [nNodes, 3], dtype f64.
        nodesPlaceholder = tf.placeholder(TFloat64.class, Placeholder.shape(Shape.of(nNodes, 3)));

        // Tet connectivity is baked in as a graph constant.
        int[][] tetsCopy = new int[nElem][4];
        for (int e = 0; e < nElem; e++) {
            System.arraycopy(tets[e], 0, tetsCopy[e], 0, 4);
        }
        Constant<TInt32> tetsConst = tf.constant(tetsCopy);  // [nElem, 4]

        // ----- Gather per-element vertex coordinates -----
        // elem_coords[e, i, :] = nodes[tets[e, i], :], shape [nElem, 4, 3].
        Operand<TFloat64> elemCoords = tf.gather(nodesPlaceholder, tetsConst, tf.constant(0));

        // ----- Per-element edge vectors e1, e2, e3 -----
        // v_i = elem_coords[:, i, :], shape [nElem, 3].
        Operand<TFloat64> v0 = sliceVert(tf, elemCoords, 0);
        Operand<TFloat64> v1 = sliceVert(tf, elemCoords, 1);
        Operand<TFloat64> v2 = sliceVert(tf, elemCoords, 2);
        Operand<TFloat64> v3 = sliceVert(tf, elemCoords, 3);

        Operand<TFloat64> e1 = tf.math.sub(v1, v0);
        Operand<TFloat64> e2 = tf.math.sub(v2, v0);
        Operand<TFloat64> e3 = tf.math.sub(v3, v0);

        // ----- Area-weighted basis gradients (per-row cross products) -----
        Operand<TFloat64> g1 = cross3(tf, e2, e3);
        Operand<TFloat64> g2 = cross3(tf, e3, e1);
        Operand<TFloat64> g3 = cross3(tf, e1, e2);
        Operand<TFloat64> g0 = tf.math.neg(tf.math.add(tf.math.add(g1, g2), g3));

        // ----- Determinant per element: det = e1 . g1, shape [nElem]. -----
        Operand<TFloat64> det = tf.reduceSum(tf.math.mul(e1, g1), tf.constant(1));
        Operand<TFloat64> absDet = tf.math.abs(det);

        // ----- Stack g_i into shape [nElem, 4, 3] then form gg = G G^T (shape [nElem, 4, 4]) -----
        // Stack along a new axis=1 ⇒ rows of gMat are g_0, g_1, g_2, g_3.
        Operand<TFloat64> gMat = tf.stack(
                java.util.Arrays.asList(g0, g1, g2, g3),
                org.tensorflow.op.core.Stack.axis(1L));

        // gg = gMat @ gMat^T per-batch ⇒ matmul with transpose_b=true.
        Operand<TFloat64> gg = tf.linalg.matMul(gMat, gMat,
                MatMul.transposeB(true));  // [nElem, 4, 4]

        // ----- K_local = gg / (6 * |det|) -----
        Operand<TFloat64> sixAbsDet = tf.math.mul(tf.constant(6.0), absDet);
        // Reshape (nElem,) → (nElem, 1, 1) for broadcasting.
        Operand<TFloat64> sixAbsDetReshaped = tf.reshape(sixAbsDet,
                tf.constant(new long[] {nElem, 1L, 1L}));
        Operand<TFloat64> kLocal = tf.math.div(gg, sixAbsDetReshaped);

        // ----- M_local = pattern * (|det| / 120) -----
        double[][] pattern = new double[][] {
            {2.0, 1.0, 1.0, 1.0},
            {1.0, 2.0, 1.0, 1.0},
            {1.0, 1.0, 2.0, 1.0},
            {1.0, 1.0, 1.0, 2.0},
        };
        Operand<TFloat64> patternConst = tf.constant(pattern);  // [4, 4]
        // Broadcast pattern[None, :, :] * (|det|/120)[:, None, None].
        Operand<TFloat64> mScale = tf.math.div(absDet, tf.constant(120.0));
        Operand<TFloat64> mScaleReshaped = tf.reshape(mScale,
                tf.constant(new long[] {nElem, 1L, 1L}));
        Operand<TFloat64> patternReshaped = tf.reshape(patternConst,
                tf.constant(new long[] {1L, 4L, 4L}));
        Operand<TFloat64> mLocal = tf.math.mul(patternReshaped, mScaleReshaped);

        // ----- Scatter-add into a [nNodes, nNodes] zero buffer -----
        // Build per-element (row, col) index pairs:
        //   rows[e, i, j] = tets[e, i],   cols[e, i, j] = tets[e, j].
        // Flattened to [nElem * 16, 2] for scatterNdAdd.
        Operand<TInt32> tetsExpRows = tf.expandDims(tetsConst, tf.constant(2));   // [nE, 4, 1]
        Operand<TInt32> tetsExpCols = tf.expandDims(tetsConst, tf.constant(1));   // [nE, 1, 4]
        Operand<TInt32> rowIdx = tf.broadcastTo(tetsExpRows,
                tf.constant(new int[] {nElem, 4, 4}));
        Operand<TInt32> colIdx = tf.broadcastTo(tetsExpCols,
                tf.constant(new int[] {nElem, 4, 4}));

        Operand<TInt32> rowFlat = tf.reshape(rowIdx, tf.constant(new long[] {(long) nElem * 16, 1L}));
        Operand<TInt32> colFlat = tf.reshape(colIdx, tf.constant(new long[] {(long) nElem * 16, 1L}));
        Operand<TInt32> indexPairs = tf.concat(java.util.Arrays.asList(rowFlat, colFlat),
                tf.constant(1));  // [nElem*16, 2]

        Operand<TFloat64> kLocalFlat = tf.reshape(kLocal,
                tf.constant(new long[] {(long) nElem * 16}));
        Operand<TFloat64> mLocalFlat = tf.reshape(mLocal,
                tf.constant(new long[] {(long) nElem * 16}));

        // scatterNd(indices, updates, shape) — non-mutating; equivalent to
        // (zeros + scatter_add). f64 scatter is supported.
        // TF-Java 1.0.0: indices and shape must share type parameter T, so cast
        // indexPairs from TInt32 to TInt64 to match shape64.
        Operand<TInt64> shape64 = tf.constant(new long[] {(long) nNodes, (long) nNodes});
        Operand<TInt64> indexPairs64 = tf.dtypes.cast(indexPairs, TInt64.class);
        kGlobalOp = tf.scatterNd(indexPairs64, kLocalFlat, shape64);
        mGlobalOp = tf.scatterNd(indexPairs64, mLocalFlat, shape64);

        this.session = new Session(graph);
    }

    /**
     * Run the assembly graph for a given set of node coordinates.
     *
     * @param nodes shape [nNodes][3]
     * @return assembled global K (index 0) and M (index 1), each shape [nNodes][nNodes]
     */
    public double[][][] assemble(double[][] nodes) {
        try (org.tensorflow.Tensor nodesTensor =
                     org.tensorflow.types.TFloat64.tensorOf(StdArrays.ndCopyOf(nodes))) {
            org.tensorflow.Result result = session.runner()
                    .feed(nodesPlaceholder.asOutput(), nodesTensor)
                    .fetch(kGlobalOp.asOutput())
                    .fetch(mGlobalOp.asOutput())
                    .run();
            try {
                double[][] kArr = new double[nNodes][nNodes];
                double[][] mArr = new double[nNodes][nNodes];
                StdArrays.copyFrom((TFloat64) result.get(0), kArr);
                StdArrays.copyFrom((TFloat64) result.get(1), mArr);
                return new double[][][] {kArr, mArr};
            } finally {
                result.close();
            }
        }
    }

    /** Helper: slice vertex {@code i} out of an [nElem, 4, 3] tensor → [nElem, 3]. */
    private static Operand<TFloat64> sliceVert(Ops tf, Operand<TFloat64> elemCoords, int i) {
        // Use gather along axis=1 with the singleton index, then squeeze.
        Operand<TInt32> idx = tf.constant(new int[] {i});
        Operand<TFloat64> sliced = tf.gather(elemCoords, idx, tf.constant(1));  // [nElem, 1, 3]
        return tf.squeeze(sliced, org.tensorflow.op.core.Squeeze.axis(java.util.Collections.singletonList(1L)));
    }

    /** Per-row 3-vector cross product on two [nElem, 3] tensors. */
    private static Operand<TFloat64> cross3(Ops tf, Operand<TFloat64> a, Operand<TFloat64> b) {
        // (a × b)_x = a_y b_z - a_z b_y, etc. We slice components by gather
        // along axis=1 and stack the three components back together.
        Operand<TFloat64> ax = comp(tf, a, 0);
        Operand<TFloat64> ay = comp(tf, a, 1);
        Operand<TFloat64> az = comp(tf, a, 2);
        Operand<TFloat64> bx = comp(tf, b, 0);
        Operand<TFloat64> by = comp(tf, b, 1);
        Operand<TFloat64> bz = comp(tf, b, 2);

        Operand<TFloat64> cx = tf.math.sub(tf.math.mul(ay, bz), tf.math.mul(az, by));
        Operand<TFloat64> cy = tf.math.sub(tf.math.mul(az, bx), tf.math.mul(ax, bz));
        Operand<TFloat64> cz = tf.math.sub(tf.math.mul(ax, by), tf.math.mul(ay, bx));

        return tf.stack(java.util.Arrays.asList(cx, cy, cz),
                org.tensorflow.op.core.Stack.axis(1L));  // [nElem, 3]
    }

    /** Extract scalar component {@code i} of a [nElem, 3] vector field → [nElem]. */
    private static Operand<TFloat64> comp(Ops tf, Operand<TFloat64> v, int i) {
        Operand<TInt32> idx = tf.constant(new int[] {i});
        Operand<TFloat64> sliced = tf.gather(v, idx, tf.constant(1));  // [nElem, 1]
        return tf.squeeze(sliced, org.tensorflow.op.core.Squeeze.axis(java.util.Collections.singletonList(1L)));
    }

    @Override
    public void close() {
        session.close();
        graph.close();
    }
}
