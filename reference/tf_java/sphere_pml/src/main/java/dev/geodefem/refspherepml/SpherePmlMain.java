package dev.geodefem.refspherepml;

import com.fasterxml.jackson.databind.ObjectMapper;
import com.fasterxml.jackson.databind.SerializationFeature;
import java.io.File;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.concurrent.Callable;
import picocli.CommandLine;
import picocli.CommandLine.Option;

/**
 * Driver for the TF-Java sphere-PML complex-Nédélec reference (Epic #88 / #156).
 *
 * <p>Runs the Gmsh MSH4 mesh parser, builds the global edge table,
 * computes per-tet centroid radii and the complex-valued ε from the
 * scalar-isotropic PML profile (mirror of
 * {@code geode_core::build_complex_epsilon_r_pml}), runs the
 * static-graph complex assembly via {@link ComplexNedelecAssemblyGraph},
 * applies PEC boundary elimination on the JVM side (dropping edges with
 * both endpoints on the outer sphere at {@code r = R_BUFFER = 2.0}), and
 * dumps the reduced {@code (K_int, Re(M_int), Im(M_int))} matrices to a
 * schema-v1 JSON sidecar.
 *
 * <p>A companion Python driver
 * ({@code reference/driver/eigensolve_from_sidecar.py --problem sphere-pml
 *   --backend tfjava}) consumes the sidecar, fuses the (Re, Im) mass
 * pair into a SciPy {@code complex128} CSR, and runs the dense LAPACK
 * ZGGEV complex generalized eigensolve (TF-Java has no native complex
 * sparse generalized eigensolver).
 *
 * <p>This split — TF-Java does the complex Nédélec assembly via two
 * parallel real scatters, SciPy does the complex eigensolve — is the
 * "TF-Java cannot natively close the spine" friction artifact called
 * out in the Epic #88 framing.
 *
 * <p>Sign convention: {@code exp(+jωt)}; the PML profile produces
 * {@code Im(ε) < 0} in the absorbing shell; the downstream eigensolver
 * canonicalizes the eigenvalue sign to {@code Im(λ) > 0} per PR #155
 * Judge's binding decision.
 */
public class SpherePmlMain implements Callable<Integer> {

    @Option(names = "--mesh",
            description = "Path to the sphere.msh fixture (default: ../../fixtures/sphere_pml/sphere.msh).")
    private String meshPath = "../../fixtures/sphere_pml/sphere.msh";

    @Option(names = "--sigma0",
            description = "PML absorption strength at r = R_BUFFER (default 5.0). "
                        + "Pass 0.0 for the PEC regression collapse.")
    private double sigma0 = 5.0;

    @Option(names = "--n-index",
            description = "Refractive index inside the dielectric sphere (default 1.5).")
    private double nIndex = 1.5;

    @Option(names = "--r-buffer",
            description = "Outer PEC wall radius (default 2.0).")
    private double rBuffer = SphereMesh.R_BUFFER;

    @Option(names = "--out",
            description = "Output JSON path for the reduced (K_int, Re(M_int), Im(M_int)) sidecar.")
    private String out = "reduced_kM_sphere_pml.json";

