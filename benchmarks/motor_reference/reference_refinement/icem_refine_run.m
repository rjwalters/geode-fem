function icem_refine_run (cfg)
  % Rerun the ICEM reference torque sweep (initial design) at a chosen
  % per-patch subdivision. Written for GEODE issue #1189; it drives the
  % pinned upstream ICEM classes (GPL, kept outside the GEODE tree) and
  % follows the evaluation sequence of upstream results/PMSM_eval_init.m
  % (the plots and exports are left out):
  %   degree [2,2], quadrature degree + 1, Dirichlet/coupling/antiperiodic
  %   boundaries and phases as upstream, harmonic coupling n = 3:6:45 (sin
  %   and cos), a full-current Newton solve at 0 deg (as upstream, it is
  %   the warm start of the first sweep solve), then for each current in
  %   order and each angle 0:1:19 deg: rotate, set current, Newton (warm
  %   started from the previous solve, as upstream), calcTorqueCoupling.
  %
  % cfg fields (all required unless noted):
  %   icem_root    patched copy made by prepare_octave_tree.py
  %   geopdes_inst GeoPDEs inst/ directory
  %   nurbs_inst   nurbs package inst/ directory
  %   shims_dir    directory holding fnder.m
  %   ref_json     benchmarks/motor_reference/reference_data.json
  %   subdiv       [n n] subdivisions per patch, rotor and stator
  %   current_idx  indices into the 3 saved currents, run in this order
  %                (upstream order is 1:3); the warm-start chain follows it
  %   out_json     output file
  %   degree       (optional) default [2 2]
  %   newton_residual (optional) Newton stop on the squared residual;
  %                default: the upstream value (1e-6)

  t_all = tic ();
  if (! isfield (cfg, "degree"))
    cfg.degree = [2, 2];
  end

  % Path: only what the evaluation needs. No upstream directory is cd-ed into.
  addpath (cfg.shims_dir);
  addpath (cfg.nurbs_inst);
  for d = {"geometry", "utils", "msh", "operators", "solve", "space", "multipatch"}
    addpath (fullfile (cfg.geopdes_inst, d{1}));
  end
  for d = {"classes", "operators", "material", "geometries"}
    addpath (genpath (fullfile (cfg.icem_root, d{1})));
  end
  warning ("off", "nrbderiv:SecondDerivative");
  warning ("off", "Octave:classdef-to-struct");
  warning ("off", "Octave:num-to-str");  % upstream disp([... 9 ...]) tab idiom

  ref = jsondecode (fileread (cfg.ref_json));
  init = ref.designs.initial;
  par = init.parameters_by_source_registration_order;
  scaleR = par.ScaleR;
  len = par.LENGTH;
  operatingAngle = par.OPERATING_ANGLE;
  currents = init.history_unscaled_current_amplitudes_A(:).' * scaleR;
  angles = 0:1:19;

  % Geometry: iteration-0 exports, scaled as upstream.
  opts.draw_geometry = false;
  [~, patchesRotor, ~] = PMSM_rotor1 (opts);
  [~, patchesStator, ~] = PMSM_stator1 (opts);
  geoRotor = mp_geo_load (fullfile (cfg.icem_root, "results/plots/final/RotorOpt0.txt"));
  for i = 1:numel (geoRotor)
    geoRotor(i).nurbs.coefs(1:2,:,:) = geoRotor(i).nurbs.coefs(1:2,:,:) * scaleR;
  end
  geoStator = mp_geo_load (fullfile (cfg.icem_root, "results/plots/final/StatorOpt0.txt"));
  for i = 1:numel (geoStator)
    geoStator(i).nurbs.coefs(1:2,:,:) = geoStator(i).nurbs.coefs(1:2,:,:) * scaleR;
  end
  srfRotor = vertcat (geoRotor.nurbs);
  srfStator = vertcat (geoStator.nurbs);

  t_setup = tic ();
  M = MotorSimulation ();
  M.MachineProperties.Length = len;
  M.FormFunctionDegree = cfg.degree;
  M.DegreeQuadrature = M.FormFunctionDegree + 1;
  M.SubdivisionsPatchesRotor = cfg.subdiv;
  M.SubdivisionsPatchesStator = cfg.subdiv;
  if (isfield (cfg, "newton_residual"))
    M.NewtonResidual = cfg.newton_residual;
  end
  M.importSurface (srfRotor, srfStator);

  MatAir = Mat_Air (); MatCopper = Mat_Copper (); MatIron = M_27 (); MatMagnet = NdFeB_Br14 ();
  M.resetMaterials ();
  M.setMaterial (MatAir, patchesRotor.Air, patchesStator.Air);
  M.setMaterial (MatCopper, [], patchesStator.Windings);
  M.setMaterial (MatIron, patchesRotor.Iron, patchesStator.Iron);
  M.setMaterial (MatMagnet, patchesRotor.Magnets, []);

  M.setDirichletBoundaries ([4], [8,12,16,20,24,32]);
  M.setCouplingBoundaries ([7,8,9,10,12], [2,3,4,9,10,11,13,14,15,17,18,19,21,22,23,25,26,28]);
  M.setAntiPeriodicBoundaries ([5,2,11], [3,1,6], [27,29,30,31], [1,5,6,7]);
  M.addRemanenceRotor ([8], MatMagnet, pi/2);
  M.generateStiffnessMatrices ();
  M.generateCouplingSpacesAndMeshes ();
  nharm = 15; mult = 6;
  harm = 3:mult:mult*nharm/2;
  M.setCouplingHarmonics (harm, harm);
  M.generateCouplingMatrices ();

  M.resetPhases ();
  M.addPhase ("U", [8,10,13,14], 1);
  M.addPhase ("U", [7,9,11,12], 1);
  M.addPhase ("U", [26,28,30,31], 1);
  M.addPhase ("U", [103,105,108,109], -1);
  M.addPhase ("W", [27,29,32,33], -1);
  M.addPhase ("W", [46,48,51,52], -1);
  M.addPhase ("W", [45,47,49,50], -1);
  M.addPhase ("W", [64,66,68,69], -1);
  M.addPhase ("V", [65,67,70,71], 1);
  M.addPhase ("V", [83,85,87,88], 1);
  M.addPhase ("V", [84,86,89,90], 1);
  M.addPhase ("V", [102,104,106,107], 1);
  PolePairs = 3; NumberWindings = 12;
  ApplicationCurrent = currents(end);
  M.setCurrentOptions (ApplicationCurrent, PolePairs, NumberWindings, operatingAngle);
  setup_s = toc (t_setup);

  % Upstream warm start: full-current solve at 0 deg before the sweep.
  t0 = tic ();
  M.setRotationAngle (0);
  M.setCurrent (ApplicationCurrent, 0);
  M.solveMagneticPotentialNewton ();
  warm_s = toc (t0);

  nc = numel (cfg.current_idx);
  T = nan (nc, numel (angles));
  W = nan (nc, numel (angles));
  for ic = 1:nc
    I = currents(cfg.current_idx(ic));
    for ia = 1:numel (angles)
      printf ("subdiv %dx%d current %.9g angle %d\n", cfg.subdiv(1), cfg.subdiv(2), I, angles(ia));
      ts = tic ();
      M.setRotationAngle (angles(ia));
      M.setCurrent (I, angles(ia));
      M.solveMagneticPotentialNewton ();
      T(ic, ia) = M.calcTorqueCoupling ();
      W(ic, ia) = toc (ts);
      printf ("  torque %.15e  wall %.2f s\n", T(ic, ia), W(ic, ia));
    end
  end

  out.issue = 1189;
  out.design = "initial (history iteration 0 exports)";
  out.subdivisions_per_patch = cfg.subdiv;
  out.degree = cfg.degree;
  out.quadrature_degree = M.DegreeQuadrature;
  out.coupling_orders_sin_cos = harm;
  out.ndof_rotor = M.NumberDOFRotor;
  out.ndof_stator = M.NumberDOFStator;
  out.ndof_coupling = M.NumberDOFCoupling;
  out.patches_rotor = M.NumberPatchesRotor;
  out.patches_stator = M.NumberPatchesStator;
  out.newton_tolerance_squared_residual = M.NewtonResidual;
  out.length_m = len;
  out.operating_angle_deg = operatingAngle;
  out.angles_deg = angles;
  out.current_indices = cfg.current_idx;
  out.currents_A = currents(cfg.current_idx);
  out.torque_Nm = T;
  out.mean_torque_Nm = mean (T, 2).';
  out.population_std_torque_Nm = std (T, 1, 2).';
  out.wall_s_per_solve = W;
  out.wall_s_setup = setup_s;
  out.wall_s_warm_start = warm_s;
  out.wall_s_total = toc (t_all);
  out.octave_version = OCTAVE_VERSION;
  out.computer = computer ();
  out.nproc = nproc ();
  fid = fopen (cfg.out_json, "w");
  fputs (fid, jsonencode (out));
  fputs (fid, "\n");
  fclose (fid);
  printf ("ndof rotor %d stator %d coupling %d\n", out.ndof_rotor, out.ndof_stator, out.ndof_coupling);
  printf ("mean %s\nstd  %s\ntotal wall %.1f s\n", mat2str (out.mean_torque_Nm, 8), ...
          mat2str (out.population_std_torque_Nm, 8), out.wall_s_total);
end
