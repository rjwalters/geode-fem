package dev.geodefem.refspherepec;

import org.tensorflow.Graph;
import org.tensorflow.Operand;
import org.tensorflow.Session;
import org.tensorflow.ndarray.StdArrays;
import org.tensorflow.ndarray.Shape;
import org.tensorflow.op.Ops;
import org.tensorflow.op.core.Constant;
import org.tensorflow.op.core.Placeholder;
import org.tensorflow.types.TFloat64;
import org.tensorflow.types.TInt32;
import org.tensorflow.types.TInt64;

import java.util.Arrays;
import java.util.Collections;

/**
 * TF-Java static-graph assembly of the global Nédélec curl-curl stiffness K
 * and ε-scaled mass M for the sphere-PEC eigenproblem.
 *
 * <p>Mirrors {@code reference/numpy/sphere_pec.py::assemble_global_nedelec}
 * and {@code reference/julia/sphere_pec.jl::assemble_global_nedelec}, but
 * expressed against the TF-Java {@code Ops} symbolic-graph API.
 *
 * <h2>Why static graph (not eager)?</h2>
 * The TF-Java framing on Epic #88 explicitly singles out the static-graph
 * surface as the validation target. Graph + Session, Placeholder input.
 *
 * <h2>What gets built once vs per-call</h2>
 * The graph is built once for a given connectivity ({@code tetEdgeIdx},
 * {@code tetEdgeSign} become constants). Re-running on different node
 * coordinates re-feeds the placeholder. {@code epsilonR} is also a
 * per-run input fed as a placeholder.
 *
 * <h2>What does NOT go through the graph</h2>
 * The eigensolve (TF-Java has no sparse generalized eigensolver) and
 * the Dirichlet BC reduction (dropping PEC boundary edges) are done on
 * the JVM side. See {@code SpherePecMain}.
 *
 * <h2>Algorithm</h2>
 * Per tet:
 * <ol>
 *   <li>Gather vertex coordinates: {@code elemCoords[e, i, :] = nodes[tets[e,i], :]}.</li>
 *   <li>Compute edge vectors e1, e2, e3 from v0.</li>
 *   <li>Compute area-weighted cofactor gradients g1, g2, g3, g0.</li>
 *   <li>Compute |det| = |e1 · g1|.</li>
 *   <li>Form cofactor Gram matrix {@code gg[e, p, q] = g_p · g_q}, shape
 *       {@code [nTets, 4, 4]}.</li>
 *   <li>For each of the 36 (i,j) local-edge pairs, compute:
 *     <pre>
 *       K_{ij} = (2/3) * (gg_ac * gg_bd - gg_ad * gg_bc) / |det|^3   (eq. K)
 *       M_{ij} = ((1+d_ac)*gg_bd - (1+d_ad)*gg_bc
 *                 - (1+d_bc)*gg_ad + (1+d_bd)*gg_ac) / (120 |det|)   (eq. M)
 *     </pre>
 *     where edge i = (a,b) and edge j = (c,d) in TET_LOCAL_EDGES order,
 *     and {@code d_xy = 1} if {@code x == y}, else 0.</li>
 *   <li>Apply the sign outer product: {@code k_signed[e,i,j] = s_i * s_j * K[e,i,j]}
 *       (resp. for M), then scale M by epsilon_r[e].</li>
 *   <li>Scatter all 36 (row, col, val) triplets per tet via
 *       {@code tf.scatterNd} into a {@code [nEdges, nEdges]} zero buffer.</li>
 * </ol>
 *
 * <p><b>TF-Java 1.0.0 note</b>: {@code tf.linalg.matMul} is rank-2 only and
 * does not broadcast over a batch dimension. Batch operations use
 * {@code tf.linalg.einsum} with an explicit string equation (same technique
 * as the cube-cavity reference).
 */
public final class NedelecAssemblyGraph implements AutoCloseable {

    /**
     * Local edge (a, b) pairs in TET_LOCAL_EDGES order (0-indexed local vertices).
     * Matches reference/numpy/nedelec_local_matrices.py::TET_LOCAL_EDGES.
     */
    private static final int[][] LOCAL_EDGES = {
        {0, 1}, {0, 2}, {0, 3}, {1, 2}, {1, 3}, {2, 3}
    };

    private final Graph graph;
    private final Session session;
    private final Placeholder<TFloat64> nodesPlaceholder;
    private final Placeholder<TFloat64> epsilonRPlaceholder;
    private final Operand<TFloat64> kGlobalOp;
    private final Operand<TFloat64> mGlobalOp;
    private final int nEdges;
    private final int nTets;

