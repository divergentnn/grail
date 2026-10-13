//! End-to-end check against the Python reference (needs the lab's media, a GPU, libvship and the av1-fullgrain dav1d
//! build): GRAIL_PORT_REFERENCE=/path/to/port_reference.json GRAIL_FULLGRAIN_LIB=DIR cargo test --release -- --ignored.
//! Runs `grail check-reference` on every case of the file; the test passes only if every case ran and every stage,
//! invariant and the Spearman gate held (check-reference exits non-zero otherwise). Ignored by default: run without
//! GRAIL_PORT_REFERENCE it fails instead of passing silently.
#[test]
#[ignore]
fn port_reference_all_cases() {
    let file = std::env::var("GRAIL_PORT_REFERENCE")
        .expect("GRAIL_PORT_REFERENCE must name port_reference.json (this test is only run on request)");
    assert!(std::env::var_os("GRAIL_FULLGRAIN_LIB").is_some(), "GRAIL_FULLGRAIN_LIB must name the av1-fullgrain build");
    let d: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&file).expect("reading the case file"))
        .expect("parsing the case file");
    let n = d["cases"].as_array().expect("cases").len();
    assert!(n > 0, "no cases in {file}");
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_grail"))
        .args(["check-reference", &file, "--threads", "2"])
        .output()
        .expect("running grail");
    let text = String::from_utf8_lossy(&out.stdout);
    eprintln!("{text}{}", String::from_utf8_lossy(&out.stderr));
    assert!(out.status.success(), "check-reference failed (exit status {})", out.status);
    assert!(text.contains(&format!("all {n} cases ran; all stages within tolerance")),
            "check-reference did not report all {n} cases within tolerance");
}
