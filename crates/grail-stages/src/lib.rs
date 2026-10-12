//! Stage interfaces of the GRAIL pipeline. The pipeline (crate `grail`) only talks to these traits; `grail-gpu`
//! implements them on CUDA (and HIP, compile-checked). A CPU backend implements the same three stages and a
//! `Backend`; nothing else in the pipeline changes. Frames are 10-bit 4:2:0, planes Y, U, V, as flat u16 slices.
use anyhow::Result;
use std::any::Any;
use std::sync::Arc;

pub const NB: usize = 8;

/// one window frame: always on the host; a backend may attach its own copy (device memory) once per frame so that
/// several stages share one upload. `stale` marks a host change that the attached copy does not have yet. The host
/// data is shared (Arc) so that a metric can read it on another thread while a stage attaches its copy.
pub struct Win {
    pub host: Arc<Vec<u16>>,
    pub w: usize,
    pub h: usize,
    pub attached: Option<Box<dyn Any>>,
    pub stale: bool,
}

impl Win {
    pub fn new(w: usize, h: usize) -> Win {
        Win { host: Arc::new(vec![0; w * h * 3 / 2]), w, h, attached: None, stale: true }
    }
    /// mutable host data (copied first if a reader still holds it)
    pub fn host_mut(&mut self) -> &mut Vec<u16> {
        Arc::make_mut(&mut self.host)
    }
    pub fn set(&mut self, data: Vec<u16>) {
        debug_assert_eq!(data.len(), self.w * self.h * 3 / 2);
        self.host = Arc::new(data);
        self.stale = true;
    }
}

/// flowdegrain2 at GRAIL's settings over a stream of active-picture frames (radius 3: frame t is ready once t + 3
/// has been pushed, or at the end of the stream)
pub trait Denoise {
    fn push(&mut self, active: &[u16]) -> Result<()>;
    fn finish(&mut self);
    /// index of the next frame that can be produced
    fn ready(&self) -> Option<usize>;
    /// advance past the next frame without producing it (its neighbours stay available)
    fn skip(&mut self) -> Result<usize>;
    /// produce the next frame and cut the scored window out of it: denoised pixels where the window overlaps the
    /// active picture `act`, `orig` (the original window frame) elsewhere; `win` in the same frame coordinates
    fn produce_window(&mut self, orig: &[u16], act: [usize; 4], win: [usize; 4], out: &mut Win) -> Result<usize>;
}

/// grain features accumulated over the scored frames (PORT_SPEC.md sections 3 and 6)
#[derive(Clone, Debug, Default)]
pub struct FeatureTotals {
    /// band powers (sums over flat pixels) per band and luma bin: source, candidate, candidate vs its grain-free decode
    pub p_s: [[f64; NB]; 5],
    pub p_c: [[f64; NB]; 5],
    pub p_ng: [[f64; NB]; 3],
    pub n_lum: [f64; NB],
    pub chroma: [[f64; 2]; 2],
    pub chroma_n: f64,
    /// per consecutive pair: [source, candidate]
    pub stick: [Vec<f64>; 2],
    pub stickc: [Vec<f64>; 2],
    pub vs: f64,
    pub vc: f64,
    pub flat_frac: Vec<f64>,
    pub has_ng: bool,
}

pub struct FeatureFrame<'a> {
    pub s: &'a mut Win,
    pub ds: &'a mut Win,
    pub c: &'a mut Win,
    pub dc: &'a mut Win,
    pub cng: Option<&'a mut Win>,
}

pub trait GrainFeatures {
    /// one scored frame; `consecutive`: the previous call was frame t - 1 (stick terms between the two)
    fn frame(&mut self, f: FeatureFrame<'_>, consecutive: bool) -> Result<()>;
    fn totals(&self) -> &FeatureTotals;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct DetFrame {
    pub tex6: f64,
    pub inkflat_s: f64,
}

pub trait LineArt {
    /// reference-only detector on one frame
    fn detect(&mut self, s: &mut Win) -> Result<DetFrame>;
    /// ringing peak (near_top, 8-bit units squared) of one frame pair
    fn ring(&mut self, s: &mut Win, c: &mut Win) -> Result<f64>;
}

pub trait Backend {
    fn name(&self) -> String;
    fn denoiser(&self, w: usize, h: usize) -> Result<Box<dyn Denoise>>;
    /// sc = 2 if the source height > 1600 else 1
    fn features(&self, w: usize, h: usize, sc: f64) -> Result<Box<dyn GrainFeatures>>;
    fn lineart(&self, w: usize, h: usize) -> Result<Box<dyn LineArt>>;
}
