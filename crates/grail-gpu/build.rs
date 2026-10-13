//! Builds the GRAIL kernels (kernels/*.cu at the repository root) into device images that the crate embeds:
//!   cuda feature: nvcc -fatbin (CUDA 12+; archs from GRAIL_CUDA_ARCHS, default 75;80;86;89;90;120 + PTX)
//!   hip feature:  hipcc --genco (ROCm; archs from GRAIL_HIP_ARCHS) -- compile-checked only in this beta
//! flowdegrain2.cu is compiled with fast math (as the Python reference's CuPy RawModule), grail.cu without.
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

fn find_tool(var: &str, sub: &str, fallbacks: &[&str], name: &str) -> PathBuf {
    if let Ok(p) = env::var(var) {
        return PathBuf::from(p);
    }
    for root in ["CUDA_PATH", "CUDA_HOME", "ROCM_PATH", "HIP_PATH"] {
        if let Ok(r) = env::var(root) {
            let p = Path::new(&r).join(sub);
            if p.exists() {
                return p;
            }
        }
    }
    for f in fallbacks {
        if Path::new(f).exists() {
            return PathBuf::from(f);
        }
    }
    PathBuf::from(name)
}

fn run(mut cmd: Command, what: &str) {
    eprintln!("grail-gpu build: {:?}", cmd);
    let st = cmd.status().unwrap_or_else(|e| panic!("{what}: cannot run {:?}: {e}", cmd.get_program()));
    if !st.success() {
        panic!("{what} failed ({st})");
    }
}

fn main() {
    // macOS has no CUDA / HIP: the crate is empty there (grail uses grail-metal)
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        return;
    }
    let man = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let kdir = man.join("../../kernels").canonicalize().expect("kernels/ directory");
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    for f in ["flowdegrain2.cu", "grail.cu", "portable.h"] {
        println!("cargo:rerun-if-changed={}", kdir.join(f).display());
    }
    for v in ["GRAIL_CUDA_ARCHS", "GRAIL_HIP_ARCHS", "NVCC", "HIPCC", "CUDA_PATH", "ROCM_PATH"] {
        println!("cargo:rerun-if-env-changed={v}");
    }
    // (file, fast math)
    let units = [("flowdegrain2", true), ("grail", false)];

    if env::var("CARGO_FEATURE_CUDA").is_ok() {
        let nvcc = find_tool("NVCC", "bin/nvcc", &["/opt/cuda/bin/nvcc", "/usr/local/cuda/bin/nvcc"], "nvcc");
        let archs = env::var("GRAIL_CUDA_ARCHS").unwrap_or_else(|_| "75;80;86;89;90;120".into());
        let archs: Vec<&str> = archs.split(';').map(str::trim).filter(|s| !s.is_empty()).collect();
        for (unit, fast) in units {
            let mut c = Command::new(&nvcc);
            c.arg("-fatbin").arg("-O3").arg("-std=c++17").arg("-I").arg(&kdir);
            if fast {
                c.arg("--use_fast_math");
            }
            for a in &archs {
                c.arg(format!("-gencode=arch=compute_{a},code=sm_{a}"));
            }
            if let Some(last) = archs.last() {
                c.arg(format!("-gencode=arch=compute_{last},code=compute_{last}"));   // PTX for newer GPUs
            }
            c.arg("-o").arg(out.join(format!("{unit}.fatbin"))).arg(kdir.join(format!("{unit}.cu")));
            run(c, "nvcc");
        }
    }
    if env::var("CARGO_FEATURE_HIP").is_ok() {
        let hipcc = find_tool("HIPCC", "bin/hipcc", &["/opt/rocm/bin/hipcc"], "hipcc");
        let archs = env::var("GRAIL_HIP_ARCHS")
            .unwrap_or_else(|_| "gfx906,gfx908,gfx90a,gfx1030,gfx1100,gfx1101,gfx1102,gfx1200,gfx1201".into());
        for (unit, fast) in units {
            let mut c = Command::new(&hipcc);
            c.arg("--genco").arg("-O3").arg("-std=c++17").arg("-x").arg("hip").arg("-I").arg(&kdir);
            if fast {
                c.arg("-ffast-math");
            }
            for a in archs.split(',') {
                c.arg(format!("--offload-arch={}", a.trim()));
            }
            c.arg("-o").arg(out.join(format!("{unit}.hsaco"))).arg(kdir.join(format!("{unit}.cu")));
            run(c, "hipcc");
        }
    }
}