    /**
     * Build the Nédélec assembly graph for a fixed tet connectivity.
     *
     * @param tets        shape [nTets][4] tet connectivity (0-based node indices)
     * @param tetEdgeIdx  shape [nTets][6] global edge indices per local edge
     * @param tetEdgeSign shape [nTets][6] signs (+1/-1) per local edge
     * @param nNodes      total node count
     * @param nEdges      total global edge count
     */
    public NedelecAssemblyGraph(int[][] tets, int[][] tetEdgeIdx, int[][] tetEdgeSign,
                                int nNodes, int nEdges) {
        this.nEdges = nEdges;
        this.nTets  = tets.length;

        this.graph = new Graph();
        Ops tf = Ops.create(graph);

        // ----- Inputs -----
        // Node coordinates: runtime input, shape [nNodes, 3].
        nodesPlaceholder = tf.placeholder(TFloat64.class,
                Placeholder.shape(Shape.of(nNodes, 3)));

        // Per-tet epsilon_r: runtime input, shape [nTets].
        epsilonRPlaceholder = tf.placeholder(TFloat64.class,
                Placeholder.shape(Shape.of(nTets)));

        // Tet connectivity, edge indices, edge signs: baked as graph constants.
        Constant<TInt32> tetsConst = tf.constant(tets);         // [nTets, 4]
        Constant<TInt32> edgeIdxConst = tf.constant(tetEdgeIdx); // [nTets, 6]
        // Convert edge signs to f64 for element-wise multiply.
        double[][] signF64 = new double[nTets][6];
        for (int e = 0; e < nTets; e++) {
            for (int k = 0; k < 6; k++) {
                signF64[e][k] = tetEdgeSign[e][k];
            }
        }
        Constant<TFloat64> edgeSignConst = tf.constant(signF64); // [nTets, 6]

        // ----- Gather per-element vertex coordinates -----
        // elemCoords[e, i, :] = nodes[tets[e, i], :], shape [nTets, 4, 3].
        Operand<TFloat64> elemCoords = tf.gather(nodesPlaceholder, tetsConst, tf.constant(0));

        // ----- Per-element edge vectors from v0 -----
        Operand<TFloat64> v0 = sliceVert(tf, elemCoords, 0); // [nTets, 3]
        Operand<TFloat64> v1 = sliceVert(tf, elemCoords, 1);
        Operand<TFloat64> v2 = sliceVert(tf, elemCoords, 2);
        Operand<TFloat64> v3 = sliceVert(tf, elemCoords, 3);

        Operand<TFloat64> e1 = tf.math.sub(v1, v0); // [nTets, 3]
        Operand<TFloat64> e2 = tf.math.sub(v2, v0);
        Operand<TFloat64> e3 = tf.math.sub(v3, v0);

        // ----- Area-weighted cofactor gradients -----
        // g1 = e2 × e3, g2 = e3 × e1, g3 = e1 × e2, g0 = -(g1+g2+g3)
        Operand<TFloat64> g1 = cross3(tf, e2, e3); // [nTets, 3]
        Operand<TFloat64> g2 = cross3(tf, e3, e1);
        Operand<TFloat64> g3 = cross3(tf, e1, e2);
        Operand<TFloat64> g0 = tf.math.neg(
                tf.math.add(tf.math.add(g1, g2), g3));

        // ----- |det| per element: dot(e1, g1), shape [nTets]. -----
        Operand<TFloat64> det = tf.reduceSum(tf.math.mul(e1, g1), tf.constant(1)); // [nTets]
        Operand<TFloat64> absDet = tf.math.abs(det);

        // ----- Stack g_0..g_3 into g_mat, shape [nTets, 4, 3]. -----
        Operand<TFloat64> gMat = tf.stack(
                Arrays.asList(g0, g1, g2, g3),
                org.tensorflow.op.core.Stack.axis(1L)); // [nTets, 4, 3]

        // ----- Cofactor Gram matrix gg[e, p, q] = g_p · g_q -----
        // einsum("eik,ejk->eij", gMat, gMat) → [nTets, 4, 4]
        Operand<TFloat64> gg = tf.linalg.einsum(
                Arrays.asList(gMat, gMat),
                "eik,ejk->eij"); // [nTets, 4, 4]

        // ----- Reciprocal powers of |det| -----
        // 1/|det|:  shape [nTets] → [nTets, 1, 1] for broadcasting.
        Operand<TFloat64> invAbsDet = tf.math.reciprocal(absDet);
        Operand<TFloat64> invAbsDet3 = tf.math.mul(tf.math.mul(invAbsDet, invAbsDet), invAbsDet);
        Operand<TFloat64> invAbsDet111  = reshape111(tf, invAbsDet);   // [nTets,1,1]
        Operand<TFloat64> invAbsDet3111 = reshape111(tf, invAbsDet3);  // [nTets,1,1]

        // ----- Compute all 36 K_{ij} and M_{ij} per tet -----
        //
        // For each of the 36 (i,j) index pairs we need:
        //   a = LOCAL_EDGES[i][0], b = LOCAL_EDGES[i][1]
        //   c = LOCAL_EDGES[j][0], d = LOCAL_EDGES[j][1]
        //   gg_ac = gg[:, a, c], etc.
        //   K_{ij} = (2/3) * (gg_ac*gg_bd - gg_ad*gg_bc) * invAbsDet3
        //   M_{ij} = ((1+d_ac)*gg_bd - (1+d_ad)*gg_bc
        //             - (1+d_bc)*gg_ad + (1+d_bd)*gg_ac) * invAbsDet / 120
        //
        // Precompute the 4×4 gg sub-scalars: for each (p,q) in 0..4×0..4,
        // gg_pq = gg[:, p, q], shape [nTets].
        // Build them via gather on the 4×4 slabs.
        //
        // Strategy: we unroll the 36 pairs and build each K_ij and M_ij as
        // a [nTets] tensor, then stack into [36, nTets], then reshape to
        // [nTets, 36] (= [nTets, 6, 6] reshaped) for the scatter step.
        //
        // Slicing gg[:, p, q] via slice+squeeze on the [nTets, 4, 4] tensor.

        // Precompute gg[:, p, q] for all p, q in 0..4.
        Operand<TFloat64>[][] ggPQ = new Operand[4][4];
        for (int p = 0; p < 4; p++) {
            for (int q = 0; q < 4; q++) {
                // gg[:, p, q]: slice [nTets, 1, 1] then squeeze → [nTets]
                Operand<TFloat64> slP = tf.gather(gg,
                        tf.constant(new int[]{p}), tf.constant(1)); // [nTets, 1, 4]
                Operand<TFloat64> slPQ = tf.gather(slP,
                        tf.constant(new int[]{q}), tf.constant(2)); // [nTets, 1, 1]
                ggPQ[p][q] = tf.squeeze(slPQ,
                        org.tensorflow.op.core.Squeeze.axis(Arrays.asList(1L, 2L)));  // [nTets]
            }
        }

        // Build [nTets, 36] K_local and M_local.
        Operand<TFloat64>[] kEntries = new Operand[36];
        Operand<TFloat64>[] mEntries = new Operand[36];

        for (int i = 0; i < 6; i++) {
            int a = LOCAL_EDGES[i][0];
            int b = LOCAL_EDGES[i][1];
            for (int j = 0; j < 6; j++) {
                int c = LOCAL_EDGES[j][0];
                int d = LOCAL_EDGES[j][1];
                int flat = i * 6 + j;

                Operand<TFloat64> gg_ac = ggPQ[a][c];
                Operand<TFloat64> gg_ad = ggPQ[a][d];
                Operand<TFloat64> gg_bc = ggPQ[b][c];
                Operand<TFloat64> gg_bd = ggPQ[b][d];

                // K_{ij} = (2/3) * (gg_ac*gg_bd - gg_ad*gg_bc) * invAbsDet3
                Operand<TFloat64> cross_k = tf.math.sub(
                        tf.math.mul(gg_ac, gg_bd),
                        tf.math.mul(gg_ad, gg_bc));
                kEntries[flat] = tf.math.mul(
                        tf.math.mul(cross_k, tf.constant(2.0 / 3.0)),
                        invAbsDet3);

                // M_{ij} = ((f_ac*gg_bd - f_ad*gg_bc - f_bc*gg_ad + f_bd*gg_ac) / (120|det|)
                double f_ac = (a == c) ? 2.0 : 1.0;
                double f_ad = (a == d) ? 2.0 : 1.0;
                double f_bc = (b == c) ? 2.0 : 1.0;
                double f_bd = (b == d) ? 2.0 : 1.0;

                Operand<TFloat64> mTerm = tf.math.add(
                        tf.math.sub(
                                tf.math.sub(
                                        tf.math.mul(tf.constant(f_ac), gg_bd),
                                        tf.math.mul(tf.constant(f_ad), gg_bc)),
                                tf.math.mul(tf.constant(f_bc), gg_ad)),
                        tf.math.mul(tf.constant(f_bd), gg_ac));
                mEntries[flat] = tf.math.mul(mTerm,
                        tf.math.mul(invAbsDet, tf.constant(1.0 / 120.0)));
            }
        }

        // Stack → [36, nTets], then transpose → [nTets, 36].
        Operand<TFloat64> kStackedT = tf.stack(Arrays.asList(kEntries),
                org.tensorflow.op.core.Stack.axis(0L)); // [36, nTets]
        Operand<TFloat64> mStackedT = tf.stack(Arrays.asList(mEntries),
                org.tensorflow.op.core.Stack.axis(0L));

        // Transpose to [nTets, 36].
        Operand<TFloat64> kLocal36 = tf.linalg.transpose(kStackedT,
                tf.constant(new int[]{1, 0})); // [nTets, 36]
        Operand<TFloat64> mLocal36 = tf.linalg.transpose(mStackedT,
                tf.constant(new int[]{1, 0}));

        // ----- Apply sign outer product -----
        // edgeSignConst: [nTets, 6]. signOuter = s_i * s_j → [nTets, 36].
        // Flatten sign outer product: reshape [nTets,6] → [nTets,6,1] and [nTets,1,6],
        // multiply, reshape to [nTets,36].
        Operand<TFloat64> signCol = tf.reshape(edgeSignConst,
                tf.constant(new long[]{nTets, 6L, 1L}));
        Operand<TFloat64> signRow = tf.reshape(edgeSignConst,
                tf.constant(new long[]{nTets, 1L, 6L}));
        Operand<TFloat64> signOuter66 = tf.math.mul(signCol, signRow); // [nTets, 6, 6]
        Operand<TFloat64> signOuter36 = tf.reshape(signOuter66,
                tf.constant(new long[]{nTets, 36L})); // [nTets, 36]

        Operand<TFloat64> kSigned = tf.math.mul(kLocal36, signOuter36); // [nTets, 36]

        // Scale M by epsilon_r[e]: reshape [nTets] → [nTets, 1].
        Operand<TFloat64> epsReshaped = tf.reshape(epsilonRPlaceholder,
                tf.constant(new long[]{nTets, 1L}));
        Operand<TFloat64> mSigned = tf.math.mul(
                tf.math.mul(mLocal36, signOuter36),
                epsReshaped); // [nTets, 36]

        // ----- Build scatter indices [nTets*36, 2] -----
        // edgeIdxConst: [nTets, 6] (int32). We need outer product of (row, col):
        //   rows[e, i, j] = tetEdgeIdx[e, i]
        //   cols[e, i, j] = tetEdgeIdx[e, j]
        // Reshape and broadcast to [nTets, 6, 6] → flatten to [nTets*36].
        Operand<TInt32> rowsExp = tf.expandDims(edgeIdxConst, tf.constant(2)); // [nTets, 6, 1]
        Operand<TInt32> colsExp = tf.expandDims(edgeIdxConst, tf.constant(1)); // [nTets, 1, 6]
        Operand<TInt32> rowBroadcast = tf.broadcastTo(rowsExp,
                tf.constant(new int[]{nTets, 6, 6}));
        Operand<TInt32> colBroadcast = tf.broadcastTo(colsExp,
                tf.constant(new int[]{nTets, 6, 6}));

        Operand<TInt32> rowFlat = tf.reshape(rowBroadcast,
                tf.constant(new long[]{(long) nTets * 36, 1L}));
        Operand<TInt32> colFlat = tf.reshape(colBroadcast,
                tf.constant(new long[]{(long) nTets * 36, 1L}));
        Operand<TInt32> indexPairs32 = tf.concat(Arrays.asList(rowFlat, colFlat),
                tf.constant(1)); // [nTets*36, 2]

        Operand<TFloat64> kFlat = tf.reshape(kSigned,
                tf.constant(new long[]{(long) nTets * 36}));
        Operand<TFloat64> mFlat = tf.reshape(mSigned,
                tf.constant(new long[]{(long) nTets * 36}));

        // Cast indices to int64 for scatterNd.
        Operand<TInt64> shape64 = tf.constant(new long[]{(long) nEdges, (long) nEdges});
        Operand<TInt64> indexPairs64 = tf.dtypes.cast(indexPairs32, TInt64.class);

        kGlobalOp = tf.scatterNd(indexPairs64, kFlat, shape64);
        mGlobalOp = tf.scatterNd(indexPairs64, mFlat, shape64);

        this.session = new Session(graph);
    }

