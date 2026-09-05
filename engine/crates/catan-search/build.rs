use std::{env, fs, path::PathBuf};
use sha2::{Digest, Sha256};

fn main() {
    if env::var_os("CARGO_FEATURE_CUDA_SIM").is_none() {
        return;
    }
    let directory = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("src/cuda");
    let mut digest = Sha256::new();
    for name in ["sim.cu", "rollout_cutoff.cuh"] {
        let path = directory.join(name);
        println!("cargo:rerun-if-changed={}", path.display());
        digest.update(name.as_bytes());
        digest.update([0]);
        digest.update(fs::read(&path).expect("CUDA source must be readable"));
    }
    digest.update(b"--gpu-architecture=compute_75\n--std=c++17\n--fmad=false");
    let expected = format!("// colonist-sim-source-sha256: {:x}\n", digest.finalize());
    let artifact = directory.join("sim.ptx");
    println!("cargo:rerun-if-changed={}", artifact.display());
    let ptx = fs::read_to_string(artifact).expect("embedded CUDA PTX must be readable");
    assert!(
        ptx.starts_with(&expected),
        "CUDA source and embedded PTX differ. Run python3 scripts/build-cuda-ptx.py from the repository root before rebuilding."
    );
}
