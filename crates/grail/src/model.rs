//! The GRAILS model (0.2.1 = the lab's v2.1) on the CPU: penalties from the raw features, the two structure paths,
//! the composite, the line-art term (evaluate.py: penalties, structure, grain_capture, model_score, lineart_*).
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;

pub const WEIGHTS_JSON: &str = include_str!("../data/weights.json");
pub const NB: usize = 8;
const NVIS: f64 = 0.25;
const NVIS_C: f64 = 0.15;

#[derive(Clone, Debug)]
pub struct Model {
    pub version: String,
    pub terms: Vec<String>,
    pub weights: Vec<f64>,
    pub sig: (f64, f64),
    pub k_coded: f64,
    pub e_coded: f64,
    pub k_ceil: f64,
    pub e_ceil: f64,
    pub p: f64,
    pub grainy_base_r: f64,
    pub la_lo: f64,
    pub la_hi: f64,
    pub la_tex_lo: f64,
    pub la_tex_hi: f64,
    pub la_tau: f64,
    pub la_w: f64,
}

impl Model {
    pub fn builtin() -> Model {
        Model::from_json(WEIGHTS_JSON).expect("built-in weights.json")
    }
    pub fn from_json(s: &str) -> anyhow::Result<Model> {
        let v: Value = serde_json::from_str(s)?;
        let f = |x: &Value| x.as_f64().unwrap_or(0.0);
        let st = &v["struct"];
        let la = &v["lineart"];
        Ok(Model {
            version: v["version"].as_str().unwrap_or("?").to_string(),
            terms: v["terms"].as_array().unwrap().iter().map(|t| t.as_str().unwrap().to_string()).collect(),
            weights: v["weights"].as_array().unwrap().iter().map(f).collect(),
            sig: (f(&v["sig"][0]), f(&v["sig"][1])),
            k_coded: f(&st["k_coded"]),
            e_coded: f(&st["e_coded"]),
            k_ceil: f(&st["k_ceil"]),
            e_ceil: f(&st["e_ceil"]),
            p: f(&st["p"]),
            grainy_base_r: f(&st["grainy_base_R"]),
            la_lo: f(&la["lo"]),
            la_hi: f(&la["hi"]),
            la_tex_lo: f(&la["tex_lo"]),
            la_tex_hi: f(&la["tex_hi"]),
            la_tau: f(&la["tau"]),
            la_w: f(&la["w"]),
        })
    }
}

