use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-env-changed=CODEX_FENCE_ROOT_HINT");

    let hint = env::var("CODEX_FENCE_ROOT_HINT")
        .ok()
        .or_else(|| env::var("CARGO_MANIFEST_DIR").ok());

    if let Some(raw_hint) = hint {
        let candidate = PathBuf::from(raw_hint);
        let canonical = candidate.canonicalize().unwrap_or(candidate);

        println!(
            "cargo:rustc-env=CODEX_FENCE_ROOT_HINT={}",
            canonical.display()
        );
    }
}
