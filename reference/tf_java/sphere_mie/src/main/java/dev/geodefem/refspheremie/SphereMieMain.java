package dev.geodefem.refspheremie;

import com.fasterxml.jackson.databind.ObjectMapper;
import com.fasterxml.jackson.databind.SerializationFeature;
import java.io.File;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.concurrent.Callable;
import picocli.CommandLine;
import picocli.CommandLine.Option;

/**
 * Driver for the TF-Java sphere-Mie (anisotropic UPML tensor-ε)
 * complex-Nédélec reference (Epic #88 / Phase J.5 / Issue #174).
 *
 * <p>Runs the Gmsh MSH4 mesh parser, builds the global edge table,
 * computes per-tet centroid vectors and the diagonal complex
 * permittivity tensor from the simplified Sacks UPML profile (mirror of
 * {@code geode_core::build_anisotropic_pml_tensor_diag}), runs the
 * static-graph tensor-ε assembly via {@link ComplexNedelecAssemblyGraph},
 * applies PEC boundary elimination on the JVM side (dropping edges with
 * both endpoints on the outer sphere at {@code r = R_BUFFER = 2.0}), and
 * dumps the reduced {@code (K_int, Re(M_int), Im(M_int))} matrices to a
 * schema-v1 JSON sidecar.
 *
 * <p>A companion Python driver
 * ({@code reference/driver/eigensolve_from_sidecar.py --problem sphere-mie
 *   --backend tfjava}) consumes the sidecar, fuses the (Re, Im) mass
 * pair into a SciPy {@code complex128} pencil, and runs the dense LAPACK
 * ZGGEV complex generalized eigensolve (TF-Java has no native complex
 * sparse generalized eigensolver).
 *
 * <p>This split — TF-Java does the tensor-ε Nédélec assembly via two
 * parallel real scatters, SciPy does the complex eigensolve — is the
 * "TF-Java cannot natively close the spine" friction artifact called
 * out in the Epic #88 framing, now extended to tensor constitutive
 * data (Phase J.5's specific DX probe).
 *
 * <p>Sign convention: {@code exp(+jωt)}; the UPML tensor carries
 * {@code Im(ε) < 0} on the transverse entries and {@code Im(ε) > 0} on
 * the radial entry ({@code 1/s_r}); the physical eigenvalues of the
 * resulting pencil come out with {@code Im(λ) < 0} on the small mesh —
 * a property of the pencil agreed on by LAPACK ZGGEV and faer QZ (see
 * the J.2 fixture description). No sign canonicalization is applied
 * downstream (unlike the scalar-PML PR #155 convention).
 */
public class SphereMieMain implements Callable<Integer> {

    @Option(names = "--mesh",
            description = "Path to the sphere.msh fixture "
                        + "(default: ../../fixtures/sphere_pml_small/sphere.msh — "
                        + "the 48-node / 197-tet small mesh shared with #158).")
    private String meshPath = "../../fixtures/sphere_pml_small/sphere.msh";

    @Option(names = "--sigma0",
            description = "UPML absorption strength at r = R_BUFFER (default 5.0). "
                        + "Pass 0.0 for the PEC regression collapse (the tensor "
                        + "degenerates to the real isotropic scalar epsilon).")
    private double sigma0 = 5.0;

    @Option(names = "--k0-ref",
            description = "Reference wavenumber omega heuristic in the UPML stretch "
                        + "s = 1 - j sigma(r)/omega (default 2.0, K0_REF in mie_sphere.rs).")
    private double k0Ref = SphereMesh.K0_REF;

    @Option(names = "--n-index",
            description = "Refractive index inside the dielectric sphere (default 1.5).")
    private double nIndex = 1.5;

    @Option(names = "--r-buffer",
            description = "Outer PEC wall radius (default 2.0).")
    private double rBuffer = SphereMesh.R_BUFFER;

    @Option(names = "--out",
            description = "Output JSON path for the reduced (K_int, Re(M_int), Im(M_int)) sidecar.")
    private String out = "reduced_kM_sphere_mie.json";

