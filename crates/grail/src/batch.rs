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

/// FNV-1a, 64 bit (stable across Rust versions, unlike std's DefaultHasher)
fn fnv(h: &mut u64, data: &[u8]) {
    for b in data {
        *h ^= *b as u64;
        *h = h.wrapping_mul(0x100000001b3);
    }
}

/// identity of a media file: size, modification time and the first MiB's hash (no full read)
fn media_identity(h: &mut u64, p: &Path) -> Result<()> {
    use std::io::Read;
    let md = std::fs::metadata(p).with_context(|| format!("reading {}", p.display()))?;
    fnv(h, &md.len().to_le_bytes());
    if let Ok(t) = md.modified() {
        let d = t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
        fnv(h, &d.as_secs().to_le_bytes());
        fnv(h, &d.subsec_nanos().to_le_bytes());
    }
    let mut head = vec![0u8; 1 << 20];
    let mut f = std::fs::File::open(p).with_context(|| format!("reading {}", p.display()))?;
    let mut got = 0;
    while got < head.len() {
        let k = f.read(&mut head[got..])?;
        if k == 0 {
            break;
        }
        got += k;
    }
    fnv(h, &head[..got]);
    Ok(())
}

/// what a stored result depends on: the model, the options that change a score, and both media files (and the
/// av1-fullgrain build when one is used). A resumed batch re-scores a pair whose fingerprint changed.
fn fingerprint(o: &Opts, model: &Model) -> Result<String> {
    let mut h: u64 = 0xcbf29ce484222325;
    let opts = format!("model {} tool {} fast {} denoise_candidate {} start {} dist_start {:?} frames {:?} crop {:?} \
                        dist_crop {:?} colour {:?} siting_left {}", model.version, score::TOOL_VERSION, o.fast,
                       o.denoise_candidate, o.start, o.dist_start, o.frames, o.crop, o.dist_crop, o.colour,
                       o.siting_left);
    fnv(&mut h, opts.as_bytes());
    media_identity(&mut h, &o.reference)?;
    media_identity(&mut h, &o.dist)?;
    let fg = o.fullgrain_lib.clone().or_else(|| std::env::var("GRAIL_FULLGRAIN_LIB").ok().map(PathBuf::from));
    if let Some(dir) = fg {
        for n in ["libdav1d.so.7", "libdav1d.so", "libdav1d.7.dylib"] {
            if let Ok(md) = std::fs::metadata(dir.join(n)) {
                fnv(&mut h, &md.len().to_le_bytes());
                if let Ok(t) = md.modified() {
                    fnv(&mut h, &t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos().to_le_bytes());
                }
                break;
            }
        }
    }
    Ok(format!("{h:016x}"))
}

