#!/usr/bin/env python3
"""Compile the resident simulator and stamp the exact CUDA sources into its PTX.

No network or toolkit headers are required. NVRTC_LIBRARY may name libnvrtc.
Use --check in CI/build checks to reject a stale embedded kernel artifact.
"""
from __future__ import annotations

import argparse
import ctypes as C
import ctypes.util
import hashlib
import os
from pathlib import Path
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
CUDA = ROOT / "engine/crates/catan-search/src/cuda"
SOURCES = ("sim.cu", "rollout_cutoff.cuh", "mref.cuh")
OPTIONS = ("--gpu-architecture=compute_75", "--std=c++17", "--fmad=false", "--device-int128")
MARKER = "// colonist-sim-source-sha256: "


def digest() -> str:
    result = hashlib.sha256()
    for name in SOURCES:
        result.update(name.encode() + b"\0" + (CUDA / name).read_bytes())
    result.update("\n".join(OPTIONS).encode())
    return result.hexdigest()


def library() -> C.CDLL:
    candidates = [os.environ.get("NVRTC_LIBRARY"), ctypes.util.find_library("nvrtc")]
    for root in (Path("/usr/local/cuda/lib64"), Path.home() / ".cache/uv/archive-v0"):
        if root.exists():
            pattern = "libnvrtc.so*" if root.name == "lib64" else "**/libnvrtc.so*"
            candidates.extend(str(path) for path in sorted(root.glob(pattern)) if path.is_file())
    errors = []
    for candidate in dict.fromkeys(item for item in candidates if item):
        try:
            return C.CDLL(candidate)
        except OSError as error:
            errors.append(str(error))
    raise RuntimeError("NVRTC was not found. Set NVRTC_LIBRARY to its shared library. " + "; ".join(errors[:2]))


def compile_ptx() -> bytes:
    lib = library()
    specifications = {
        "nvrtcCreateProgram": [C.POINTER(C.c_void_p), C.c_char_p, C.c_char_p, C.c_int, C.POINTER(C.c_char_p), C.POINTER(C.c_char_p)],
        "nvrtcCompileProgram": [C.c_void_p, C.c_int, C.POINTER(C.c_char_p)],
        "nvrtcGetProgramLogSize": [C.c_void_p, C.POINTER(C.c_size_t)],
        "nvrtcGetProgramLog": [C.c_void_p, C.c_char_p],
        "nvrtcGetPTXSize": [C.c_void_p, C.POINTER(C.c_size_t)],
        "nvrtcGetPTX": [C.c_void_p, C.c_char_p],
        "nvrtcDestroyProgram": [C.POINTER(C.c_void_p)],
    }
    for name, argtypes in specifications.items():
        function = getattr(lib, name)
        function.argtypes = argtypes
        function.restype = C.c_int

    def checked(code: int, operation: str) -> None:
        if code != 0:
            raise RuntimeError(f"{operation} failed with NVRTC status {code}")

    program = C.c_void_p()
    header_count = len(SOURCES) - 1
    headers = (C.c_char_p * header_count)(*((CUDA / name).read_bytes() for name in SOURCES[1:]))
    names = (C.c_char_p * header_count)(*(name.encode() for name in SOURCES[1:]))
    checked(lib.nvrtcCreateProgram(C.byref(program), (CUDA / SOURCES[0]).read_bytes(), b"sim.cu", header_count, headers, names), "create")
    try:
        options = (C.c_char_p * len(OPTIONS))(*(option.encode() for option in OPTIONS))
        code = lib.nvrtcCompileProgram(program, len(options), options)
        size = C.c_size_t()
        checked(lib.nvrtcGetProgramLogSize(program, C.byref(size)), "log size")
        log = C.create_string_buffer(size.value)
        checked(lib.nvrtcGetProgramLog(program, log), "log")
        if log.value:
            print(log.value.decode(errors="replace"), file=sys.stderr)
        checked(code, "compile")
        checked(lib.nvrtcGetPTXSize(program, C.byref(size)), "PTX size")
        ptx = C.create_string_buffer(size.value)
        checked(lib.nvrtcGetPTX(program, ptx), "PTX")
        return ptx.value
    finally:
        lib.nvrtcDestroyProgram(C.byref(program))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    target = CUDA / "sim.ptx"
    fingerprint = digest()
    marker = (MARKER + fingerprint + "\n").encode()
    if args.check:
        if not target.exists() or not target.read_bytes().startswith(marker):
            print("Stale CUDA artifact: run python3 scripts/build-cuda-ptx.py", file=sys.stderr)
            return 1
        print(f"CUDA source/artifact stamp matches: {fingerprint}")
        return 0
    ptx = compile_ptx()
    # A concurrent edit must not stamp the binary with a newer source digest.
    if digest() != fingerprint:
        raise RuntimeError("CUDA source changed during compilation; output was not installed")
    with tempfile.NamedTemporaryFile(dir=CUDA, prefix=".sim-", suffix=".ptx", delete=False) as file:
        temporary = Path(file.name)
        file.write(marker + ptx)
    try:
        os.replace(temporary, target)
    finally:
        temporary.unlink(missing_ok=True)
    print(f"Built {target} from {fingerprint}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError) as error:
        print(error, file=sys.stderr)
        raise SystemExit(1)