    @Override
    public Integer call() throws Exception {
        System.out.printf(
                "[tfjava-sphere-mie] mesh=%s, sigma0=%g, k0_ref=%g, n_index=%g, r_buffer=%g%n",
                meshPath, sigma0, k0Ref, nIndex, rBuffer);

        // ----- 1. Mesh I/O -----
        SphereMesh.Mesh mesh = SphereMesh.read(meshPath);
        int nNodes = mesh.nodes.length;
        int nTets  = mesh.tets.length;
        System.out.printf("[tfjava-sphere-mie] nNodes=%d, nTets=%d%n", nNodes, nTets);

        // ----- 2. Per-tet centroid vectors + diagonal UPML tensor -----
        double[][] centroids = SphereMesh.tetCentroids(mesh.nodes, mesh.tets);
        double[][][] epsTensorParts = SphereMesh.buildAnisotropicPmlTensorDiag(
                mesh.tetTags, centroids, nIndex, sigma0, k0Ref);
        double[][] epsRe = epsTensorParts[0]; // [nTets][3]
        double[][] epsIm = epsTensorParts[1]; // [nTets][3]

        // ----- 3. Edge enumeration -----
        SphereMesh.EdgeTable et = SphereMesh.buildEdges(mesh.tets);
        int nEdges = et.edges.length;
        System.out.printf("[tfjava-sphere-mie] nEdges=%d%n", nEdges);

        // ----- 4. PEC mask -----
        boolean[] interiorMask = SphereMesh.interiorEdgeMask(mesh.nodes, et.edges);
        int nInt = SphereMesh.countTrue(interiorMask);
        int[] interiorIdx = SphereMesh.whereTrue(interiorMask);
        System.out.printf("[tfjava-sphere-mie] nInt=%d (PEC reduced from %d)%n", nInt, nEdges);

        // ----- 5. Assemble K, Re(M), Im(M) via the tensor-ε static graph -----
        System.out.println("[tfjava-sphere-mie] Building TF-Java tensor-epsilon assembly graph...");
        double[][] kGlobal;
        double[][] mReGlobal;
        double[][] mImGlobal;
        try (ComplexNedelecAssemblyGraph asm = new ComplexNedelecAssemblyGraph(
                mesh.tets, et.tetEdgeIdx, et.tetEdgeSign, nNodes, nEdges)) {
            System.out.println("[tfjava-sphere-mie] Running assembly (this may take a moment)...");
            ComplexNedelecAssemblyGraph.AssemblyResult result =
                    asm.assemble(mesh.nodes, epsRe, epsIm);
            kGlobal   = result.kGlobal;
            mReGlobal = result.mReGlobal;
            mImGlobal = result.mImGlobal;
        }
        System.out.println("[tfjava-sphere-mie] Assembly complete.");

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
        System.out.printf("[tfjava-sphere-mie] trace(K_int)       = %.12e%n", trK);
        System.out.printf("[tfjava-sphere-mie] trace(Re(M_int))   = %.12e%n", trMre);
        System.out.printf("[tfjava-sphere-mie] trace(Im(M_int))   = %.12e%n", trMim);
        System.out.printf("[tfjava-sphere-mie] Frobenius(K_int)   = %.12e%n", frobK);
        System.out.printf("[tfjava-sphere-mie] Frobenius(Re(M))   = %.12e%n", frobMre);
        System.out.printf("[tfjava-sphere-mie] Frobenius(Im(M))   = %.12e%n", frobMim);

        // ----- Dump fixture sidecar -----
        Map<String, Object> fixture = buildFixtureMap(
                nNodes, nTets, nEdges, nInt, nIndex, sigma0, k0Ref, rBuffer,
                interiorIdx, epsRe, epsIm,
                kInt, mReInt, mImInt,
                trK, trMre, trMim, frobK, frobMre, frobMim);

        ObjectMapper mapper = new ObjectMapper();
        mapper.enable(SerializationFeature.INDENT_OUTPUT);
        File outFile = new File(out);
        mapper.writeValue(outFile, fixture);
        System.out.printf("[tfjava-sphere-mie] Wrote %s%n", outFile.getAbsolutePath());
        return 0;
    }

    // ------------------------------------------------------------------
    // Fixture JSON construction
    // ------------------------------------------------------------------

