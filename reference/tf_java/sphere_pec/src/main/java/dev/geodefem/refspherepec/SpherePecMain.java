package dev.geodefem.refspherepec;

import com.fasterxml.jackson.databind.ObjectMapper;
import com.fasterxml.jackson.databind.SerializationFeature;
import java.io.File;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.concurrent.Callable;
import picocli.CommandLine;
import picocli.CommandLine.Option;

/**
 * Driver for the TF-Java sphere-PEC Nédélec reference (Epic #88 / #134).
 *
 * <p>Runs the Gmsh MSH4 mesh parser, builds the global edge table, computes
 * per-tet epsilon_r, runs the static-graph Nédélec assembly via
 * {@link NedelecAssemblyGraph}, applies PEC boundary elimination on the JVM
 * side (dropping edges with both endpoints on the outer sphere at
 * {@code r = R_BUFFER = 2.0}), and dumps the reduced {@code (K_int, M_int)}
 * matrices to a schema-v1 JSON sidecar. A companion Python driver
 * ({@code reference/driver/eigensolve_from_sidecar.py --backend tfjava})
 * consumes the sidecar and calls SciPy's eigensolver.
 *
 * <p>This split — TF-Java does the Nédélec assembly, SciPy does the
 * eigensolve — is the "TF-Java cannot natively close the spine" friction
 * artifact called out in the Epic #88 framing.
 */
public class SpherePecMain implements Callable<Integer> {

    @Option(names = "--mesh",
            description = "Path to the sphere.msh fixture (default: ../../fixtures/sphere_pec/sphere.msh).")
    private String meshPath = "../../fixtures/sphere_pec/sphere.msh";

    @Option(names = "--n-index",
            description = "Refractive index inside the dielectric sphere (default 1.5).")
    private double nIndex = 1.5;

    @Option(names = "--r-buffer",
            description = "Outer PEC wall radius (default 2.0).")
    private double rBuffer = 2.0;

    @Option(names = "--out",
            description = "Output JSON path for the reduced (K_int, M_int) sidecar.")
    private String out = "reduced_kM_sphere_pec.json";

    @Override
    public Integer call() throws Exception {
        System.out.printf("[tfjava-sphere-pec] mesh=%s, n_index=%g, r_buffer=%g%n",
                meshPath, nIndex, rBuffer);

        // ----- 1. Mesh I/O -----
        SphereMesh.Mesh mesh = SphereMesh.read(meshPath);
        int nNodes = mesh.nodes.length;
        int nTets  = mesh.tets.length;
        System.out.printf("[tfjava-sphere-pec] nNodes=%d, nTets=%d%n", nNodes, nTets);

        // ----- 2. ε_r assignment -----
        double[] epsilonR = SphereMesh.buildEpsilonR(mesh.tetTags, nIndex);

        // ----- 3. Edge enumeration -----
        SphereMesh.EdgeTable et = SphereMesh.buildEdges(mesh.tets);
        int nEdges = et.edges.length;
        System.out.printf("[tfjava-sphere-pec] nEdges=%d%n", nEdges);

        // ----- 4. PEC mask -----
        boolean[] interiorMask = SphereMesh.interiorEdgeMask(mesh.nodes, et.edges);
        int nInt = SphereMesh.countTrue(interiorMask);
        int[] interiorIdx = SphereMesh.whereTrue(interiorMask);
        System.out.printf("[tfjava-sphere-pec] nInt=%d (PEC reduced from %d)%n", nInt, nEdges);

        // ----- 5. Assemble K, M via the static graph -----
        System.out.println("[tfjava-sphere-pec] Building TF-Java assembly graph...");
        double[][] kGlobal;
        double[][] mGlobal;
        try (NedelecAssemblyGraph asm = new NedelecAssemblyGraph(
                mesh.tets, et.tetEdgeIdx, et.tetEdgeSign, nNodes, nEdges)) {
            System.out.println("[tfjava-sphere-pec] Running assembly (this may take a moment)...");
            double[][][] result = asm.assemble(mesh.nodes, epsilonR);
            kGlobal = result[0];
            mGlobal = result[1];
        }
        System.out.println("[tfjava-sphere-pec] Assembly complete.");

        // ----- 6. Apply PEC BC (drop boundary rows/cols) -----
        double[][] kInt = SphereMesh.submatrix(kGlobal, interiorIdx);
        double[][] mInt = SphereMesh.submatrix(mGlobal, interiorIdx);

        // ----- Quick numerical sanity readouts -----
        double trK = SphereMesh.trace(kInt);
        double trM = SphereMesh.trace(mInt);
        double frobK = frobenius(kInt);
        double frobM = frobenius(mInt);
        System.out.printf("[tfjava-sphere-pec] trace(K_int)    = %.12e%n", trK);
        System.out.printf("[tfjava-sphere-pec] trace(M_int)    = %.12e%n", trM);
        System.out.printf("[tfjava-sphere-pec] Frobenius(K_int) = %.12e%n", frobK);
        System.out.printf("[tfjava-sphere-pec] Frobenius(M_int) = %.12e%n", frobM);

        // ----- Dump fixture sidecar -----
        Map<String, Object> fixture = buildFixtureMap(
                nNodes, nTets, nEdges, nInt, nIndex, rBuffer,
                interiorIdx, kInt, mInt, trK, trM, frobK, frobM);

        ObjectMapper mapper = new ObjectMapper();
        mapper.enable(SerializationFeature.INDENT_OUTPUT);
        File outFile = new File(out);
        mapper.writeValue(outFile, fixture);
        System.out.printf("[tfjava-sphere-pec] Wrote %s%n", outFile.getAbsolutePath());
        return 0;
    }

