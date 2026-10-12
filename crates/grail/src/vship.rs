//! SSIMULACRA2 through libvship (the library under FFVship), loaded at run time. Same C API and colourspace as the
//! Python reference (vship_api.py): 10-bit limited range, chroma left, BT.709, or BT.2020 + PQ for HDR10.
//! libvship is MIT-licensed with an added non-AI clause (README, "Third-party"); it is not bundled.
use anyhow::{bail, Context, Result};
use libloading::Library;
use std::ffi::{c_char, c_void, CStr};

#[repr(C)]
#[derive(Clone, Copy)]
struct Colorspace {
    width: i64,
    height: i64,
    target_width: i64,
    target_height: i64,
    sample: i32,
    range: i32,
    subw: i32,
    subh: i32,
    chroma_location: i32,
    color_family: i32,
    yuv_matrix: i32,
    transfer: i32,
    primaries: i32,
    crop_top: i32,
    crop_bottom: i32,
    crop_left: i32,
    crop_right: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Handler {
    id: u32,
}

type InitFn = unsafe extern "C" fn(*mut Handler, Colorspace, Colorspace) -> i32;
type ComputeFn = unsafe extern "C" fn(Handler, *mut f64, *const *const u8, *const *const u8, *const i64, *const i64) -> i32;
type FreeFn = unsafe extern "C" fn(Handler) -> i32;
type ErrFn = unsafe extern "C" fn(*mut c_char, i32) -> i32;

pub struct Vship {
    _lib: Library,
    init: InitFn,
    compute: ComputeFn,
    free: FreeFn,
    err: ErrFn,
    #[allow(dead_code)]
    pub path: String,
}

/// FFVship's mapping of a file's chroma location (ffprobe name) to vship's enum: unspecified -> left
pub fn chroma_loc(name: &str) -> i32 {
    match name {
        "center" => 1,
        "topleft" => 2,
        "top" => 3,
        "left" | "unspecified" | "" => 0,
        other => {
            eprintln!("grail: chroma location {other} not handled by FFVship's mapping; using left");
            0
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Colour {
    Sdr,
    Pq,
}

impl Vship {
    pub fn load() -> Result<Vship> {
        let mut cands: Vec<String> = vec![];
        if let Ok(p) = std::env::var("GRAIL_VSHIP") {
            cands.push(p);
        }
        cands.extend(["libvship.so".into(), "/usr/local/lib/libvship.so".into(), "/usr/lib/libvship.so".into()]);
        let mut errs = vec![];
        for p in &cands {
            match unsafe { Library::new(p) } {
                Ok(lib) => {
                    let get = |n: &[u8]| -> Result<*const c_void> {
                        let s: libloading::Symbol<*const c_void> = unsafe { lib.get(n) }?;
                        Ok(*s)
                    };
                    let init = get(b"Vship_SSIMU2Init\0").context("Vship_SSIMU2Init")?;
                    let compute = get(b"Vship_ComputeSSIMU2\0").context("Vship_ComputeSSIMU2")?;
                    let free = get(b"Vship_SSIMU2Free\0").context("Vship_SSIMU2Free")?;
                    let err = get(b"Vship_GetDetailedLastError\0").context("Vship_GetDetailedLastError")?;
                    return Ok(Vship {
                        init: unsafe { std::mem::transmute::<*const c_void, InitFn>(init) },
                        compute: unsafe { std::mem::transmute::<*const c_void, ComputeFn>(compute) },
                        free: unsafe { std::mem::transmute::<*const c_void, FreeFn>(free) },
                        err: unsafe { std::mem::transmute::<*const c_void, ErrFn>(err) },
                        _lib: lib,
                        path: p.clone(),
                    });
                }
                Err(e) => errs.push(format!("{p}: {e}")),
            }
        }
        bail!("libvship not found (set GRAIL_VSHIP): {}", errs.join("; "))
    }

    fn last_error(&self) -> String {
        let mut buf = vec![0 as c_char; 1024];
        unsafe { (self.err)(buf.as_mut_ptr(), 1024) };
        unsafe { CStr::from_ptr(buf.as_ptr()) }.to_string_lossy().into_owned()
    }

    /// chroma locations (vship enum: 0 left, 1 center, 2 top-left, 3 top) of the reference and the distorted side
    pub fn ssimu2(&self, w: usize, h: usize, colour: Colour, loc: (i32, i32)) -> Result<Ssimu2<'_>> {
        let pq = colour == Colour::Pq;
        let cs = Colorspace {
            width: w as i64,
            height: h as i64,
            target_width: -1,
            target_height: -1,
            sample: 5,
            range: 0,
            subw: 1,
            subh: 1,
            chroma_location: 0,
            color_family: 0,
            yuv_matrix: if pq { 9 } else { 1 },
            transfer: if pq { 16 } else { 1 },
            primaries: if pq { 9 } else { 1 },
            crop_top: 0,
            crop_bottom: 0,
            crop_left: 0,
            crop_right: 0,
        };
        let mut hd = Handler::default();
        let (src, dis) = (Colorspace { chroma_location: loc.0, ..cs }, Colorspace { chroma_location: loc.1, ..cs });
        let r = unsafe { (self.init)(&mut hd, src, dis) };
        if r != 0 {
            bail!("vship SSIMU2Init: error {r}: {}", self.last_error());
        }
        Ok(Ssimu2 { v: self, hd, w, h })
    }
}

/// one SSIMULACRA2 handler for a frame size and colourspace
pub struct Ssimu2<'a> {
    v: &'a Vship,
    hd: Handler,
    w: usize,
    h: usize,
}

impl Ssimu2<'_> {
    /// SSIMULACRA2 of two flat 10-bit 4:2:0 frames (u16 Y, U, V)
    pub fn score(&self, a: &[u16], b: &[u16]) -> Result<f64> {
        let (w, h) = (self.w, self.h);
        let n = w * h + 2 * (w / 2) * (h / 2);
        assert!(a.len() >= n && b.len() >= n);
        let planes = |x: &[u16]| -> [*const u8; 3] {
            let p = x.as_ptr() as *const u8;
            unsafe { [p, p.add(w * h * 2), p.add(w * h * 2 + (w / 2) * (h / 2) * 2)] }
        };
        let (pa, pb) = (planes(a), planes(b));
        let ls = [(w * 2) as i64, w as i64, w as i64];
        let mut s = 0f64;
        let r = unsafe { (self.v.compute)(self.hd, &mut s, pa.as_ptr(), pb.as_ptr(), ls.as_ptr(), ls.as_ptr()) };
        if r != 0 {
            bail!("vship ComputeSSIMU2: error {r}: {}", self.v.last_error());
        }
        Ok(s)
    }
}

impl Drop for Ssimu2<'_> {
    fn drop(&mut self) {
        unsafe { (self.v.free)(self.hd) };
    }
}
