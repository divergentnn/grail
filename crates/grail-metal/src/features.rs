//! GRAIL grain features (PORT_SPEC.md sections 3 and 6; extract.features, the terms that carry weight in v2.1).
//! Per scored frame on the window: flat mask and luma bins from Ds, residual band powers of s - Ds and c - Dc per
//! bin, chroma residual power, consecutive-frame stick correlations, texture energy of the originals; plus the band
//! power of c - c_nograin for the grainy-base check (R).
use crate::args;
use crate::buf::{launch, launch_n, launch_reduce, DBuf};
use crate::ops::{corr2, count_lt, parts_for, sum_parts, Kern, Select};
use anyhow::Result;

pub const NB: usize = 8;
const ACC_N: usize = 144;
const ACC_STICK: usize = 128;
const ACC_FLAT: usize = 141;

/// device frames of one scored window frame: flat u16 4:2:0 (Y, U, V), window size
#[derive(Clone, Copy)]
pub struct FrameSet {
    pub s: u64,
    pub ds: u64,
    pub c: u64,
    pub dc: u64,
    /// grain-free decode of the candidate (grainy-base check, full mode on the synthetic path)
    pub cng: Option<u64>,
}

pub use grail_stages::FeatureTotals;

pub struct Features {
    w: usize,
    h: usize,
    n: usize,
    nc: usize,
    // kernels
    g_low: Kern,
    u_low: Kern,
    gb: [Kern; 5],
    g24: Kern,
    // buffers
    f: [DBuf; 5],          // s, Ds, c, Dc, c_ng as float / 4 (Y then U, V)
    low: DBuf,
    low2: DBuf,
    m1: DBuf,
    m2: DBuf,
    loc: DBuf,
    tmp: DBuf,
    mask: [DBuf; 2],
    lum: DBuf,
    rs: DBuf,              // r + 5 Gaussians (6 n)
    rc: DBuf,
    rng: DBuf,             // r + 3 Gaussians (4 n)
    os: DBuf,              // source original + 4 Gaussians (s itself unused: 4 n)
    oc: DBuf,
    planes: [DBuf; 2],     // stick planes (4 n), ping-pong
    cr: DBuf,              // chroma residuals (4 nc)
    cg: DBuf,
    acc: DBuf,
    tex: DBuf,
    cacc: DBuf,
    th: DBuf,
    cnt: DBuf,
    qtmp: DBuf,
    sel: Select,
    cur: usize,
    prev_valid: bool,
    pub tot: FeatureTotals,
}

impl Features {
    /// w x h window; sc = 2 if the SOURCE height > 1600 else 1
    pub fn new(w: usize, h: usize, sc: f64) -> Result<Features> {
        let n = w * h;
        let nc = (w / 2) * (h / 2);
        let fsz = (n + 2 * nc) * 4;
        let wn = ((31.0 * sc) as usize) | 1;
        Ok(Features {
            w,
            h,
            n,
            nc,
            g_low: Kern::gauss_scipy(4.0 * sc)?,
            u_low: Kern::uniform(wn)?,
            gb: [Kern::gauss_scipy(0.6)?, Kern::gauss_scipy(1.2)?, Kern::gauss_scipy(2.4)?, Kern::gauss_scipy(4.8)?,
                 Kern::gauss_scipy(9.6)?],
            g24: Kern::gauss_scipy(2.4)?,
            f: [DBuf::new(fsz)?, DBuf::new(fsz)?, DBuf::new(fsz)?, DBuf::new(fsz)?, DBuf::new(fsz)?],
            low: DBuf::f32(n)?,
            low2: DBuf::f32(n)?,
            m1: DBuf::f32(n)?,
            m2: DBuf::f32(n)?,
            loc: DBuf::f32(n)?,
            tmp: DBuf::f32(4 * n.max(nc))?,
            mask: [DBuf::new(n)?, DBuf::new(n)?],
            lum: DBuf::new(n)?,
            rs: DBuf::f32(6 * n)?,
            rc: DBuf::f32(6 * n)?,
            rng: DBuf::f32(4 * n)?,
            os: DBuf::f32(4 * n)?,
            oc: DBuf::f32(4 * n)?,
            planes: [DBuf::f32(4 * n)?, DBuf::f32(4 * n)?],
            cr: DBuf::f32(4 * nc)?,
            cg: DBuf::f32(4 * nc)?,
            acc: DBuf::f32(parts_for(n) * ACC_N)?,
            tex: DBuf::f32(((w / 64) * (h / 64)).max(1) * 2)?,
            cacc: DBuf::f32(parts_for(nc) * 5)?,
            th: DBuf::new(4)?,
            cnt: DBuf::new(8)?,
            qtmp: DBuf::new(16)?,
            sel: Select::new()?,
            cur: 0,
            prev_valid: false,
            tot: FeatureTotals::default(),
        })
    }

