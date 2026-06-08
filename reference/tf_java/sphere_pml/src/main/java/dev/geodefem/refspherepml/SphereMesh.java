package dev.geodefem.refspherepml;

import java.io.BufferedReader;
import java.io.FileReader;
import java.io.IOException;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
import java.util.Map;

/**
 * Minimal Gmsh MSH 4.x parser + edge / centroid / complex-epsilon helpers
 * for the sphere-PML fixture.
 *
 * <p>Mirror of {@code reference/tf_java/sphere_pec/.../SphereMesh.java}
 * extended with:
 * <ul>
 *   <li>{@link #tetCentroidRadii(double[][], int[][])} — per-tet centroid
 *       distance from the origin, the geometric input to the PML profile.
 *       Mirror of {@code geode_core::tet_centroid_radii} and
 *       {@code reference/numpy/sphere_pml.py::tet_centroid_radii}.</li>
 *   <li>{@link #buildComplexEpsilonRPml(int[], double[], double, double)} —
 *       per-tet complex relative permittivity realizing the
 *       scalar-isotropic PML. Mirror of
 *       {@code geode_core::build_complex_epsilon_r_pml} /
 *       {@code reference/numpy/sphere_pml.py::build_complex_epsilon_r_pml}.</li>
 * </ul>
 *
 * <p>Mesh I/O, edge enumeration, and PEC mask are unchanged from the
 * sphere-PEC sibling — the PML problem differs only in the per-tet
 * constitutive scaling on the mass. The bundled
 * {@code reference/fixtures/sphere_pml/sphere.msh} is the same file as
 * {@code reference/fixtures/sphere_pec/sphere.msh} (774 nodes, 3335 tets).
 *
 * <p>Decision: mesh construction and the complex-epsilon profile do NOT
 * go through TF-Java. The symbolic-graph assembly path consumes the
 * resulting {@code nodes}, {@code tets}, and per-tet
 * {@code epsilonR_re / epsilonR_im} arrays as inputs. This matches the
 * JAX/Julia/NumPy pattern.
 */
public final class SphereMesh {

    private SphereMesh() {}

    // Physical-group tags — mirror of geode_core::mesh::sphere::PHYS_*.
    public static final int PHYS_SPHERE_INTERIOR = 1; // tets in r <= R_SPHERE
    public static final int PHYS_VACUUM_GAP      = 2; // tets in R_SPHERE < r <= R_PML_INNER
    public static final int PHYS_PML_SHELL       = 5; // tets in R_PML_INNER < r <= R_BUFFER

    public static final double R_SPHERE     = 1.0; // inner dielectric radius
    public static final double R_PML_INNER  = 1.5; // PML absorption start
    public static final double R_BUFFER     = 2.0; // outer PEC wall radius

    /** Mesh output tuple. */
    public static final class Mesh {
        /** Node coordinates, shape [nNodes][3], 0-based. */
        public final double[][] nodes;
        /** Tet connectivity, shape [nTets][4], 0-based. */
        public final int[][] tets;
        /** Per-tet 3D physical group tag, shape [nTets]. */
        public final int[] tetTags;

        public Mesh(double[][] nodes, int[][] tets, int[] tetTags) {
            this.nodes   = nodes;
            this.tets    = tets;
            this.tetTags = tetTags;
        }
    }

    /**
     * Parse a Gmsh MSH 4.x ASCII file and return the sphere mesh.
     *
     * @param path  path to the {@code .msh} file
     * @return      parsed mesh
     * @throws IOException if I/O fails or the file is not a supported MSH4 file
     */
    public static Mesh read(String path) throws IOException {
        try (BufferedReader br = new BufferedReader(new FileReader(path))) {
            return parse(br);
        }
    }

    // ------------------------------------------------------------------
    // Parser internals (identical to the sphere_pec sibling)
    // ------------------------------------------------------------------