    private static Map<String, Object> buildFixtureMap(
            int nNodes, int nTets, int nEdges, int nInt,
            double nIndex, double sigma0, double k0Ref, double rBuffer,
            int[] interiorIdx,
            double[][] epsRe, double[][] epsIm,
            double[][] kInt, double[][] mReInt, double[][] mImInt,
            double trK, double trMre, double trMim,
            double frobK, double frobMre, double frobMim) {

        Map<String, Object> fixture = new LinkedHashMap<>();
        fixture.put("schema_version", "1");
        fixture.put("fixture_id", "sphere_mie_small/n48_aniso_upml_mie_tfjava_sidecar");
        fixture.put("description",
                "TF-Java static-graph reference for the anisotropic-UPML tensor-epsilon "
              + "dielectric-sphere Mie vector-Nedelec assembly (Epic #88 / Phase J.5 / "
              + "Issue #174). Assembles the real Nedelec curl-curl K and the complex "
              + "tensor-epsilon-scaled mass M for all global edges. The constitutive "
              + "input is a per-tet DIAGONAL 3x3 complex permittivity tensor carried as "
              + "two parallel [nTets, 3] f64 placeholders (TF-Java 1.0.0 has no native "
              + "c128 typed value); the per-axis cofactor gram is contracted against "
              + "the tensor inside the graph, and Re(M) / Im(M) are emitted as parallel "
              + "f64 tensors, then fused into a complex128 pencil by the Python "
              + "eigensolve driver. PEC BC produces K_int, Re(M_int), Im(M_int). The "
              + "complex generalized eigensolve (TF-Java cannot natively close the "
              + "spine) is delegated to SciPy LAPACK ZGGEV via "
              + "reference/driver/eigensolve_from_sidecar.py "
              + "(--problem sphere-mie --backend tfjava).");
        fixture.put("units",
                "lambda = k^2 (inverse-length squared) under exp(+j omega t); physical "
              + "Im(lambda) < 0 on the small-mesh tensor pencil (property of the pencil, "
              + "no downstream sign canonicalization); dimensionless mesh coordinates");

        // Inputs.
        Map<String, Object> inputs = new LinkedHashMap<>();
        inputs.put("mesh_path", field(new int[]{1}, "str",
                "Path to the bundled small sphere.msh fixture (shared with #158).",
                new String[]{"reference/fixtures/sphere_pml_small/sphere.msh"}));
        inputs.put("sigma_0", field(new int[]{1}, "f64",
                "UPML absorption strength at r=R_BUFFER. 0 collapses the tensor to "
              + "the real isotropic scalar epsilon (PEC regression case).",
                new double[]{sigma0}));
        inputs.put("k0_ref", field(new int[]{1}, "f64",
                "Reference wavenumber omega heuristic in the UPML stretch "
              + "s = 1 - j sigma(r)/omega (K0_REF in mie_sphere.rs).",
                new double[]{k0Ref}));
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
        // Per-tet diagonal tensor, row-major (tet, axis) flattened,
        // real-imag interleaved per reference/SCHEMA.md.
        inputs.put("epsilon_tensor_diag", field(new int[]{epsRe.length, 3}, "c128",
                "Per-tet diagonal anisotropic complex permittivity tensor "
              + "(epsilon_x, epsilon_y, epsilon_z), global Cartesian basis — mirror of "
              + "geode_core::build_anisotropic_pml_tensor_diag (JVM twin: "
              + "SphereMesh.buildAnisotropicPmlTensorDiag). On-disk: row-major "
              + "(tet, axis) flattened, real-imag interleaved per reference/SCHEMA.md.",
                interleaveC128Tensor(epsRe, epsIm)));
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
                "Dirichlet-reduced Nedelec curl-curl stiffness matrix (real).",
                flatten(kInt), 1.0e-8));
        outputs.put("m_re_int", outputField(new int[]{nInt, nInt}, "f64",
                "Re(Dirichlet-reduced tensor-epsilon-scaled Nedelec mass matrix).",
                flatten(mReInt), 1.0e-8));
        outputs.put("m_im_int", outputField(new int[]{nInt, nInt}, "f64",
                "Im(Dirichlet-reduced tensor-epsilon-scaled Nedelec mass matrix).",
                flatten(mImInt), 1.0e-8));
        fixture.put("outputs", outputs);

        // Provenance.
        Map<String, Object> provenance = new LinkedHashMap<>();
        provenance.put("source", "reference/tf_java/sphere_mie (Epic #88 / Phase J.5 / Issue #174)");
        provenance.put("verified_against",
                "reference/numpy/sphere_mie.py and "
              + "reference/fixtures/sphere_mie_small/baseline.json (NumPy J.2 canonical)");
        provenance.put("issue", "#174");
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
     * Real-imag interleaved encoding of a row-major-flattened [nTets][3]
     * complex tensor per {@code reference/SCHEMA.md} "Complex encoding
     * (c128)". Output layout matches
     * {@code np.ascontiguousarray(z).reshape(-1).view(np.float64).tolist()}
     * on a contiguous {@code complex128} (nTets, 3) array.
     */
    private static double[] interleaveC128Tensor(double[][] re, double[][] im) {
        int n = re.length;
        double[] out = new double[n * 3 * 2];
        int ptr = 0;
        for (int i = 0; i < n; i++) {
            for (int axis = 0; axis < 3; axis++) {
                out[ptr++] = re[i][axis];
                out[ptr++] = im[i][axis];
            }
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
        int exit = new CommandLine(new SphereMieMain()).execute(args);
        System.exit(exit);
    }
}
