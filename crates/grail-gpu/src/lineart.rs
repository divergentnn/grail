//! Anime / line-art path (PORT_SPEC.md section 8; lineart.py): the reference-only detector (ink strokes over flat
//! colour, textured-cell share) and the ringing peak next to the source's lines.
use crate::args;
use crate::buf::{launch_n, launch_reduce, DBuf};
use crate::ops::{corr2, Kern, Select};
use anyhow::Result;

pub struct LineArt {
    w: usize,
    h: usize,
    g08: Kern,
    g10: Kern,
    u7: Kern,
    d1: Kern,
    s1: Kern,
    y: DBuf,
    c: DBuf,
    a: DBuf,
    b: DBuf,
    sm: DBuf,
    sx: DBuf,
    sy: DBuf,
    edge: DBuf,
    col: DBuf,
    m1: DBuf,
    m2: DBuf,
    y2: DBuf,
    gs: DBuf,
    gc: DBuf,
    val: DBuf,
    tmp: DBuf,
    cnt: DBuf,
    sum: DBuf,
    thr: DBuf,
    sel: Select,
}

pub use grail_stages::DetFrame;

impl LineArt {
    pub fn new(w: usize, h: usize) -> Result<LineArt> {
        let n = w * h;
        Ok(LineArt {
            w,
            h,
            g08: Kern::gauss_scipy(0.8)?,
            g10: Kern::gauss_scipy(1.0)?,
            u7: Kern::uniform(7)?,
            d1: Kern::new(&[-1.0, 0.0, 1.0])?,
            s1: Kern::new(&[1.0, 2.0, 1.0])?,
            y: DBuf::f32(n)?,
            c: DBuf::f32(n)?,
            a: DBuf::f32(n)?,
            b: DBuf::f32(n)?,
            sm: DBuf::f32(n)?,
            sx: DBuf::f32(n)?,
            sy: DBuf::f32(n)?,
            edge: DBuf::new(n)?,
            col: DBuf::new(n)?,
            m1: DBuf::f32(n)?,
            m2: DBuf::f32(n)?,
            y2: DBuf::f32(n)?,
            gs: DBuf::f32(n)?,
            gc: DBuf::f32(n)?,
            val: DBuf::f32(n)?,
            tmp: DBuf::f32(n)?,
            cnt: DBuf::new(32)?,
            sum: DBuf::new(8)?,
            thr: DBuf::new(4)?,
            sel: Select::new()?,
        })
    }

    /// detector on one reference frame (device u16 window, Y first)
    pub fn detect(&mut self, s: u64) -> Result<DetFrame> {
        let (w, h) = (self.w, self.h);
        let n = w * h;
        launch_n("u16_to_f32", n, args![s, &self.y, n as i64])?;
        // grey closing 9x9 = max filter then min filter (separable)
        launch_n("minmax1d", n, args![&self.y, &self.a, w as i32, h as i32, 4i32, 1i32, 1i32])?;
        launch_n("minmax1d", n, args![&self.a, &self.b, w as i32, h as i32, 4i32, 0i32, 1i32])?;
        launch_n("minmax1d", n, args![&self.b, &self.a, w as i32, h as i32, 4i32, 1i32, 0i32])?;
        launch_n("minmax1d", n, args![&self.a, &self.b, w as i32, h as i32, 4i32, 0i32, 0i32])?;
        self.cnt.zero()?;
        launch_n("lineart_cells", (w / 8) * (h / 8), args![&self.y, &self.b, w as i32, h as i32, &self.cnt])?;
        let mut c = [0u64; 4];
        self.cnt.read(&mut c)?;
        let cells = c[3] as f64;
        Ok(DetFrame {
            tex6: if cells > 0.0 { c[0] as f64 / cells } else { 0.0 },
            inkflat_s: c[2] as f64 / (c[1] as f64 + 0.02 * cells),
        })
    }

    /// ringing next to the source's lines: near_top (8-bit units squared) of one frame pair
    pub fn ring(&mut self, s: u64, c: u64) -> Result<f64> {
        let (w, h) = (self.w, self.h);
        let n = w * h;
        launch_n("u16_to_f32_q", n, args![s, &self.y, n as i64])?;
        launch_n("u16_to_f32_q", n, args![c, &self.c, n as i64])?;
        let t = self.tmp.ptr;
        corr2(self.y.ptr, self.sm.ptr, t, w, h, 1, &self.g08, &self.g08, false)?;
        // Sobel along axis 0 and axis 1 (integer weights: cupyx accumulates in float64)
        corr2(self.sm.ptr, self.sx.ptr, t, w, h, 1, &self.d1, &self.s1, true)?;
        corr2(self.sm.ptr, self.sy.ptr, t, w, h, 1, &self.s1, &self.d1, true)?;
        launch_n("edge_mask", n, args![&self.sx, &self.sy, &self.edge, 6.0f32, n as i64])?;
        launch_n("edt_col", n, args![&self.edge, &self.col, w as i32, h as i32])?;
        corr2(self.y.ptr, self.m1.ptr, t, w, h, 1, &self.u7, &self.u7, false)?;
        launch_n("ew_sqr", n, args![&self.y, &self.y2, n as i64])?;
        corr2(self.y2.ptr, self.m2.ptr, t, w, h, 1, &self.u7, &self.u7, false)?;
        corr2(self.y.ptr, self.gs.ptr, t, w, h, 1, &self.g10, &self.g10, false)?;
        corr2(self.c.ptr, self.gc.ptr, t, w, h, 1, &self.g10, &self.g10, false)?;
        self.cnt.zero()?;
        // A.FLAT_T * 3 = 3.5999999999999996 compared against float32: 3.6f
        launch_n("ring_near", n, args![&self.col, &self.m1, &self.m2, &self.y, &self.gs, &self.c, &self.gc,
                                       w as i32, h as i32, 3.6f32, &self.val, &self.cnt])?;
        let mut k = [0u64; 1];
        self.cnt.read(&mut k)?;
        let nn = k[0] as usize;
        if nn <= 500 {
            return Ok(0.0);
        }
        let top = (nn / 1000).max(1);
        // non-near pixels hold -1: the top-th largest near value has ascending rank n - top
        self.sel.kth(self.val.ptr, n, (n - top) as u32, self.thr.ptr)?;
        self.sum.zero()?;
        self.cnt.zero()?;
        launch_reduce("sum_above", n, args![&self.val, &self.thr, n as i64, &self.sum, &self.cnt])?;
        let mut s = [0f64; 1];
        self.sum.read(&mut s)?;
        let mut cg = [0u64; 1];
        self.cnt.read(&mut cg)?;
        let mut th = [0f32; 1];
        self.thr.read(&mut th)?;
        Ok((s[0] + (top as f64 - cg[0] as f64) * th[0] as f64) / top as f64)
    }
}
