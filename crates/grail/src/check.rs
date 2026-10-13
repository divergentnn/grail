//! `grail check-reference port_reference.json`: runs every case of the Python reference's port test set through the
//! Rust pipeline (full mode) and compares each intermediate value, stage by stage.
use crate::model::Model;
use crate::score::{self, Crop, Opts};
use crate::vship::Colour;
use grail_stages::Backend;
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

static HWDEC: std::sync::Mutex<score::HwDec> = std::sync::Mutex::new(score::HwDec::None);

pub fn set_hwdec(h: score::HwDec) {
    *HWDEC.lock().unwrap() = h;
}

/// (stage, tolerance on the reference platform (CUDA), tolerance on other backends, description).
/// Backend acceptance policy (docs/technical.md, "Backend acceptance"): every backend must meet the same window-level
/// tolerances and Q; only the per-frame SSIMULACRA2 rows are looser off the reference platform, where other compilers
/// and fast-math functions can move one frame's motion search (Metal: one frame of a clean CGI case off by 1.03 on
/// SSIMULACRA2(Ds, Dc), its window mean by 0.013).
const STAGES: &[(&str, f64, f64, &str)] = &[
    ("S0", 0.05, 0.05, "plain SSIMULACRA2, window mean (decode + libvship)"),
    ("S0_frame", 0.01, 0.1, "plain SSIMULACRA2, worst frame"),
    ("active", 0.0, 0.0, "active picture (reference and candidate), mismatches"),
    ("S_D", 0.25, 0.25, "SSIMULACRA2(Ds, Dc), window mean (both denoised)"),
    ("S_D_frame", 1.0, 1.5, "SSIMULACRA2(Ds, Dc), worst frame"),
    ("S_Dp", 0.25, 0.25, "SSIMULACRA2(Ds, grain-free decode), window mean"),
    ("S_Dp_frame", 1.0, 1.5, "SSIMULACRA2(Ds, grain-free decode), worst frame"),
    ("sigma_g", 0.005, 0.005, "source grain rms (8-bit units)"),
    ("P_rel", 0.02, 0.02, "band powers P_s, P_c per bin, worst relative error (bins with >= 1 % of the flat pixels)"),
    ("n_lum_rel", 0.01, 0.01, "flat pixels per luma bin, worst relative error"),
    ("chroma_rel", 0.02, 0.02, "chroma residual power, worst relative error"),
    ("stick", 0.01, 0.01, "stick / stickc medians, worst absolute error"),
    ("tex_ratio", 0.005, 0.005, "texture energy ratio"),
    ("R", 0.02, 0.02, "grainy-base capture ratio R"),
    ("path", 0.0, 0.0, "structure path mismatches"),
    ("S_struct", 0.25, 0.25, "structure term"),
    ("g", 0.005, 0.005, "grain blend weight"),
    ("penalty_sum", 0.1, 0.1, "weighted penalty sum (SSIMULACRA2 points)"),
    ("lineart_a", 0.01, 0.01, "line-art confidence a"),
    ("near_top_frame", 0.05, 0.05, "ringing peak per frame (8-bit units^2), worst"),
    ("p_L", 0.01, 0.01, "line-art penalty"),
    ("Q", 0.1, 0.1, "GRAILS score"),
];

/// PORT_SPEC.md section 12: Spearman between the port's Q and the reference's over the cases run
const SPEARMAN_GATE: f64 = 0.995;

fn f(v: &Value) -> Option<f64> {
    v.as_f64()
}

fn rel(a: f64, b: f64) -> f64 {
    (a - b).abs() / b.abs().max(1e-6)
}

fn ranks(v: &[f64]) -> Vec<f64> {
    let mut idx: Vec<usize> = (0..v.len()).collect();
    idx.sort_by(|&a, &b| v[a].partial_cmp(&v[b]).unwrap());
    let mut r = vec![0.0; v.len()];
    let mut i = 0;
    while i < idx.len() {
        let mut j = i;
        while j + 1 < idx.len() && v[idx[j + 1]] == v[idx[i]] {
            j += 1;
        }
        for k in i..=j {
            r[idx[k]] = (i + j) as f64 / 2.0;
        }
        i = j + 1;
    }
    r
}