    private static Mesh parse(BufferedReader br) throws IOException {
        double[][] nodes   = null;
        long[] nodeGmshIds = null;
        List<int[]> tetList    = new ArrayList<>();
        List<Integer> tagList  = new ArrayList<>();

        String line;
        while ((line = br.readLine()) != null) {
            line = line.trim();
            switch (line) {
                case "$MeshFormat":
                    skipUntilEnd(br, "$EndMeshFormat");
                    break;
                case "$PhysicalNames":
                    skipUntilEnd(br, "$EndPhysicalNames");
                    break;
                case "$Entities":
                    skipUntilEnd(br, "$EndEntities");
                    break;
                case "$PartitionedEntities":
                    skipUntilEnd(br, "$EndPartitionedEntities");
                    break;
                case "$Nodes": {
                    Object[] result = parseNodes(br);
                    nodes       = (double[][]) result[0];
                    nodeGmshIds = (long[]) result[1];
                    break;
                }
                case "$Elements":
                    if (nodes == null) {
                        throw new IOException("$Elements section before $Nodes");
                    }
                    parseElements(br, nodeGmshIds, tetList, tagList);
                    break;
                default:
                    if (line.startsWith("$") && !line.startsWith("$End")) {
                        String endTag = "$End" + line.substring(1);
                        skipUntilEnd(br, endTag);
                    }
                    break;
            }
        }

        if (nodes == null) throw new IOException("No $Nodes section found");
        if (tetList.isEmpty()) throw new IOException("No tetrahedral elements found");

        int[][] tets = tetList.toArray(new int[0][]);
        int[] tetTags = tagList.stream().mapToInt(Integer::intValue).toArray();

        System.out.printf("[sphere-pml] Parsed mesh: %d nodes, %d tets%n",
                nodes.length, tets.length);
        return new Mesh(nodes, tets, tetTags);
    }

    private static Object[] parseNodes(BufferedReader br) throws IOException {
        String header = br.readLine();
        if (header == null) throw new IOException("Unexpected EOF in $Nodes header");
        String[] hp = header.trim().split("\\s+");
        int numEntityBlocks = Integer.parseInt(hp[0]);
        int totalNodes      = Integer.parseInt(hp[1]);

        double[][] nodes    = new double[totalNodes][3];
        long[] gmshIds      = new long[totalNodes];
        long maxTag = Long.parseLong(hp[3]);
        long[] tagToIdx = new long[(int)(maxTag + 1)];
        Arrays.fill(tagToIdx, -1);

        int ptr = 0;
        for (int b = 0; b < numEntityBlocks; b++) {
            String bh = br.readLine();
            if (bh == null) throw new IOException("Unexpected EOF in $Nodes block header");
            String[] bhp = bh.trim().split("\\s+");
            int nInBlock = Integer.parseInt(bhp[3]);

            long[] blockTags = new long[nInBlock];
            for (int i = 0; i < nInBlock; i++) {
                String tagLine = br.readLine();
                if (tagLine == null) throw new IOException("Unexpected EOF reading node tags");
                blockTags[i] = Long.parseLong(tagLine.trim());
            }
            for (int i = 0; i < nInBlock; i++) {
                String coordLine = br.readLine();
                if (coordLine == null) throw new IOException("Unexpected EOF reading node coords");
                String[] cp = coordLine.trim().split("\\s+");
                int idx = ptr + i;
                nodes[idx][0] = Double.parseDouble(cp[0]);
                nodes[idx][1] = Double.parseDouble(cp[1]);
                nodes[idx][2] = Double.parseDouble(cp[2]);
                gmshIds[idx]  = blockTags[i];
                tagToIdx[(int) blockTags[i]] = idx;
            }
            ptr += nInBlock;
        }

        String endLine = br.readLine();
        if (endLine == null || !endLine.trim().equals("$EndNodes")) {
            throw new IOException("Expected $EndNodes, got: " + endLine);
        }

        return new Object[] { nodes, tagToIdx };
    }

