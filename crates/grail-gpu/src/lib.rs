//! GRAIL GPU stages. The kernels are plain CUDA C++ (kernels/ at the repository root), compiled by build.rs with
//! nvcc (or hipcc for the HIP build), embedded in the binary and launched through the driver API loaded at run time.
#![cfg(not(target_os = "macos"))]
// (on macOS the crate is empty: no CUDA / HIP there; grail uses grail-metal)
pub mod backend;
pub mod buf;
pub mod denoise;
pub mod driver;
pub mod features;
pub mod images;
pub mod lineart;
pub mod ops;

pub use buf::DBuf;
pub use driver::{gpu, try_init, Backend};
pub use backend::GpuBackend;