    fn gauss(&self, src: u64, dst: u64, k: &Kern, planes: usize, w: usize, h: usize) -> Result<()> {
        corr2(src, dst, self.tmp.ptr, w, h, planes, k, k, false)
    }

    /// one scored frame; `consecutive`: the previous call was frame t - 1 (stick terms between the two)
    pub fn frame(&mut self, fr: &FrameSet, consecutive: bool) -> Result<()> {
        let (w, h, n, nc) = (self.w, self.h, self.n, self.nc);
        let tot_n = n + 2 * nc;
        let srcs = [Some(fr.s), Some(fr.ds), Some(fr.c), Some(fr.dc), fr.cng];
        for (i, s) in srcs.iter().enumerate() {
            if let Some(p) = s {
                launch_n("u16_to_f32_q", tot_n, args![*p, &self.f[i], tot_n as i64])?;
            }
        }
        let fp = |i: usize| self.f[i].ptr;
        // flat mask from Ds
        self.gauss(fp(1), self.low.ptr, &self.g_low, 1, w, h)?;
        launch_n("ew_sqr", n, args![&self.low, &self.low2, n as i64])?;
        self.gauss(self.low.ptr, self.m1.ptr, &self.u_low, 1, w, h)?;
        self.gauss(self.low2.ptr, self.m2.ptr, &self.u_low, 1, w, h)?;
        launch_n("local_sd", n, args![&self.m1, &self.m2, &self.loc, n as i64])?;
        let below = count_lt(self.loc.ptr, n, 1.2, &self.cnt)?;
        let use_p10 = (below as f64 / n as f64) < 0.03;
        if use_p10 {
            // cupy.percentile(loc, 10)
            self.sel.quantile(self.loc.ptr, n, 0.10, self.th.ptr, &self.qtmp)?;
        }
        launch("flat_threshold", (1, 1, 1), (32, 1, 1), 0, args![&self.th, use_p10 as i32, &self.th])?;
        let mcur = self.cur;
        launch_n("flat_mask", n, args![&self.loc, &self.low, &self.th, &self.mask[mcur], &self.lum, n as i64])?;
        // residual bands
        let resid = |a: usize, b: usize, out: &DBuf| launch_n("ew_sub", n, args![fp(a), fp(b), out.ptr, n as i64]);
        resid(0, 1, &self.rs)?;
        resid(2, 3, &self.rc)?;
        for (i, k) in self.gb.iter().enumerate() {
            self.gauss(self.rs.ptr, self.rs.ptr + ((i + 1) * n * 4) as u64, k, 1, w, h)?;
            self.gauss(self.rc.ptr, self.rc.ptr + ((i + 1) * n * 4) as u64, k, 1, w, h)?;
        }
        let ng = if fr.cng.is_some() {
            resid(2, 4, &self.rng)?;
            for i in 0..3 {
                self.gauss(self.rng.ptr, self.rng.ptr + ((i + 1) * n * 4) as u64, &self.gb[i], 1, w, h)?;
            }
            self.tot.has_ng = true;
            self.rng.ptr
        } else {
            0
        };
        let pv = consecutive && self.prev_valid;
        let mprev = if pv { self.mask[1 - mcur].ptr } else { 0 };
        launch_reduce("feat_accum", n, args![&self.rs, &self.rc, ng, &self.mask[mcur], &self.lum, mprev,
                                              &self.planes[1 - mcur], &self.planes[mcur], n as i64, &self.acc])?;
        // texture energy of the originals (bands 1..3: G0.6 .. G4.8)
        for i in 0..4 {
            self.gauss(fp(0), self.os.ptr + (i * n * 4) as u64, &self.gb[i], 1, w, h)?;
            self.gauss(fp(2), self.oc.ptr + (i * n * 4) as u64, &self.gb[i], 1, w, h)?;
        }
        let (tw, th) = (w / 64, h / 64);
        if tw > 0 && th > 0 {
            let o = |b: &DBuf, i: usize| b.ptr + (i * n * 4) as u64;
            launch("tex_tiles", (tw as u32, th as u32, 1), (256, 1, 1), 0,
                   args![o(&self.os, 0), o(&self.os, 1), o(&self.os, 2), o(&self.os, 3), o(&self.oc, 0),
                         o(&self.oc, 1), o(&self.oc, 2), o(&self.oc, 3), &self.mask[mcur], &self.loc, w as i32, &self.tex])?;
        }
        // chroma residuals (U, V of s - Ds, then of c - Dc), minus their G2.4
        let (w2, h2) = (w / 2, h / 2);
        let off = (n * 4) as u64;
        launch_n("ew_sub", 2 * nc, args![fp(0) + off, fp(1) + off, &self.cr, (2 * nc) as i64])?;
        launch_n("ew_sub", 2 * nc, args![fp(2) + off, fp(3) + off, self.cr.ptr + (2 * nc * 4) as u64, (2 * nc) as i64])?;
        self.gauss(self.cr.ptr, self.cg.ptr, &self.g24, 4, w2, h2)?;
        launch_reduce("chroma_accum", nc, args![&self.cr, &self.cg, &self.mask[mcur], w as i32, w2 as i32, h2 as i32, &self.cacc])?;
        // read back
        let acc = sum_parts(&self.acc, parts_for(n), ACC_N)?;
        let tex = if tw > 0 && th > 0 { sum_parts(&self.tex, tw * th, 2)? } else { vec![0.0; 2] };
        let ca = sum_parts(&self.cacc, parts_for(nc), 5)?;
        let t = &mut self.tot;
        t.flat_frac.push(acc[ACC_FLAT] / n as f64);
        for k in 0..NB {
            let a = &acc[k * 16..k * 16 + 16];
            if a[0] < 2000.0 {
                continue;
            }
            t.n_lum[k] += a[0];
            for b in 0..5 {
                t.p_s[b][k] += a[1 + b];
                t.p_c[b][k] += a[6 + b];
            }
            for b in 0..3 {
                t.p_ng[b][k] += a[11 + b];
            }
        }
        if ca[4] > 500.0 {
            t.chroma[0][0] += ca[0];
            t.chroma[0][1] += ca[1];
            t.chroma[1][0] += ca[2];
            t.chroma[1][1] += ca[3];
            t.chroma_n += ca[4];
        }
        if pv && acc[ACC_STICK + 12] > 5000.0 {
            let s = &acc[ACC_STICK..ACC_STICK + 12];
            let corr = |a: f64, b: f64, c: f64| a / (b * c + 1e-9).sqrt();
            t.stick[0].push(corr(s[0], s[1], s[2]));
            t.stick[1].push(corr(s[3], s[4], s[5]));
            t.stickc[0].push(corr(s[6], s[7], s[8]));
            t.stickc[1].push(corr(s[9], s[10], s[11]));
        }
        t.vs += tex[0];
        t.vc += tex[1];
        self.prev_valid = true;
        self.cur = 1 - self.cur;
        Ok(())
    }
}
