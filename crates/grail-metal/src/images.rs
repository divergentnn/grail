//! Kernels of the two Metal libraries (compiled from kernels/metal/*.metal when the device is opened).
pub const FDG_KERNELS: &[&str] = &[
    "sep3_0", "sep3_1", "sep5_0", "sep5_1", "sep7_0", "sep7_1", "sep8_0", "sep8_1", "sep19_0", "sep19_1",
    "sep37_0", "sep37_1", "sep", "down", "raster", "dense", "warp_y", "weight_only", "reduce_frames", "chroma",
];
pub const GRAIL_KERNELS: &[&str] = &[
    "copy_u32", "copy_u8", "fill_u8", "u16_to_f32", "u16_to_f32_q", "cut_window", "ew_absdiff", "ew_energy", "ew_sub",
    "ew_sqr", "ew_abs", "ew_div_bc", "ew_q8", "ew_rint", "ew_guide", "ew_alpha", "ew_blend", "ew_finish", "ew_pack",
    "gather_stride", "sel_init", "sel_hist", "sel_scan", "sel_value", "noise_var", "quantile_finish", "corr_f", "corr_d", "corr_x_f", "corr_x_d", "corr_y_f", "corr_y_d",
    "minmax1d", "local_sd", "count_lt", "flat_mask", "flat_threshold", "feat_accum", "tex_tiles", "chroma_accum",
    "lineart_cells", "edge_mask", "edt_col", "ring_near", "sum_above",
];