/// an error message without the media paths (--json output holds no file names): the paths as given, as resolved,
/// and their file names become <ref> / <dist>
fn redact(msg: &str, o: &Opts) -> String {
    let mut m = msg.to_string();
    for (p, tag) in [(&o.reference, "<ref>"), (&o.dist, "<dist>")] {
        let mut forms: Vec<String> = vec![p.display().to_string()];
        if let Ok(c) = std::fs::canonicalize(p) {
            forms.push(c.display().to_string());
        }
        if let Some(f) = p.file_name() {
            forms.push(f.to_string_lossy().into_owned());
        }
        forms.sort_by_key(|f| std::cmp::Reverse(f.len()));       // longest first: a full path before its file name
        for f in forms.into_iter().filter(|f| !f.is_empty()) {
            m = m.replace(&f, tag);
        }
    }
    m
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
    // resume: a pair is skipped only if the output already holds a successful row with the same id AND the same
    // fingerprint (model, score-changing options, media identity); otherwise it is scored again and a new row is
    // appended (readers take the last row of an id)
    let done: std::collections::HashSet<(String, String)> = match out {
        Some(p) if p.exists() => std::fs::read_to_string(p)?.lines()
            .filter_map(|l| serde_json::from_str::<Value>(l).ok())
            .filter(|v| v.get("error").is_none())
            .filter_map(|v| Some((v["id"].to_string(), v.get("fingerprint")?.as_str()?.to_string())))
            .collect(),
        _ => Default::default(),
    };
    let backend = crate::new_backend()?;
    let mut sink: Box<dyn Write> = match out {
        Some(p) => Box::new(std::fs::OpenOptions::new().create(true).append(true).open(p)?),
        None => Box::new(std::io::stdout()),
    };
    let n = items.len();
    for (k, (id, o)) in items.into_iter().enumerate() {
        let fp = fingerprint(&o, model);
        if let Ok(f) = &fp {
            if done.contains(&(id.to_string(), f.clone())) {
                continue;
            }
        }
        let t0 = std::time::Instant::now();
        let res = fp.and_then(|f| score::score(&o, model, backend.as_ref()).map(|r| (r, f)));
        let row = match res {
            Ok((r, f)) => {
                let label = if r.mode == "full" { format!("{} {:.2}", r.flavour, r.score) }
                            else { format!("{} {:.2} ({})", r.flavour, r.score, r.mode) };
                eprintln!("[{}/{n}] {id}: {label}  ({:.1} s)", k + 1, t0.elapsed().as_secs_f64());
                if full {
                    let mut v = serde_json::to_value(&r)?;
                    v["id"] = id.clone();
                    v["label"] = json!(label);
                    v["fingerprint"] = json!(f);
                    v
                } else {
                    json!({"id": id, "label": label, "flavour": r.flavour, "mode": r.mode,
                           "model_version": r.model_version, "grails": r.score, "ssimulacra2": r.ssimulacra2,
                           "structure": r.structure, "grain": r.grain,
                           "lineart": r.lineart.term, "a": r.lineart.a, "path": r.path, "g": r.g, "sigma_g": r.sigma_g,
                           "frames": r.frames, "frames_scored": r.frames_scored, "window": r.window,
                           "ref": o.reference, "dist": o.dist, "seconds": t0.elapsed().as_secs_f64(),
                           "fingerprint": f})
                }
            }
            Err(e) => {
                let msg = format!("{e:#}");
                if full {
                    // --json output holds no file names (CONTRIBUTING.md): no paths, and none in the message
                    let msg = redact(&msg, &o);
                    eprintln!("[{}/{n}] {id}: FAILED: {msg}", k + 1);
                    json!({"id": id, "error": msg})
                } else {
                    eprintln!("[{}/{n}] {id}: FAILED: {msg}", k + 1);
                    json!({"id": id, "ref": o.reference, "dist": o.dist, "error": msg})
                }
            }
        };
        writeln!(sink, "{}", serde_json::to_string(&row)?)?;
        sink.flush()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(r: &Path, d: &Path) -> Opts {
        Opts { reference: r.into(), dist: d.into(), fast: false, denoise_candidate: false, start: 0, dist_start: None,
               frames: None, crop: Crop::Auto, dist_crop: None, colour: None, fullgrain_lib: None, threads: 1,
               timings: false, siting_left: false, hwdec: score::HwDec::None }
    }

    #[test]
    fn redaction_removes_media_paths() {
        let o = opts(Path::new("/films/Secret Title (2020)/scene.mkv"), Path::new("enc/secret_crf30.mkv"));
        let m = "ffmpeg failed on /films/Secret Title (2020)/scene.mkv: scene.mkv: bad; enc/secret_crf30.mkv; \
                 secret_crf30.mkv missing";
        let r = redact(m, &o);
        assert!(!r.contains("scene.mkv") && !r.contains("secret_crf30") && !r.contains("Secret Title"), "{r}");
        assert_eq!(r.matches("<ref>").count(), 2);
        assert_eq!(r.matches("<dist>").count(), 2);
    }

    #[test]
    fn fingerprint_tracks_model_options_and_media() {
        let dir = std::env::temp_dir().join(format!("grail_fp_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (r, d) = (dir.join("r.bin"), dir.join("d.bin"));
        std::fs::write(&r, b"reference").unwrap();
        std::fs::write(&d, b"encode").unwrap();
        let m = Model::builtin();
        let o = opts(&r, &d);
        let f0 = fingerprint(&o, &m).unwrap();
        assert_eq!(f0, fingerprint(&o, &m).unwrap());
        let mut o2 = o.clone();
        o2.fast = true;
        assert_ne!(f0, fingerprint(&o2, &m).unwrap());
        let mut o3 = o.clone();
        o3.threads = 8;                      // does not change a score: same fingerprint
        assert_eq!(f0, fingerprint(&o3, &m).unwrap());
        let mut m2 = m.clone();
        m2.version = "9.9.9".into();
        assert_ne!(f0, fingerprint(&o, &m2).unwrap());
        std::fs::write(&d, b"encode, changed").unwrap();
        assert_ne!(f0, fingerprint(&o, &m).unwrap());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
