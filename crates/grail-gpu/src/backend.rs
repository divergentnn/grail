//! grail-stages implementation on the GPU (CUDA; HIP compile-checked).
use crate::args;
use crate::buf::{launch_n, DBuf};
use crate::denoise::Denoiser;
use crate::features::{FrameSet, Features};
use crate::lineart::LineArt as GLineArt;
use anyhow::Result;
use grail_stages::{Backend, DetFrame, Denoise, FeatureFrame, FeatureTotals, GrainFeatures, LineArt, Win};

/// the device copy of a window frame (uploaded once per frame, shared by the stages)
pub fn dev(w: &mut Win) -> Result<u64> {
    if w.attached.as_ref().map(|a| !a.is::<DBuf>()).unwrap_or(true) {
        w.attached = Some(Box::new(DBuf::new(w.host.len() * 2)?));
        w.stale = true;
    }
    let b = w.attached.as_ref().unwrap().downcast_ref::<DBuf>().unwrap();
    if w.stale {
        b.write(&w.host)?;
        w.stale = false;
    }
    Ok(b.ptr)
}

pub struct GpuBackend {
    pub device: String,
}

impl GpuBackend {
    pub fn new() -> Result<GpuBackend> {
        let g = crate::driver::try_init()?;
        g.bind_thread()?;
        Ok(GpuBackend { device: format!("{} ({:?})", g.device_name, g.backend) })
    }
}

impl Backend for GpuBackend {
    fn name(&self) -> String {
        self.device.clone()
    }
    fn denoiser(&self, w: usize, h: usize) -> Result<Box<dyn Denoise>> {
        Ok(Box::new(GpuDenoise { d: Denoiser::new(w, h)?, out: DBuf::new(w * h * 3)?, orig: None, aw: w, ah: h }))
    }
    fn features(&self, w: usize, h: usize, sc: f64) -> Result<Box<dyn GrainFeatures>> {
        Ok(Box::new(GpuFeatures(Features::new(w, h, sc)?)))
    }
    fn lineart(&self, w: usize, h: usize) -> Result<Box<dyn LineArt>> {
        Ok(Box::new(GpuLineArt(GLineArt::new(w, h)?)))
    }
}

struct GpuDenoise {
    d: Denoiser,
    out: DBuf,
    orig: Option<DBuf>,
    aw: usize,
    ah: usize,
}

impl Denoise for GpuDenoise {
    fn push(&mut self, active: &[u16]) -> Result<()> {
        self.d.push(active)
    }
    fn finish(&mut self) {
        self.d.finish()
    }
    fn ready(&self) -> Option<usize> {
        self.d.ready()
    }
    fn skip(&mut self) -> Result<usize> {
        self.d.step(false, &self.out)
    }
    fn produce_window(&mut self, orig: &[u16], act: [usize; 4], win: [usize; 4], out: &mut Win) -> Result<usize> {
        let t = self.d.step(true, &self.out)?;
        debug_assert_eq!((act[2], act[3]), (self.aw, self.ah));
        let (w, h) = (win[2], win[3]);
        let n = w * h * 3 / 2;
        assert_eq!(orig.len(), n);
        if self.orig.as_ref().map(|b| b.bytes < n * 2).unwrap_or(true) {
            self.orig = Some(DBuf::new(n * 2)?);
        }
        let src = self.orig.as_ref().unwrap();
        src.write(orig)?;
        out.host_mut().resize(n, 0);
        out.stale = false;
        let dst = dev(out)?;
        launch_n("cut_window", n, args![&self.out, act[2] as i32, act[3] as i32, src, win[0] as i32, win[1] as i32,
                                        w as i32, h as i32, act[0] as i32, act[1] as i32, dst])?;
        let b = out.attached.as_ref().unwrap().downcast_ref::<DBuf>().unwrap();
        let host: &mut Vec<u16> = std::sync::Arc::make_mut(&mut out.host);
        b.read(host.as_mut_slice())?;
        out.stale = false;
        Ok(t)
    }
}

struct GpuFeatures(Features);

impl GrainFeatures for GpuFeatures {
    fn frame(&mut self, f: FeatureFrame<'_>, consecutive: bool) -> Result<()> {
        let fs = FrameSet {
            s: dev(f.s)?,
            ds: dev(f.ds)?,
            c: dev(f.c)?,
            dc: dev(f.dc)?,
            cng: match f.cng {
                Some(w) => Some(dev(w)?),
                None => None,
            },
        };
        self.0.frame(&fs, consecutive)
    }
    fn totals(&self) -> &FeatureTotals {
        &self.0.tot
    }
}

struct GpuLineArt(GLineArt);

impl LineArt for GpuLineArt {
    fn detect(&mut self, s: &mut Win) -> Result<DetFrame> {
        let p = dev(s)?;
        self.0.detect(p)
    }
    fn ring(&mut self, s: &mut Win, c: &mut Win) -> Result<f64> {
        let (a, b) = (dev(s)?, dev(c)?);
        self.0.ring(a, b)
    }
}