    // ------------------------------------------------------------------
    // Fixture JSON construction
    // ------------------------------------------------------------------

    private static Map<String, Object> buildFixtureMap(
            int nNodes, int nTets, int nEdges, int nInt,
            double nIndex, double rBuffer,
            int[] interiorIdx, double[][] kInt, double[][] mInt,
            double trK, double trM, double frobK, double frobM) {

        Map<String, Object> fixture = new LinkedHashMap<>();
        fixture.put("schema_version", "1");
        fixture.put("fixture_id", "sphere_pec/n774_pec_eigenmode_tfjava");
        fixture.put("description",
                "TF-Java static-graph reference for the vector-Nédélec sphere-PEC "
                + "eigenmode pipeline (Epic #88 / #134). Assembles the Nédélec "
                + "curl-curl K and ε-scaled mass M for all global edges, then "
                + "applies PEC BC to produce K_int and M_int. The eigensolve is "
                + "delegated to SciPy via reference/driver/eigensolve_from_sidecar.py "
                + "(TF-Java cannot natively close the spine).");
        fixture.put("units",
                "lambda = k^2 (inverse-length squared); dimensionless mesh coordinates");

        // Inputs.
        Map<String, Object> inputs = new LinkedHashMap<>();
        inputs.put("mesh_path", field(new int[]{1}, "str",
                "Path to the bundled sphere.msh fixture.",
                new String[]{"reference/fixtures/sphere_pec/sphere.msh"}));
        inputs.put("n_index", field(new int[]{1}, "f64",
                "Refractive index inside the dielectric sphere; epsilon_r = n^2 inside.",
                new double[]{nIndex}));
        inputs.put("r_buffer", field(new int[]{1}, "f64",
                "Outer PEC wall radius (= R_BUFFER = 2.0).",
                new double[]{rBuffer}));
        inputs.put("n_int", field(new int[]{1}, "i64",
                "Number of interior edges (DOFs after PEC elimination).",
                new int[]{nInt}));
        inputs.put("interior_idx", field(new int[]{nInt}, "i64",
                "Interior-DOF row/col indices into the full (nEdges, nEdges) matrix.",
                toLong(interiorIdx)));
        // Include n and side as placeholders so eigensolve_from_sidecar.py can parse them.
        // The sphere problem doesn't have these, but the sidecar driver reads them.
        // We repurpose "n" as nInt and "side" as r_buffer for compatibility.
        inputs.put("n", field(new int[]{1}, "i64",
                "Interior DOF count (= n_int). Alias for eigensolve_from_sidecar.py compatibility.",
                new int[]{nInt}));
        inputs.put("side", field(new int[]{1}, "f64",
                "Outer PEC wall radius (= r_buffer). Alias for sidecar driver compatibility.",
                new double[]{rBuffer}));
        fixture.put("inputs", inputs);

        // Outputs.
        Map<String, Object> outputs = new LinkedHashMap<>();
        outputs.put("n_nodes", outputField(new int[]{1}, "f64",
                "Number of mesh nodes. Strict equality.",
                new double[]{nNodes}, 0.5));
        outputs.put("n_tets", outputField(new int[]{1}, "f64",
                "Number of mesh tets. Strict equality.",
                new double[]{nTets}, 0.5));
        outputs.put("n_edges", outputField(new int[]{1}, "f64",
                "Total global edge count (before PEC elimination).",
                new double[]{nEdges}, 0.5));
        outputs.put("n_interior_edges", outputField(new int[]{1}, "f64",
                "Interior edge count (DOFs after PEC elimination).",
                new double[]{nInt}, 0.5));
        outputs.put("k_diag_sum", outputField(new int[]{1}, "f64",
                "trace(K_int) — TF-Java assembly readback.",
                new double[]{trK}, 1.0e-6));
        outputs.put("m_diag_sum", outputField(new int[]{1}, "f64",
                "trace(M_int) — TF-Java assembly readback.",
                new double[]{trM}, 1.0e-6));
        outputs.put("k_int_frobenius", outputField(new int[]{1}, "f64",
                "Frobenius norm of K_int.",
                new double[]{frobK}, 1.0e-6));
        outputs.put("m_int_frobenius", outputField(new int[]{1}, "f64",
                "Frobenius norm of M_int.",
                new double[]{frobM}, 1.0e-6));
        outputs.put("k_int_diag", outputField(new int[]{nInt}, "f64",
                "Diagonal of K_int (per-DOF stiffness).",
                diagVector(kInt), 1.0e-8));
        outputs.put("m_int_diag", outputField(new int[]{nInt}, "f64",
                "Diagonal of M_int (per-DOF mass).",
                diagVector(mInt), 1.0e-8));
        outputs.put("k_int", outputField(new int[]{nInt, nInt}, "f64",
                "Dirichlet-reduced Nédélec curl-curl stiffness matrix.",
                flatten(kInt), 1.0e-8));
        outputs.put("m_int", outputField(new int[]{nInt, nInt}, "f64",
                "Dirichlet-reduced ε-scaled Nédélec mass matrix.",
                flatten(mInt), 1.0e-8));
        fixture.put("outputs", outputs);

        // Provenance.
        Map<String, Object> provenance = new LinkedHashMap<>();
        provenance.put("source", "reference/tf_java/sphere_pec (Epic #88 / #134)");
        provenance.put("verified_against",
                "reference/numpy/sphere_pec.py and reference/fixtures/sphere_pec/baseline.json");
        provenance.put("issue", "#134");
        fixture.put("provenance", provenance);

        return fixture;
    }