    private static void parseElements(BufferedReader br, long[] tagToIdx,
            List<int[]> tetList, List<Integer> tagList) throws IOException {
        String header = br.readLine();
        if (header == null) throw new IOException("Unexpected EOF in $Elements header");
        String[] hp = header.trim().split("\\s+");
        int numEntityBlocks = Integer.parseInt(hp[0]);

        for (int b = 0; b < numEntityBlocks; b++) {
            String bh = br.readLine();
            if (bh == null) throw new IOException("Unexpected EOF in $Elements block header");
            String[] bhp = bh.trim().split("\\s+");
            int entityTag   = Integer.parseInt(bhp[1]);
            int elemType    = Integer.parseInt(bhp[2]);
            int nInBlock    = Integer.parseInt(bhp[3]);

            boolean isTet4 = (elemType == 4);

            for (int i = 0; i < nInBlock; i++) {
                String eLine = br.readLine();
                if (eLine == null) throw new IOException("Unexpected EOF reading element");
                if (!isTet4) continue;
                String[] ep = eLine.trim().split("\\s+");
                int[] tet = new int[4];
                for (int v = 0; v < 4; v++) {
                    long nodeTag = Long.parseLong(ep[v + 1]);
                    tet[v] = (int) tagToIdx[(int) nodeTag];
                    if (tet[v] < 0) {
                        throw new IOException("Node tag " + nodeTag + " not found in node table");
                    }
                }
                tetList.add(tet);
                tagList.add(entityTag);
            }
        }

        String endLine = br.readLine();
        if (endLine == null || !endLine.trim().equals("$EndElements")) {
            throw new IOException("Expected $EndElements, got: " + endLine);
        }
    }

    private static void skipUntilEnd(BufferedReader br, String endTag) throws IOException {
        String line;
        while ((line = br.readLine()) != null) {
            if (line.trim().equals(endTag)) return;
        }
        throw new IOException("Unexpected EOF looking for " + endTag);
    }

    // ------------------------------------------------------------------
    // Edge enumeration + PEC mask (mirror of sphere_pec sibling)
    // ------------------------------------------------------------------

    /** Six local-edge vertex pairs, matching TET_LOCAL_EDGES in the NumPy reference. */
    public static final int[][] TET_LOCAL_EDGES = {
        {0, 1}, {0, 2}, {0, 3}, {1, 2}, {1, 3}, {2, 3}
    };

    public static final class EdgeTable {
        /** Global edges, shape [nEdges][2], lo < hi (0-based node indices). */
        public final int[][] edges;
        /** Per-tet global edge indices, shape [nTets][6]. */
        public final int[][] tetEdgeIdx;
        /** Per-tet edge signs (+1/-1), shape [nTets][6]. */
        public final int[][] tetEdgeSign;

        public EdgeTable(int[][] edges, int[][] tetEdgeIdx, int[][] tetEdgeSign) {
            this.edges       = edges;
            this.tetEdgeIdx  = tetEdgeIdx;
            this.tetEdgeSign = tetEdgeSign;
        }
    }

