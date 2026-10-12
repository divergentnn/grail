//! End-to-end check against the Python reference (needs the lab's media, a CUDA GPU, libvship and the av1-fullgrain
//! dav1d build): GRAIL_PORT_REFERENCE=/path/to/port_reference.json GRAIL_FULLGRAIN_LIB=DIR cargo test --release
//! -- --ignored. Runs `grail check-reference` on all 13 cases; every stage must be within its tolerance.
#[test]
#[ignore]
fn port_reference_13_cases() {
    let Ok(file) = std::env::var("GRAIL_PORT_REFERENCE") else {
        eprintln!("GRAIL_PORT_REFERENCE not set: skipped");
        return;
    };
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_grail"))
        .args(["check-reference", &file, "--threads", "2"])
        .output()
        .expect("running grail");
    let text = String::from_utf8_lossy(&out.stdout);
    eprintln!("{text}{}", String::from_utf8_lossy(&out.stderr));
    assert!(out.status.success());
    assert!(text.contains("all stages within tolerance"), "some stage over its tolerance");
}
