package dev.geodefem.refcubecavity;

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
 * Driver for the TF-Java cube-cavity reference (Epic #88 / #93).
 *
 * <p>Runs the static-graph assembly via {@link AssemblyGraph}, applies the
 * Dirichlet boundary reduction on the JVM side, and dumps the reduced
 * (K_int, M_int) matrices to a JSON sidecar in the canonical
 * {@code reference/SCHEMA.md} v1 fixture format. A companion Python
 * driver ({@code reference/driver/eigensolve_from_tfjava.py}) consumes
 * the sidecar and calls SciPy's eigensolver.
 *
 * <p>This split — TF-Java does the assembly, SciPy does the eigensolve —
 * is the explicit "TF-Java cannot natively close the spine" friction
 * artifact called out in the #93 acceptance criteria and the #88
 * framing notes.
 */
public class CubeCavityMain implements Callable<Integer> {

    @Option(names = "--n", description = "Cells per side (default 4).")
    private int n = 4;

    @Option(names = "--side", description = "Cube edge length (default 1.0).")
    private double side = 1.0;

    @Option(names = "--out",
            description = "Output JSON path for the reduced (K_int, M_int) sidecar.")
    private String out = "reduced_kM.json";

    @Override
    public Integer call() throws Exception {
        System.out.printf("[tfjava-cube-cavity] n=%d, side=%g%n", n, side);

        CubeMesh.Mesh mesh = CubeMesh.build(n, side);
        boolean[] mask = CubeMesh.interiorMask(mesh.nodes, side);
        int nNodes = mesh.nodes.length;
        int nElem = mesh.tets.length;
        int nInt = countTrue(mask);
        int[] interiorIdx = whereTrue(mask);

        System.out.printf("[tfjava-cube-cavity] nNodes=%d, nElem=%d, nInt=%d%n",
                nNodes, nElem, nInt);

        // ----- Assemble K, M via the static graph -----
        double[][] kGlobal;
        double[][] mGlobal;
        try (AssemblyGraph asm = new AssemblyGraph(mesh.tets, nNodes)) {
            double[][][] result = asm.assemble(mesh.nodes);
            kGlobal = result[0];
            mGlobal = result[1];
        }

        // ----- Apply Dirichlet BC (drop boundary rows/cols) -----
        double[][] kInt = submatrix(kGlobal, interiorIdx);
        double[][] mInt = submatrix(mGlobal, interiorIdx);

        // ----- Quick numerical sanity readouts -----
        double trK = trace(kInt);
        double trM = trace(mInt);
        System.out.printf("[tfjava-cube-cavity] trace(K_int) = %.12e%n", trK);
        System.out.printf("[tfjava-cube-cavity] trace(M_int) = %.12e%n", trM);

        // ----- Dump fixture sidecar -----
        Map<String, Object> fixture = buildFixtureMap(n, side, nNodes, nElem, nInt,
                interiorIdx, kInt, mInt, trK, trM);

        ObjectMapper mapper = new ObjectMapper();
        mapper.enable(SerializationFeature.INDENT_OUTPUT);
        File outFile = new File(out);
        mapper.writeValue(outFile, fixture);
        System.out.printf("[tfjava-cube-cavity] Wrote %s%n", outFile.getAbsolutePath());
        return 0;
    }

    private static Map<String, Object> buildFixtureMap(
            int n, double side, int nNodes, int nElem, int nInt,
            int[] interiorIdx, double[][] kInt, double[][] mInt,
            double trK, double trM) {
        Map<String, Object> fixture = new LinkedHashMap<>();
        fixture.put("schema_version", "1");
        fixture.put("fixture_id", String.format("cube_cavity/n%d_tfjava_reduced", n));
        fixture.put("description",
                "Dirichlet-reduced (K_int, M_int) matrices for the unit-cube "
                + "scalar Helmholtz problem, produced by the TF-Java static-graph "
                + "assembly pipeline (Epic #88 / #93). Consumed by the SciPy "
                + "eigensolve driver to close the eigenproblem at the TF-Java "
                + "weakness boundary.");
        fixture.put("units", "dimensionless");

        Map<String, Object> inputs = new LinkedHashMap<>();
        inputs.put("n", field(new int[] {1}, "i64",
                "Cells per side.", new int[] {n}));
        inputs.put("side", field(new int[] {1}, "f64",
                "Cube edge length.", new double[] {side}));
        inputs.put("interior_idx", field(new int[] {nInt}, "i64",
                "Interior-DOF row/col indices into the full (nNodes, nNodes) matrix.",
                toLong(interiorIdx)));
        fixture.put("inputs", inputs);

        Map<String, Object> outputs = new LinkedHashMap<>();
        outputs.put("k_diag_sum", outputField(new int[] {1}, "f64",
                "trace(K_int) — TF-Java assembly readback.",
                new double[] {trK}, 1.0e-12));
        outputs.put("m_diag_sum", outputField(new int[] {1}, "f64",
                "trace(M_int) — TF-Java assembly readback.",
                new double[] {trM}, 1.0e-12));
        outputs.put("k_int", outputField(new int[] {nInt, nInt}, "f64",
                "Dirichlet-reduced stiffness matrix.",
                flatten(kInt), 1.0e-10));
        outputs.put("m_int", outputField(new int[] {nInt, nInt}, "f64",
                "Dirichlet-reduced mass matrix.",
                flatten(mInt), 1.0e-10));
        fixture.put("outputs", outputs);

        Map<String, Object> provenance = new LinkedHashMap<>();
        provenance.put("source", "reference/tf_java/cube_cavity (Epic #88 / #93)");
        provenance.put("verified_against",
                "reference/numpy/cube_cavity_minimal.py and reference/jax/cube_cavity.py");
        provenance.put("issue", "#93");
        fixture.put("provenance", provenance);
        return fixture;
    }

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

    private static long[] toLong(int[] xs) {
        long[] out = new long[xs.length];
        for (int i = 0; i < xs.length; i++) out[i] = xs[i];
        return out;
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

    private static int countTrue(boolean[] b) {
        int c = 0;
        for (boolean v : b) if (v) c++;
        return c;
    }

    private static int[] whereTrue(boolean[] b) {
        List<Integer> out = new ArrayList<>();
        for (int i = 0; i < b.length; i++) if (b[i]) out.add(i);
        int[] arr = new int[out.size()];
        for (int i = 0; i < arr.length; i++) arr[i] = out.get(i);
        return arr;
    }

    private static double[][] submatrix(double[][] mat, int[] idx) {
        int n = idx.length;
        double[][] out = new double[n][n];
        for (int i = 0; i < n; i++) {
            for (int j = 0; j < n; j++) {
                out[i][j] = mat[idx[i]][idx[j]];
            }
        }
        return out;
    }

    private static double trace(double[][] m) {
        double t = 0.0;
        for (int i = 0; i < m.length; i++) t += m[i][i];
        return t;
    }

    public static void main(String[] args) {
        int exit = new CommandLine(new CubeCavityMain()).execute(args);
        System.exit(exit);
    }
}
