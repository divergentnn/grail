//! `grail check-reference port_reference.json`: runs every case of the Python reference's port test set through the
//! Rust pipeline (full mode) and compares each intermediate value, stage by stage.
use crate::model::Model;
use crate::score::{self, Crop, Opts};
use crate::vship::Colour;
use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// (stage, tolerance, description)
const STAGES: &[(&str, f64, &str)] = &[
    ("S0", 0.05, "plain SSIMULACRA2, window mean (decode + libvship)"),
    ("S0_frame", 0.01, "plain SSIMULACRA2, worst frame"),
    ("active", 0.0, "active picture (reference and candidate), mismatches"),
    ("S_D", 0.25, "SSIMULACRA2(Ds, Dc), window mean (both denoised)"),
    ("S_D_frame", 1.0, "SSIMULACRA2(Ds, Dc), worst frame"),
    ("S_Dp", 0.25, "SSIMULACRA2(Ds, grain-free decode), window mean"),
    ("S_Dp_frame", 1.0, "SSIMULACRA2(Ds, grain-free decode), worst frame"),
    ("sigma_g", 0.005, "source grain rms (8-bit units)"),
    ("P_rel", 0.02, "band powers P_s, P_c per bin, worst relative error (bins with >= 1 % of the flat pixels)"),
    ("n_lum_rel", 0.01, "flat pixels per luma bin, worst relative error"),
    ("chroma_rel", 0.02, "chroma residual power, worst relative error"),
    ("stick", 0.01, "stick / stickc medians, worst absolute error"),
    ("tex_ratio", 0.005, "texture energy ratio"),
    ("R", 0.02, "grainy-base capture ratio R"),
    ("path", 0.0, "structure path mismatches"),
    ("S_struct", 0.25, "structure term"),
    ("g", 0.005, "grain blend weight"),
    ("penalty_sum", 0.1, "weighted penalty sum (SSIMULACRA2 points)"),
    ("lineart_a", 0.01, "line-art confidence a"),
    ("near_top_frame", 0.05, "ringing peak per frame (8-bit units^2), worst"),
    ("p_L", 0.01, "line-art penalty"),
    ("Q", 0.1, "GRAILS score"),
];

fn f(v: &Value) -> Option<f64> {
    v.as_f64()
}

fn rel(a: f64, b: f64) -> f64 {
    (a - b).abs() / b.abs().max(1e-6)
}

