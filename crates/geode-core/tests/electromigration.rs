//! Electromigration models (issue #1558): Black, Blech, Korhonen 1-D.
use geode_core::postproc::electromigration::*;

// Cu-like illustrative inputs (test-only; the library defaults nothing).
const OMEGA: f64 = 1.18e-29;
const ZSTAR: f64 = 1.0;
const RHO: f64 = 2.0e-8;
const SIGMA_CRIT: f64 = 5.0e8;
const KAPPA: f64 = 1.0e-14;

fn line(l: f64) -> KorhonenLine {
    KorhonenLine {
        length: l,
        kappa: KAPPA,
        omega: OMEGA,
        z_star: ZSTAR,
        resistivity: RHO,
    }
}

#[test]
fn black_matches_closed_form_over_grid() {
    for &j in &[1e9, 5e9, 2e10] {
        for &t in &[300.0, 358.0, 500.0] {
            let got = black_mttf(3.0, 2.0, 0.9, j, t).unwrap();
            let want = 3.0 * j.powf(-2.0) * (0.9 / (8.617333262e-5 * t)).exp();
            assert!((got / want - 1.0).abs() < 1e-12);
        }
    }
    // sign of j ignored; doubling j with n=2 quarters MTTF
    let a = black_mttf(1.0, 2.0, 0.7, 1e9, 400.0).unwrap();
    let b = black_mttf(1.0, 2.0, 0.7, -2e9, 400.0).unwrap();
    assert!((a / b - 4.0).abs() < 1e-12);
}

#[test]
fn black_invalid_inputs() {
    assert_eq!(
        black_mttf(1.0, 2.0, 0.9, 0.0, 300.0).unwrap(),
        f64::INFINITY
    );
    assert!(black_mttf(1.0, 2.0, 0.9, 1e9, 0.0).is_err());
    assert!(black_mttf(1.0, 2.0, 0.9, 1e9, -5.0).is_err());
    assert!(black_mttf(0.0, 2.0, 0.9, 1e9, 300.0).is_err());
    assert!(black_mttf(1.0, f64::NAN, 0.9, 1e9, 300.0).is_err());
}

#[test]
fn blech_threshold_bracket() {
    let crit = blech_critical_jl(OMEGA, SIGMA_CRIT, ZSTAR, RHO).unwrap();
    let want = 2.0 * OMEGA * SIGMA_CRIT / (ZSTAR * 1.602176634e-19 * RHO);
    assert!((crit / want - 1.0).abs() < 1e-12);
    let l = 50e-6;
    let args = (OMEGA, SIGMA_CRIT, ZSTAR, RHO);
    assert!(blech_is_immortal(0.99 * crit / l, l, args.0, args.1, args.2, args.3).unwrap());
    assert!(!blech_is_immortal(1.01 * crit / l, l, args.0, args.1, args.2, args.3).unwrap());
    assert!(blech_is_immortal(-0.99 * crit / l, l, args.0, args.1, args.2, args.3).unwrap());
    assert!(blech_is_immortal(1.0, 0.0, args.0, args.1, args.2, args.3).is_err());
    assert!(blech_critical_jl(OMEGA, SIGMA_CRIT, 0.0, RHO).is_err());
}

