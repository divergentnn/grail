//! Device images built by build.rs and the kernels each one provides.
use crate::driver::Backend;

pub const FDG_KERNELS: &[&str] = &[
    "sep3_0", "sep3_1", "sep5_0", "sep5_1", "sep7_0", "sep7_1", "sep8_0", "sep8_1", "sep19_0", "sep19_1",
    "sep37_0", "sep37_1", "sep", "down", "raster", "dense", "warp_y", "weight_only", "reduce_frames", "chroma",
];
pub const GRAIL_KERNELS: &[&str] = &[
    "u16_to_f32", "u16_to_f32_q", "cut_window", "ew_absdiff", "ew_energy", "ew_sub", "ew_sqr", "ew_abs", "ew_div_bc",
    "ew_q8", "ew_rint", "ew_guide", "ew_alpha", "ew_blend", "ew_finish", "ew_pack", "gather_stride", "sel_init",
    "sel_hist", "sel_scan", "sel_value", "noise_var", "quantile_finish", "corr_f", "corr_d", "minmax1d", "local_sd",
    "count_lt", "flat_mask", "flat_threshold", "feat_accum", "tex_tiles", "chroma_accum", "lineart_cells",
    "edge_mask", "edt_col", "ring_near", "sum_above",
];

#[cfg(feature = "cuda")]
static CUDA_FDG: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/flowdegrain2.fatbin"));
#[cfg(feature = "cuda")]
static CUDA_GRAIL: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/grail.fatbin"));
#[cfg(feature = "hip")]
static HIP_FDG: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/flowdegrain2.hsaco"));
#[cfg(feature = "hip")]
static HIP_GRAIL: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/grail.hsaco"));

pub fn available(b: Backend) -> bool {
    match b {
        Backend::Cuda => cfg!(feature = "cuda"),
        Backend::Hip => cfg!(feature = "hip"),
    }
}

/// (image, kernels) per module; images are copied into aligned buffers that live for the process
pub fn modules(b: Backend) -> Vec<(&'static [u8], &'static [&'static str])> {
    let raw: Vec<(&'static [u8], &'static [&'static str])> = match b {
        #[cfg(feature = "cuda")]
        Backend::Cuda => vec![(CUDA_FDG, FDG_KERNELS), (CUDA_GRAIL, GRAIL_KERNELS)],
        #[cfg(feature = "hip")]
        Backend::Hip => vec![(HIP_FDG, FDG_KERNELS), (HIP_GRAIL, GRAIL_KERNELS)],
        #[allow(unreachable_patterns)]
        _ => vec![],
    };
    raw.into_iter()
        .map(|(img, k)| {
            // module loaders want 8-byte aligned images; include_bytes! gives no alignment guarantee
            let mut v: Vec<u64> = vec![0; img.len().div_ceil(8) + 1];
            let bytes = unsafe { std::slice::from_raw_parts_mut(v.as_mut_ptr() as *mut u8, img.len()) };
            bytes.copy_from_slice(img);
            let leaked: &'static [u64] = Box::leak(v.into_boxed_slice());
            let s: &'static [u8] = unsafe { std::slice::from_raw_parts(leaked.as_ptr() as *const u8, img.len()) };
            (s, k)
        })
        .collect()
}
