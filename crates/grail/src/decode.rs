//! Decoding through an ffmpeg subprocess, raw 10-bit 4:2:0 frames on a pipe; the command lines mirror the Python
//! reference (extract.decode_denoise / decode_nograin), so the pixels are identical.
use anyhow::{anyhow, bail, Context, Result};
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{sync_channel, Receiver};
use std::thread::JoinHandle;

#[derive(Clone, Debug)]
pub struct Probe {
    pub width: usize,
    pub height: usize,
    pub codec: String,
    pub packets: usize,
    pub transfer: String,
    pub chroma_location: String,
    /// colour range ("tv", "pc", "unknown" / ""), matrix (color_space), primaries, pixel format, as ffprobe names them
    pub range: String,
    pub matrix: String,
    pub primaries: String,
    pub pix_fmt: String,
}

pub fn probe(path: &Path) -> Result<Probe> {
    let o = Command::new("ffprobe")
        .args(["-v", "error", "-select_streams", "v:0", "-count_packets", "-show_entries",
               "stream=width,height,codec_name,nb_read_packets,color_transfer,chroma_location,color_range,color_space,\
                color_primaries,pix_fmt", "-of", "default=nw=1"])
        .arg(path)
        .output()
        .context("running ffprobe")?;
    if !o.status.success() {
        bail!("ffprobe {}: {}", path.display(), String::from_utf8_lossy(&o.stderr).trim());
    }
    let mut p = Probe { width: 0, height: 0, codec: String::new(), packets: 0, transfer: String::new(),
                        chroma_location: String::new(), range: String::new(), matrix: String::new(),
                        primaries: String::new(), pix_fmt: String::new() };
    for line in String::from_utf8_lossy(&o.stdout).lines() {
        if let Some((k, v)) = line.split_once('=') {
            match k {
                "width" => p.width = v.parse().unwrap_or(0),
                "height" => p.height = v.parse().unwrap_or(0),
                "codec_name" => p.codec = v.to_string(),
                "nb_read_packets" => p.packets = v.parse().unwrap_or(0),
                "color_transfer" => p.transfer = v.to_string(),
                "chroma_location" => p.chroma_location = v.to_string(),
                "color_range" => p.range = v.to_string(),
                "color_space" => p.matrix = v.to_string(),
                "color_primaries" => p.primaries = v.to_string(),
                "pix_fmt" => p.pix_fmt = v.to_string(),
                _ => {}
            }
        }
    }
    if p.width == 0 || p.height == 0 {
        bail!("{}: no video stream", path.display());
    }
    Ok(p)
}

/// AV1 film grain in a stream, from its frame headers (av1-fullgrain's parser, vendored in `av1/`; no pixel
/// decoding). The reference contract (PORT_SPEC.md section 1, extract.grain_info) is "any frame with apply_grain = 1":
/// `applies`. `supports` (some sequence header has film_grain_params_present) alone does not make an encode a
/// synthetic-grain one.
#[derive(Clone, Debug, Default)]
pub struct GrainInfo {
    /// some sequence header sets film_grain_params_present
    pub supports: bool,
    /// some shown frame applies grain (apply_grain = 1)
    pub applies: bool,
    /// shown frames parsed (display order, from the start of the file)
    pub frames: usize,
    /// the grain seed of each shown frame (None: no grain on that frame)
    pub seeds: Vec<Option<u32>>,
}