pub fn run(file: &Path, only: Option<&str>, fg: Option<PathBuf>, threads: usize, out: Option<&Path>, model: &Model)
           -> Result<()> {
    let d: Value = serde_json::from_str(&std::fs::read_to_string(file)?)?;
    let fg = fg.or_else(|| std::env::var("GRAIL_FULLGRAIN_LIB").ok().map(PathBuf::from));
    let mut worst: BTreeMap<&str, f64> = BTreeMap::new();
    let mut rows = vec![];
    for c in d["cases"].as_array().context("cases")? {
        let name = c["case"].as_str().unwrap_or("?");
        if only.map(|o| !name.contains(o) && !c["key"].as_str().unwrap_or("").contains(o)).unwrap_or(false) {
            continue;
        }
        let inp = &c["inputs"];
        let s = inp["start_frame"].as_u64().unwrap() as usize;
        let e = inp["end_frame"].as_u64().unwrap() as usize;
        let rect = |v: &Value| -> [usize; 4] {
            let a: Vec<usize> = v.as_array().unwrap().iter().map(|x| x.as_u64().unwrap() as usize).collect();
            [a[0], a[1], a[2], a[3]]
        };
        let o = Opts {
            reference: PathBuf::from(inp["ref"]["path"].as_str().unwrap()),
            dist: PathBuf::from(inp["cand"]["path"].as_str().unwrap()),
            fast: false,
            denoise_candidate: false,
            start: inp["ref"]["lead"].as_u64().unwrap() as usize + s,
            dist_start: Some(inp["cand"]["lead"].as_u64().unwrap() as usize + s),
            frames: Some(e - s),
            crop: Crop::Rect(rect(&inp["ref"]["crop"])),
            dist_crop: Some(rect(&inp["cand"]["crop"])),
            colour: Some(if inp["colour"].as_str() == Some("pq") { Colour::Pq } else { Colour::Sdr }),
            fullgrain_lib: fg.clone(),
            threads,
            timings: false,
        };
        eprintln!("case {name} ({})", c["key"].as_str().unwrap_or(""));
        let t0 = std::time::Instant::now();
        let r = score::score(&o, model).with_context(|| format!("case {name}"))?;
        let secs = t0.elapsed().as_secs_f64();
        let mut err: BTreeMap<&str, f64> = BTreeMap::new();
        let ft = &c["features"];
        err.insert("S0", (r.ssimulacra2 - f(&c["S0_ssimulacra2"]).unwrap()).abs());
        let fmax = |mine: &[f64], refv: &Value| -> f64 {
            refv.as_array().map(|a| a.iter().zip(mine).map(|(x, y)| (x.as_f64().unwrap() - y).abs()).fold(0.0, f64::max))
                .unwrap_or(f64::NAN)
        };
        err.insert("S0_frame", fmax(&r.per_frame.ssimulacra2, &c["S0_frames"]));
        let act_ref = ft["active"].as_array().map(|a| a.iter().map(|x| x.as_u64().unwrap() as usize).collect::<Vec<_>>());
        let cact_ref = ft["cand_active"].as_array().map(|a| a.iter().map(|x| x.as_u64().unwrap() as usize).collect::<Vec<_>>());
        let mism = (act_ref.as_deref() != Some(&r.active[..])) as u8 + (cact_ref.as_deref() != Some(&r.dist_active[..])) as u8;
        err.insert("active", mism as f64);
        if let Some(x) = r.s_d {
            err.insert("S_D", (x - f(&c["S_D"]).unwrap()).abs());
            err.insert("S_D_frame", fmax(&r.per_frame.s_d, &c["S_D_frames"]));
        }
        if let (Some(x), Some(y)) = (r.s_dp, f(&c["S_Dp"])) {
            err.insert("S_Dp", (x - y).abs());
            err.insert("S_Dp_frame", fmax(&r.per_frame.s_dp, &c["S_Dp_frames"]));
        }
        err.insert("sigma_g", (r.sigma_g - f(&ft["sigma_g"]).unwrap()).abs());
        let nl: Vec<f64> = ft["n_lum"].as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect();
        let ntot: f64 = nl.iter().sum();
        let mut prel: f64 = 0.0;
        for (mine, key) in [(&r.features.p_s, "P_s"), (&r.features.p_c, "P_c")] {
            for b in 0..5 {
                for k in 0..8 {
                    if nl[k] >= 0.01 * ntot && nl[k] > 0.0 {
                        let rv = ft[key][b][k].as_f64().unwrap();
                        if rv > 1e-4 {
                            prel = prel.max(rel(mine[b][k], rv));
                        }
                    }
                }
            }
        }
        err.insert("P_rel", prel);
        err.insert("n_lum_rel", (0..8).filter(|&k| nl[k] > 0.0).map(|k| rel(r.features.n_lum[k], nl[k])).fold(0.0, f64::max));
        let mut crel: f64 = 0.0;
        for (mine, key) in [(&r.features.chroma_s, "chroma_s"), (&r.features.chroma_c, "chroma_c")] {
            for i in 0..2 {
                let rv = ft[key][i].as_f64().unwrap();
                if rv > 1e-4 {
                    crel = crel.max(rel(mine[i], rv));
                }
            }
        }
        err.insert("chroma_rel", crel);
        let mut st: f64 = 0.0;
        for (mine, key) in [(r.features.stick_s, "stick_s"), (r.features.stick_c, "stick_c"),
                            (r.features.stickc_s, "stickc_s"), (r.features.stickc_c, "stickc_c")] {
            if let (Some(a), Some(b)) = (mine, f(&ft[key])) {
                st = st.max((a - b).abs());
            }
        }
        err.insert("stick", st);
        if let (Some(a), Some(b)) = (r.features.tex_energy_ratio, f(&ft["tex_energy_ratio"])) {
            err.insert("tex_ratio", (a - b).abs());
        }
        if let (Some(a), Some(b)) = (r.grain_capture_r, f(&c["grain_capture_R"])) {
            if f(&c["S_Dp"]).is_some() {
                err.insert("R", (a - b).abs());
            }
        }
        err.insert("path", (r.path != c["path_used"].as_str().unwrap_or("")) as u8 as f64);
        err.insert("S_struct", (r.structure - f(&c["S_struct"]).unwrap()).abs());
        err.insert("g", (r.g - f(&c["g"]).unwrap()).abs());
        err.insert("penalty_sum", (-r.grain - f(&c["weighted_penalty_sum"]).unwrap()).abs());
        let la = &c["lineart"];
        err.insert("lineart_a", (r.lineart.a - f(&la["a"]).unwrap()).abs());
        err.insert("near_top_frame", fmax(&r.per_frame.lineart_near_top, &la["near_top_frames"]));
        err.insert("p_L", (r.lineart.p_l - f(&la["p_L"]).unwrap_or(0.0)).abs());
        let dq = r.score - f(&c["Q"]).unwrap();
        err.insert("Q", dq.abs());
        let mut pen = BTreeMap::new();
        for (t, v) in c["penalties"].as_object().unwrap() {
            if let Some(m) = r.penalties.get(t) {
                pen.insert(t.clone(), json!({"ref": v, "port": m}));
            }
        }
        let seed_ok = {
            let sv = serde_json::to_value(&r.std_decoder)?;
            let rv = &c["seed_view"];
            ["seed_frames", "seed_pct_mean", "seed_pct_p90", "seed_pct_worst", "seed_bad_share", "seed_degenerate",
             "seed_overlap_share"].iter().all(|k| match (sv.get(*k), rv.get(*k)) {
                (Some(a), Some(b)) => (a.as_f64().unwrap_or(-1.0) - b.as_f64().unwrap_or(-2.0)).abs() < 1e-6,
                (None, None) => true,
                _ => false,
            })
        };
        eprintln!("  Q {:.3} vs {:.3} (dQ {:+.3}); S0 {:.3} vs {:.3}; S_struct {:.3} vs {:.3}; path {} vs {}; \
                   sigma_g {:.4} vs {:.4}; a {:.2}; seeds {}; {:.1} s",
                  r.score, f(&c["Q"]).unwrap(), dq, r.ssimulacra2, f(&c["S0_ssimulacra2"]).unwrap(), r.structure,
                  f(&c["S_struct"]).unwrap(), r.path, c["path_used"].as_str().unwrap_or(""), r.sigma_g,
                  f(&ft["sigma_g"]).unwrap(), r.lineart.a, if seed_ok { "match" } else { "DIFFER" }, secs);
        for (k, v) in &err {
            let w = worst.entry(k).or_insert(0.0);
            if v.is_finite() && *v > *w {
                *w = *v;
            }
        }
        rows.push(json!({"case": name, "key": c["key"], "Q_ref": c["Q"], "Q_port": r.score, "dQ": dq,
                          "errors": err, "penalties": pen, "seed_view_match": seed_ok, "seconds": secs,
                          "port": serde_json::to_value(&r)?}));
    }
    println!("\nper-case |dQ|:");
    for r in &rows {
        println!("  {:24} {:46} Q ref {:8.3} port {:8.3}  dQ {:+.4}", r["case"].as_str().unwrap(),
                 r["key"].as_str().unwrap(), r["Q_ref"].as_f64().unwrap(), r["Q_port"].as_f64().unwrap(),
                 r["dQ"].as_f64().unwrap());
    }
    println!("\nper-stage worst error over {} cases:", rows.len());
    println!("  {:16} {:>12} {:>10}  {}", "stage", "worst", "tolerance", "");
    let mut ok_all = true;
    for (k, tol, desc) in STAGES {
        let v = worst.get(k).copied();
        let ok = v.map(|x| x <= *tol).unwrap_or(true);
        ok_all &= ok;
        println!("  {:16} {:>12} {:>10}  {} {}", k, v.map(|x| format!("{x:.5}")).unwrap_or("-".into()), tol,
                 if ok { "ok  " } else { "OVER" }, desc);
    }
    if let Some(p) = out {
        std::fs::write(p, serde_json::to_string_pretty(&json!({"cases": rows, "worst": worst}))?)?;
    }
    println!("\n{}", if ok_all { "all stages within tolerance" } else { "some stages over tolerance (see above)" });
    Ok(())
}
