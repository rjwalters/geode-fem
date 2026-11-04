//! Dump GEODE's `TabulatedBhLaw` on the M27 table to CSV on a dense B grid
//! (issue #1160). `benchmarks/motor_nonlinear_m1/pchip_compare.py` reads
//! this dump and compares it with the reference code's MATLAB `pchip`.
//!
//! ```text
//! cargo run -p geode-core --release --example m27_law_dump -- \
//!     benchmarks/motor_reference/reference_data.json \
//!     > benchmarks/motor_nonlinear_m1/geode_m27_law_dump.csv
//! ```
//!
//! Grid: 60 log-spaced points on [1e-8, 1e-3) T (the reference's 1e-5 T
//! origin clamp sits inside), 100 uniform sub-intervals on each of the 46
//! table segments (knots included), and a 0.0025 T grid on the
//! (2.3, 3.0] T saturation tail. Values print in shortest round-trip form.

use geode_core::assembly::magnetostatic_nonlinear::{ReluctivityLaw, TabulatedBhLaw};

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: m27_law_dump <reference_data.json>");
    let text = std::fs::read_to_string(&path).expect("read reference_data.json");
    let v: serde_json::Value = serde_json::from_str(&text).expect("parse reference_data.json");
    let arr = |key: &str| -> Vec<f64> {
        v["material"][key]
            .as_array()
            .unwrap_or_else(|| panic!("material.{key} missing"))
            .iter()
            .map(|x| x.as_f64().expect("numeric"))
            .collect()
    };
    let (b, h) = (arr("B_T"), arr("H_A_per_m"));
    let law = TabulatedBhLaw::from_hb(&h, &b).expect("M27 law");

    let mut grid = Vec::new();
    for i in 0..60 {
        grid.push(10f64.powf(-8.0 + 5.0 * i as f64 / 60.0));
    }
    let n_seg = 100;
    for k in 0..b.len() - 1 {
        for j in 0..n_seg {
            grid.push(b[k] + (b[k + 1] - b[k]) * j as f64 / n_seg as f64);
        }
    }
    let b_last = *b.last().unwrap();
    grid.push(b_last);
    for i in 1..=280 {
        grid.push(b_last + 0.0025 * i as f64);
    }
    grid.sort_by(f64::total_cmp);
    grid.dedup();

    println!("B_T,H_A_per_m,nu_m_per_H,dHdB_m_per_H,dnu_db2");
    for &bb in &grid {
        let s = bb * bb;
        println!(
            "{:e},{:e},{:e},{:e},{:e}",
            bb,
            law.h_of_b(bb),
            law.nu(s),
            law.dh_db(bb),
            law.dnu_db2(s)
        );
    }
}