    @Override
    public Integer call() throws Exception {
        System.out.printf(
                "[tfjava-sphere-pml] mesh=%s, sigma0=%g, n_index=%g, r_buffer=%g%n",
                meshPath, sigma0, nIndex, rBuffer);

        // ----- 1. Mesh I/O -----
        SphereMesh.Mesh mesh = SphereMesh.read(meshPath);
        int nNodes = mesh.nodes.length;
        int nTets  = mesh.tets.length;
        System.out.printf("[tfjava-sphere-pml] nNodes=%d, nTets=%d%n", nNodes, nTets);

        // ----- 2. Per-tet centroid radii + complex epsilon profile -----
        double[] centroidRadii = SphereMesh.tetCentroidRadii(mesh.nodes, mesh.tets);
        double[][] epsRImparts = SphereMesh.buildComplexEpsilonRPml(
                mesh.tetTags, centroidRadii, nIndex, sigma0);
        double[] epsRe = epsRImparts[0];
        double[] epsIm = epsRImparts[1];

        // ----- 3. Edge enumeration -----
        SphereMesh.EdgeTable et = SphereMesh.buildEdges(mesh.tets);
        int nEdges = et.edges.length;
        System.out.printf("[tfjava-sphere-pml] nEdges=%d%n", nEdges);

        // ----- 4. PEC mask -----
        boolean[] interiorMask = SphereMesh.interiorEdgeMask(mesh.nodes, et.edges);
        int nInt = SphereMesh.countTrue(interiorMask);
        int[] interiorIdx = SphereMesh.whereTrue(interiorMask);
        System.out.printf("[tfjava-sphere-pml] nInt=%d (PEC reduced from %d)%n", nInt, nEdges);

        // ----- 5. Assemble K, Re(M), Im(M) via the complex static graph -----
        System.out.println("[tfjava-sphere-pml] Building TF-Java complex assembly graph...");
        double[][] kGlobal;
        double[][] mReGlobal;
        double[][] mImGlobal;
        try (ComplexNedelecAssemblyGraph asm = new ComplexNedelecAssemblyGraph(
                mesh.tets, et.tetEdgeIdx, et.tetEdgeSign, nNodes, nEdges)) {
            System.out.println("[tfjava-sphere-pml] Running assembly (this may take a moment)...");
            ComplexNedelecAssemblyGraph.AssemblyResult result =
                    asm.assemble(mesh.nodes, epsRe, epsIm);
            kGlobal   = result.kGlobal;
            mReGlobal = result.mReGlobal;
            mImGlobal = result.mImGlobal;
        }
        System.out.println("[tfjava-sphere-pml] Assembly complete.");

        // ----- 6. Apply PEC BC (drop boundary rows/cols) -----
        double[][] kInt   = SphereMesh.submatrix(kGlobal,   interiorIdx);
        double[][] mReInt = SphereMesh.submatrix(mReGlobal, interiorIdx);
        double[][] mImInt = SphereMesh.submatrix(mImGlobal, interiorIdx);

        // ----- Quick numerical sanity readouts -----
        double trK   = SphereMesh.trace(kInt);
        double trMre = SphereMesh.trace(mReInt);
        double trMim = SphereMesh.trace(mImInt);
        double frobK   = frobenius(kInt);
        double frobMre = frobenius(mReInt);
        double frobMim = frobenius(mImInt);
        System.out.printf("[tfjava-sphere-pml] trace(K_int)       = %.12e%n", trK);
        System.out.printf("[tfjava-sphere-pml] trace(Re(M_int))   = %.12e%n", trMre);
        System.out.printf("[tfjava-sphere-pml] trace(Im(M_int))   = %.12e%n", trMim);
        System.out.printf("[tfjava-sphere-pml] Frobenius(K_int)   = %.12e%n", frobK);
        System.out.printf("[tfjava-sphere-pml] Frobenius(Re(M))   = %.12e%n", frobMre);
        System.out.printf("[tfjava-sphere-pml] Frobenius(Im(M))   = %.12e%n", frobMim);

        // ----- Dump fixture sidecar -----
        Map<String, Object> fixture = buildFixtureMap(
                nNodes, nTets, nEdges, nInt, nIndex, sigma0, rBuffer,
                interiorIdx, epsRe, epsIm,
                kInt, mReInt, mImInt,
                trK, trMre, trMim, frobK, frobMre, frobMim);

        ObjectMapper mapper = new ObjectMapper();
        mapper.enable(SerializationFeature.INDENT_OUTPUT);
        File outFile = new File(out);
        mapper.writeValue(outFile, fixture);
        System.out.printf("[tfjava-sphere-pml] Wrote %s%n", outFile.getAbsolutePath());
        return 0;
    }

    // ------------------------------------------------------------------
    // Fixture JSON construction
    // ------------------------------------------------------------------

