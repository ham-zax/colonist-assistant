use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

fn git(cwd: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn git_path(repo_root: &Path, name: &str) -> Option<PathBuf> {
    let path = PathBuf::from(git(repo_root, &["rev-parse", "--git-path", name])?);
    Some(if path.is_absolute() {
        path
    } else {
        repo_root.join(path)
    })
}

fn main() {
    let manifest_dir = PathBuf::from(
        env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is set by Cargo"),
    );
    let repo_root = PathBuf::from(
        git(&manifest_dir, &["rev-parse", "--show-toplevel"])
            .expect("native host build must run from a Git checkout"),
    );
    let ptx_path = repo_root.join("engine/crates/catan-search/src/cuda/sim.ptx");

    for relative in [
        "engine/Cargo.toml",
        "engine/Cargo.lock",
        "engine/crates/catan-core",
        "engine/crates/catan-search",
        "engine/crates/catan-wasm",
        "engine/crates/catan-native-host",
    ] {
        println!(
            "cargo:rerun-if-changed={}",
            repo_root.join(relative).display()
        );
    }
    for name in ["HEAD", "index"] {
        let path = git_path(&repo_root, name).expect("native host build must resolve Git metadata");
        println!("cargo:rerun-if-changed={}", path.display());
    }

    let sha = git(&repo_root, &["rev-parse", "HEAD"])
        .expect("native host build must resolve the Git revision");
    let dirty = !git(&repo_root, &["status", "--porcelain"])
        .expect("native host build must inspect Git status")
        .is_empty();
    let built_at_unix_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock must be after Unix epoch")
        .as_millis();
    let ptx = fs::read(&ptx_path).expect("native GPU PTX must exist at build time");
    let ptx_sha256 = format!("{:x}", Sha256::digest(ptx));

    println!("cargo:rustc-env=COLONIST_NATIVE_HOST_GIT_SHA={sha}");
    println!(
        "cargo:rustc-env=COLONIST_NATIVE_HOST_DIRTY={}",
        u8::from(dirty)
    );
    println!("cargo:rustc-env=COLONIST_NATIVE_HOST_BUILT_AT_UNIX_MS={built_at_unix_ms}");
    println!("cargo:rustc-env=COLONIST_NATIVE_HOST_PTX_SHA256={ptx_sha256}");
}
