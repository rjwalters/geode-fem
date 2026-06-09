package dev.geodefem.refspheremie;

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
 * (real-valued) and the <b>tensor-valued</b> complex ε-scaled mass M,
 * decomposed into real and imaginary parts (TF-Java 1.0.0 has no native
 * c128 typed value).
 *
 * <p>Mirrors {@code reference/numpy/sphere_mie.py::assemble_global_nedelec_anisotropic}
 * and the Burn-side {@code geode_core::assemble_global_nedelec_with_anisotropic_epsilon},
 * but expressed against the TF-Java {@code Ops} symbolic-graph API.
 *
 * <h2>What is new vs the Phase H.4 scalar graph</h2>
 * The sphere-PML sibling ({@code refspherepml.ComplexNedelecAssemblyGraph})
 * scales a single shared per-element mass block by a per-tet <em>scalar</em>
 * (ε_re[e], ε_im[e]) after the kernel contraction. Under a diagonal
 * permittivity tensor that late-scaling shortcut is no longer available:
 * the integrand {@code N_iᵀ diag(ε_x, ε_y, ε_z) N_j} couples the tensor
 * into the cofactor gram itself. The scalar gram
 * {@code gg_pq = g_p · g_q} is replaced by the per-axis product
 * {@code gg^(α)_pq = g_p[α] g_q[α]} (a {@code [nTets, 3]} tensor per
 * (p, q) pair — conveniently just the <em>element-wise</em> product of
 * the two cofactor vectors, no reduction), and each of the 36 local
 * mass entries becomes a per-axis 3-vector contracted against
 * {@code (ε_x, ε_y, ε_z)} inside the graph:
 *
 * <pre>
 * M_ij = Σ_α ε_α / (120 |det|) [  (1+δ_ac) gg^(α)_bd − (1+δ_ad) gg^(α)_bc
 *                               − (1+δ_bc) gg^(α)_ad + (1+δ_bd) gg^(α)_ac ]
 * </pre>
 *
 * Since {@code Σ_α gg^(α)_pq = gg_pq}, equal weights collapse this to
 * exactly ε × the scalar mass — the natural isotropic-collapse
 * regression exercised by the σ₀ = 0 tests.
 *
 * <h2>Why decomposed (Re(M), Im(M))?</h2>
 * TF-Java 1.0.0 has no first-class complex128 typed value, and in the
 * tensor case the constitutive input itself is complex-valued per axis.
 * We carry Re(ε) and Im(ε) as parallel {@code [nTets, 3] TFloat64}
 * placeholders and emit {@code Re(M)} and {@code Im(M)} as parallel
 * {@code TFloat64} tensors. Because the geometric kernel
 * {@code gg^(α)} is real, the complex contraction splits exactly:
 * {@code Re(M_ij) = Σ_α Re(ε_α) m^(α)_ij} and
 * {@code Im(M_ij) = Σ_α Im(ε_α) m^(α)_ij} — two parallel real
 * contractions over the same per-axis kernel. The Python driver fuses
 * the resulting pair into a single SciPy {@code complex128} pencil
 * before the eigensolve, exactly as in Phase H.4.
 *
 * <h2>What gets built once vs per-call</h2>
 * The graph is built once for a given connectivity ({@code tetEdgeIdx},
 * {@code tetEdgeSign} become constants). Re-running on different node
 * coordinates or a different tensor profile re-feeds the placeholders.
 *
 * <h2>What does NOT go through the graph</h2>
 * The complex generalized eigensolve (TF-Java has no sparse complex
 * generalized eigensolver) and the Dirichlet BC reduction (dropping PEC
 * boundary edges) are done on the JVM side. See {@link SphereMieMain}.
 *
 * <p><b>TF-Java 1.0.0 note</b>: {@code tf.linalg.matMul} is rank-2 only
 * and does not broadcast over a batch dimension. The per-axis kernel is
 * expressed with element-wise {@code mul} + {@code reduceSum} (the
 * per-axis gram needs no einsum at all — see above), same technique
 * family as the cube-cavity / sphere-PEC / sphere-PML references.
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
     * Build the tensor-ε complex-Nédélec assembly graph for a fixed tet
     * connectivity.
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

        // Per-tet diagonal permittivity tensor (ε_x, ε_y, ε_z), real and
        // imaginary parts: runtime inputs, shape [nTets, 3]. This is the
        // tensor-constitutive surface that distinguishes the Mie graph
        // from the scalar Phase H.4 graph.
        epsilonReplaceholder = tf.placeholder(TFloat64.class,
                Placeholder.shape(Shape.of(nTets, 3)));
        epsilonImplaceholder = tf.placeholder(TFloat64.class,
                Placeholder.shape(Shape.of(nTets, 3)));

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

        // ----- Area-weighted cofactor gradients, each [nTets, 3] -----
        Operand<TFloat64> g1 = cross3(tf, e2, e3);
        Operand<TFloat64> g2 = cross3(tf, e3, e1);
        Operand<TFloat64> g3 = cross3(tf, e1, e2);
        Operand<TFloat64> g0 = tf.math.neg(
                tf.math.add(tf.math.add(g1, g2), g3));

        @SuppressWarnings("unchecked")
        Operand<TFloat64>[] gVec = new Operand[]{g0, g1, g2, g3};

        // ----- |det| per element -----
        Operand<TFloat64> det = tf.reduceSum(tf.math.mul(e1, g1), tf.constant(1));
        Operand<TFloat64> absDet = tf.math.abs(det);

        // ----- Per-axis cofactor gram gg^(α)_pq = g_p[α] g_q[α] -----
        // For each (p, q) this is just the ELEMENT-WISE product of the two
        // cofactor vectors — shape [nTets, 3], no reduction. The scalar
        // gram (needed by the ε-independent curl-curl K) is its axis-sum.
        @SuppressWarnings("unchecked")
        Operand<TFloat64>[][] ggAxisPQ = new Operand[4][4];
        @SuppressWarnings("unchecked")
        Operand<TFloat64>[][] ggPQ = new Operand[4][4];
        for (int p = 0; p < 4; p++) {
            for (int q = 0; q < 4; q++) {
                ggAxisPQ[p][q] = tf.math.mul(gVec[p], gVec[q]);            // [nTets, 3]
                ggPQ[p][q] = tf.reduceSum(ggAxisPQ[p][q], tf.constant(1)); // [nTets]
            }
        }

        // ----- Reciprocal powers of |det| -----
        Operand<TFloat64> invAbsDet = tf.math.reciprocal(absDet);
        Operand<TFloat64> invAbsDet3 = tf.math.mul(tf.math.mul(invAbsDet, invAbsDet), invAbsDet);
        Operand<TFloat64> invAbsDet120 = tf.math.mul(invAbsDet, tf.constant(1.0 / 120.0));

        // ----- Compute all 36 K_{ij}, Re(M_{ij}), Im(M_{ij}) per tet -----
        @SuppressWarnings("unchecked")
        Operand<TFloat64>[] kEntries = new Operand[36];
        @SuppressWarnings("unchecked")
        Operand<TFloat64>[] mReEntries = new Operand[36];
        @SuppressWarnings("unchecked")
        Operand<TFloat64>[] mImEntries = new Operand[36];

        for (int i = 0; i < 6; i++) {
            int a = LOCAL_EDGES[i][0];
            int b = LOCAL_EDGES[i][1];
            for (int j = 0; j < 6; j++) {
                int c = LOCAL_EDGES[j][0];
                int d = LOCAL_EDGES[j][1];
                int flat = i * 6 + j;

                // K_{ij} = (2/3) * (gg_ac*gg_bd - gg_ad*gg_bc) * invAbsDet3
                // (curl-curl is ε-independent; uses the scalar gram).
                Operand<TFloat64> cross_k = tf.math.sub(
                        tf.math.mul(ggPQ[a][c], ggPQ[b][d]),
                        tf.math.mul(ggPQ[a][d], ggPQ[b][c]));
                kEntries[flat] = tf.math.mul(
                        tf.math.mul(cross_k, tf.constant(2.0 / 3.0)),
                        invAbsDet3);

                // Per-axis Kronecker-lifted mass term, shape [nTets, 3]:
                // m^(α)_ij = (1+δ_ac) gg^(α)_bd − (1+δ_ad) gg^(α)_bc
                //          − (1+δ_bc) gg^(α)_ad + (1+δ_bd) gg^(α)_ac
                double f_ac = (a == c) ? 2.0 : 1.0;
                double f_ad = (a == d) ? 2.0 : 1.0;
                double f_bc = (b == c) ? 2.0 : 1.0;
                double f_bd = (b == d) ? 2.0 : 1.0;

                Operand<TFloat64> mTermAxis = tf.math.add(
                        tf.math.sub(
                                tf.math.sub(
                                        tf.math.mul(tf.constant(f_ac), ggAxisPQ[b][d]),
                                        tf.math.mul(tf.constant(f_ad), ggAxisPQ[b][c])),
                                tf.math.mul(tf.constant(f_bc), ggAxisPQ[a][d])),
                        tf.math.mul(tf.constant(f_bd), ggAxisPQ[a][c]));

                // Contract against the per-axis tensor (real kernel ⇒ the
                // complex contraction splits into two real ones):
                //   Re(M_ij) = Σ_α Re(ε_α) m^(α)_ij / (120 |det|)
                //   Im(M_ij) = Σ_α Im(ε_α) m^(α)_ij / (120 |det|)
                Operand<TFloat64> mWeightedRe = tf.reduceSum(
                        tf.math.mul(mTermAxis, epsilonReplaceholder), tf.constant(1));
                Operand<TFloat64> mWeightedIm = tf.reduceSum(
                        tf.math.mul(mTermAxis, epsilonImplaceholder), tf.constant(1));
                mReEntries[flat] = tf.math.mul(mWeightedRe, invAbsDet120);
                mImEntries[flat] = tf.math.mul(mWeightedIm, invAbsDet120);
            }
        }

        // Stack → [36, nTets], then transpose → [nTets, 36].
        Operand<TFloat64> kStackedT = tf.stack(Arrays.asList(kEntries),
                org.tensorflow.op.core.Stack.axis(0L));
        Operand<TFloat64> mReStackedT = tf.stack(Arrays.asList(mReEntries),
                org.tensorflow.op.core.Stack.axis(0L));
        Operand<TFloat64> mImStackedT = tf.stack(Arrays.asList(mImEntries),
                org.tensorflow.op.core.Stack.axis(0L));

        Operand<TFloat64> kLocal36 = tf.linalg.transpose(kStackedT,
                tf.constant(new int[]{1, 0}));
        Operand<TFloat64> mReLocal36 = tf.linalg.transpose(mReStackedT,
                tf.constant(new int[]{1, 0}));
        Operand<TFloat64> mImLocal36 = tf.linalg.transpose(mImStackedT,
                tf.constant(new int[]{1, 0}));

        // ----- Apply sign outer product -----
        Operand<TFloat64> signCol = tf.reshape(edgeSignConst,
                tf.constant(new long[]{nTets, 6L, 1L}));
        Operand<TFloat64> signRow = tf.reshape(edgeSignConst,
                tf.constant(new long[]{nTets, 1L, 6L}));
        Operand<TFloat64> signOuter66 = tf.math.mul(signCol, signRow);
        Operand<TFloat64> signOuter36 = tf.reshape(signOuter66,
                tf.constant(new long[]{nTets, 36L}));

        Operand<TFloat64> kSigned   = tf.math.mul(kLocal36,   signOuter36);
        Operand<TFloat64> mSignedRe = tf.math.mul(mReLocal36, signOuter36);
        Operand<TFloat64> mSignedIm = tf.math.mul(mImLocal36, signOuter36);

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
     * Result struct for {@link #assemble(double[][], double[][], double[][])}.
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
     * Run the assembly graph for given node coordinates and per-tet
     * diagonal permittivity tensor.
     *
     * @param nodes      shape [nNodes][3]
     * @param epsilonRe  per-tet Re(ε_x, ε_y, ε_z), shape [nTets][3]
     * @param epsilonIm  per-tet Im(ε_x, ε_y, ε_z), shape [nTets][3]
     * @return assembled K, Re(M), Im(M), each shape [nEdges][nEdges]
     */
    public AssemblyResult assemble(double[][] nodes, double[][] epsilonRe, double[][] epsilonIm) {
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
    // Graph construction helpers (mirror of sphere_pml sibling)
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