#[test]
fn korhonen_steady_state_linear_and_threshold() {
    let l = 50e-6;
    let ln = line(l);
    let j = 1e10;
    let tau = ln.time_constant();
    let p = korhonen_evolve(&ln, &Waveform::Dc(j), 40.0 * tau, 64, 400).unwrap();
    let g = ln.drift_gradient(j);
    // linear profile sigma = G (L/2 - x), exact for the discrete scheme
    for (x, s) in p.x.iter().zip(&p.sigma) {
        assert!((s - g * (l / 2.0 - x)).abs() < 1e-6 * (g * l).abs());
    }
    // difference across the line = G L = Z* e rho j L / Omega
    let diff = p.sigma_at_0 - p.sigma_at_l;
    assert!((diff / (g * l) - 1.0).abs() < 1e-6);
    assert!((p.sigma_at_0 / ln.steady_end_stress(j) - 1.0).abs() < 1e-6);
    // end stress crosses sigma_crit exactly at the Blech jL
    let crit = blech_critical_jl(OMEGA, SIGMA_CRIT, ZSTAR, RHO).unwrap();
    for (f, above) in [(0.98, false), (1.02, true)] {
        let jj = f * crit / l;
        let p = korhonen_evolve(&ln, &Waveform::Dc(jj), 40.0 * tau, 64, 400).unwrap();
        assert_eq!(p.peak_tensile() > SIGMA_CRIT, above);
    }
    // zero-mean (atom conservation)
    let mean: f64 = p.sigma.iter().sum::<f64>() / p.sigma.len() as f64;
    assert!(mean.abs() < 1e-6 * (g * l).abs());
}

#[test]
fn korhonen_sign_reversal_and_zero() {
    let ln = line(20e-6);
    let t = 30.0 * ln.time_constant();
    let a = korhonen_evolve(&ln, &Waveform::Dc(1e10), t, 32, 200).unwrap();
    let b = korhonen_evolve(&ln, &Waveform::Dc(-1e10), t, 32, 200).unwrap();
    assert!(a.sigma_at_0 > 0.0 && a.sigma_at_l < 0.0);
    assert!((a.sigma_at_0 + b.sigma_at_0).abs() < 1e-9 * a.sigma_at_0);
    assert!((a.sigma_at_0 - b.sigma_at_l).abs() < 1e-9 * a.sigma_at_0);
    let z = korhonen_evolve(&ln, &Waveform::Dc(0.0), t, 32, 200).unwrap();
    assert!(z.sigma.iter().all(|&s| s == 0.0));
}

#[test]
fn korhonen_fast_pulse_matches_time_average() {
    let ln = line(50e-6);
    let tau = ln.time_constant();
    let (peak, duty) = (4e10, 0.25);
    let period = tau / 2000.0;
    let pulsed = Waveform::Pulsed { peak, period, duty };
    assert!((pulsed.average() - 1e10).abs() < 1e-3);
    let t_end = 30.0 * tau;
    let a = korhonen_evolve(&ln, &pulsed, t_end, 64, 3000).unwrap();
    let b = korhonen_evolve(&ln, &Waveform::Dc(pulsed.average()), t_end, 64, 3000).unwrap();
    let scale = ln.steady_end_stress(pulsed.average());
    assert!((a.sigma_at_0 - b.sigma_at_0).abs() < 1e-3 * scale);
    // finely resolved pulses (period spans 8 steps) agree within 2 % too
    let fine = korhonen_evolve(&ln, &pulsed, 3.0 * tau, 64, 3 * 2000 * 8).unwrap();
    let avg = korhonen_evolve(&ln, &Waveform::Dc(pulsed.average()), 3.0 * tau, 64, 400).unwrap();
    assert!((fine.sigma_at_0 - avg.sigma_at_0).abs() < 2e-2 * scale);
}

#[test]
fn korhonen_errors_and_kappa() {
    let ln = line(1e-6);
    assert!(korhonen_evolve(&ln, &Waveform::Dc(1e9), 1.0, 1, 10).is_err());
    assert!(korhonen_evolve(&ln, &Waveform::Dc(1e9), 1.0, 8, 0).is_err());
    let mut bad = ln;
    bad.length = 0.0;
    assert!(korhonen_evolve(&bad, &Waveform::Dc(1e9), 1.0, 8, 4).is_err());
    let k = korhonen_kappa(1e-20, 1e11, OMEGA, 373.0).unwrap();
    assert!((k - 1e-20 * 1e11 * OMEGA / (1.380649e-23 * 373.0)).abs() < 1e-30);
    assert!(korhonen_kappa(1e-20, 1e11, OMEGA, 0.0).is_err());
}