    // ------------------------------------------------------------------
    // Fixture field helpers
    // ------------------------------------------------------------------

    private static Map<String, Object> field(int[] shape, String dtype, String description,
                                              Object data) {
        Map<String, Object> f = new LinkedHashMap<>();
        f.put("shape", shape);
        f.put("dtype", dtype);
        f.put("description", description);
        f.put("data", data);
        return f;
    }

    private static Map<String, Object> outputField(int[] shape, String dtype, String description,
                                                    Object data, double tol) {
        Map<String, Object> f = field(shape, dtype, description, data);
        f.put("tolerance_abs", tol);
        return f;
    }

    // ------------------------------------------------------------------
    // Math helpers
    // ------------------------------------------------------------------

    private static double frobenius(double[][] m) {
        double s = 0.0;
        for (double[] row : m) {
            for (double v : row) {
                s += v * v;
            }
        }
        return Math.sqrt(s);
    }

    private static double[] diagVector(double[][] m) {
        double[] d = new double[m.length];
        for (int i = 0; i < m.length; i++) d[i] = m[i][i];
        return d;
    }

    private static double[] flatten(double[][] m) {
        int rows = m.length;
        int cols = rows == 0 ? 0 : m[0].length;
        double[] out = new double[rows * cols];
        for (int i = 0; i < rows; i++) {
            System.arraycopy(m[i], 0, out, i * cols, cols);
        }
        return out;
    }

    private static long[] toLong(int[] xs) {
        long[] out = new long[xs.length];
        for (int i = 0; i < xs.length; i++) out[i] = xs[i];
        return out;
    }

    public static void main(String[] args) {
        int exit = new CommandLine(new SpherePecMain()).execute(args);
        System.exit(exit);
    }
}
