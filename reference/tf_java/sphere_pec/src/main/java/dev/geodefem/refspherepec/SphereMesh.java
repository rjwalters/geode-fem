package dev.geodefem.refspherepec;

import java.io.BufferedReader;
import java.io.FileReader;
import java.io.IOException;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

/**
 * Minimal Gmsh MSH 4.x parser for the sphere PEC fixture.
 *
 * <p>Reads the bundled {@code reference/fixtures/sphere_pec/sphere.msh}
 * file (MSH format 4.1, ASCII) and extracts:
 * <ul>
 *   <li>Node coordinates: shape {@code [nNodes][3]}, 0-based indexing.</li>
 *   <li>Tet connectivity: shape {@code [nTets][4]}, 0-based, from all
 *       {@code Tetrahedron} blocks.</li>
 *   <li>Per-tet physical group tags: shape {@code [nTets]}, from the
 *       {@code $PhysicalNames} and {@code $Elements} / {@code $Entities}
 *       metadata.</li>
 * </ul>
 *
 * <p>This is a correctness-anchoring reference, not a production mesh
 * library. The parser covers the exact dialect produced by Gmsh 4.x for
 * the bundled sphere fixture; unknown section types are skipped.
 *
 * <p>Physical-group tag assignment mirrors
 * {@code reference/numpy/sphere_pec.py::read_sphere_fixture}: the
 * {@code gmsh:physical} tag for each element block is the tag of the
 * entity that block belongs to, per the MSH4 element block header
 * {@code (entity_dim entity_tag element_type n_elements)}.
 *
 * <p>Decision: mesh construction does NOT go through TF-Java. The
 * symbolic-graph assembly path consumes the resulting {@code nodes} and
 * {@code tets} arrays as inputs to the Nédélec local matrix kernel. This
 * matches the JAX/Julia/NumPy pattern.
 */
public final class SphereMesh {

    private SphereMesh() {}

    // Physical-group tags — mirror of geode_core::mesh::sphere::PHYS_*.
    public static final int PHYS_SPHERE_INTERIOR = 1; // tets in r <= R_SPHERE
    public static final int PHYS_VACUUM_GAP      = 2; // tets in R_SPHERE < r <= R_PML_INNER
    public static final int PHYS_PML_SHELL       = 5; // tets in R_PML_INNER < r <= R_BUFFER

    public static final double R_BUFFER = 2.0; // outer PEC wall radius

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
    // Parser internals
    // ------------------------------------------------------------------

    private static Mesh parse(BufferedReader br) throws IOException {
        // We read section by section in a top-level loop.
        double[][] nodes   = null;
        long[] nodeGmshIds = null; // Gmsh 1-based node IDs -> 0-based index map
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
                    // Skip unknown or unneeded sections.
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

        System.out.printf("[sphere-pec] Parsed mesh: %d nodes, %d tets%n",
                nodes.length, tets.length);
        return new Mesh(nodes, tets, tetTags);
    }

