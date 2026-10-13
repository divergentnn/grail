//! The scoring pipeline: decode (ffmpeg), active picture, measurement denoise of the active picture (window cut
//! afterwards), SSIMULACRA2 pairs, grain features, line-art path, model. One pass over the frames: every stage streams
//! (the denoiser keeps radius 3 frames on each side), except the reference's active-picture scan, which needs all of
//! its frames first (kept in memory up to GRAIL_MEM_MB, else decoded twice).
use crate::decode::{self, inspect_av1_grain, probe, DecodeSpec, Decoder, Grain, GrainInfo, Probe};
use crate::model::{self, Feats, Model};
use crate::seed::{seed_view, SeedView};
use crate::vship::{Colour, Vship};
use anyhow::{bail, Context, Result};
use grail_stages::{Backend, Denoise, FeatureFrame, FeatureTotals, Win};
use serde::Serialize;
use std::collections::{BTreeMap, VecDeque};
use std::path::PathBuf;
use std::time::Instant;

pub const CTX: usize = 3;
pub const FLAVOUR: &str = "GRAILS";
pub const TOOL_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone, Debug)]
pub enum Crop {
    Auto,
    Rect([usize; 4]),
}

#[derive(Clone, Debug)]
pub struct Opts {
    pub reference: PathBuf,
    pub dist: PathBuf,
    pub fast: bool,
    pub denoise_candidate: bool,
    /// first scored frame in the reference / the candidate (file frame numbers)
    pub start: usize,
    pub dist_start: Option<usize>,
    pub frames: Option<usize>,
    pub crop: Crop,
    pub dist_crop: Option<[usize; 4]>,
    pub colour: Option<Colour>,
    pub fullgrain_lib: Option<PathBuf>,
    pub threads: usize,
    pub timings: bool,
    /// plain pair with chroma siting left on both sides (as the reference's in-process libvship calls) instead of
    /// each file's siting (as FFVship, the reference's full-mode S0)
    pub siting_left: bool,
    pub hwdec: HwDec,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HwDec {
    /// NVDEC for non-AV1 streams wider than 2048 px (4K HEVC: NVDEC 167 frames/s vs software 41), software otherwise
    Auto,
    None,
    /// NVDEC for every non-AV1 stream (AV1 stays on dav1d: full-frame grain needs it)
    Cuda,
    /// VideoToolbox (macOS) for every non-AV1 stream
    VideoToolbox,
}

#[derive(Clone, Debug, Serialize, Default)]
pub struct PerFrame {
    pub frame: Vec<usize>,
    pub ssimulacra2: Vec<f64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub s_d: Vec<f64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub s_dp: Vec<f64>,
    pub lineart_near_top: Vec<f64>,
}

#[derive(Clone, Debug, Serialize, Default)]
pub struct LineArtOut {
    pub a: f64,
    pub inkflat_s: f64,
    pub tex6: f64,
    pub peak: f64,
    pub p_l: f64,
    /// -(1 - g) a w_L p_L: what the path takes off the score
    pub term: f64,
}

#[derive(Clone, Debug, Serialize, Default)]
pub struct Report {
    pub flavour: String,
    pub mode: String,
    /// the GRAILS model (weights.json "version"); separate from the tool's version
    pub model_version: String,
    pub tool_version: String,
    pub score: f64,
    pub ssimulacra2: f64,
    pub structure: f64,
    pub grain: f64,
    pub lineart: LineArtOut,
    pub path: String,
    pub g: f64,
    pub sigma_g: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub s_d: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub s_dp: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grain_capture_r: Option<f64>,
    pub penalties: BTreeMap<String, f64>,
    pub features: Feats,
    /// frames of the window (fast mode scores a subset of them)
    pub frames: usize,
    pub frames_scored: usize,
    pub window: [usize; 4],
    pub dist_window: [usize; 4],
    pub active: [usize; 4],
    pub dist_active: [usize; 4],
    pub colour: String,
    pub codec: String,
    pub std_decoder: SeedView,
    pub per_frame: PerFrame,
    pub gpu: String,
    pub timings: BTreeMap<String, f64>,
    /// frames each decoder delivered and the frames its range asked for (decoded-frame accounting)
    pub decoded: BTreeMap<String, [usize; 2]>,
    /// conditions that did not stop the score but are worth knowing (e.g. a stream shorter than its packet count)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

fn even_box(act: [i64; 4], w: usize, h: usize) -> [usize; 4] {
    // extract.decode_denoise: clipped to this file, even-aligned, whole frame below 64 px
    let ax = act[0].max(0) & !1;
    let ay = act[1].max(0) & !1;
    let aw = ((w as i64).min(act[0] + act[2]) - ax) & !1;
    let ah = ((h as i64).min(act[1] + act[3]) - ay) & !1;
    if aw < 64 || ah < 64 {
        return [0, 0, w & !1, h & !1];
    }
    [ax as usize, ay as usize, aw as usize, ah as usize]
}

/// extract.detect_active: rows / columns brighter than black + 12 (10-bit) over every 2nd frame
struct ActiveScan {
    w: usize,
    h: usize,
    mr: Vec<u16>,
    mc: Vec<u16>,
    k: usize,
}
impl ActiveScan {
    fn new(w: usize, h: usize) -> Self {
        ActiveScan { w, h, mr: vec![0; h], mc: vec![0; w], k: 0 }
    }
    fn add(&mut self, f: &[u16]) {
        if self.k % 2 == 0 {
            for y in 0..self.h {
                let row = &f[y * self.w..(y + 1) * self.w];
                let mut m = self.mr[y];
                for (x, &v) in row.iter().enumerate() {
                    if v > m {
                        m = v;
                    }
                    if v > self.mc[x] {
                        self.mc[x] = v;
                    }
                }
                self.mr[y] = m;
            }
        }
        self.k += 1;
    }
    fn result(&self) -> [i64; 4] {
        let rows: Vec<usize> = (0..self.h).filter(|&y| self.mr[y] > 64 + 12).collect();
        let cols: Vec<usize> = (0..self.w).filter(|&x| self.mc[x] > 64 + 12).collect();
        if rows.len() < 64 || cols.len() < 64 {
            return [0, 0, self.w as i64, self.h as i64];
        }
        let (y0, x0) = ((rows[0] & !1) as i64, (cols[0] & !1) as i64);
        let y1 = (self.h as i64).min(((rows[rows.len() - 1] + 2) & !1) as i64);
        let x1 = (self.w as i64).min(((cols[cols.len() - 1] + 2) & !1) as i64);
        [x0, y0, x1 - x0, y1 - y0]
    }
}

fn crop_frame(f: &[u16], fw: usize, fh: usize, c: [usize; 4]) -> Vec<u16> {
    let [x, y, w, h] = c;
    let mut out = Vec::with_capacity(w * h * 3 / 2);
    for r in y..y + h {
        out.extend_from_slice(&f[r * fw + x..r * fw + x + w]);
    }
    let (cw, ch) = (fw / 2, fh / 2);
    for p in 0..2 {
        let base = fw * fh + p * cw * ch;
        for r in y / 2..(y + h) / 2 {
            out.extend_from_slice(&f[base + r * cw + x / 2..base + r * cw + (x + w) / 2]);
        }
    }
    out
}

enum Source {
    Live(Decoder),
    Mem(VecDeque<Vec<u16>>),
}
impl Source {
    fn next(&mut self) -> Option<Vec<u16>> {
        match self {
            Source::Live(d) => d.next(),
            Source::Mem(q) => q.pop_front(),
        }
    }
}

/// one decoded stream: full frames -> (active picture -> denoiser), window copies on the host
struct Stream {
    src: Source,
    fw: usize,
    fh: usize,
    act: [usize; 4],
    win: [usize; 4],
    den: Option<Box<dyn Denoise>>,
    pushed: usize,
    total: usize,
    windows: VecDeque<(usize, Vec<u16>)>,
    wait: f64,
    full: bool,
}

impl Stream {
    fn pull(&mut self) -> Result<bool> {
        if self.pushed >= self.total {
            if let Some(d) = self.den.as_mut() {
                d.finish();
            }
            return Ok(false);
        }
        let t0 = Instant::now();
        let f = self.src.next();
        self.wait += t0.elapsed().as_secs_f64();
        let Some(f) = f else {
            self.total = self.pushed;
            if let Some(d) = self.den.as_mut() {
                d.finish();
            }
            return Ok(false);
        };
        let win = if self.full {
            if let Some(d) = self.den.as_mut() {
                d.push(&crop_frame(&f, self.fw, self.fh, self.act))?;
            }
            crop_frame(&f, self.fw, self.fh, self.win)
        } else {
            f
        };
        self.windows.push_back((self.pushed, win));
        while self.windows.len() > 2 * CTX + 3 {
            self.windows.pop_front();
        }
        self.pushed += 1;
        Ok(true)
    }