    private static Map<String, Object> buildFixtureMap(
            int nNodes, int nTets, int nEdges, int nInt,
            double nIndex, double sigma0, double rBuffer,
            int[] interiorIdx,
            double[] epsRe, double[] epsIm,
            double[][] kInt, double[][] mReInt, double[][] mImInt,
            double trK, double trMre, double trMim,
            double frobK, double frobMre, double frobMim) {

        Map<String, Object> fixture = new LinkedHashMap<>();
        fixture.put("schema_version", "1");
        fixture.put("fixture_id", "sphere_pml/n774_pml_eigenmode_tfjava_sidecar");
        fixture.put("description",
                "TF-Java static-graph reference for the scalar-isotropic complex-epsilon "
              + "sphere-PML vector-Nédélec assembly (Epic #88 / Phase H.4 / Issue #156). "
              + "Assembles the real Nédélec curl-curl K and the complex epsilon-scaled "
              + "mass M for all global edges (TF-Java 1.0.0 has no native c128 typed "
              + "value, so Re(M) and Im(M) are emitted as parallel f64 tensors and "
              + "fused into a complex128 CSR by the Python eigensolve driver), then "
              + "applies PEC BC to produce K_int, Re(M_int), Im(M_int). The complex "
              + "generalized eigensolve (TF-Java cannot natively close the spine) is "
              + "delegated to SciPy LAPACK ZGGEV via "
              + "reference/driver/eigensolve_from_sidecar.py "
              + "(--problem sphere-pml --backend tfjava).");
        fixture.put("units",
                "lambda = k^2 (inverse-length squared) with eigensolver-determined sign of "
              + "Im(lambda) canonicalized to Im(lambda) > 0 per Epic #88 PR #155 NumPy "
              + "tiebreaker; dimensionless mesh coordinates");

        // Inputs.
        Map<String, Object> inputs = new LinkedHashMap<>();
        inputs.put("mesh_path", field(new int[]{1}, "str",
                "Path to the bundled sphere.msh fixture.",
                new String[]{"reference/fixtures/sphere_pml/sphere.msh"}));
        inputs.put("sigma_0", field(new int[]{1}, "f64",
                "PML absorption strength at r=R_BUFFER. 0 collapses the profile to "
              + "the real PEC epsilon (regression case).",
                new double[]{sigma0}));
        inputs.put("n_index", field(new int[]{1}, "f64",
                "Refractive index inside the dielectric sphere; epsilon_r = n^2 inside.",
                new double[]{nIndex}));
        inputs.put("r_buffer", field(new int[]{1}, "f64",
                "Outer PEC wall radius (= R_BUFFER = 2.0).",
                new double[]{rBuffer}));
        inputs.put("r_sphere", field(new int[]{1}, "f64",
                "Inner dielectric sphere radius (= R_SPHERE = 1.0).",
                new double[]{SphereMesh.R_SPHERE}));
        inputs.put("r_pml_inner", field(new int[]{1}, "f64",
                "PML inner radius (= R_PML_INNER = 1.5).",
                new double[]{SphereMesh.R_PML_INNER}));
        inputs.put("n_int", field(new int[]{1}, "i64",
                "Number of interior edges (DOFs after PEC elimination).",
                new int[]{nInt}));
        inputs.put("interior_idx", field(new int[]{nInt}, "i64",
                "Interior-DOF row/col indices into the full (nEdges, nEdges) matrix.",
                toLong(interiorIdx)));
        // Per-tet complex permittivity, encoded as real-imag interleaved per reference/SCHEMA.md.
        inputs.put("epsilon_r_complex", field(new int[]{nTets}, "c128",
                "Per-tet complex relative permittivity from the scalar-isotropic PML "
              + "profile (mirror of geode_core::build_complex_epsilon_r_pml). On-disk: "
              + "real-imag interleaved per reference/SCHEMA.md.",
                interleaveC128(epsRe, epsIm)));
        // Compatibility aliases for the consolidated sidecar driver (PR #150).
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
        outputs.put("m_re_diag_sum", outputField(new int[]{1}, "f64",
                "trace(Re(M_int)) — TF-Java assembly readback.",
                new double[]{trMre}, 1.0e-6));
        outputs.put("m_im_diag_sum", outputField(new int[]{1}, "f64",
                "trace(Im(M_int)) — TF-Java assembly readback.",
                new double[]{trMim}, 1.0e-6));
        outputs.put("k_int_frobenius", outputField(new int[]{1}, "f64",
                "Frobenius norm of K_int.",
                new double[]{frobK}, 1.0e-6));
        outputs.put("m_re_int_frobenius", outputField(new int[]{1}, "f64",
                "Frobenius norm of Re(M_int).",
                new double[]{frobMre}, 1.0e-6));
        outputs.put("m_im_int_frobenius", outputField(new int[]{1}, "f64",
                "Frobenius norm of Im(M_int).",
                new double[]{frobMim}, 1.0e-6));
        outputs.put("k_int_diag", outputField(new int[]{nInt}, "f64",
                "Diagonal of K_int (per-DOF stiffness).",
                diagVector(kInt), 1.0e-8));
        outputs.put("m_re_int_diag", outputField(new int[]{nInt}, "f64",
                "Diagonal of Re(M_int) (per-DOF mass, real part).",
                diagVector(mReInt), 1.0e-8));
        outputs.put("m_im_int_diag", outputField(new int[]{nInt}, "f64",
                "Diagonal of Im(M_int) (per-DOF mass, imaginary part).",
                diagVector(mImInt), 1.0e-8));
        outputs.put("k_int", outputField(new int[]{nInt, nInt}, "f64",
                "Dirichlet-reduced Nédélec curl-curl stiffness matrix (real).",
                flatten(kInt), 1.0e-8));
        outputs.put("m_re_int", outputField(new int[]{nInt, nInt}, "f64",
                "Re(Dirichlet-reduced epsilon-scaled Nédélec mass matrix).",
                flatten(mReInt), 1.0e-8));
        outputs.put("m_im_int", outputField(new int[]{nInt, nInt}, "f64",
                "Im(Dirichlet-reduced epsilon-scaled Nédélec mass matrix).",
                flatten(mImInt), 1.0e-8));
        fixture.put("outputs", outputs);

        // Provenance.
        Map<String, Object> provenance = new LinkedHashMap<>();
        provenance.put("source", "reference/tf_java/sphere_pml (Epic #88 / Phase H.4 / Issue #156)");
        provenance.put("verified_against",
                "reference/numpy/sphere_pml.py and reference/fixtures/sphere_pml/baseline.json "
              + "(NumPy canonical tiebreaker per PR #155)");
        provenance.put("issue", "#156");
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

    /**
     * Real-imag interleaved encoding per {@code reference/SCHEMA.md} "Complex encoding (c128)".
     * Output layout matches {@code np.asarray(z).view(np.float64).tolist()} on a contiguous
     * {@code complex128} array.
     */
    private static double[] interleaveC128(double[] re, double[] im) {
        int n = re.length;
        double[] out = new double[n * 2];
        for (int i = 0; i < n; i++) {
            out[2 * i]     = re[i];
            out[2 * i + 1] = im[i];
        }
        return out;
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
        int exit = new CommandLine(new SpherePmlMain()).execute(args);
        System.exit(exit);
    }
}