/// Spearman rank correlation
pub fn spearman(a: &[f64], b: &[f64]) -> f64 {
    let (ra, rb) = (ranks(a), ranks(b));
    let n = a.len() as f64;
    let (ma, mb) = (ra.iter().sum::<f64>() / n, rb.iter().sum::<f64>() / n);
    let cov: f64 = ra.iter().zip(&rb).map(|(x, y)| (x - ma) * (y - mb)).sum();
    let va: f64 = ra.iter().map(|x| (x - ma).powi(2)).sum();
    let vb: f64 = rb.iter().map(|y| (y - mb).powi(2)).sum();
    cov / (va * vb).sqrt()
}

fn put_err(err: &mut BTreeMap<&str, f64>, problems: &mut Vec<String>, k: &'static str,
           v: std::result::Result<f64, String>) {
    match v {
        Ok(x) if x.is_finite() => {
            err.insert(k, x);
        }
        Ok(x) => problems.push(format!("{k}: non-finite error {x}")),
        Err(m) => problems.push(format!("{k}: {m}")),
    }
}

/// worst |a - b| over two per-frame series; a missing reference series or a length mismatch is an error
fn frame_err(mine: &[f64], refv: &Value) -> std::result::Result<f64, String> {
    let Some(a) = refv.as_array() else {
        return Err("reference per-frame values missing".into());
    };
    if a.len() != mine.len() {
        return Err(format!("per-frame length {} vs reference {}", mine.len(), a.len()));
    }
    let mut w: f64 = 0.0;
    for (x, y) in a.iter().zip(mine) {
        let x = x.as_f64().ok_or("non-numeric reference value")?;
        w = w.max((x - y).abs());
    }
    Ok(w)
}