    /**
     * Run the assembly graph for given node coordinates and per-tet permittivity.
     *
     * @param nodes    shape [nNodes][3]
     * @param epsilonR shape [nTets]
     * @return assembled global K (index 0) and M (index 1), each shape [nEdges][nEdges]
     */
    public double[][][] assemble(double[][] nodes, double[] epsilonR) {
        try (org.tensorflow.Tensor nodesTensor =
                     TFloat64.tensorOf(StdArrays.ndCopyOf(nodes));
             org.tensorflow.Tensor epsTensor =
                     TFloat64.tensorOf(StdArrays.ndCopyOf(epsilonR))) {
            org.tensorflow.Result result = session.runner()
                    .feed(nodesPlaceholder.asOutput(), nodesTensor)
                    .feed(epsilonRPlaceholder.asOutput(), epsTensor)
                    .fetch(kGlobalOp.asOutput())
                    .fetch(mGlobalOp.asOutput())
                    .run();
            try {
                double[][] kArr = new double[nEdges][nEdges];
                double[][] mArr = new double[nEdges][nEdges];
                StdArrays.copyFrom((TFloat64) result.get(0), kArr);
                StdArrays.copyFrom((TFloat64) result.get(1), mArr);
                return new double[][][] {kArr, mArr};
            } finally {
                result.close();
            }
        }
    }

