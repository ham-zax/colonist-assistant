use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn main() {
    println!("cargo:rerun-if-changed=../../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../../.git/index");
    let sha = git(&["rev-parse", "HEAD"]).unwrap_or_else(|| "unknown".to_string());
    let dirty = git(&["status", "--porcelain"]).is_some_and(|value| !value.is_empty());
    println!("cargo:rustc-env=COLONIST_BUILD_GIT_SHA={sha}");
    println!("cargo:rustc-env=COLONIST_BUILD_DIRTY={}", u8::from(dirty));
}