    /// take the window of frame idx (each frame is taken once, in order)
    fn take_window(&mut self, idx: usize) -> Result<Vec<u16>> {
        let pos = self.windows.iter().position(|(i, _)| *i == idx)
            .with_context(|| format!("window frame {idx} not buffered"))?;
        Ok(std::mem::take(&mut self.windows[pos].1))
    }

    /// make frame idx available: its window on the host and, with a denoiser, its denoised window in `out`
    /// (earlier frames that are not scored are skipped)
    fn produce(&mut self, idx: usize, out: Option<&mut Win>) -> Result<()> {
        if self.den.is_none() {
            while self.pushed <= idx {
                if !self.pull()? {
                    bail!("stream ended before frame {idx}");
                }
            }
            return Ok(());
        }
        loop {
            match self.den.as_ref().unwrap().ready() {
                Some(j) if j < idx => {
                    self.den.as_mut().unwrap().skip()?;
                }
                Some(j) if j == idx => {
                    let pos = self.windows.iter().position(|(i, _)| *i == idx)
                        .with_context(|| format!("window frame {idx} not buffered"))?;
                    let (act, win) = (self.act, self.win);
                    let orig = &self.windows[pos].1;
                    self.den.as_mut().unwrap().produce_window(orig, act, win, out.expect("output window"))?;
                    return Ok(());
                }
                Some(j) => bail!("denoiser already past frame {idx} ({j})"),
                None => {
                    if !self.pull()? && self.den.as_ref().unwrap().ready().is_none() {
                        bail!("stream ended before frame {idx}");
                    }
                }
            }
        }
    }
}

/// Colour handling GRAILS supports: limited range; SDR as BT.709 (matrix, primaries; BT.709 / BT.601 transfer) or
/// HDR10 (PQ, BT.2020 non-constant luminance, BT.2020 primaries). Unspecified tags are read as the supported ones.
/// Full range is always rejected; other matrices, primaries or transfers are rejected unless --colour says how to
/// read the file.
fn check_colour(p: &Probe, path: &std::path::Path, forced: bool) -> Result<Colour> {
    let un = |v: &str| v.is_empty() || v == "unknown" || v == "unspecified";
    if p.range == "pc" || p.range == "jpeg" {
        bail!("{}: full-range video is not supported (GRAILS reads limited range only)", path.display());
    }
    if p.pix_fmt.starts_with("yuvj") {
        bail!("{}: full-range video ({}) is not supported (GRAILS reads limited range only)", path.display(), p.pix_fmt);
    }
    // 8- or 10-bit YUV; 4:2:2 and 4:4:4 are converted to 4:2:0 by ffmpeg, as the reference did (one of its cases has
    // a 4:4:4 source); RGB, grey, alpha and deeper formats are not supported
    let fmt_ok = matches!(p.pix_fmt.as_str(), "yuv420p" | "yuv420p10le" | "yuv420p10be" | "yuv422p" | "yuv422p10le"
                          | "yuv422p10be" | "yuv444p" | "yuv444p10le" | "yuv444p10be");
    if !p.pix_fmt.is_empty() && !fmt_ok {
        bail!("{}: pixel format {} is not supported (8- or 10-bit YUV only; convert first)", path.display(), p.pix_fmt);
    }
    if p.transfer == "smpte2084" {
        let ok = (un(&p.matrix) || p.matrix == "bt2020nc") && (un(&p.primaries) || p.primaries == "bt2020");
        if !ok && !forced {
            bail!("{}: PQ with matrix {} / primaries {} is not supported (HDR10 is BT.2020; pass --colour pq to \
                   read it as HDR10)", path.display(), p.matrix, p.primaries);
        }
        return Ok(Colour::Pq);
    }
    let tr_ok = un(&p.transfer) || p.transfer == "bt709" || p.transfer == "smpte170m" || p.transfer == "bt470bg";
    let ok = tr_ok && (un(&p.matrix) || p.matrix == "bt709") && (un(&p.primaries) || p.primaries == "bt709");
    if !ok && !forced {
        bail!("{}: transfer {} / matrix {} / primaries {} not supported (SDR is read as BT.709, HDR as HDR10 PQ; HLG \
               and other colour spaces are not; pass --colour sdr|pq to read it anyway)",
              path.display(), or_un(&p.transfer), or_un(&p.matrix), or_un(&p.primaries));
    }
    Ok(Colour::Sdr)
}

fn or_un(v: &str) -> &str {
    if v.is_empty() { "unspecified" } else { v }
}

fn mean(v: &[f64]) -> f64 {
    v.iter().sum::<f64>() / v.len().max(1) as f64
}

pub fn score(o: &Opts, model: &Model, backend: &dyn Backend) -> Result<Report> {
    let t_start = Instant::now();
    let mut tm: BTreeMap<String, f64> = BTreeMap::new();
    let pr = probe(&o.reference)?;
    let pd = probe(&o.dist)?;
    for (p, path) in [(&pr, &o.reference), (&pd, &o.dist)] {
        if p.width % 2 == 1 || p.height % 2 == 1 {
            bail!("{}: odd frame size {}x{}: 4:2:0 frames with odd dimensions are not supported", path.display(),
                  p.width, p.height);
        }
    }
    let cr = check_colour(&pr, &o.reference, o.colour.is_some())?;
    let cd = check_colour(&pd, &o.dist, o.colour.is_some())?;
    if cr != cd && o.colour.is_none() {
        bail!("the source is {} and the encode {}: they must match (or pass --colour)",
              if cr == Colour::Pq { "HDR10 (PQ)" } else { "SDR" }, if cd == Colour::Pq { "HDR10 (PQ)" } else { "SDR" });
    }
    let colour = o.colour.unwrap_or(cr);
    // AV1 film grain: any shown frame applying grain (the reference's contract), from the frame headers; an
    // inspection failure stops the score (it is never read as "no grain")
    let gi_dist: Option<GrainInfo> = if pd.codec == "av1" { Some(inspect_av1_grain(&o.dist)?) } else { None };
    let gi_ref: Option<GrainInfo> = if pr.codec == "av1" { Some(inspect_av1_grain(&o.reference)?) } else { None };
    let synth = gi_dist.as_ref().map(|g| g.applies).unwrap_or(false);
    let ref_grain = gi_ref.as_ref().map(|g| g.applies).unwrap_or(false);
    let mut warnings: Vec<String> = vec![];
    let mut decoded: BTreeMap<String, [usize; 2]> = BTreeMap::new();
    if let Some(g) = &gi_dist {
        if g.supports && !g.applies {
            warnings.push("the encode's AV1 sequence allows film grain but no frame applies it: scored as coded".into());
        }
    }
    for (g, p, what) in [(&gi_dist, &pd, "encode"), (&gi_ref, &pr, "source")] {
        if let Some(g) = g {
            if p.packets > 0 && g.frames != p.packets {
                warnings.push(format!("{what}: {} shown frames in the AV1 frame headers, {} packets", g.frames, p.packets));
            }
        }
    }
    let fg_lib = if synth || ref_grain {
        let lib = o.fullgrain_lib.clone().or_else(|| std::env::var("GRAIL_FULLGRAIN_LIB").ok().map(PathBuf::from));
        let Some(lib) = lib else {
            bail!("{} carries AV1 film grain: GRAILS scores it rendered full-frame and needs the av1-fullgrain \
                   dav1d build (--fullgrain-lib DIR or GRAIL_FULLGRAIN_LIB); standard tiled grain is never scored",
                  if synth { o.dist.display() } else { o.reference.display() });
        };
        decode::check_fullgrain_lib(&lib)?;
        Some(lib)
    } else {
        None
    };
    // frame ranges
    let rs = o.start;
    let ds = o.dist_start.unwrap_or(o.start);
    let avail = |p: &Probe, s: usize| if p.packets > 0 { p.packets.saturating_sub(s) } else { usize::MAX };
    let n = o.frames.unwrap_or(usize::MAX).min(avail(&pr, rs)).min(avail(&pd, ds));
    if n == 0 || n == usize::MAX {
        bail!("no frames to score (reference {} packets from {rs}, candidate {} from {ds})", pr.packets, pd.packets);
    }
    let range = |p: &Probe, s: usize| -> (usize, usize) {
        let a = s.saturating_sub(CTX);
        let b = if p.packets > 0 { p.packets.min(s + n + CTX) } else { s + n + CTX };
        (a, b)
    };
    let (ra, rb) = range(&pr, rs);
    let (da, db) = range(&pd, ds);
    let (rfirst, dfirst) = (rs - ra, ds - da);
    let scored: Vec<usize> = if o.fast {
        let mut v: Vec<usize> = (0..n).step_by(4).flat_map(|a| [a, a + 1]).filter(|&t| t < n).collect();
        v.dedup();
        v
    } else {
        (0..n).collect()
    };
    let denoise_cand = !o.fast || o.denoise_candidate || !synth;
    let threads = o.threads.max(1);

    let hw = |p: &Probe| p.codec != "av1" && match o.hwdec {
        HwDec::None => false,
        HwDec::Cuda | HwDec::VideoToolbox => true,
        HwDec::Auto => p.width > 2048,
    };
    // host memory for decoded frames: the reference's frames are kept for the active-picture scan up to this budget
    // (else the reference is decoded twice); each candidate queue holds at most 1/4 of it (and 48 frames)
    let budget: usize = std::env::var("GRAIL_MEM_MB").ok().and_then(|v| v.parse().ok()).unwrap_or(4096) << 20;
    // the candidate's window is known before the reference's scan when the window is given
    let dist_window = |win: [usize; 4]| -> Result<[usize; 4]> {
        let (w, h) = (win[2], win[3]);
        let dwin = match o.dist_crop {
            Some(c) => c,
            None if (pd.width, pd.height) == (pr.width, pr.height) => win,
            None if (pd.width, pd.height) == (w, h) => [0, 0, w, h],
            None => bail!("candidate is {}x{}, reference {}x{}: pass --dist-crop", pd.width, pd.height, pr.width, pr.height),
        };
        if dwin[2] != w || dwin[3] != h || dwin[0] + w > pd.width || dwin[1] + h > pd.height {
            bail!("candidate window {:?} does not fit (window {w}x{h}, candidate {}x{})", dwin, pd.width, pd.height);
        }
        if dwin.iter().any(|v| v % 2 == 1) {
            bail!("candidate window {:?} must be even (4:2:0 chroma)", dwin);
        }
        Ok(dwin)
    };
    if let Crop::Rect(r) = o.crop {
        if r.iter().any(|v| v % 2 == 1) {
            bail!("window {:?} must be even (4:2:0 chroma)", r);
        }
    }
    let early_dwin = match o.crop {
        Crop::Rect(r) => Some(dist_window(r)?),
        Crop::Auto => None,
    };
    // candidate decoders start first and run while the reference is scanned (queues sized by the memory budget)
    let t0 = Instant::now();
    let dgrain = match (&fg_lib, synth) {
        (Some(l), true) => Grain::FullFrame(l.clone()),
        _ => Grain::Plain,
    };
    let ccrop = if denoise_cand { None } else { early_dwin };
    let (cw, ch) = match ccrop { Some(c) => (c[2], c[3]), None => (pd.width, pd.height) };
    let q = |w: usize, h: usize| ((budget / 4) / (w * h * 3)).clamp(8, 48).min(db - da + 1);
    let cdec = Decoder::start(DecodeSpec { path: o.dist.clone(), av1: pd.codec == "av1", a: da, b: db, crop: ccrop,
                                           grain: dgrain, threads, hwaccel: hw(&pd), w: cw, h: ch },
                              q(cw, ch))?;
    let ngdec = if synth {
        let (nw, nh) = match early_dwin { Some(c) => (c[2], c[3]), None => (pd.width, pd.height) };
        Some(Decoder::start(DecodeSpec { path: o.dist.clone(), av1: true, a: da, b: db, crop: early_dwin,
                                         grain: Grain::Off, threads, hwaccel: false, w: nw, h: nh }, q(nw, nh))?)
    } else {
        None
    };
    // reference: all frames for the active-picture scan (kept in memory within the budget, else decoded again)
    // an AV1 source with film grain is watched with its grain rendered full-frame, like an encode (deterministic: the
    // pinned av1-fullgrain build, not whatever the system libdav1d does)
    let rgrain = match (&fg_lib, ref_grain) {
        (Some(l), true) => Grain::FullFrame(l.clone()),
        _ => Grain::Plain,
    };
    let rspec = DecodeSpec { path: o.reference.clone(), av1: pr.codec == "av1", a: ra, b: rb, crop: None,
                             grain: rgrain, threads, hwaccel: hw(&pr), w: pr.width, h: pr.height };
    let fbytes = pr.width * pr.height * 3;
    let mut scan = ActiveScan::new(pr.width, pr.height);
    let mut mem: VecDeque<Vec<u16>> = VecDeque::new();
    let mut keep = (rb - ra) * fbytes <= budget;
    {
        let mut d = Decoder::start(rspec.clone(), 8)?;
        while let Some(f) = d.next() {
            scan.add(&f);
            if keep {
                mem.push_back(f);
                if mem.len() * fbytes > budget {
                    keep = false;
                    mem.clear();
                }
            }
        }
        let got = d.finish()?;
        decoded.insert("reference".into(), [got, rb - ra]);
    }
    tm.insert("decode_reference_scan".into(), t0.elapsed().as_secs_f64());
    let ract = even_box(scan.result(), pr.width, pr.height);
    let win = match o.crop {
        Crop::Auto => ract,
        Crop::Rect(r) => r,
    };
    let (w, h) = (win[2], win[3]);
    if win.iter().any(|v| v % 2 == 1) || win[0] + w > pr.width || win[1] + h > pr.height || w < 64 || h < 64 {
        bail!("window {:?} must be even, at least 64x64 and inside the {}x{} reference", win, pr.width, pr.height);
    }
    let dwin = match early_dwin {
        Some(d) => d,
        None => dist_window(win)?,
    };
    let dact = even_box([ract[0] as i64 + dwin[0] as i64 - win[0] as i64, ract[1] as i64 + dwin[1] as i64 - win[1] as i64,
                         ract[2] as i64, ract[3] as i64], pd.width, pd.height);
    let sc = if pr.height > 1600 { 2.0 } else { 1.0 };

    // metric handles and stages
    let t_init = Instant::now();
    let vs = Vship::load()?;
    // plain pair: each file's chroma siting, as FFVship reads it (the reference's S0); denoised pairs: left on both
    // sides, as the reference's in-process libvship calls (vship_api.py)
    let ss0 = if o.siting_left {
        vs.ssimu2(w, h, colour, (0, 0))?
    } else {
        vs.ssimu2(w, h, colour, (crate::vship::chroma_loc(&pr.chroma_location), crate::vship::chroma_loc(&pd.chroma_location)))?
    };
    let ss = vs.ssimu2(w, h, colour, (0, 0))?;
    let ss2 = vs.ssimu2(w, h, colour, (0, 0))?;
    tm.insert("init".into(), t_init.elapsed().as_secs_f64());

    let rsrc = if keep && !mem.is_empty() { Source::Mem(mem) } else { Source::Live(Decoder::start(rspec, 8)?) };
    let mut rstream = Stream {
        src: rsrc, fw: pr.width, fh: pr.height, act: ract, win, den: Some(backend.denoiser(ract[2], ract[3])?),
        pushed: 0, total: rb - ra, windows: VecDeque::new(), wait: 0.0, full: true,
    };
    let cfull = ccrop.is_none();
    let mut cstream = Stream {
        src: Source::Live(cdec), fw: pd.width, fh: pd.height, act: dact, win: dwin,
        den: if denoise_cand { Some(backend.denoiser(dact[2], dact[3])?) } else { None },
        pushed: 0, total: db - da, windows: VecDeque::new(), wait: 0.0, full: cfull,
    };
    let mut ngstream = ngdec.map(|d| Stream {
        src: Source::Live(d), fw: pd.width, fh: pd.height, act: [0, 0, w, h], win: dwin, den: None, pushed: 0,
        total: db - da, windows: VecDeque::new(), wait: 0.0, full: early_dwin.is_none(),
    });

    let mut feats = backend.features(w, h, sc)?;
    let mut la = backend.lineart(w, h)?;
    // window slots, reused frame to frame (a backend keeps its device copies attached to them)
    let (mut s_win, mut c_win, mut ng_win) = (Win::new(w, h), Win::new(w, h), Win::new(w, h));
    let (mut ds_win, mut dc_win) = (Win::new(w, h), Win::new(w, h));
    let mut pf = PerFrame::default();
    let (mut det_tex, mut det_ink) = (vec![], vec![]);
    let mut last: Option<usize> = None;
    let mut t_metric_wait = 0.0;
    let mut t_metric_calls = 0.0;
    let mut t_feat = 0.0;
    let mut t_den = 0.0;
    let mut t_la = 0.0;

    // the SSIMULACRA2 pairs (libvship: its own GPU work and host conversions) run on one worker thread per pair, fed
    // through short queues, so they overlap the denoise, features and line art of the following frames; the main
    // thread only waits when a queue is full and at the end
    type Pair = (std::sync::Arc<Vec<u16>>, std::sync::Arc<Vec<u16>>);
    // each worker also sums the wall time spent inside libvship's calls (host conversion, upload, GPU kernels): what
    // SSIMULACRA2 costs, most of it hidden behind the other stages
    let metric_worker = |h: &crate::vship::Ssimu2<'_>, rx: std::sync::mpsc::Receiver<Pair>| -> Result<(Vec<f64>, f64)> {
        let mut busy = 0.0;
        let mut v = vec![];
        for (a, b) in rx {
            let t = Instant::now();
            v.push(h.score(&a, &b)?);
            busy += t.elapsed().as_secs_f64();
        }
        Ok((v, busy))
    };
    std::thread::scope(|sc| -> Result<()> {
        let mut queues = vec![];
        let mut workers = vec![];
        for (h, on) in [(&ss0, true), (&ss, denoise_cand), (&ss2, synth)] {
            if on {
                let (tx, rx) = std::sync::mpsc::sync_channel::<Pair>(4);
                queues.push(Some(tx));
                workers.push(Some(sc.spawn(move || metric_worker(h, rx))));
            } else {
                queues.push(None);
                workers.push(None);
            }
        }
        let mut send = |k: usize, pair: Pair| -> bool {
            let tt = Instant::now();
            let ok = queues[k].as_ref().map(|q| q.send(pair).is_ok()).unwrap_or(true);
            t_metric_wait += tt.elapsed().as_secs_f64();
            ok
        };
        let mut run = || -> Result<()> {
            for &t in &scored {
                let tt = Instant::now();
                rstream.produce(rfirst + t, Some(&mut ds_win))?;
                s_win.set(rstream.take_window(rfirst + t)?);
                cstream.produce(dfirst + t, if denoise_cand { Some(&mut dc_win) } else { None })?;
                c_win.set(cstream.take_window(dfirst + t)?);
                if let Some(st) = ngstream.as_mut() {
                    st.produce(dfirst + t, None)?;
                    ng_win.set(st.take_window(dfirst + t)?);
                }
                t_den += tt.elapsed().as_secs_f64();
                pf.frame.push(t);
                // a worker that failed has dropped its queue: stop here, its error is reported below
                if !send(0, (s_win.host.clone(), c_win.host.clone()))
                    || !send(1, (ds_win.host.clone(), dc_win.host.clone()))
                    || !send(2, (ds_win.host.clone(), ng_win.host.clone())) {
                    return Ok(());
                }
                let tt = Instant::now();
                let consecutive = last.map(|l| l + 1 == t).unwrap_or(false);
                let ff = if denoise_cand {
                    FeatureFrame { s: &mut s_win, ds: &mut ds_win, c: &mut c_win, dc: &mut dc_win,
                                   cng: if synth { Some(&mut ng_win) } else { None } }
                } else {
                    FeatureFrame { s: &mut s_win, ds: &mut ds_win, c: &mut c_win, dc: &mut ng_win, cng: None }
                };
                feats.frame(ff, consecutive)?;
                last = Some(t);
                t_feat += tt.elapsed().as_secs_f64();
                let tt = Instant::now();
                if t % 4 == 0 {
                    let d = la.detect(&mut s_win)?;
                    det_tex.push(d.tex6);
                    det_ink.push(d.inkflat_s);
                }
                pf.lineart_near_top.push(la.ring(&mut s_win, &mut c_win)?);
                t_la += tt.elapsed().as_secs_f64();
            }
            Ok(())
        };
        let r = run();
        drop(run);
        drop(send);
        queues.clear();
        let tt = Instant::now();
        let mut res = vec![];
        for w in workers {
            res.push(w.map(|h| h.join().unwrap_or_else(|_| Err(anyhow::anyhow!("libvship thread panicked")))));
        }
        t_metric_wait += tt.elapsed().as_secs_f64();
        r?;
        let mut res = res.into_iter();
        let mut take = |busy: &mut f64| -> Result<Vec<f64>> {
            Ok(match res.next().flatten().transpose()? {
                Some((v, b)) => {
                    *busy += b;
                    v
                }
                None => vec![],
            })
        };
        pf.ssimulacra2 = take(&mut t_metric_calls)?;
        pf.s_d = take(&mut t_metric_calls)?;
        pf.s_dp = take(&mut t_metric_calls)?;
        for (what, v, on) in [("plain", &pf.ssimulacra2, true), ("denoised pair", &pf.s_d, denoise_cand),
                              ("grain-free pair", &pf.s_dp, synth)] {
            if on && v.len() != pf.frame.len() {
                bail!("SSIMULACRA2 ({what}): {} of {} frames scored", v.len(), pf.frame.len());
            }
        }
        Ok(())
    })?;
    // drain the decoders (ffmpeg exits cleanly) and account for the frames each delivered
    let drain = |src: Source| -> Result<Option<usize>> {
        match src {
            Source::Live(mut d) => {
                while d.next().is_some() {}
                Ok(Some(d.finish()?))
            }
            Source::Mem(_) => Ok(None),
        }
    };
    tm.insert("decode_wait_reference".into(), rstream.wait);
    tm.insert("decode_wait_candidate".into(), cstream.wait);
    if let Some(k) = drain(rstream.src)? {
        decoded.insert("reference".into(), [k, rb - ra]);
    }
    if let Some(k) = drain(cstream.src)? {
        decoded.insert("candidate".into(), [k, db - da]);
    }
    if let Some(st) = ngstream {
        tm.insert("decode_wait_grain_free".into(), st.wait);
        if let Some(k) = drain(st.src)? {
            decoded.insert("grain_free".into(), [k, db - da]);
        }
    }
    for (name, [got, want]) in &decoded {
        if got > want {
            bail!("{name}: ffmpeg delivered {got} frames for a range of {want}");
        }
        if got < want {
            warnings.push(format!("{name}: {got} frames decoded of {want} in its range (the stream has fewer frames \
                                   than packets); scored frames are all present, fewer context frames at the end"));
        }
    }
    let t_wall = t_start.elapsed().as_secs_f64();

