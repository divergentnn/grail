//! `grail score --batch LIST.jsonl`: many pairs in one process (one GPU initialization), one JSON line out per pair.
use crate::model::Model;
use crate::score::{self, Crop, Opts};
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::io::Write;
use std::path::{Path, PathBuf};

fn rect(v: &Value) -> Result<Option<[usize; 4]>> {
    Ok(match v {
        Value::Null => None,
        Value::String(s) => Some(crate::parse_rect(s)?),
        Value::Array(a) if a.len() == 4 => {
            let mut r = [0usize; 4];
            for (i, x) in a.iter().enumerate() {
                r[i] = x.as_u64().context("crop values must be integers")? as usize;
            }
            Some(r)
        }
        _ => bail!("crop: \"x,y,w,h\", [x, y, w, h] or \"auto\""),
    })
}

/// one pair's options: the line's fields over the command line's defaults
fn item(line: &Value, base: &Opts) -> Result<Opts> {
    let mut o = base.clone();
    o.reference = PathBuf::from(line["ref"].as_str().context("\"ref\" missing")?);
    o.dist = PathBuf::from(line["dist"].as_str().context("\"dist\" missing")?);
    let u = |k: &str| line.get(k).and_then(|v| v.as_u64()).map(|v| v as usize);
    if let Some(v) = u("start") {
        o.start = v;
    }
    if let Some(v) = u("dist_start") {
        o.dist_start = Some(v);
    }
    if let Some(v) = u("frames") {
        o.frames = Some(v);
    }
    match &line["crop"] {
        Value::String(s) if s == "auto" => o.crop = Crop::Auto,
        Value::Null => {}
        v => o.crop = Crop::Rect(rect(v)?.unwrap()),
    }
    if let Some(r) = rect(&line["dist_crop"])? {
        o.dist_crop = Some(r);
    }
    if let Some(c) = line["colour"].as_str() {
        o.colour = Some(crate::parse_colour(c)?);
    }
    if let Some(b) = line["fast"].as_bool() {
        o.fast = b;
    }
    if let Some(b) = line["denoise_candidate"].as_bool() {
        o.denoise_candidate = b;
    }
    Ok(o)
}

pub fn run(list: &Path, out: Option<&Path>, base: &Opts, full: bool, model: &Model) -> Result<()> {
    let text = std::fs::read_to_string(list).with_context(|| format!("reading {}", list.display()))?;
    let mut items = vec![];
    for (i, ln) in text.lines().enumerate().filter(|(_, l)| !l.trim().is_empty() && !l.trim_start().starts_with('#')) {
        let v: Value = serde_json::from_str(ln).with_context(|| format!("{} line {}", list.display(), i + 1))?;
        let o = item(&v, base).with_context(|| format!("{} line {}", list.display(), i + 1))?;
        let id = v.get("id").cloned().unwrap_or(json!(i + 1));
        items.push((id, o));
    }
    // resume: pairs whose id is already in the output file are skipped
    let done: std::collections::HashSet<String> = match out {
        Some(p) if p.exists() => std::fs::read_to_string(p)?.lines()
            .filter_map(|l| serde_json::from_str::<Value>(l).ok())
            .filter(|v| v.get("error").is_none())
            .map(|v| v["id"].to_string()).collect(),
        _ => Default::default(),
    };
    let backend = crate::new_backend()?;
    let mut sink: Box<dyn Write> = match out {
        Some(p) => Box::new(std::fs::OpenOptions::new().create(true).append(true).open(p)?),
        None => Box::new(std::io::stdout()),
    };
    let n = items.len();
    for (k, (id, o)) in items.into_iter().enumerate() {
        if done.contains(&id.to_string()) {
            continue;
        }
        let t0 = std::time::Instant::now();
        let row = match score::score(&o, model, backend.as_ref()) {
            Ok(r) => {
                let label = if r.mode == "full" { format!("{} {:.2}", r.flavour, r.score) }
                            else { format!("{} {:.2} ({})", r.flavour, r.score, r.mode) };
                eprintln!("[{}/{n}] {id}: {label}  ({:.1} s)", k + 1, t0.elapsed().as_secs_f64());
                if full {
                    let mut v = serde_json::to_value(&r)?;
                    v["id"] = id.clone();
                    v["label"] = json!(label);
                    v
                } else {
                    json!({"id": id, "label": label, "flavour": r.flavour, "mode": r.mode,
                           "model_version": r.model_version, "grails": r.score, "ssimulacra2": r.ssimulacra2,
                           "structure": r.structure, "grain": r.grain,
                           "lineart": r.lineart.term, "a": r.lineart.a, "path": r.path, "g": r.g, "sigma_g": r.sigma_g,
                           "frames": r.frames, "frames_scored": r.frames_scored, "window": r.window,
                           "ref": o.reference, "dist": o.dist, "seconds": t0.elapsed().as_secs_f64()})
                }
            }
            Err(e) => {
                eprintln!("[{}/{n}] {id}: FAILED: {e:#}", k + 1);
                json!({"id": id, "ref": o.reference, "dist": o.dist, "error": format!("{e:#}")})
            }
        };
        writeln!(sink, "{}", serde_json::to_string(&row)?)?;
        sink.flush()?;
    }
    Ok(())
}