    // ------------------------------------------------------------------
    // Graph construction helpers
    // ------------------------------------------------------------------

    /** Reshape a [nTets] tensor to [nTets, 1, 1] for broadcasting. */
    private Operand<TFloat64> reshape111(Ops tf, Operand<TFloat64> v) {
        return tf.reshape(v, tf.constant(new long[]{nTets, 1L, 1L}));
    }

    /** Slice vertex {@code i} out of an [nTets, 4, 3] tensor → [nTets, 3]. */
    private static Operand<TFloat64> sliceVert(Ops tf, Operand<TFloat64> elemCoords, int i) {
        Operand<TInt32> idx = tf.constant(new int[]{i});
        Operand<TFloat64> sliced = tf.gather(elemCoords, idx, tf.constant(1)); // [nTets, 1, 3]
        return tf.squeeze(sliced,
                org.tensorflow.op.core.Squeeze.axis(Collections.singletonList(1L)));
    }

    /** Per-row 3-vector cross product on two [nTets, 3] tensors → [nTets, 3]. */
    private static Operand<TFloat64> cross3(Ops tf, Operand<TFloat64> a, Operand<TFloat64> b) {
        Operand<TFloat64> ax = comp(tf, a, 0);
        Operand<TFloat64> ay = comp(tf, a, 1);
        Operand<TFloat64> az = comp(tf, a, 2);
        Operand<TFloat64> bx = comp(tf, b, 0);
        Operand<TFloat64> by = comp(tf, b, 1);
        Operand<TFloat64> bz = comp(tf, b, 2);

        Operand<TFloat64> cx = tf.math.sub(tf.math.mul(ay, bz), tf.math.mul(az, by));
        Operand<TFloat64> cy = tf.math.sub(tf.math.mul(az, bx), tf.math.mul(ax, bz));
        Operand<TFloat64> cz = tf.math.sub(tf.math.mul(ax, by), tf.math.mul(ay, bx));

        return tf.stack(Arrays.asList(cx, cy, cz),
                org.tensorflow.op.core.Stack.axis(1L)); // [nTets, 3]
    }

    /** Extract scalar component {@code i} of a [nTets, 3] vector field → [nTets]. */
    private static Operand<TFloat64> comp(Ops tf, Operand<TFloat64> v, int i) {
        Operand<TInt32> idx = tf.constant(new int[]{i});
        Operand<TFloat64> sliced = tf.gather(v, idx, tf.constant(1)); // [nTets, 1]
        return tf.squeeze(sliced,
                org.tensorflow.op.core.Squeeze.axis(Collections.singletonList(1L)));
    }

    @Override
    public void close() {
        session.close();
        graph.close();
    }
}
