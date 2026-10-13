//! GRAIL stages on Apple GPUs through Metal: the flowdegrain2 measurement denoiser, the grain features and the
//! line-art path. The kernels are the CUDA kernels of kernels/ ported to Metal Shading Language (kernels/metal/),
//! compiled from source when the device is opened; the host-side orchestration mirrors crates/grail-gpu step for step.
#[cfg(target_os = "macos")]
pub mod backend;
#[cfg(target_os = "macos")]
pub mod buf;
#[cfg(target_os = "macos")]
pub mod denoise;
#[cfg(target_os = "macos")]
pub mod driver;
#[cfg(target_os = "macos")]
pub mod features;
pub mod images;
#[cfg(target_os = "macos")]
pub mod lineart;
#[cfg(target_os = "macos")]
pub mod ops;

#[cfg(target_os = "macos")]
pub use backend::MetalBackend;
#[cfg(target_os = "macos")]
pub use buf::DBuf;
#[cfg(target_os = "macos")]
pub use driver::try_init;