    /**
     * Build globally-oriented edge table + per-tet (index, sign) tables.
     * Mirror of {@code reference/numpy/sphere_pec.py::build_edges}.
     */
    public static EdgeTable buildEdges(int[][] tets) {
        int nTets = tets.length;

        Map<Long, Integer> edgeMap = new java.util.TreeMap<>();
        for (int e = 0; e < nTets; e++) {
            for (int[] localEdge : TET_LOCAL_EDGES) {
                int a = tets[e][localEdge[0]];
                int b = tets[e][localEdge[1]];
                int lo = Math.min(a, b);
                int hi = Math.max(a, b);
                long key = ((long) lo << 20) | hi;
                edgeMap.computeIfAbsent(key, k -> edgeMap.size());
            }
        }

        int nEdges = edgeMap.size();
        int[][] edges = new int[nEdges][2];
        for (Map.Entry<Long, Integer> entry : edgeMap.entrySet()) {
            long key = entry.getKey();
            int idx  = entry.getValue();
            edges[idx][0] = (int) (key >> 20);
            edges[idx][1] = (int) (key & 0xFFFFF);
        }

        int[][] tetEdgeIdx  = new int[nTets][6];
        int[][] tetEdgeSign = new int[nTets][6];
        for (int e = 0; e < nTets; e++) {
            for (int k = 0; k < 6; k++) {
                int a = tets[e][TET_LOCAL_EDGES[k][0]];
                int b = tets[e][TET_LOCAL_EDGES[k][1]];
                int lo = Math.min(a, b);
                int hi = Math.max(a, b);
                long key = ((long) lo << 20) | hi;
                int gIdx = edgeMap.get(key);
                tetEdgeIdx[e][k]  = gIdx;
                tetEdgeSign[e][k] = (a < b) ? 1 : -1;
            }
        }

        return new EdgeTable(edges, tetEdgeIdx, tetEdgeSign);
    }

    // ------------------------------------------------------------------
    // PML-specific: per-tet centroid radii + complex epsilon profile
    // ------------------------------------------------------------------

    /**
     * Per-tet centroid distance from the origin.
     *
     * <p>Mirror of {@code geode_core::tet_centroid_radii} and
     * {@code reference/numpy/sphere_pml.py::tet_centroid_radii}. Used by
     * {@link #buildComplexEpsilonRPml(int[], double[], double, double)}
     * to decide which tets sit in the absorbing shell and how strongly
     * to absorb in each.
     *
     * @param nodes shape [nNodes][3]
     * @param tets  shape [nTets][4]
     * @return per-tet centroid radius, length nTets
     */
    public static double[] tetCentroidRadii(double[][] nodes, int[][] tets) {
        int nTets = tets.length;
        double[] r = new double[nTets];
        for (int e = 0; e < nTets; e++) {
            double cx = 0.0, cy = 0.0, cz = 0.0;
            for (int v = 0; v < 4; v++) {
                int n = tets[e][v];
                cx += nodes[n][0];
                cy += nodes[n][1];
                cz += nodes[n][2];
            }
            cx *= 0.25;
            cy *= 0.25;
            cz *= 0.25;
            r[e] = Math.sqrt(cx * cx + cy * cy + cz * cz);
        }
        return r;
    }

