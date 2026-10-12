//! Shared GPU operations: scipy.ndimage-style separable filters (mode "reflect"), radix selection (order statistics
//! without a host round trip) and small reductions.
use crate::args;
use crate::buf::{launch, launch_n, launch_reduce, DBuf};
use anyhow::Result;

/// a 1-D correlation kernel on the device (weights w[0 .. 2r])
pub struct Kern {
    pub buf: DBuf,
    pub r: i32,
}
impl Kern {
    pub fn new(w: &[f32]) -> Result<Kern> {
        assert!(w.len() % 2 == 1);
        Ok(Kern { buf: DBuf::upload(w)?, r: (w.len() / 2) as i32 })
    }
    /// scipy.ndimage gaussian_filter weights (truncate 3.0: radius int(3 sigma + 0.5)), float64 -> float32
    pub fn gauss_scipy(sigma: f64) -> Result<Kern> {
        let r = (3.0 * sigma + 0.5) as i64;
        let s2 = sigma * sigma;
        let phi: Vec<f64> = (-r..=r).map(|x| (-0.5 / s2 * (x * x) as f64).exp()).collect();
        let sum: f64 = phi.iter().sum();
        Kern::new(&phi.iter().map(|v| (v / sum) as f32).collect::<Vec<_>>())
    }
    /// scipy.ndimage uniform_filter weights (odd size)
    pub fn uniform(size: usize) -> Result<Kern> {
        Kern::new(&vec![(1.0 / size as f64) as f32; size])
    }
}

/// separable correlation over `planes` planes of W x H floats: axis 0 (rows) first, then axis 1, like
/// scipy.ndimage.gaussian_filter / uniform_filter; kv / kh may differ (Sobel). `double` accumulates in float64.
#[allow(clippy::too_many_arguments)]
pub fn corr2(src: u64, dst: u64, tmp: u64, w: usize, h: usize, planes: usize, kv: &Kern, kh: &Kern, double: bool)
             -> Result<()> {
    let n = (w * h * planes) as i64;
    let name = if double { "corr_d" } else { "corr_f" };
    launch_n(name, n as usize, args![src, tmp, w as i32, h as i32, n, &kv.buf, kv.r, 0i32])?;
    launch_n(name, n as usize, args![tmp, dst, w as i32, h as i32, n, &kh.buf, kh.r, 1i32])
}

pub fn filt(src: &DBuf, dst: &DBuf, tmp: &DBuf, w: usize, h: usize, planes: usize, k: &Kern) -> Result<()> {
    corr2(src.ptr, dst.ptr, tmp.ptr, w, h, planes, k, k, false)
}

/// k-th smallest (0-based) of n floats at `v` -> *out (device float); the input is not modified
pub struct Select {
    st: DBuf,
    hist: DBuf,
}
impl Select {
    pub fn new() -> Result<Select> {
        Ok(Select { st: DBuf::zeroed(8)?, hist: DBuf::zeroed(256 * 4)? })
    }
    pub fn kth(&self, v: u64, n: usize, k: u32, out: u64) -> Result<()> {
        launch("sel_init", (1, 1, 1), (256, 1, 1), 0, args![&self.st, &self.hist, k])?;
        for shift in [24i32, 16, 8, 0] {
            launch_reduce("sel_hist", n, args![v, n as i64, &self.st, &self.hist, shift])?;
            launch("sel_scan", (1, 1, 1), (32, 1, 1), 0, args![&self.st, &self.hist, shift])?;
        }
        launch("sel_value", (1, 1, 1), (32, 1, 1), 0, args![&self.st, out])
    }
    /// cupy.quantile(x, q) (method linear, float32 result) of n floats -> *out; scratch: two floats at tmp
    pub fn quantile(&self, v: u64, n: usize, q: f64, out: u64, tmp: &DBuf) -> Result<()> {
        let idx = q * (n as f64 - 1.0);
        let lo = idx.floor();
        let hi = (lo as usize + 1).min(n - 1);
        let w = (idx - lo) as f32;
        self.kth(v, n, lo as u32, tmp.ptr)?;
        self.kth(v, n, hi as u32, tmp.ptr + 4)?;
        launch("quantile_finish", (1, 1, 1), (32, 1, 1), 0, args![tmp.ptr, tmp.ptr + 4, w, out])
    }
}

/// count of a[i] < th (synchronous)
pub fn count_lt(a: u64, n: usize, th: f32, scratch: &DBuf) -> Result<u64> {
    scratch.zero()?;
    launch_reduce("count_lt", n, args![a, th, n as i64, scratch])?;
    let mut c = [0u64; 1];
    scratch.read(&mut c)?;
    Ok(c[0])
}
