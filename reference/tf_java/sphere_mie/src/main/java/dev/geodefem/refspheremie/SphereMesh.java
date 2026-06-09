package dev.geodefem.refspheremie;

import java.io.BufferedReader;
import java.io.FileReader;
import java.io.IOException;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

/**
 * Minimal Gmsh MSH 4.x parser + edge / centroid / anisotropic-tensor-ε
 * helpers for the sphere-Mie (anisotropic UPML) fixture.
 *
 * <p>Mirror of {@code reference/tf_java/sphere_pml/.../SphereMesh.java}
 * (Phase H.4) with the scalar-isotropic PML profile replaced by the
 * anisotropic UPML pieces:
 * <ul>
 *   <li>{@link #tetCentroids(double[][], int[][])} — per-tet centroid
 *       <em>vector</em> (not just the radius — the tensor builder needs
 *       the radial direction). Mirror of {@code geode_core::tet_centroids}
 *       and {@code reference/numpy/sphere_mie.py::tet_centroids}.</li>
 *   <li>{@link #buildAnisotropicPmlTensorDiag(int[], double[][], double, double, double)}
 *       — per-tet diagonal complex permittivity tensor
 *       {@code (ε_x, ε_y, ε_z)} realizing the simplified Sacks UPML.
 *       Mirror of {@code geode_core::build_anisotropic_pml_tensor_diag} /
 *       {@code reference/numpy/sphere_mie.py::build_anisotropic_pml_tensor_diag}.</li>
 * </ul>
 *
 * <p>Mesh I/O, edge enumeration, and PEC mask are unchanged from the
 * sphere-PML sibling — the Mie problem differs only in the per-tet
 * constitutive scaling on the mass (a diagonal tensor instead of a
 * scalar). The bundled small mesh
 * ({@code reference/fixtures/sphere_pml_small/sphere.msh}, 48 nodes,
 * 197 tets) is shared with the #158 sphere_pml_small fixture.
 *
 * <p>Decision: mesh construction and the tensor-ε profile do NOT go
 * through TF-Java. The symbolic-graph assembly path consumes the
 * resulting {@code nodes}, {@code tets}, and per-tet
 * {@code epsilonRe / epsilonIm} {@code [nTets][3]} arrays as inputs.
 * This matches the JAX/Julia/NumPy pattern.
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

    /**
     * Reference wavenumber used by the anisotropic UPML stretching
     * profile. Mirror of {@code K0_REF} in
     * {@code crates/geode-core/tests/mie_sphere.rs} and
     * {@code reference/numpy/sphere_mie.py}.
     */
    public static final double K0_REF = 2.0;

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
    // Parser internals (identical to the sphere_pml sibling)
    // ------------------------------------------------------------------

    private static Mesh parse(BufferedReader br) throws IOException {
        double[][] nodes   = null;
        long[] nodeGmshIds = null;
        Map<Integer, Integer> volEntityToPhysical = new HashMap<>();
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
                    volEntityToPhysical = parseEntities(br);
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
                    parseElements(br, nodeGmshIds, volEntityToPhysical, tetList, tagList);
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

        System.out.printf("[sphere-mie] Parsed mesh: %d nodes, %d tets%n",
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
            Map<Integer, Integer> volEntityToPhysical,
            List<int[]> tetList, List<Integer> tagList) throws IOException {
        String header = br.readLine();
        if (header == null) throw new IOException("Unexpected EOF in $Elements header");
        String[] hp = header.trim().split("\\s+");
        int numEntityBlocks = Integer.parseInt(hp[0]);

        for (int b = 0; b < numEntityBlocks; b++) {
            String bh = br.readLine();
            if (bh == null) throw new IOException("Unexpected EOF in $Elements block header");
            String[] bhp = bh.trim().split("\\s+");
            int entityDim  = Integer.parseInt(bhp[0]);
            int entityTag  = Integer.parseInt(bhp[1]);
            int elemType   = Integer.parseInt(bhp[2]);
            int nInBlock   = Integer.parseInt(bhp[3]);

            boolean isTet4 = (elemType == 4);

            // Resolve the per-tet physical-group tag via the $Entities map.
            // MSH4 element blocks carry the geometric entity tag, not the
            // physical group tag — meshio's `gmsh:physical` is what the
            // NumPy/Burn sides consume, so we mirror that semantics here.
            // Fallback to entityTag preserves the legacy single-physical-
            // group shortcut for meshes without an $Entities section.
            int physicalTag = entityTag;
            if (isTet4 && entityDim == 3) {
                Integer mapped = volEntityToPhysical.get(entityTag);
                if (mapped != null) physicalTag = mapped;
            }

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
                tagList.add(physicalTag);
            }
        }

        String endLine = br.readLine();
        if (endLine == null || !endLine.trim().equals("$EndElements")) {
            throw new IOException("Expected $EndElements, got: " + endLine);
        }
    }

    /**
     * Parse the {@code $Entities} section and return a
     * {@code volumeEntityTag → physicalGroupTag} map for 3-D entities.
     *
     * <p>MSH 4.x volume entity rows have the layout:
     * <pre>
     * volTag xMin yMin zMin xMax yMax zMax numPhysicalTags physTag1 ... numBoundingSurfaces ...
     * </pre>
     *
     * <p>The bundled sphere fixtures assign exactly one physical-group tag
     * per volume; only the first physical tag is recorded. Volumes with
     * zero physical tags are skipped.
     */
    private static Map<Integer, Integer> parseEntities(BufferedReader br) throws IOException {
        String header = br.readLine();
        if (header == null) throw new IOException("Unexpected EOF in $Entities header");
        String[] hp = header.trim().split("\\s+");
        int numPoints   = Integer.parseInt(hp[0]);
        int numCurves   = Integer.parseInt(hp[1]);
        int numSurfaces = Integer.parseInt(hp[2]);
        int numVolumes  = Integer.parseInt(hp[3]);

        int subVolumeRows = numPoints + numCurves + numSurfaces;
        for (int i = 0; i < subVolumeRows; i++) {
            if (br.readLine() == null) {
                throw new IOException("Unexpected EOF in $Entities sub-volume section");
            }
        }

        Map<Integer, Integer> map = new HashMap<>();
        for (int i = 0; i < numVolumes; i++) {
            String line = br.readLine();
            if (line == null) throw new IOException("Unexpected EOF in $Entities volume row");
            String[] p = line.trim().split("\\s+");
            int volTag  = Integer.parseInt(p[0]);
            int numPhys = Integer.parseInt(p[7]);
            if (numPhys >= 1) {
                int physTag = Integer.parseInt(p[8]);
                map.put(volTag, physTag);
            }
        }

        String endLine = br.readLine();
        if (endLine == null || !endLine.trim().equals("$EndEntities")) {
            throw new IOException("Expected $EndEntities, got: " + endLine);
        }
        return map;
    }

    private static void skipUntilEnd(BufferedReader br, String endTag) throws IOException {
        String line;
        while ((line = br.readLine()) != null) {
            if (line.trim().equals(endTag)) return;
        }
        throw new IOException("Unexpected EOF looking for " + endTag);
    }

    // ------------------------------------------------------------------
    // Edge enumeration + PEC mask (mirror of sphere_pml sibling)
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
    // Mie-specific: per-tet centroid vectors + anisotropic UPML tensor
    // ------------------------------------------------------------------

    /**
     * Per-tet centroid position vectors, shape {@code [nTets][3]}.
     *
     * <p>Companion to the sphere-PML sibling's {@code tetCentroidRadii}
     * for callers that need the full vector centroid — the anisotropic
     * tensor builder needs the radial <em>direction</em>, not just its
     * magnitude. Mirror of {@code geode_core::tet_centroids} and
     * {@code reference/numpy/sphere_mie.py::tet_centroids}.
     *
     * @param nodes shape [nNodes][3]
     * @param tets  shape [nTets][4]
     * @return per-tet centroid vector, shape [nTets][3]
     */
    public static double[][] tetCentroids(double[][] nodes, int[][] tets) {
        int nTets = tets.length;
        double[][] c = new double[nTets][3];
        for (int e = 0; e < nTets; e++) {
            double cx = 0.0, cy = 0.0, cz = 0.0;
            for (int v = 0; v < 4; v++) {
                int n = tets[e][v];
                cx += nodes[n][0];
                cy += nodes[n][1];
                cz += nodes[n][2];
            }
            c[e][0] = cx * 0.25;
            c[e][1] = cy * 0.25;
            c[e][2] = cz * 0.25;
        }
        return c;
    }

    /**
     * Per-tet <b>diagonal anisotropic</b> complex permittivity tensor.
     *
     * <p>Line-for-line mirror of
     * {@code geode_core::build_anisotropic_pml_tensor_diag} (issue #54)
     * and {@code reference/numpy/sphere_mie.py::build_anisotropic_pml_tensor_diag}:
     * <ul>
     *   <li>Tet in {@code sphere_interior} ({@code PHYS_SPHERE_INTERIOR}):
     *       real isotropic {@code (n², n², n²)}.</li>
     *   <li>Tet in {@code vacuum_gap} (any tag other than
     *       {@code PHYS_PML_SHELL}), or a PML-shell tet whose centroid
     *       sits at {@code r_c ≤ R_PML_INNER}: real isotropic
     *       {@code (1, 1, 1)}.</li>
     *   <li>Tet in {@code pml_shell} with {@code r_c > R_PML_INNER}: the
     *       simplified Sacks UPML with
     *       {@code s_r = s_t = s = 1 − jσ(r_c)/ω},
     *       <pre>
     *         σ(r_c) = σ₀ · clamp((r_c − R_PML_INNER) / (R_BUFFER − R_PML_INNER), 0, 1)²
     *         ε_α    = bg · ((1/s) r̂_α² + s (1 − r̂_α²)),    r̂ = c / |c|
     *       </pre>
     *       where {@code bg} is the background scalar (n² in the
     *       dielectric, 1 elsewhere; the shell carries bg = 1 on the
     *       bundled fixtures).</li>
     * </ul>
     *
     * <p>{@code ω} is approximated by {@code k0Ref}
     * ({@code max(k0Ref, 1e-12)}), the reference-wavenumber heuristic
     * shared with Silver-Müller.
     *
     * <p>Sign convention: {@code exp(+jωt)} → {@code Im(ε) < 0} in the
     * shell on the transverse entries (which carry {@code s}); the
     * radial entry carries {@code 1/s} with {@code Im > 0}. The net
     * eigenvalue sign on the small-mesh tensor pencil is
     * {@code Im(λ) < 0} — a property of the pencil, not a solver choice
     * (see the J.2 fixture description).
     *
     * <p>Returned tensors are real and imaginary parts laid out as
     * parallel {@code double[nTets][3]} arrays. TF-Java 1.0.0 has no
     * native c128 typed value, so we keep the two parts separate
     * everywhere on the JVM side and let the Python driver fuse them
     * into a {@code complex128} pencil for the eigensolve.
     *
     * @param tetTags   per-tet physical group tags
     * @param centroids per-tet centroid vectors (output of {@link #tetCentroids})
     * @param nInside   refractive index inside the dielectric sphere
     * @param sigma0    PML absorption strength at {@code r = R_BUFFER};
     *                  0 collapses the tensor to the real isotropic scalar ε
     * @param k0Ref     reference wavenumber ω heuristic (default {@link #K0_REF})
     * @return [re, im] arrays each of shape [nTets][3] (ε_x, ε_y, ε_z per tet)
     */
    public static double[][][] buildAnisotropicPmlTensorDiag(
            int[] tetTags, double[][] centroids,
            double nInside, double sigma0, double k0Ref) {
        if (tetTags.length != centroids.length) {
            throw new IllegalArgumentException(
                    "tetTags and centroids length mismatch: "
                    + tetTags.length + " vs " + centroids.length);
        }
        int n = tetTags.length;
        double epsInside = nInside * nInside;
        double width = R_BUFFER - R_PML_INNER;
        double omega = Math.max(k0Ref, 1e-12);

        double[][] re = new double[n][3];
        double[][] im = new double[n][3];

        for (int i = 0; i < n; i++) {
            int tag = tetTags[i];
            // Background scalar: n² in the dielectric, 1 elsewhere.
            double bg = (tag == PHYS_SPHERE_INTERIOR) ? epsInside : 1.0;

            double cx = centroids[i][0];
            double cy = centroids[i][1];
            double cz = centroids[i][2];
            double rc = Math.sqrt(cx * cx + cy * cy + cz * cz);

            if (tag == PHYS_PML_SHELL && rc > R_PML_INNER) {
                // PML shell with centroid strictly past R_PML_INNER.
                double u = (rc - R_PML_INNER) / width;
                if (u < 0.0) u = 0.0;
                if (u > 1.0) u = 1.0;
                double sigma = sigma0 * u * u;

                // s = 1 - jσ/ω (complex). s_inv = conj(s) / |s|².
                double sRe = 1.0;
                double sIm = -sigma / omega;
                double sAbs2 = sRe * sRe + sIm * sIm;
                double sInvRe = sRe / sAbs2;
                double sInvIm = -sIm / sAbs2;

                // Radial unit vector at the centroid (guarded |c| ≈ 0,
                // matching the Burn-side defensive branch).
                double invR = (rc > 1e-12) ? 1.0 / rc : 0.0;
                double rx = cx * invR;
                double ry = cy * invR;
                double rz = cz * invR;
                double[] w = { rx * rx, ry * ry, rz * rz }; // r̂_α²

                // ε_α = bg · (s_inv r̂_α² + s (1 − r̂_α²))
                for (int axis = 0; axis < 3; axis++) {
                    double wa = w[axis];
                    re[i][axis] = bg * (sInvRe * wa + sRe * (1.0 - wa));
                    im[i][axis] = bg * (sInvIm * wa + sIm * (1.0 - wa));
                }
            } else {
                // Real isotropic default: interior, vacuum gap, and the
                // defensive r_c <= R_PML_INNER guard inside the shell.
                for (int axis = 0; axis < 3; axis++) {
                    re[i][axis] = bg;
                    im[i][axis] = 0.0;
                }
            }
        }
        return new double[][][] { re, im };
    }

    // ------------------------------------------------------------------
    // PEC mask + linear algebra utilities (mirror of sphere_pml sibling)
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