    /**
     * Per-tet complex relative permittivity realizing the scalar-isotropic PML.
     *
     * <p>Mirror of {@code geode_core::build_complex_epsilon_r_pml} and
     * {@code reference/numpy/sphere_pml.py::build_complex_epsilon_r_pml}.
     *
     * <p>Profile:
     * <ul>
     *   <li>Tet in {@code sphere_interior} ({@code PHYS_SPHERE_INTERIOR}):
     *       {@code ε = n_inside² + 0j} (real dielectric).</li>
     *   <li>Tet in {@code vacuum_gap} (any tag except {@code PHYS_PML_SHELL}
     *       and not the dielectric): {@code ε = 1 + 0j} (real vacuum).</li>
     *   <li>Tet in {@code pml_shell} ({@code PHYS_PML_SHELL}): quadratic
     *       absorption ramp anchored at {@code R_PML_INNER},
     *       <pre>
     *         ε(r) = 1 − j σ₀ ((r − R_PML_INNER) / (R_BUFFER − R_PML_INNER))²
     *       </pre>
     *       with the ramp coordinate {@code u} clamped to {@code [0, 1]}.</li>
     * </ul>
     *
     * <p>Sign convention: {@code exp(+jωt)} → outgoing-wave attenuation
     * requires {@code Im(ε) < 0}. The downstream eigensolver canonicalizes
     * the eigenvalue sign to {@code Im(λ) > 0} per PR #155 Judge's
     * binding decision.
     *
     * <p>Returned arrays are real and imaginary parts laid out as parallel
     * {@code double[nTets]} arrays. TF-Java 1.0.0 has no native c128 typed
     * value, so we keep the two parts separate everywhere on the JVM side
     * and let the Python driver fuse them into a {@code complex128} CSR
     * for the eigensolve.
     *
     * @param tetTags  per-tet physical group tags
     * @param radii    per-tet centroid radii (output of {@link #tetCentroidRadii})
     * @param nInside  refractive index inside the dielectric sphere
     * @param sigma0   PML absorption strength at {@code r = R_BUFFER};
     *                 0 collapses the profile to the real PEC ε
     * @return [re, im] arrays each of length nTets
     */
    public static double[][] buildComplexEpsilonRPml(
            int[] tetTags, double[] radii, double nInside, double sigma0) {
        if (tetTags.length != radii.length) {
            throw new IllegalArgumentException(
                    "tetTags and radii length mismatch: "
                    + tetTags.length + " vs " + radii.length);
        }
        int n = tetTags.length;
        double epsInside = nInside * nInside;
        double width = R_BUFFER - R_PML_INNER;

        double[] re = new double[n];
        double[] im = new double[n];

        for (int i = 0; i < n; i++) {
            int tag = tetTags[i];
            if (tag == PHYS_SPHERE_INTERIOR) {
                re[i] = epsInside;
                im[i] = 0.0;
            } else if (tag == PHYS_PML_SHELL) {
                double u = (radii[i] - R_PML_INNER) / width;
                if (u < 0.0) u = 0.0;
                if (u > 1.0) u = 1.0;
                re[i] = 1.0;
                im[i] = -sigma0 * u * u;
            } else {
                re[i] = 1.0;
                im[i] = 0.0;
            }
        }
        return new double[][] { re, im };
    }

    // ------------------------------------------------------------------
    // PEC mask + linear algebra utilities (mirror of sphere_pec sibling)
    // ------------------------------------------------------------------

    /** Boolean interior-edge mask (PEC boundary elimination). */
    public static boolean[] interiorEdgeMask(double[][] nodes, int[][] edges) {
        double tol = 1e-6 * Math.max(R_BUFFER, 1.0);
        boolean[] onBoundary = new boolean[nodes.length];
        for (int i = 0; i < nodes.length; i++) {
            double r = Math.sqrt(
                    nodes[i][0] * nodes[i][0]
                    + nodes[i][1] * nodes[i][1]
                    + nodes[i][2] * nodes[i][2]);
            onBoundary[i] = Math.abs(r - R_BUFFER) < tol;
        }

        boolean[] mask = new boolean[edges.length];
        for (int e = 0; e < edges.length; e++) {
            mask[e] = !(onBoundary[edges[e][0]] && onBoundary[edges[e][1]]);
        }
        return mask;
    }

    /** Count of true entries. */
    public static int countTrue(boolean[] b) {
        int c = 0;
        for (boolean v : b) if (v) c++;
        return c;
    }

    /** Indices where mask is true. */
    public static int[] whereTrue(boolean[] b) {
        List<Integer> out = new ArrayList<>();
        for (int i = 0; i < b.length; i++) if (b[i]) out.add(i);
        int[] arr = new int[out.size()];
        for (int i = 0; i < arr.length; i++) arr[i] = out.get(i);
        return arr;
    }

    /** Extract submatrix at rows and cols given by idx. */
    public static double[][] submatrix(double[][] mat, int[] idx) {
        int n = idx.length;
        double[][] out = new double[n][n];
        for (int i = 0; i < n; i++) {
            for (int j = 0; j < n; j++) {
                out[i][j] = mat[idx[i]][idx[j]];
            }
        }
        return out;
    }

    /** Matrix trace. */
    public static double trace(double[][] m) {
        double t = 0.0;
        for (int i = 0; i < m.length; i++) t += m[i][i];
        return t;
    }
}
