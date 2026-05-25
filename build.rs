use std::env;
use std::path::PathBuf;

fn main() {
    let capture_dir = PathBuf::from("capture");
    println!("cargo:rerun-if-changed=build.rs");

    // -------------------------------------------------------------------------
    // Compile C sources into a static library
    // -------------------------------------------------------------------------
    cc::Build::new()
        .std("gnu11")
        .flag("-Wall")
        .flag("-Wextra")
        .flag("-O2")
        .define("DEBUG", None)
        .define("_GNU_SOURCE", None)
        .file(capture_dir.join("capture.c"))
        .file(capture_dir.join("chanhop.c"))
        .compile("capture");

    // -------------------------------------------------------------------------
    // Link pthread (needed by chanhop thread in consumer code)
    // -------------------------------------------------------------------------
    println!("cargo:rustc-link-lib=pthread");

    // -------------------------------------------------------------------------
    // Re-run build if C sources or headers change
    // -------------------------------------------------------------------------
    println!("cargo:rerun-if-changed=capture/capture.c");
    println!("cargo:rerun-if-changed=capture/capture.h");
    println!("cargo:rerun-if-changed=capture/chanhop.c");
    println!("cargo:rerun-if-changed=capture/chanhop.h");

    // -------------------------------------------------------------------------
    // Run bindgen over both headers
    // -------------------------------------------------------------------------
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    bindgen::Builder::default()
        .header(capture_dir.join("capture.h").to_str().unwrap())
        .header(capture_dir.join("chanhop.h").to_str().unwrap())
        // only generate bindings for our own types, not kernel headers
        .allowlist_type("frame_info_t")
        .allowlist_type("capture_config_t")
        .allowlist_type("frame_callback_t")
        .allowlist_type("chanhop_config_t")
        .allowlist_type("chanhop_band_t")
        .allowlist_var("g_capture_stop")
        .allowlist_var("g_chanhop_stop")
        .allowlist_function("capture_start")
        .allowlist_function("capture_stop")
        .allowlist_function("chanhop_start")
        .allowlist_function("chanhop_stop")
        .use_core()
        .generate()
        .expect("bindgen failed")
        .write_to_file(out_dir.join("bindings.rs"))
        .expect("failed to write bindings");
    }