    // model
    let ft = finish_features(feats.totals());
    let sigma_g = ft.sigma_g;
    let gg = model::smoothstep(sigma_g, model.sig.0, model.sig.1);
    let s0 = mean(&pf.ssimulacra2);
    let s_d = if pf.s_d.is_empty() { None } else { Some(mean(&pf.s_d)) };
    let s_dp = if pf.s_dp.is_empty() { None } else { Some(mean(&pf.s_dp)) };
    let r = if !o.fast || o.denoise_candidate { model::grain_capture(&ft) } else { None };
    let (s_struct, path) = model::structure(model, sigma_g, synth, r, s_d, s_dp);
    let pen = model::penalties(&ft);
    let wsum: f64 = model.terms.iter().zip(&model.weights).map(|(t, wt)| wt * pen.get(t).copied().unwrap_or(0.0)).sum();
    let mut q = s0 + gg * (s_struct - s0 - wsum);
    let a = model::lineart_confidence(model, mean(&det_ink), mean(&det_tex));
    let peak = pf.lineart_near_top.iter().cloned().fold(0.0, f64::max);
    let p_l = model::lineart_penalty(model, peak);
    let mut term = 0.0;
    if a > 0.0 && model.la_w != 0.0 {
        term = -(1.0 - gg) * a * model.la_w * p_l;
        q += term;
    }
    // grain seed of each scored frame from the encode's frame headers (the reference took them from the bitstream
    // too: av1-grain-seed, which the vendored parser ports)
    let seeds: Vec<Option<u32>> = (0..n).map(|t| {
        gi_dist.as_ref().filter(|g| g.applies).and_then(|g| g.seeds.get(ds + t).copied().flatten())
    }).collect();
    tm.insert("denoise_and_decode".into(), t_den);
    // ssimulacra2_wait: time the pipeline waited on the SSIMULACRA2 workers (full queues, the final join);
    // ssimulacra2_calls: wall time inside libvship's calls, summed over the workers (runs alongside the other stages)
    tm.insert("ssimulacra2_wait".into(), t_metric_wait);
    tm.insert("ssimulacra2_calls".into(), t_metric_calls);
    tm.insert("features".into(), t_feat);
    tm.insert("lineart".into(), t_la);
    tm.insert("total".into(), t_wall);
    Ok(Report {
        flavour: FLAVOUR.into(),
        mode: if o.fast { if o.denoise_candidate { "fast+denoise-candidate".into() } else { "fast".into() } } else { "full".into() },
        model_version: model.version.clone(),
        tool_version: TOOL_VERSION.into(),
        score: q,
        ssimulacra2: s0,
        structure: s_struct,
        grain: -wsum,
        lineart: LineArtOut { a, inkflat_s: mean(&det_ink), tex6: mean(&det_tex), peak, p_l, term },
        path: path.into(),
        g: gg,
        sigma_g,
        s_d,
        s_dp,
        grain_capture_r: r,
        penalties: pen,
        features: ft,
        frames: n,
        frames_scored: scored.len(),
        window: win,
        dist_window: dwin,
        active: ract,
        dist_active: if denoise_cand { dact } else { [0, 0, 0, 0] },
        colour: if colour == Colour::Pq { "pq".into() } else { "sdr".into() },
        codec: pd.codec.clone(),
        std_decoder: seed_view(&seeds, pd.height),
        per_frame: pf,
        gpu: backend.name(),
        timings: tm,
        decoded,
        warnings,
    })
}

fn finish_features(t: &FeatureTotals) -> Feats {
    let nb = model::NB;
    let tot: f64 = t.n_lum.iter().sum();
    let norm = |p: &[[f64; 8]]| -> Vec<Vec<f64>> {
        p.iter().map(|row| (0..nb).map(|k| row[k] / t.n_lum[k].max(1.0)).collect()).collect()
    };
    let p_s = norm(&t.p_s);
    let p_c = norm(&t.p_c);
    let share: Vec<f64> = t.n_lum.iter().map(|x| x / tot.max(1.0)).collect();
    let sigma_g = (0..nb).map(|k| share[k] * (0..3).map(|b| p_s[b][k]).sum::<f64>()).sum::<f64>().sqrt();
    let cn = t.chroma_n.max(1.0);
    Feats {
        sigma_g,
        flat_frac: mean(&t.flat_frac),
        p_s,
        p_c,
        n_lum: t.n_lum.to_vec(),
        chroma_s: vec![t.chroma[0][0] / cn, t.chroma[0][1] / cn],
        chroma_c: vec![t.chroma[1][0] / cn, t.chroma[1][1] / cn],
        stick_s: model::median(&t.stick[0]),
        stick_c: model::median(&t.stick[1]),
        stickc_s: model::median(&t.stickc[0]),
        stickc_c: model::median(&t.stickc[1]),
        tex_energy_ratio: if t.vs != 0.0 { Some(t.vc / t.vs) } else { None },
        p_ng: if t.has_ng { Some(norm(&t.p_ng)) } else { None },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn probe(range: &str, matrix: &str, primaries: &str, transfer: &str, pix: &str) -> Probe {
        Probe { width: 1920, height: 1080, codec: "hevc".into(), packets: 10, transfer: transfer.into(),
                chroma_location: String::new(), range: range.into(), matrix: matrix.into(), primaries: primaries.into(),
                pix_fmt: pix.into() }
    }

    #[test]
    fn colour_checks() {
        let p = std::path::Path::new("x.mkv");
        assert_eq!(check_colour(&probe("tv", "bt709", "bt709", "bt709", "yuv420p"), p, false).unwrap(), Colour::Sdr);
        assert_eq!(check_colour(&probe("", "", "", "", "yuv420p"), p, false).unwrap(), Colour::Sdr);
        assert_eq!(check_colour(&probe("tv", "bt2020nc", "bt2020", "smpte2084", "yuv420p10le"), p, false).unwrap(),
                   Colour::Pq);
        // full range: never, not even with --colour
        assert!(check_colour(&probe("pc", "bt709", "bt709", "bt709", "yuv420p"), p, true).is_err());
        assert!(check_colour(&probe("tv", "bt709", "bt709", "bt709", "yuvj420p"), p, true).is_err());
        // 4:4:4 is converted (as the reference did); more than 10 bits or RGB: never
        assert!(check_colour(&probe("tv", "bt709", "bt709", "bt709", "yuv444p10le"), p, false).is_ok());
        assert!(check_colour(&probe("tv", "bt709", "bt709", "bt709", "yuv420p12le"), p, true).is_err());
        assert!(check_colour(&probe("tv", "bt709", "bt709", "bt709", "gbrp10le"), p, true).is_err());
        // HLG, BT.601 matrix, BT.2020 SDR: only with --colour
        for pr in [probe("tv", "bt2020nc", "bt2020", "arib-std-b67", "yuv420p10le"),
                   probe("tv", "smpte170m", "smpte170m", "smpte170m", "yuv420p"),
                   probe("tv", "bt2020nc", "bt2020", "bt2020-10", "yuv420p10le"),
                   probe("tv", "bt709", "bt709", "smpte2084", "yuv420p10le")] {
            assert!(check_colour(&pr, p, false).is_err(), "{pr:?}");
            assert!(check_colour(&pr, p, true).is_ok(), "{pr:?}");
        }
    }
}
