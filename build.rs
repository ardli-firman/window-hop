use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    println!("cargo:rerun-if-env-changed=WINDOW_HOP_PAYLOAD_PATH");

    let out_dir = env::var("OUT_DIR").expect("OUT_DIR must be set by Cargo");
    let dest_path = Path::new(&out_dir).join("embedded_payload.rs");

    // Check if an explicit payload path is provided via environment variable
    let payload_path = env::var("WINDOW_HOP_PAYLOAD_PATH")
        .ok()
        .map(PathBuf::from)
        .filter(|p| p.exists() && p.is_file());

    if let Some(path) = payload_path {
        println!("cargo:rerun-if-changed={}", path.display());
        let canonical = fs::canonicalize(&path).unwrap_or(path);
        let content = format!(
            "pub const EMBEDDED_PAYLOAD: Option<&'static [u8]> = Some(include_bytes!(r\"{}\"));\n",
            canonical.display()
        );
        fs::write(&dest_path, content).expect("Failed to write embedded_payload.rs");
    } else {
        // Fallback when no payload is provided (e.g. dev build, unit test, or unbundled build)
        fs::write(
            &dest_path,
            "pub const EMBEDDED_PAYLOAD: Option<&'static [u8]> = None;\n",
        )
        .expect("Failed to write embedded_payload.rs");
    }
}
