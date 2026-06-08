package dev.geodefem.refspherepml;

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
 * (real-valued) and the **complex** ε-scaled mass M, decomposed into real
 * and imaginary parts (TF-Java 1.0.0 has no native c128 typed value).
 *
 * <p>Mirrors {@code reference/numpy/sphere_pml.py::assemble_global_nedelec_complex}
 * and the Burn-side {@code geode_core::assemble_global_nedelec_with_complex_epsilon},
 * but expressed against the TF-Java {@code Ops} symbolic-graph API.
 *
 * <h2>Why decomposed (Re(M), Im(M))?</h2>
 * TF-Java 1.0.0 has no first-class complex128 typed value. We carry the
 * real and imaginary parts of ε as parallel {@code TFloat64} placeholders
 * and emit {@code Re(M)} and {@code Im(M)} as parallel {@code TFloat64}
 * tensors. The Python driver fuses them into a single SciPy
 * {@code complex128} CSR before the eigensolve. This is exactly the
 * paired-real pattern the c128 schema (PR #151) already supports on disk
 * (real-imag interleaved).
 *
 * <p>The Burn-side {@code assemble_global_nedelec_with_complex_epsilon}
 * uses the same decomposition: stiffness K is real-valued, mass returns
 * {@code (m_re, m_im)} as two real Burn tensors that
 * {@code burn_complex_mass_to_faer} fuses into {@code faer::Mat<c64>}.
 * The TF-Java path mirrors this contract step-for-step.
 *
 * <h2>What gets built once vs per-call</h2>
 * The graph is built once for a given connectivity ({@code tetEdgeIdx},
 * {@code tetEdgeSign} become constants). Re-running on different node
 * coordinates or different PML strength re-feeds the placeholders.
 *
 * <h2>What does NOT go through the graph</h2>
 * The complex generalized eigensolve (TF-Java has no sparse complex
 * generalized eigensolver) and the Dirichlet BC reduction (dropping PEC
 * boundary edges) are done on the JVM side. See {@link SpherePmlMain}.
 *
 * <h2>Algorithm (per tet)</h2>
 * Identical local-kernel structure to
 * {@code NedelecAssemblyGraph} in the sphere_pec sibling, but the
 * per-element mass block gets scaled twice: once by {@code ε_re[e]} to
 * land in the {@code Re(M)} scatter, once by {@code ε_im[e]} to land in
 * the {@code Im(M)} scatter. The stiffness K scatter is unchanged.
 *
 * <p><b>TF-Java 1.0.0 note</b>: {@code tf.linalg.matMul} is rank-2 only
 * and does not broadcast over a batch dimension. Batch operations use
 * {@code tf.linalg.einsum} with an explicit string equation (same
 * technique as the cube-cavity and sphere-PEC references).
 */
public final class ComplexNedelecAssemblyGraph implements AutoCloseable {

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
    private final Placeholder<TFloat64> epsilonReplaceholder;
    private final Placeholder<TFloat64> epsilonImplaceholder;
    private final Operand<TFloat64> kGlobalOp;
    private final Operand<TFloat64> mReGlobalOp;
    private final Operand<TFloat64> mImGlobalOp;
    private final int nEdges;
    private final int nTets;

    /**
     * Build the complex-Nédélec assembly graph for a fixed tet connectivity.
     *
     * @param tets        shape [nTets][4] tet connectivity (0-based node indices)
     * @param tetEdgeIdx  shape [nTets][6] global edge indices per local edge
     * @param tetEdgeSign shape [nTets][6] signs (+1/-1) per local edge
     * @param nNodes      total node count
     * @param nEdges      total global edge count
     */
    public ComplexNedelecAssemblyGraph(int[][] tets, int[][] tetEdgeIdx, int[][] tetEdgeSign,
                                       int nNodes, int nEdges) {
        this.nEdges = nEdges;
        this.nTets  = tets.length;

        this.graph = new Graph();
        Ops tf = Ops.create(graph);

        // ----- Inputs -----
        // Node coordinates: runtime input, shape [nNodes, 3].
        nodesPlaceholder = tf.placeholder(TFloat64.class,
                Placeholder.shape(Shape.of(nNodes, 3)));

        // Per-tet epsilon_r real and imaginary parts: runtime inputs, shape [nTets].
        epsilonReplaceholder = tf.placeholder(TFloat64.class,
                Placeholder.shape(Shape.of(nTets)));
        epsilonImplaceholder = tf.placeholder(TFloat64.class,
                Placeholder.shape(Shape.of(nTets)));

        // Tet connectivity, edge indices, edge signs: baked as graph constants.
        Constant<TInt32> tetsConst = tf.constant(tets);
        Constant<TInt32> edgeIdxConst = tf.constant(tetEdgeIdx);
        double[][] signF64 = new double[nTets][6];
        for (int e = 0; e < nTets; e++) {
            for (int k = 0; k < 6; k++) {
                signF64[e][k] = tetEdgeSign[e][k];
            }
        }
        Constant<TFloat64> edgeSignConst = tf.constant(signF64);

        // ----- Gather per-element vertex coordinates -----
        Operand<TFloat64> elemCoords = tf.gather(nodesPlaceholder, tetsConst, tf.constant(0));

        // ----- Per-element edge vectors from v0 -----
        Operand<TFloat64> v0 = sliceVert(tf, elemCoords, 0);
        Operand<TFloat64> v1 = sliceVert(tf, elemCoords, 1);
        Operand<TFloat64> v2 = sliceVert(tf, elemCoords, 2);
        Operand<TFloat64> v3 = sliceVert(tf, elemCoords, 3);

        Operand<TFloat64> e1 = tf.math.sub(v1, v0);
        Operand<TFloat64> e2 = tf.math.sub(v2, v0);
        Operand<TFloat64> e3 = tf.math.sub(v3, v0);

        // ----- Area-weighted cofactor gradients -----
        Operand<TFloat64> g1 = cross3(tf, e2, e3);
        Operand<TFloat64> g2 = cross3(tf, e3, e1);
        Operand<TFloat64> g3 = cross3(tf, e1, e2);
        Operand<TFloat64> g0 = tf.math.neg(
                tf.math.add(tf.math.add(g1, g2), g3));

        // ----- |det| per element -----
        Operand<TFloat64> det = tf.reduceSum(tf.math.mul(e1, g1), tf.constant(1));
        Operand<TFloat64> absDet = tf.math.abs(det);

        // ----- Stack g_0..g_3 into g_mat, shape [nTets, 4, 3]. -----
        Operand<TFloat64> gMat = tf.stack(
                Arrays.asList(g0, g1, g2, g3),
                org.tensorflow.op.core.Stack.axis(1L));

        // ----- Cofactor Gram matrix gg[e, p, q] = g_p · g_q -----
        Operand<TFloat64> gg = tf.linalg.einsum(
                Arrays.asList(gMat, gMat),
                "eik,ejk->eij");

        // ----- Reciprocal powers of |det| -----
        Operand<TFloat64> invAbsDet = tf.math.reciprocal(absDet);
        Operand<TFloat64> invAbsDet3 = tf.math.mul(tf.math.mul(invAbsDet, invAbsDet), invAbsDet);

        // ----- Compute all 36 K_{ij} and M_{ij} per tet -----
        @SuppressWarnings("unchecked")
        Operand<TFloat64>[][] ggPQ = new Operand[4][4];
        for (int p = 0; p < 4; p++) {
            for (int q = 0; q < 4; q++) {
                Operand<TFloat64> slP = tf.gather(gg,
                        tf.constant(new int[]{p}), tf.constant(1));
                Operand<TFloat64> slPQ = tf.gather(slP,
                        tf.constant(new int[]{q}), tf.constant(2));
                ggPQ[p][q] = tf.squeeze(slPQ,
                        org.tensorflow.op.core.Squeeze.axis(Arrays.asList(1L, 2L)));
            }
        }

        @SuppressWarnings("unchecked")
        Operand<TFloat64>[] kEntries = new Operand[36];
        @SuppressWarnings("unchecked")
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

                // M_{ij} = ((1+d_ac)*gg_bd - (1+d_ad)*gg_bc
                //           - (1+d_bc)*gg_ad + (1+d_bd)*gg_ac) / (120 |det|)
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
                org.tensorflow.op.core.Stack.axis(0L));
        Operand<TFloat64> mStackedT = tf.stack(Arrays.asList(mEntries),
                org.tensorflow.op.core.Stack.axis(0L));

        Operand<TFloat64> kLocal36 = tf.linalg.transpose(kStackedT,
                tf.constant(new int[]{1, 0}));
        Operand<TFloat64> mLocal36 = tf.linalg.transpose(mStackedT,
                tf.constant(new int[]{1, 0}));

        // ----- Apply sign outer product -----
        Operand<TFloat64> signCol = tf.reshape(edgeSignConst,
                tf.constant(new long[]{nTets, 6L, 1L}));
        Operand<TFloat64> signRow = tf.reshape(edgeSignConst,
                tf.constant(new long[]{nTets, 1L, 6L}));
        Operand<TFloat64> signOuter66 = tf.math.mul(signCol, signRow);
        Operand<TFloat64> signOuter36 = tf.reshape(signOuter66,
                tf.constant(new long[]{nTets, 36L}));

        Operand<TFloat64> kSigned = tf.math.mul(kLocal36, signOuter36);

        // ----- Scale M by per-tet epsilon_r real and imaginary parts -----
        // m_signed_re = M_local * sign_outer * eps_re[e]
        // m_signed_im = M_local * sign_outer * eps_im[e]
        Operand<TFloat64> epsReReshaped = tf.reshape(epsilonReplaceholder,
                tf.constant(new long[]{nTets, 1L}));
        Operand<TFloat64> epsImReshaped = tf.reshape(epsilonImplaceholder,
                tf.constant(new long[]{nTets, 1L}));

        Operand<TFloat64> mSignedShared = tf.math.mul(mLocal36, signOuter36);
        Operand<TFloat64> mSignedRe = tf.math.mul(mSignedShared, epsReReshaped);
        Operand<TFloat64> mSignedIm = tf.math.mul(mSignedShared, epsImReshaped);

        // ----- Build scatter indices [nTets*36, 2] -----
        Operand<TInt32> rowsExp = tf.expandDims(edgeIdxConst, tf.constant(2));
        Operand<TInt32> colsExp = tf.expandDims(edgeIdxConst, tf.constant(1));
        Operand<TInt32> rowBroadcast = tf.broadcastTo(rowsExp,
                tf.constant(new int[]{nTets, 6, 6}));
        Operand<TInt32> colBroadcast = tf.broadcastTo(colsExp,
                tf.constant(new int[]{nTets, 6, 6}));

        Operand<TInt32> rowFlat = tf.reshape(rowBroadcast,
                tf.constant(new long[]{(long) nTets * 36, 1L}));
        Operand<TInt32> colFlat = tf.reshape(colBroadcast,
                tf.constant(new long[]{(long) nTets * 36, 1L}));
        Operand<TInt32> indexPairs32 = tf.concat(Arrays.asList(rowFlat, colFlat),
                tf.constant(1));

        Operand<TFloat64> kFlat = tf.reshape(kSigned,
                tf.constant(new long[]{(long) nTets * 36}));
        Operand<TFloat64> mReFlat = tf.reshape(mSignedRe,
                tf.constant(new long[]{(long) nTets * 36}));
        Operand<TFloat64> mImFlat = tf.reshape(mSignedIm,
                tf.constant(new long[]{(long) nTets * 36}));

        Operand<TInt64> shape64 = tf.constant(new long[]{(long) nEdges, (long) nEdges});
        Operand<TInt64> indexPairs64 = tf.dtypes.cast(indexPairs32, TInt64.class);

        kGlobalOp   = tf.scatterNd(indexPairs64, kFlat,   shape64);
        mReGlobalOp = tf.scatterNd(indexPairs64, mReFlat, shape64);
        mImGlobalOp = tf.scatterNd(indexPairs64, mImFlat, shape64);

        this.session = new Session(graph);
    }

    /**
     * Result struct for {@link #assemble(double[][], double[], double[])}.
     */
    public static final class AssemblyResult {
        public final double[][] kGlobal;
        public final double[][] mReGlobal;
        public final double[][] mImGlobal;

        public AssemblyResult(double[][] kGlobal, double[][] mReGlobal, double[][] mImGlobal) {
            this.kGlobal   = kGlobal;
            this.mReGlobal = mReGlobal;
            this.mImGlobal = mImGlobal;
        }
    }

    /**
     * Run the assembly graph for given node coordinates and per-tet permittivity.
     *
     * @param nodes      shape [nNodes][3]
     * @param epsilonRe  per-tet Re(ε), shape [nTets]
     * @param epsilonIm  per-tet Im(ε), shape [nTets]
     * @return assembled K, Re(M), Im(M), each shape [nEdges][nEdges]
     */
    public AssemblyResult assemble(double[][] nodes, double[] epsilonRe, double[] epsilonIm) {
        try (org.tensorflow.Tensor nodesTensor =
                     TFloat64.tensorOf(StdArrays.ndCopyOf(nodes));
             org.tensorflow.Tensor epsReTensor =
                     TFloat64.tensorOf(StdArrays.ndCopyOf(epsilonRe));
             org.tensorflow.Tensor epsImTensor =
                     TFloat64.tensorOf(StdArrays.ndCopyOf(epsilonIm))) {
            org.tensorflow.Result result = session.runner()
                    .feed(nodesPlaceholder.asOutput(),     nodesTensor)
                    .feed(epsilonReplaceholder.asOutput(), epsReTensor)
                    .feed(epsilonImplaceholder.asOutput(), epsImTensor)
                    .fetch(kGlobalOp.asOutput())
                    .fetch(mReGlobalOp.asOutput())
                    .fetch(mImGlobalOp.asOutput())
                    .run();
            try {
                double[][] kArr   = new double[nEdges][nEdges];
                double[][] mReArr = new double[nEdges][nEdges];
                double[][] mImArr = new double[nEdges][nEdges];
                StdArrays.copyFrom((TFloat64) result.get(0), kArr);
                StdArrays.copyFrom((TFloat64) result.get(1), mReArr);
                StdArrays.copyFrom((TFloat64) result.get(2), mImArr);
                return new AssemblyResult(kArr, mReArr, mImArr);
            } finally {
                result.close();
            }
        }
    }

    // ------------------------------------------------------------------
    // Graph construction helpers (mirror of sphere_pec sibling)
    // ------------------------------------------------------------------

    /** Slice vertex {@code i} out of an [nTets, 4, 3] tensor → [nTets, 3]. */
    private static Operand<TFloat64> sliceVert(Ops tf, Operand<TFloat64> elemCoords, int i) {
        Operand<TInt32> idx = tf.constant(new int[]{i});
        Operand<TFloat64> sliced = tf.gather(elemCoords, idx, tf.constant(1));
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
                org.tensorflow.op.core.Stack.axis(1L));
    }

    /** Extract scalar component {@code i} of a [nTets, 3] vector field → [nTets]. */
    private static Operand<TFloat64> comp(Ops tf, Operand<TFloat64> v, int i) {
        Operand<TInt32> idx = tf.constant(new int[]{i});
        Operand<TFloat64> sliced = tf.gather(v, idx, tf.constant(1));
        return tf.squeeze(sliced,
                org.tensorflow.op.core.Squeeze.axis(Collections.singletonList(1L)));
    }

    @Override
    public void close() {
        session.close();
        graph.close();
    }
}