#[allow(clippy::too_many_arguments)]
pub fn run(file: &Path, only: Option<&str>, fg: Option<PathBuf>, threads: usize, out: Option<&Path>, model: &Model,
           every: usize, offset: usize, backend: &dyn Backend) -> Result<()> {
    let d: Value = serde_json::from_str(&std::fs::read_to_string(file)?)?;
    let fg = fg.or_else(|| std::env::var("GRAIL_FULLGRAIN_LIB").ok().map(PathBuf::from));
    let reference_platform = backend.reference_platform();
    let mut worst: BTreeMap<&str, f64> = BTreeMap::new();
    let mut rows = vec![];
    // every way a case can fail: it did not run, a value is missing / non-finite / of another length, the seed view
    // differs, an invariant does not hold; stage tolerances are checked on the worst values at the end
    let mut failures: Vec<Value> = vec![];
    let mut selected = 0usize;
    for (ci, c) in d["cases"].as_array().context("cases")?.iter().enumerate() {
        let name = c["case"].as_str().unwrap_or("?");
        if only.map(|o| !name.contains(o) && !c["key"].as_str().unwrap_or("").contains(o)).unwrap_or(false) {
            continue;
        }
        if every > 1 && ci % every != offset % every {
            continue;
        }
        selected += 1;
        let key = c["key"].as_str().unwrap_or("").to_string();
        let mut fail = |what: String| {
            eprintln!("  FAIL {name}: {what}");
            failures.push(json!({"case": name, "key": key, "error": what}));
        };
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
            siting_left: false,
            hwdec: *HWDEC.lock().unwrap(),
        };
        eprintln!("case {name} ({key})");
        let t0 = std::time::Instant::now();
        let r = match score::score(&o, model, backend) {
            Ok(r) => r,
            Err(e) => {
                fail(format!("did not run: {e:#}"));
                continue;
            }
        };
        let secs = t0.elapsed().as_secs_f64();
        let mut err: BTreeMap<&str, f64> = BTreeMap::new();
        let ft = &c["features"];
        let num = |v: &Value, what: &str| -> std::result::Result<f64, String> {
            v.as_f64().ok_or_else(|| format!("reference value {what} missing"))
        };
        let mut problems: Vec<String> = vec![];
        put_err(&mut err, &mut problems, "S0", num(&c["S0_ssimulacra2"], "S0").map(|x| (r.ssimulacra2 - x).abs()));
        put_err(&mut err, &mut problems, "S0_frame", frame_err(&r.per_frame.ssimulacra2, &c["S0_frames"]));
        let ivec = |v: &Value| v.as_array().map(|a| a.iter().map(|x| x.as_u64().unwrap_or(u64::MAX) as usize).collect::<Vec<_>>());
        let mism = (ivec(&ft["active"]).as_deref() != Some(&r.active[..])) as u8
            + (ivec(&ft["cand_active"]).as_deref() != Some(&r.dist_active[..])) as u8;
        put_err(&mut err, &mut problems, "active", Ok(mism as f64));
        match (r.s_d, c["S_D"].as_f64()) {
            (Some(x), Some(y)) => {
                put_err(&mut err, &mut problems, "S_D", Ok((x - y).abs()));
                put_err(&mut err, &mut problems, "S_D_frame", frame_err(&r.per_frame.s_d, &c["S_D_frames"]));
            }
            (a, b) => problems.push(format!("S_D present in port {} / reference {}", a.is_some(), b.is_some())),
        }
        match (r.s_dp, c["S_Dp"].as_f64()) {
            (Some(x), Some(y)) => {
                put_err(&mut err, &mut problems, "S_Dp", Ok((x - y).abs()));
                put_err(&mut err, &mut problems, "S_Dp_frame", frame_err(&r.per_frame.s_dp, &c["S_Dp_frames"]));
            }
            (None, None) => {}
            (a, b) => problems.push(format!("S_D' (synthetic path) in port {} / reference {}", a.is_some(), b.is_some())),
        }
        put_err(&mut err, &mut problems, "sigma_g", num(&ft["sigma_g"], "sigma_g").map(|x| (r.sigma_g - x).abs()));
        let nl: Vec<f64> = ft["n_lum"].as_array().map(|a| a.iter().map(|x| x.as_f64().unwrap_or(f64::NAN)).collect())
            .unwrap_or_default();
        if nl.len() != 8 {
            problems.push("reference n_lum missing".into());
        } else {
            let ntot: f64 = nl.iter().sum();
            let mut prel: f64 = 0.0;
            for (mine, key) in [(&r.features.p_s, "P_s"), (&r.features.p_c, "P_c")] {
                for b in 0..5 {
                    for k in 0..8 {
                        if nl[k] >= 0.01 * ntot && nl[k] > 0.0 {
                            let rv = ft[key][b][k].as_f64().unwrap_or(f64::NAN);
                            if !(rv <= 1e-4) {
                                prel = prel.max(rel(mine[b][k], rv));
                            }
                        }
                    }
                }
            }
            put_err(&mut err, &mut problems, "P_rel", Ok(prel));
            let nrel = (0..8).filter(|&k| nl[k] > 0.0).map(|k| rel(r.features.n_lum[k], nl[k])).fold(0.0, f64::max);
            put_err(&mut err, &mut problems, "n_lum_rel", Ok(nrel));
        }
        let mut crel: f64 = 0.0;
        for (mine, key) in [(&r.features.chroma_s, "chroma_s"), (&r.features.chroma_c, "chroma_c")] {
            for i in 0..2 {
                let rv = ft[key][i].as_f64().unwrap_or(f64::NAN);
                if !(rv <= 1e-4) {
                    crel = crel.max(rel(mine[i], rv));
                }
            }
        }
        put_err(&mut err, &mut problems, "chroma_rel", Ok(crel));
        let mut st: f64 = 0.0;
        for (mine, key) in [(r.features.stick_s, "stick_s"), (r.features.stick_c, "stick_c"),
                            (r.features.stickc_s, "stickc_s"), (r.features.stickc_c, "stickc_c")] {
            match (mine, f(&ft[key])) {
                (Some(a), Some(b)) => st = st.max((a - b).abs()),
                (None, None) => {}
                (a, b) => problems.push(format!("{key} present in port {} / reference {}", a.is_some(), b.is_some())),
            }
        }
        put_err(&mut err, &mut problems, "stick", Ok(st));
        match (r.features.tex_energy_ratio, f(&ft["tex_energy_ratio"])) {
            (Some(a), Some(b)) => put_err(&mut err, &mut problems, "tex_ratio", Ok((a - b).abs())),
            (None, None) => {}
            (a, b) => problems.push(format!("tex_energy_ratio present in port {} / reference {}", a.is_some(), b.is_some())),
        }
        if f(&c["S_Dp"]).is_some() {
            match (r.grain_capture_r, f(&c["grain_capture_R"])) {
                (Some(a), Some(b)) => put_err(&mut err, &mut problems, "R", Ok((a - b).abs())),
                (a, b) => problems.push(format!("R present in port {} / reference {}", a.is_some(), b.is_some())),
            }
        }
        put_err(&mut err, &mut problems, "path", Ok((r.path != c["path_used"].as_str().unwrap_or("")) as u8 as f64));
        put_err(&mut err, &mut problems, "S_struct", num(&c["S_struct"], "S_struct").map(|x| (r.structure - x).abs()));
        put_err(&mut err, &mut problems, "g", num(&c["g"], "g").map(|x| (r.g - x).abs()));
        put_err(&mut err, &mut problems, "penalty_sum", num(&c["weighted_penalty_sum"], "weighted_penalty_sum").map(|x| (-r.grain - x).abs()));
        let la = &c["lineart"];
        put_err(&mut err, &mut problems, "lineart_a", num(&la["a"], "lineart.a").map(|x| (r.lineart.a - x).abs()));
        put_err(&mut err, &mut problems, "near_top_frame", frame_err(&r.per_frame.lineart_near_top, &la["near_top_frames"]));
        put_err(&mut err, &mut problems, "p_L", Ok((r.lineart.p_l - f(&la["p_L"]).unwrap_or(0.0)).abs()));
        let q_ref = f(&c["Q"]).unwrap_or(f64::NAN);
        let dq = r.score - q_ref;
        put_err(&mut err, &mut problems, "Q", Ok(dq.abs()));
        // invariants of the model (PORT_SPEC.md section 12): Q <= 100; Q = S0 to the last bit where g = 0 and a = 0
        if !(r.score <= 100.0) {
            problems.push(format!("Q = {} > 100", r.score));
        }
        if r.g == 0.0 && r.lineart.a == 0.0 && r.score.to_bits() != r.ssimulacra2.to_bits() {
            problems.push(format!("clean case (g = 0, a = 0) but Q {} != S0 {}", r.score, r.ssimulacra2));
        }
        let mut pen = BTreeMap::new();
        for (t, v) in c["penalties"].as_object().into_iter().flatten() {
            if let Some(m) = r.penalties.get(t) {
                pen.insert(t.clone(), json!({"ref": v, "port": m}));
            }
        }
        // the standard-decoder seed view: the summarised fields and info.template_overlap_share (0.2.2 layout);
        // info.exact_pct_mean_stock_tool (the stock av1-grain-seed's own rating) is the reference's only: not compared
        let seed_ok = {
            let sv = serde_json::to_value(&r.std_decoder)?;
            let rv = &c["seed_view"];
            let get = |v: &Value, k: &str| -> Option<Value> {
                match k.strip_prefix("info.") {
                    Some(sub) => v.get("info").and_then(|i| i.get(sub)).cloned(),
                    None => v.get(k).cloned(),
                }
            };
            ["seed_frames", "seed_pct_mean", "seed_pct_p90", "seed_pct_worst", "seed_bad_share", "seed_degenerate",
             "info.template_overlap_share"].iter().all(|k| match (get(&sv, k), get(rv, k)) {
                (Some(a), Some(b)) => match (a.as_f64(), b.as_f64()) {
                    (Some(x), Some(y)) => (x - y).abs() < 1e-6,
                    _ => false,
                },
                (None, None) => true,
                _ => false,
            })
        };
        if !seed_ok {
            problems.push("standard-decoder seed view differs from the reference".into());
        }
        eprintln!("  Q {:.3} vs {:.3} (dQ {:+.3}); S0 {:.3}; S_struct {:.3}; path {} vs {}; sigma_g {:.4}; a {:.2}; \
                   seeds {}; {:.1} s", r.score, q_ref, dq, r.ssimulacra2, r.structure, r.path,
                  c["path_used"].as_str().unwrap_or(""), r.sigma_g, r.lineart.a, if seed_ok { "match" } else { "DIFFER" },
                  secs);
        for p in &problems {
            fail(p.clone());
        }
        for (k, v) in &err {
            let w = worst.entry(k).or_insert(0.0);
            if *v > *w {
                *w = *v;
            }
        }
        rows.push(json!({"case": name, "key": key, "Q_ref": c["Q"], "Q_port": r.score, "dQ": dq,
                          "errors": err, "problems": problems, "penalties": pen, "seed_view_match": seed_ok,
                          "seconds": secs, "port": serde_json::to_value(&r)?}));
    }
    if selected == 0 {
        bail!("no case of {} matches the selection", file.display());
    }
    let mut spearman_ok = true;
    if rows.len() > 2 {
        let qa: Vec<f64> = rows.iter().map(|r| r["Q_ref"].as_f64().unwrap_or(f64::NAN)).collect();
        let qb: Vec<f64> = rows.iter().map(|r| r["Q_port"].as_f64().unwrap_or(f64::NAN)).collect();
        let mut sd: Vec<f64> = rows.iter().map(|r| r["dQ"].as_f64().unwrap_or(f64::NAN).abs()).collect();
        sd.sort_by(|a, b| a.total_cmp(b));
        let rho = spearman(&qa, &qb);
        spearman_ok = rho >= SPEARMAN_GATE;
        println!("\n{} cases: Spearman(port Q, reference Q) = {:.6} (gate {SPEARMAN_GATE}: {}); |dQ| median {:.5}, \
                  p99 {:.5}, max {:.5}", rows.len(), rho, if spearman_ok { "ok" } else { "FAILED" }, sd[sd.len() / 2],
                 sd[(sd.len() * 99 / 100).min(sd.len() - 1)], sd[sd.len() - 1]);
    }
    println!("\nper-case |dQ|:");
    for r in &rows {
        println!("  {:24} {:46} Q ref {:8.3} port {:8.3}  dQ {:+.4}", r["case"].as_str().unwrap(),
                 r["key"].as_str().unwrap(), r["Q_ref"].as_f64().unwrap_or(f64::NAN), r["Q_port"].as_f64().unwrap(),
                 r["dQ"].as_f64().unwrap_or(f64::NAN));
    }
    println!("\nper-stage worst error over {} cases ({} tolerances):", rows.len(),
             if reference_platform { "reference-platform" } else { "other-backend" });
    println!("  {:16} {:>12} {:>10}  {}", "stage", "worst", "tolerance", "");
    let mut over = vec![];
    for (k, tol_ref, tol_other, desc) in STAGES {
        let tol = if reference_platform { *tol_ref } else { *tol_other };
        let v = worst.get(k).copied();
        let ok = v.map(|x| x <= tol).unwrap_or(true);
        if !ok {
            over.push(*k);
        }
        println!("  {:16} {:>12} {:>10}  {} {}", k, v.map(|x| format!("{x:.5}")).unwrap_or("-".into()), tol,
                 if ok { "ok  " } else { "OVER" }, desc);
    }
    if let Some(p) = out {
        std::fs::write(p, serde_json::to_string_pretty(&json!({"cases": rows, "worst": worst, "failures": failures,
                                                                 "reference_platform": reference_platform}))?)?;
    }
    if failures.is_empty() && over.is_empty() && spearman_ok {
        println!("\nall {} cases ran; all stages within tolerance", rows.len());
        return Ok(());
    }
    println!("\nFAILED: {} case problem(s), stages over tolerance: {:?}{}", failures.len(), over,
             if spearman_ok { "" } else { ", Spearman gate" });
    bail!("check-reference failed ({} case problem(s), {} stage(s) over tolerance{})", failures.len(), over.len(),
          if spearman_ok { "" } else { ", Spearman below the gate" })
}