/// Inspect every temporal unit of an AV1 stream (ffmpeg remuxes it to IVF; no decoding). Fails closed: an ffmpeg
/// error, a parser error, a stream without a sequence header or without frames is an error, never "no grain".
pub fn inspect_av1_grain(path: &Path) -> Result<GrainInfo> {
    use crate::av1::obu::{Av1Parser, IvfReader};
    let mut child = Command::new("ffmpeg")
        .args(["-hide_banner", "-nostdin", "-v", "error", "-i"])
        .arg(path)
        .args(["-map", "0:v:0", "-c", "copy", "-f", "ivf", "-"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("running ffmpeg (AV1 stream inspection)")?;
    let out = child.stdout.take().unwrap();
    let mut err = child.stderr.take().unwrap();
    let errt = std::thread::spawn(move || {
        let mut t = String::new();
        let _ = err.read_to_string(&mut t);
        t
    });
    let parsed = (|| -> Result<GrainInfo> {
        let mut rd = IvfReader::new(std::io::BufReader::new(out)).context("reading the IVF remux")?;
        let mut p = Av1Parser::new();
        let mut gi = GrainInfo::default();
        let mut seen_seq = false;
        while let Some(tu) = rd.next_frame().context("reading the IVF remux")? {
            let shown = p.temporal_unit(&tu).map_err(|e| anyhow!("AV1 frame headers: {e}"))?;
            if let Some(fg) = p.film_grain_params_present() {
                seen_seq = true;
                gi.supports |= fg;
            }
            for f in shown {
                gi.applies |= f.grain.is_some();
                gi.seeds.push(f.grain.map(|g| g.seed));
                gi.frames += 1;
            }
        }
        if !seen_seq {
            bail!("no AV1 sequence header in the stream");
        }
        if gi.frames == 0 {
            bail!("no shown frames in the stream");
        }
        Ok(gi)
    })();
    if parsed.is_err() {
        let _ = child.kill();      // exact child, by handle
    }
    let st = child.wait()?;
    let errt = errt.join().unwrap_or_default();
    let gi = parsed.with_context(|| format!("inspecting the AV1 film grain of {}", path.display()))?;
    if !st.success() {
        bail!("inspecting the AV1 film grain of {}: ffmpeg failed: {}", path.display(), errt.trim());
    }
    Ok(gi)
}

/// Remove every DAV1D_GRAIN* variable a libdav1d could read from the inherited environment (av1-fullgrain reads
/// DAV1D_GRAIN_MODE, _MODE_FILE, _FORCE, _TEMPLATES, _WARMUP, _BANDS, _STATS, _SIMD): a decode only sees the values
/// it sets itself.
fn clear_grain_env(cmd: &mut Command) {
    for (k, _) in std::env::vars_os() {
        if k.to_string_lossy().starts_with("DAV1D_GRAIN") {
            cmd.env_remove(k);
        }
    }
}

#[derive(Clone, Debug)]
pub enum Grain {
    /// as the file decodes by default (no AV1 film grain in it, or not AV1)
    Plain,
    /// AV1 film grain rendered FULL-FRAME by the av1-fullgrain dav1d build in this directory
    FullFrame(PathBuf),
    /// AV1 decoded without its film grain (libdav1d -filmgrain 0)
    Off,
}

#[derive(Clone, Debug)]
pub struct DecodeSpec {
    pub path: PathBuf,
    pub av1: bool,
    /// decoded frame range [a, b) of the file
    pub a: usize,
    pub b: usize,
    pub crop: Option<[usize; 4]>,
    pub grain: Grain,
    pub threads: usize,
    /// hardware decoding (ffmpeg -hwaccel cuda, or videotoolbox on macOS; frames come back to system memory)
    pub hwaccel: bool,
    /// frame size of the output (crop or the whole frame)
    pub w: usize,
    pub h: usize,
}

pub struct Decoder {
    rx: Receiver<Vec<u16>>,
    child: Child,
    reader: Option<JoinHandle<Result<usize>>>,
    stderr: Option<JoinHandle<String>>,
    pub spec: DecodeSpec,
}

/// check that a directory holds the av1-fullgrain build of libdav1d
pub fn check_fullgrain_lib(dir: &Path) -> Result<()> {
    let names: &[&str] = if cfg!(target_os = "macos") { &["libdav1d.7.dylib"] } else { &["libdav1d.so.7", "libdav1d.so"] };
    let so = names.iter().map(|n| dir.join(n)).find(|p| p.exists())
        .ok_or_else(|| anyhow!("{}: no {} there", dir.display(), names[0]))?;
    let bytes = std::fs::read(&so)?;
    if !bytes.windows(16).any(|w| w == b"DAV1D_GRAIN_MODE") {
        bail!("{}: not the av1-fullgrain build (no full-frame grain mode)", so.display());
    }
    Ok(())
}

impl Decoder {
    pub fn start(spec: DecodeSpec, queue: usize) -> Result<Decoder> {
        let mut vf = format!("trim=start_frame={}:end_frame={},setpts=PTS-STARTPTS", spec.a, spec.b);
        if let Some([x, y, w, h]) = spec.crop {
            vf.push_str(&format!(",crop={w}:{h}:{x}:{y}"));
        }
        let mut cmd = Command::new("ffmpeg");
        cmd.args(["-hide_banner", "-nostats", "-nostdin", "-v", "error"]);
        cmd.args(["-threads", &spec.threads.to_string()]);
        if spec.hwaccel {
            cmd.args(["-hwaccel", if cfg!(target_os = "macos") { "videotoolbox" } else { "cuda" }]);
        }
        if spec.av1 {
            cmd.args(["-c:v", "libdav1d"]);
        }
        // a libdav1d reads its grain settings from the environment: every decode starts from a clean one
        clear_grain_env(&mut cmd);
        match &spec.grain {
            Grain::Off => {
                cmd.args(["-filmgrain", "0"]);
                // an av1-fullgrain libdav1d installed as the system one may be built to force its grain on even when
                // the player turns grain off (grain_force): this decode must stay grain-free
                cmd.env("DAV1D_GRAIN_FORCE", "0");
            }
            Grain::FullFrame(dir) => {
                // the av1-fullgrain libdav1d replaces the system one in ffmpeg (dyld searches DYLD_LIBRARY_PATH for
                // the library's file name before its install path); only the pinned settings reach it
                let var = if cfg!(target_os = "macos") { "DYLD_LIBRARY_PATH" } else { "LD_LIBRARY_PATH" };
                let old = std::env::var(var).unwrap_or_default();
                let lp = if old.is_empty() { dir.display().to_string() } else { format!("{}:{old}", dir.display()) };
                cmd.env(var, lp).env("DAV1D_GRAIN_MODE", "full").env("DAV1D_GRAIN_FORCE", "1");
            }
            Grain::Plain => {}
        }
        cmd.arg("-i").arg(&spec.path);
        // passthrough: every decoded frame once, in order (no duplicated or dropped frames from a frame-rate guess)
        cmd.args(["-map", "0:v:0", "-vf", &vf, "-pix_fmt", "yuv420p10le", "-fps_mode", "passthrough", "-f", "rawvideo",
                  "-"]);
        cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = cmd.spawn().context("starting ffmpeg")?;
        let mut out = child.stdout.take().unwrap();
        let err = child.stderr.take().unwrap();
        if spec.w % 2 == 1 || spec.h % 2 == 1 {
            bail!("{}: odd frame size {}x{} (4:2:0 needs even dimensions)", spec.path.display(), spec.w, spec.h);
        }
        let fsz = spec.w * spec.h * 3 / 2;
        let (tx, rx) = sync_channel::<Vec<u16>>(queue.max(1));
        let reader = std::thread::spawn(move || -> Result<usize> {
            let mut n = 0;
            loop {
                let mut v = vec![0u16; fsz];
                let bytes = unsafe { std::slice::from_raw_parts_mut(v.as_mut_ptr() as *mut u8, fsz * 2) };
                let mut got = 0;
                while got < bytes.len() {
                    let k = out.read(&mut bytes[got..])?;
                    if k == 0 {
                        break;
                    }
                    got += k;
                }
                if got == 0 {
                    return Ok(n);
                }
                if got < bytes.len() {
                    bail!("truncated frame from ffmpeg ({got} of {} bytes)", bytes.len());
                }
                if cfg!(target_endian = "big") {
                    for x in v.iter_mut() {
                        *x = u16::from_le(*x);
                    }
                }
                n += 1;
                if tx.send(v).is_err() {
                    return Ok(n);
                }
            }
        });
        let stderr = std::thread::spawn(move || {
            let mut text = String::new();
            for line in BufReader::new(err).lines().map_while(|l| l.ok()) {
                if text.len() < 4000 && !line.contains("deprecated") {
                    text.push_str(&line);
                    text.push('\n');
                }
            }
            text
        });
        Ok(Decoder { rx, child, reader: Some(reader), stderr: Some(stderr), spec })
    }

    pub fn next(&mut self) -> Option<Vec<u16>> {
        self.rx.recv().ok()
    }

    /// wait for ffmpeg; returns the number of frames it delivered
    pub fn finish(mut self) -> Result<usize> {
        // drain so the reader thread can end
        while self.rx.try_recv().is_ok() {}
        drop(std::mem::replace(&mut self.rx, sync_channel(1).1));
        let n = self.reader.take().unwrap().join().map_err(|_| anyhow!("decoder thread panicked"))?;
        let st = self.child.wait()?;
        let text = self.stderr.take().unwrap().join().map_err(|_| anyhow!("stderr thread panicked"))?;
        let n = n?;
        if !st.success() {
            bail!("ffmpeg failed on {}: {}", self.spec.path.display(), text.trim());
        }
        Ok(n)
    }
}

impl Drop for Decoder {
    fn drop(&mut self) {
        if self.reader.is_some() {
            let _ = self.child.kill();     // exact child, by handle
            let _ = self.child.wait();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// a few frames of an AV1 encode made here with SVT-AV1 (skipped where ffmpeg or libsvtav1 is missing)
    fn encode(dir: &Path, name: &str, params: &str) -> Option<PathBuf> {
        let out = dir.join(name);
        let st = Command::new("ffmpeg")
            .args(["-hide_banner", "-nostdin", "-v", "error", "-y", "-f", "lavfi", "-i",
                   "testsrc2=size=192x128:rate=24:duration=0.5", "-pix_fmt", "yuv420p10le", "-c:v", "libsvtav1",
                   "-preset", "12", "-svtav1-params", params])
            .arg(&out)
            .status()
            .ok()?;
        st.success().then_some(out)
    }

    #[test]
    fn frame_level_grain_detection() {
        let dir = std::env::temp_dir().join(format!("grail_av1_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let Some(grain) = encode(&dir, "grain.mkv", "film-grain=8:film-grain-denoise=0") else {
            eprintln!("ffmpeg with libsvtav1 not available: skipped");
            return;
        };
        let clean = encode(&dir, "clean.mkv", "film-grain=0").unwrap();
        let g = inspect_av1_grain(&grain).unwrap();
        assert!(g.supports && g.applies && g.frames == 12, "{g:?}");
        assert!(g.seeds.iter().all(|s| s.is_some()));
        let c = inspect_av1_grain(&clean).unwrap();
        assert!(!c.supports && !c.applies && c.frames == 12, "{c:?}");
        // fail closed: a file that is not an AV1 stream is an error, not "no grain"
        let bogus = dir.join("bogus.mkv");
        std::fs::write(&bogus, b"not a video").unwrap();
        assert!(inspect_av1_grain(&bogus).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