    /**
     * Parse the {@code $Nodes} section.
     *
     * <p>MSH4 format:
     * <pre>
     * $Nodes
     * numEntityBlocks numNodes minNodeTag maxNodeTag
     * entityDim entityTag parametric numNodesInBlock
     * nodeTag...     (one per line, numNodesInBlock lines)
     * x y z...       (one per line, numNodesInBlock lines)
     * ...
     * $EndNodes
     * </pre>
     *
     * @return {@code [nodes (double[][]), nodeGmshIds (long[])]}
     */
    private static Object[] parseNodes(BufferedReader br) throws IOException {
        // Header: numEntityBlocks totalNodes minTag maxTag
        String header = br.readLine();
        if (header == null) throw new IOException("Unexpected EOF in $Nodes header");
        String[] hp = header.trim().split("\\s+");
        int numEntityBlocks = Integer.parseInt(hp[0]);
        int totalNodes      = Integer.parseInt(hp[1]);

        double[][] nodes    = new double[totalNodes][3];
        long[] gmshIds      = new long[totalNodes]; // index i -> Gmsh node tag
        // We also need a reverse map: Gmsh node tag -> 0-based index.
        // Build it as we go, using a simple array (tags may be sparse in general,
        // but for the sphere fixture they are compact 1..nNodes).
        long maxTag = Long.parseLong(hp[3]);
        // Allocate reverse map for tag -> 0-based index.
        long[] tagToIdx = new long[(int)(maxTag + 1)]; // index by tag
        Arrays.fill(tagToIdx, -1);

        int ptr = 0; // next free slot in nodes[]
        for (int b = 0; b < numEntityBlocks; b++) {
            String bh = br.readLine();
            if (bh == null) throw new IOException("Unexpected EOF in $Nodes block header");
            String[] bhp = bh.trim().split("\\s+");
            // entityDim entityTag parametric numNodesInBlock
            int nInBlock = Integer.parseInt(bhp[3]);

            // Read node tags first.
            long[] blockTags = new long[nInBlock];
            for (int i = 0; i < nInBlock; i++) {
                String tagLine = br.readLine();
                if (tagLine == null) throw new IOException("Unexpected EOF reading node tags");
                blockTags[i] = Long.parseLong(tagLine.trim());
            }
            // Read coordinates.
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

        // Return both the node array and the reverse-lookup array.
        return new Object[] { nodes, tagToIdx };
    }

    /**
     * Parse the {@code $Elements} section and collect tetrahedra.
     *
     * <p>MSH4 format:
     * <pre>
     * $Elements
     * numEntityBlocks numElements minElemTag maxElemTag
     * entityDim entityTag elementType numElementsInBlock
     * elemTag nodeTag...    (one per line)
     * ...
     * $EndElements
     * </pre>
     *
     * <p>Element type 4 = 4-node tetrahedron (Tet4). For each tet block the
     * geometric entity tag is resolved to the physical-group tag via the
     * {@code $Entities} map ({@link #parseEntities}); this matches the
     * per-tet {@code gmsh:physical} cell-data that
     * {@code reference/numpy/sphere_pec.py::read_sphere_fixture} consumes.
     *
     * <p>If a tet block's entity is absent from the map (no $Entities
     * section, or no physical-group assignment), the block is collected
     * with the raw entity tag as a fallback.
     */
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
            // entityDim entityTag elementType numElementsInBlock
            int entityDim  = Integer.parseInt(bhp[0]);
            int entityTag  = Integer.parseInt(bhp[1]);
            int elemType   = Integer.parseInt(bhp[2]);
            int nInBlock   = Integer.parseInt(bhp[3]);

            boolean isTet4 = (elemType == 4); // Gmsh element type 4 = 4-node tet

            // Resolve the physical-group tag from $Entities. The MSH4 block
            // header carries the geometric entity tag, not the physical
            // tag; meshio's `gmsh:physical` (what the NumPy/Burn sides
            // consume) is the physical-group tag. Fallback to entityTag
            // preserves the single-physical-group shortcut for fixtures
            // where the two coincide.
            int physicalTag = entityTag;
            if (isTet4 && entityDim == 3) {
                Integer mapped = volEntityToPhysical.get(entityTag);
                if (mapped != null) physicalTag = mapped;
            }

            for (int i = 0; i < nInBlock; i++) {
                String eLine = br.readLine();
                if (eLine == null) throw new IOException("Unexpected EOF reading element");
                if (!isTet4) continue; // skip non-tet blocks
                String[] ep = eLine.trim().split("\\s+");
                // ep[0] = elem tag, ep[1..4] = node tags (Gmsh 1-based)
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
    // Edge enumeration + PEC mask — JVM side (mirrors numpy/sphere_pec.py)
    // ------------------------------------------------------------------

    /** Six local-edge vertex pairs, matching TET_LOCAL_EDGES in the NumPy reference. */
    public static final int[][] TET_LOCAL_EDGES = {
        {0, 1}, {0, 2}, {0, 3}, {1, 2}, {1, 3}, {2, 3}
    };

    /** Result of edge enumeration. */
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
     *
     * <p>Mirror of {@code reference/numpy/sphere_pec.py::build_edges}.
     * Edges are deduplicated and sorted lexicographically (lower-node-tag
     * first), matching the NumPy reference's {@code np.unique} approach.
     *
     * @param tets shape [nTets][4]
     * @return {@link EdgeTable}
     */
    public static EdgeTable buildEdges(int[][] tets) {
        int nTets = tets.length;

        // 1. Collect all (lo, hi) pairs — one per local edge per tet.
        //    Use a sorted set of (lo*offset + hi) longs for dedup.
        //    Maximum node count for this fixture is ~800, so a 32-bit key
        //    with offset = 2^20 works fine. Use a Map<Long, Integer> to
        //    get a stable insertion order for the unique edges.
        Map<Long, Integer> edgeMap = new java.util.TreeMap<>(); // sorted by key = lexicographic
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

        // 2. Build the edge array from the map (TreeMap is sorted → lexicographic).
        int nEdges = edgeMap.size();
        int[][] edges = new int[nEdges][2];
        for (Map.Entry<Long, Integer> entry : edgeMap.entrySet()) {
            long key = entry.getKey();
            int idx  = entry.getValue();
            edges[idx][0] = (int) (key >> 20);
            edges[idx][1] = (int) (key & 0xFFFFF);
        }

        // 3. Build per-tet (index, sign) arrays.
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

    /**
     * Per-tet relative permittivity assignment.
     *
     * <p>Mirror of {@code reference/numpy/sphere_pec.py::build_epsilon_r}.
     * Tets tagged {@code PHYS_SPHERE_INTERIOR} get {@code n_inside²};
     * all others get {@code 1.0}.
     *
     * @param tetTags per-tet physical group tags
     * @param nInside refractive index inside the sphere
     * @return per-tet epsilon_r
     */
    public static double[] buildEpsilonR(int[] tetTags, double nInside) {
        double epsInside = nInside * nInside;
        double[] eps = new double[tetTags.length];
        for (int i = 0; i < tetTags.length; i++) {
            eps[i] = (tetTags[i] == PHYS_SPHERE_INTERIOR) ? epsInside : 1.0;
        }
        return eps;
    }

    /**
     * Boolean interior-edge mask (PEC boundary elimination).
     *
     * <p>Mirror of
     * {@code reference/numpy/sphere_pec.py::sphere_pec_interior_edges}:
     * an edge is interior iff at least one endpoint is NOT on the outer
     * PEC sphere ({@code |r| ≈ R_BUFFER}).
     *
     * @param nodes shape [nNodes][3]
     * @param edges shape [nEdges][2]
     * @return boolean mask, length nEdges
     */
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
            // Interior = at least one endpoint is NOT on the outer wall.
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