/// fast mode against the reference's fast-mode inputs (variants.py: S0 and structure on the subsample, the grain
/// features on the subsample, AV1 synthetic grain not denoised). The reference's fast Q is formed here from those
/// inputs with the v2.1 model (sigma_g of the subsample; line-art peak over the subsample's frames).
#[allow(clippy::too_many_arguments)]
pub fn run_fast(file: &Path, variants: &str, only: Option<&str>, fg: Option<PathBuf>, threads: usize, out: Option<&Path>,
                model: &Model, backend: &dyn Backend, skip_missing: bool) -> Result<()> {
    use crate::model::{self as M, Feats};
    let d: Value = serde_json::from_str(&std::fs::read_to_string(file)?)?;
    let mut var: BTreeMap<String, Value> = BTreeMap::new();
    for fname in variants.split(',') {
        for ln in std::fs::read_to_string(fname.trim())?.lines().filter(|l| !l.trim().is_empty()) {
            // Python's json writes NaN / Infinity, which JSON does not have
            let ln = ln.replace("-Infinity", "null").replace("Infinity", "null").replace("NaN", "null");
            let v: Value = serde_json::from_str(&ln)?;
            var.insert(v["key"].as_str().unwrap_or("").to_string(), v);
        }
    }
    let fg = fg.or_else(|| std::env::var("GRAIL_FULLGRAIN_LIB").ok().map(PathBuf::from));
    let mut rows = vec![];
    let mut worst: BTreeMap<&str, f64> = BTreeMap::new();
    let mut failures: Vec<Value> = vec![];
    let mut skipped: Vec<String> = vec![];
    let mut selected = 0usize;
    for c in d["cases"].as_array().context("cases")? {
        let name = c["case"].as_str().unwrap_or("?");
        let key = c["key"].as_str().unwrap_or("");
        if only.map(|o| !name.contains(o) && !key.contains(o)).unwrap_or(false) {
            continue;
        }
        let Some(v) = var.get(key) else {
            if skip_missing {
                eprintln!("  skipped {name}: no fast-mode reference row (--skip-missing)");
                skipped.push(key.to_string());
            } else {
                eprintln!("  FAIL {name}: no fast-mode reference row");
                failures.push(json!({"case": name, "key": key, "error": "no fast-mode reference row"}));
            }
            continue;
        };
        selected += 1;
        let fa = &v["fast"];
        let inp = &c["inputs"];
        let s = inp["start_frame"].as_u64().unwrap() as usize;
        let e = inp["end_frame"].as_u64().unwrap() as usize;
        let n = e - s;
        let rect = |v: &Value| -> [usize; 4] {
            let a: Vec<usize> = v.as_array().unwrap().iter().map(|x| x.as_u64().unwrap() as usize).collect();
            [a[0], a[1], a[2], a[3]]
        };
        let o = Opts {
            reference: PathBuf::from(inp["ref"]["path"].as_str().unwrap()),
            dist: PathBuf::from(inp["cand"]["path"].as_str().unwrap()),
            fast: true,
            denoise_candidate: false,
            start: inp["ref"]["lead"].as_u64().unwrap() as usize + s,
            dist_start: Some(inp["cand"]["lead"].as_u64().unwrap() as usize + s),
            frames: Some(n),
            crop: Crop::Rect(rect(&inp["ref"]["crop"])),
            dist_crop: Some(rect(&inp["cand"]["crop"])),
            colour: Some(if inp["colour"].as_str() == Some("pq") { Colour::Pq } else { Colour::Sdr }),
            fullgrain_lib: fg.clone(),
            threads,
            timings: false,
            siting_left: true,       // variants.py scored the plain pair in-process (vship_api: left on both sides)
            hwdec: *HWDEC.lock().unwrap(),
        };
        eprintln!("case {name} ({key}) fast");
        let t0 = std::time::Instant::now();
        let r = match score::score(&o, model, backend) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("  FAIL {name}: did not run: {e:#}");
                failures.push(json!({"case": name, "key": key, "error": format!("did not run: {e:#}")}));
                continue;
            }
        };
        let secs = t0.elapsed().as_secs_f64();
        // reference fast Q from its inputs
        let idx: Vec<usize> = (0..n).step_by(4).flat_map(|a| [a, a + 1]).filter(|&t| t < n).collect();
        let la = &c["lineart"];
        let near: Vec<f64> = la["near_top_frames"].as_array().map(|a| a.iter().map(|x| x.as_f64().unwrap()).collect()).unwrap_or_default();
        let peak = idx.iter().filter_map(|&t| near.get(t)).cloned().fold(0.0, f64::max);
        let a = f(&la["a"]).unwrap_or(0.0);
        let s0 = f(&fa["s0_ssimu2"]).unwrap();
        let mut err: BTreeMap<&str, f64> = BTreeMap::new();
        err.insert("S0", (r.ssimulacra2 - s0).abs());
        let (q_ref, g_ref) = if v["clean_shortcut"].as_bool() == Some(true) {
            (s0, 0.0)
        } else {
            let ff: Feats = {
                let x = &fa["features"];
                let vv = |k: &str| -> Vec<Vec<f64>> { x[k].as_array().unwrap().iter().map(|r| r.as_array().unwrap().iter().map(|y| y.as_f64().unwrap()).collect()).collect() };
                let v1 = |k: &str| -> Vec<f64> { x[k].as_array().unwrap().iter().map(|y| y.as_f64().unwrap()).collect() };
                Feats { sigma_g: x["sigma_g"].as_f64().unwrap(), flat_frac: x["flat_frac"].as_f64().unwrap_or(0.0),
                        p_s: vv("P_s"), p_c: vv("P_c"), n_lum: v1("n_lum"), chroma_s: v1("chroma_s"), chroma_c: v1("chroma_c"),
                        stick_s: f(&x["stick_s"]), stick_c: f(&x["stick_c"]), stickc_s: f(&x["stickc_s"]),
                        stickc_c: f(&x["stickc_c"]), tex_energy_ratio: f(&x["tex_energy_ratio"]), p_ng: None }
            };
            let synth = v["synth"].as_bool() == Some(true);
            let sr = f(&fa["st_ssimu2"]).unwrap();
            let (st, _) = M::structure(model, ff.sigma_g, synth, None, Some(sr), Some(sr));
            let pen = M::penalties(&ff);
            let wsum: f64 = model.terms.iter().zip(&model.weights).map(|(t, w)| w * pen.get(t).copied().unwrap_or(0.0)).sum();
            let g = M::smoothstep(ff.sigma_g, model.sig.0, model.sig.1);
            let raw = if synth { r.s_dp } else { r.s_d };
            if let Some(x) = raw {
                err.insert("S_raw", (x - sr).abs());
            }
            err.insert("sigma_g", (r.sigma_g - ff.sigma_g).abs());
            let mut prel: f64 = 0.0;
            let ntot: f64 = ff.n_lum.iter().sum();
            for (mine, refp) in [(&r.features.p_s, &ff.p_s), (&r.features.p_c, &ff.p_c)] {
                for b in 0..5 {
                    for k in 0..8 {
                        if ff.n_lum[k] >= 0.01 * ntot && ff.n_lum[k] > 0.0 && refp[b][k] > 1e-4 {
                            prel = prel.max(rel(mine[b][k], refp[b][k]));
                        }
                    }
                }
            }
            err.insert("P_rel", prel);
            err.insert("penalty_sum", (-r.grain - wsum).abs());
            err.insert("S_struct", (r.structure - st).abs());
            (s0 + g * (st - s0 - wsum), g)
        };
        let lterm = if a > 0.0 { -(1.0 - g_ref) * a * model.la_w * M::lineart_penalty(model, peak) } else { 0.0 };
        let q_ref = q_ref + lterm;
        let dq = r.score - q_ref;
        err.insert("Q", dq.abs());
        eprintln!("  fast Q {:.3} vs {:.3} (dQ {:+.4}); S0 {:.3} vs {:.3}; frames {}; {:.1} s", r.score, q_ref, dq,
                  r.ssimulacra2, s0, r.frames_scored, secs);
        let mut problems: Vec<String> = err.iter().filter(|(_, x)| !x.is_finite()).map(|(k, x)| format!("{k}: {x}")).collect();
        if !(r.score <= 100.0) {
            problems.push(format!("Q = {} > 100", r.score));
        }
        if r.g == 0.0 && r.lineart.a == 0.0 && r.score.to_bits() != r.ssimulacra2.to_bits() {
            problems.push(format!("clean case (g = 0, a = 0) but Q {} != S0 {}", r.score, r.ssimulacra2));
        }
        for p in &problems {
            eprintln!("  FAIL {name}: {p}");
            failures.push(json!({"case": name, "key": key, "error": p}));
        }
        for (k, x) in &err {
            let w = worst.entry(k).or_insert(0.0);
            if x.is_finite() && *x > *w {
                *w = *x;
            }
        }
        rows.push(json!({"case": name, "key": key, "Q_ref_fast": q_ref, "Q_port_fast": r.score, "dQ": dq, "errors": err,
                          "Q_full_ref": c["Q"], "seconds": secs}));
    }
    println!("\nfast mode, per-case |dQ| (reference = its fast-mode inputs through the v2.1 model):");
    for r in &rows {
        println!("  {:24} {:46} Q ref {:8.3} port {:8.3}  dQ {:+.4}   (full-mode Q {:8.3})", r["case"].as_str().unwrap(),
                 r["key"].as_str().unwrap(), r["Q_ref_fast"].as_f64().unwrap(), r["Q_port_fast"].as_f64().unwrap(),
                 r["dQ"].as_f64().unwrap(), r["Q_full_ref"].as_f64().unwrap());
    }
    // the full mode's window-level tolerances
    let tol = |k: &str| STAGES.iter().find(|s| s.0 == k).map(|s| s.1).unwrap_or(match k {
        "S_raw" => 0.25,
        _ => 0.0,
    });
    println!("\nfast mode, per-stage worst error:");
    let mut over = vec![];
    for (k, x) in &worst {
        let ok = *x <= tol(k);
        if !ok {
            over.push(*k);
        }
        println!("  {:16} {:>12.5} {:>10}  {}", k, x, tol(k), if ok { "ok" } else { "OVER" });
    }
    if let Some(p) = out {
        std::fs::write(p, serde_json::to_string_pretty(&json!({"cases": rows, "worst": worst, "failures": failures,
                                                                 "skipped": skipped}))?)?;
    }
    if selected == 0 && failures.is_empty() {
        bail!("no case of {} matches the selection (with a fast-mode reference row)", file.display());
    }
    if failures.is_empty() && over.is_empty() {
        println!("\nall {} cases ran; all stages within tolerance{}", rows.len(),
                 if skipped.is_empty() { String::new() } else { format!(" ({} without a fast-mode row skipped)", skipped.len()) });
        return Ok(());
    }
    bail!("check-reference --fast-variants failed ({} case problem(s), stages over tolerance: {:?})", failures.len(), over)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_errors_are_strict() {
        assert_eq!(frame_err(&[1.0, 2.0], &json!([1.0, 2.5])).unwrap(), 0.5);
        assert!(frame_err(&[1.0, 2.0], &json!([1.0])).is_err());
        assert!(frame_err(&[1.0], &Value::Null).is_err());
        assert!(frame_err(&[1.0], &json!(["x"])).is_err());
        let mut err = BTreeMap::new();
        let mut problems = vec![];
        put_err(&mut err, &mut problems, "Q", Ok(f64::NAN));
        assert_eq!(problems.len(), 1);
        assert!(err.is_empty());
    }

    #[test]
    fn spearman_basics() {
        assert!((spearman(&[1.0, 2.0, 3.0, 4.0], &[10.0, 20.0, 30.0, 40.0]) - 1.0).abs() < 1e-12);
        assert!((spearman(&[1.0, 2.0, 3.0, 4.0], &[4.0, 3.0, 2.0, 1.0]) + 1.0).abs() < 1e-12);
    }
}