pub fn smoothstep(x: f64, lo: f64, hi: f64) -> f64 {
    let t = ((x - lo) / (hi - lo)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// the raw features that carry weight in v2.1 (extract.features)
#[derive(Clone, Debug, Serialize, Default)]
pub struct Feats {
    pub sigma_g: f64,
    pub flat_frac: f64,
    #[serde(rename = "P_s")]
    pub p_s: Vec<Vec<f64>>,
    #[serde(rename = "P_c")]
    pub p_c: Vec<Vec<f64>>,
    pub n_lum: Vec<f64>,
    pub chroma_s: Vec<f64>,
    pub chroma_c: Vec<f64>,
    pub stick_s: Option<f64>,
    pub stick_c: Option<f64>,
    pub stickc_s: Option<f64>,
    pub stickc_c: Option<f64>,
    pub tex_energy_ratio: Option<f64>,
    /// grainy-base check: P_c of (candidate - grain-free decode), bands 0..2 per bin (normalized)
    #[serde(skip)]
    pub p_ng: Option<Vec<Vec<f64>>>,
}

pub fn median(v: &[f64]) -> Option<f64> {
    if v.is_empty() {
        return None;
    }
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = s.len();
    Some(if n % 2 == 1 { s[n / 2] } else { 0.5 * (s[n / 2 - 1] + s[n / 2]) })
}

/// evaluate.penalties (the terms GRAILS v2.1 uses, plus `under` and `soft` for reference; weight 0)
pub fn penalties(f: &Feats) -> BTreeMap<String, f64> {
    let n = &f.n_lum;
    let tot: f64 = n.iter().sum();
    let sh: Vec<f64> = n.iter().map(|x| x / tot.max(1.0)).collect();
    let ok: Vec<bool> = n.iter().map(|x| *x > 0.0).collect();
    let band = |p: &Vec<Vec<f64>>, k: usize, b: std::ops::Range<usize>| -> f64 { b.map(|i| p[i][k]).sum() };
    let mut p = BTreeMap::new();
    let (mut under, mut under_amp, mut over_amp, mut over, mut soft, mut blob, mut gvis) = (0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    for k in 0..NB {
        if !ok[k] {
            continue;
        }
        let gs = band(&f.p_s, k, 0..3);
        let gc = band(&f.p_c, k, 0..3);
        let vis = gs / (gs + NVIS * NVIS);
        let vis_c = vis.max(gc / (gc + NVIS * NVIS));
        let amp = (gc.sqrt() - gs.sqrt()) / (gs.sqrt() + NVIS);
        under += sh[k] * vis * (-amp).max(0.0);
        under_amp += sh[k] * (gs.sqrt() - gc.sqrt()).max(0.0);
        over_amp += sh[k] * (gc.sqrt() - gs.sqrt()).max(0.0);
        over += sh[k] * vis_c * amp.max(0.0);
        let fs_s = f.p_s[0][k] / gs.max(1e-9);
        let fs_c = f.p_c[0][k] / gc.max(1e-9);
        soft += sh[k] * vis * ((fs_s + 1e-3) / (fs_c + 1e-3)).ln().max(0.0);
        let cs_s = band(&f.p_s, k, 3..5) / gs.max(1e-9);
        let cs_c = band(&f.p_c, k, 3..5) / gc.max(1e-9);
        blob += sh[k] * vis * ((cs_c + 1e-3) / (cs_s + 1e-3)).ln().max(0.0);
        gvis += sh[k] * vis;
    }
    let mut chroma = 0.0;
    for i in 0..2 {
        let (cs, cc) = (f.chroma_s[i], f.chroma_c[i]);
        let vc = cs / (cs + NVIS_C * NVIS_C);
        chroma += vc.max(cc / (cc + NVIS_C * NVIS_C)) * (cc.sqrt() - cs.sqrt()).abs() / (cs.sqrt() + NVIS_C);
    }
    chroma /= 2.0;
    let z = |a: Option<f64>, b: Option<f64>| (a.unwrap_or(0.0) - b.unwrap_or(0.0)).max(0.0);
    p.insert("under".into(), under);
    p.insert("under_amp".into(), under_amp);
    p.insert("over_amp".into(), over_amp);
    p.insert("over".into(), over);
    p.insert("soft".into(), soft);
    p.insert("blob".into(), blob);
    p.insert("chroma".into(), chroma);
    p.insert("stick".into(), z(f.stick_c, f.stick_s) * gvis);
    p.insert("stickc".into(), z(f.stickc_c, f.stickc_s) * gvis);
    // 0.2.1 compatibility quirk, kept on purpose: a texture energy ratio of exactly 0 (the candidate has no energy in
    // the source's textured tiles) gives NO texloss, because the reference reads `tex_energy_ratio or 1`
    // (evaluate.py:132), while PORT_SPEC.md's formula max(0, -ln(ratio)) would make it infinite. Changing it would
    // change scores; it is documented in docs/technical.md ("Compatibility quirks").
    let ter = match f.tex_energy_ratio {
        Some(r) if r != 0.0 => r,
        _ => 1.0,
    };
    p.insert("texloss".into(), (-ter.ln()).max(0.0));
    p
}

/// evaluate.grain_capture: R = sum share * P_c[:3] / sum share * P_ng[:3]
pub fn grain_capture(f: &Feats) -> Option<f64> {
    let ng = f.p_ng.as_ref()?;
    let tot: f64 = f.n_lum.iter().sum();
    let mut num = 0.0;
    let mut den = 0.0;
    for k in 0..NB {
        let w = f.n_lum[k] / tot.max(1.0);
        num += w * (0..3).map(|b| f.p_c[b][k]).sum::<f64>();
        den += w * (0..3).map(|b| ng[b][k]).sum::<f64>();
    }
    Some(num / den.max(1e-9))
}

/// evaluate.structure -> (S_struct, path)
pub fn structure(m: &Model, sigma_g: f64, synth: bool, r: Option<f64>, s_d: Option<f64>, s_dp: Option<f64>) -> (f64, &'static str) {
    let mut path = if synth { "synth" } else { "coded" };
    if path == "synth" && m.grainy_base_r > 0.0 && r.unwrap_or(0.0) > m.grainy_base_r {
        path = "coded";
    }
    let p = m.p;
    let mut d2 = if path == "synth" && s_dp.is_some() {
        (100.0 - s_dp.unwrap()).powf(p)
    } else {
        path = "coded";
        (100.0 - s_d.unwrap_or(f64::NAN)).max(0.0).powf(p) - (m.k_coded * sigma_g.powf(m.e_coded)).powf(p)
    };
    d2 -= (m.k_ceil * sigma_g.powf(m.e_ceil)).powf(p);
    (100.0 - d2.max(0.0).powf(1.0 / p), path)
}

pub fn lineart_confidence(m: &Model, inkflat_s: f64, tex6: f64) -> f64 {
    smoothstep(inkflat_s, m.la_lo, m.la_hi) * (1.0 - smoothstep(tex6, m.la_tex_lo, m.la_tex_hi))
}

pub fn lineart_penalty(m: &Model, peak: f64) -> f64 {
    (peak.max(0.0).sqrt() - m.la_tau).max(0.0)
}

#[cfg(test)]
mod tests {
    //! The model on the reference's own features (no GPU, no media): penalties, structure and Q of the 13
    //! port_reference cases must match the Python reference to rounding.
    use super::*;
    use serde_json::Value;

    fn feats(x: &Value) -> Feats {
        let vv = |k: &str| -> Vec<Vec<f64>> {
            x[k].as_array().unwrap().iter().map(|r| r.as_array().unwrap().iter().map(|y| y.as_f64().unwrap()).collect()).collect()
        };
        let v1 = |k: &str| -> Vec<f64> { x[k].as_array().unwrap().iter().map(|y| y.as_f64().unwrap()).collect() };
        Feats {
            sigma_g: x["sigma_g"].as_f64().unwrap(),
            flat_frac: x["flat_frac"].as_f64().unwrap(),
            p_s: vv("P_s"),
            p_c: vv("P_c"),
            n_lum: v1("n_lum"),
            chroma_s: v1("chroma_s"),
            chroma_c: v1("chroma_c"),
            stick_s: x["stick_s"].as_f64(),
            stick_c: x["stick_c"].as_f64(),
            stickc_s: x["stickc_s"].as_f64(),
            stickc_c: x["stickc_c"].as_f64(),
            tex_energy_ratio: x["tex_energy_ratio"].as_f64(),
            p_ng: None,
        }
    }

    #[test]
    fn reference_cases_model_level() {
        let d: Value = serde_json::from_str(include_str!("../tests/data/port_reference_model.json")).unwrap();
        let m = Model::builtin();
        for c in d["cases"].as_array().unwrap() {
            let name = c["case"].as_str().unwrap();
            let f = feats(&c["features"]);
            let pen = penalties(&f);
            for (t, v) in c["penalties"].as_object().unwrap() {
                if let Some(mine) = pen.get(t) {
                    assert!((mine - v.as_f64().unwrap()).abs() < 1e-9, "{name}: penalty {t} {mine} vs {v}");
                }
            }
            let wsum: f64 = m.terms.iter().zip(&m.weights).map(|(t, w)| w * pen.get(t).copied().unwrap_or(0.0)).sum();
            assert!((wsum - c["weighted_penalty_sum"].as_f64().unwrap()).abs() < 1e-9, "{name}: weighted sum");
            let synth = c["features"]["path"].as_str() == Some("synth");
            let r = c["S_Dp"].as_f64().map(|_| c["grain_capture_R"].as_f64().unwrap());
            let (st, path) = structure(&m, f.sigma_g, synth, r, c["S_D"].as_f64(), c["S_Dp"].as_f64());
            assert_eq!(path, c["path_used"].as_str().unwrap(), "{name}: path");
            assert!((st - c["S_struct"].as_f64().unwrap()).abs() < 1e-9, "{name}: S_struct {st}");
            let g = smoothstep(f.sigma_g, m.sig.0, m.sig.1);
            assert!((g - c["g"].as_f64().unwrap()).abs() < 1e-12, "{name}: g");
            let s0 = c["S0_ssimulacra2"].as_f64().unwrap();
            let mut q = s0 + g * (st - s0 - wsum);
            let la = &c["lineart"];
            let a = lineart_confidence(&m, la["inkflat_s"].as_f64().unwrap_or(0.0), la["tex6"].as_f64().unwrap_or(0.0));
            assert!((a - la["a"].as_f64().unwrap()).abs() < 1e-12, "{name}: a");
            if a > 0.0 {
                let peak = la["near_top_frames"].as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).fold(0.0, f64::max);
                q -= (1.0 - g) * a * m.la_w * lineart_penalty(&m, peak);
            }
            assert!((q - c["Q"].as_f64().unwrap()).abs() < 1e-9, "{name}: Q {q} vs {}", c["Q"]);
            assert!(q <= 100.0);
        }
    }

    #[test]
    fn no_grain_no_line_art_is_ssimulacra2_exactly() {
        let m = Model::builtin();
        let s0 = 73.123456789;
        let g = smoothstep(0.2, m.sig.0, m.sig.1);
        assert_eq!(g, 0.0);
        let q = s0 + g * (91.0 - s0 - 12.5);
        assert_eq!(q.to_bits(), s0.to_bits());
    }
}
