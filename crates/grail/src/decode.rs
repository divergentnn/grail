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
}

pub fn probe(path: &Path) -> Result<Probe> {
    let o = Command::new("ffprobe")
        .args(["-v", "error", "-select_streams", "v:0", "-count_packets", "-show_entries",
               "stream=width,height,codec_name,nb_read_packets,color_transfer,chroma_location", "-of", "default=nw=1"])
        .arg(path)
        .output()
        .context("running ffprobe")?;
    if !o.status.success() {
        bail!("ffprobe {}: {}", path.display(), String::from_utf8_lossy(&o.stderr).trim());
    }
    let mut p = Probe { width: 0, height: 0, codec: String::new(), packets: 0, transfer: String::new(),
                        chroma_location: String::new() };
    for line in String::from_utf8_lossy(&o.stdout).lines() {
        if let Some((k, v)) = line.split_once('=') {
            match k {
                "width" => p.width = v.parse().unwrap_or(0),
                "height" => p.height = v.parse().unwrap_or(0),
                "codec_name" => p.codec = v.to_string(),
                "nb_read_packets" => p.packets = v.parse().unwrap_or(0),
                "color_transfer" => p.transfer = v.to_string(),
                "chroma_location" => p.chroma_location = v.to_string(),
                _ => {}
            }
        }
    }
    if p.width == 0 || p.height == 0 {
        bail!("{}: no video stream", path.display());
    }
    Ok(p)
}

/// AV1 sequence header flag film_grain_params_present (trace_headers on the first packet, no decoding)
pub fn av1_film_grain_present(path: &Path) -> Result<bool> {
    let o = Command::new("ffmpeg")
        .args(["-hide_banner", "-nostdin", "-v", "info", "-i"])
        .arg(path)
        .args(["-map", "0:v:0", "-c", "copy", "-bsf:v", "trace_headers", "-frames:v", "1", "-f", "null", "-"])
        .output()
        .context("running ffmpeg trace_headers")?;
    let err = String::from_utf8_lossy(&o.stderr);
    for line in err.lines() {
        if line.contains("film_grain_params_present") {
            return Ok(line.trim_end().ends_with("= 1"));
        }
    }
    Ok(false)
}

#[derive(Clone, Debug)]
pub enum Grain {
    /// as the file decodes by default (no AV1 film grain in it, or not AV1)
    Plain,
    /// AV1 film grain rendered FULL-FRAME by the av1-fullgrain dav1d build in this directory
    FullFrame(PathBuf),
    /// AV1 decoded without its film grain (libdav1d -filmgrain 0); seeds collected from the side data
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
    stderr: Option<JoinHandle<(String, Vec<(usize, u32)>)>>,
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
        let showinfo = matches!(spec.grain, Grain::Off);
        if showinfo {
            vf.push_str(",showinfo=checksum=0");
        }
        if let Some([x, y, w, h]) = spec.crop {
            vf.push_str(&format!(",crop={w}:{h}:{x}:{y}"));
        }
        let mut cmd = Command::new("ffmpeg");
        cmd.args(["-hide_banner", "-nostats", "-nostdin", "-v", if showinfo { "info" } else { "error" }]);
        cmd.args(["-threads", &spec.threads.to_string()]);
        if spec.hwaccel {
            cmd.args(["-hwaccel", if cfg!(target_os = "macos") { "videotoolbox" } else { "cuda" }]);
        }
        if spec.av1 {
            cmd.args(["-c:v", "libdav1d"]);
        }
        match &spec.grain {
            Grain::Off => {
                cmd.args(["-filmgrain", "0"]);
                for (k, _) in std::env::vars() {
                    if k.starts_with("DAV1D_GRAIN") {
                        cmd.env_remove(k);
                    }
                }
                // an av1-fullgrain libdav1d installed as the system one may be built to force its grain on even when
                // the player turns grain off (grain_force): this decode must stay grain-free
                cmd.env("DAV1D_GRAIN_FORCE", "0");
            }
            Grain::FullFrame(dir) => {
                // the av1-fullgrain libdav1d replaces the system one in ffmpeg (dyld searches DYLD_LIBRARY_PATH for
                // the library's file name before its install path)
                let var = if cfg!(target_os = "macos") { "DYLD_LIBRARY_PATH" } else { "LD_LIBRARY_PATH" };
                let old = std::env::var(var).unwrap_or_default();
                let lp = if old.is_empty() { dir.display().to_string() } else { format!("{}:{old}", dir.display()) };
                cmd.env(var, lp).env("DAV1D_GRAIN_MODE", "full").env("DAV1D_GRAIN_FORCE", "1");
            }
            Grain::Plain => {}
        }
        cmd.arg("-i").arg(&spec.path);
        cmd.args(["-map", "0:v:0", "-vf", &vf, "-pix_fmt", "yuv420p10le", "-f", "rawvideo", "-"]);
        cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = cmd.spawn().context("starting ffmpeg")?;
        let mut out = child.stdout.take().unwrap();
        let err = child.stderr.take().unwrap();
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
            // keep error text; parse showinfo: "n:   2 pts: ..." then "side data - Film grain parameters: type av1; seed=N"
            let mut text = String::new();
            let mut seeds = vec![];
            let mut cur: Option<usize> = None;
            for line in BufReader::new(err).lines().map_while(|l| l.ok()) {
                if line.contains("Parsed_showinfo") {
                    if let Some(p) = line.find(" n:") {
                        cur = line[p + 3..].split_whitespace().next().and_then(|v| v.parse().ok());
                    } else if line.contains("Film grain parameters") {
                        if let (Some(t), Some(p)) = (cur, line.find("seed=")) {
                            let s: String = line[p + 5..].chars().take_while(|c| c.is_ascii_digit()).collect();
                            if let Ok(s) = s.parse() {
                                seeds.push((t, s));
                            }
                        }
                    }
                } else if !line.starts_with("  ") && !line.starts_with("Input #") && !line.starts_with("Output #")
                    && !line.starts_with("Stream") && !line.contains("deprecated") {
                    if text.len() < 4000 {
                        text.push_str(&line);
                        text.push('\n');
                    }
                }
            }
            (text, seeds)
        });
        Ok(Decoder { rx, child, reader: Some(reader), stderr: Some(stderr), spec })
    }

    pub fn next(&mut self) -> Option<Vec<u16>> {
        self.rx.recv().ok()
    }

    /// wait for ffmpeg; returns (frames read, per-frame AV1 grain seeds (output index, seed))
    pub fn finish(mut self) -> Result<(usize, Vec<(usize, u32)>)> {
        // drain so the reader thread can end
        while self.rx.try_recv().is_ok() {}
        drop(std::mem::replace(&mut self.rx, sync_channel(1).1));
        let n = self.reader.take().unwrap().join().map_err(|_| anyhow!("decoder thread panicked"))?;
        let st = self.child.wait()?;
        let (text, seeds) = self.stderr.take().unwrap().join().map_err(|_| anyhow!("stderr thread panicked"))?;
        let n = n?;
        if !st.success() {
            bail!("ffmpeg failed on {}: {}", self.spec.path.display(), text.trim());
        }
        Ok((n, seeds))
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
