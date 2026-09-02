# WSL GPU Development Bridge

For normal Windows Chrome + WSL GPU development, register the Native Messaging host once and keep the strategy engine in WSL.

```text
Chrome extension
  -> io.colonist_assistant.gpu
  -> stable Windows bridge
  -> wsl.exe --exec
  -> current WSL colonist-assistant-gpu
  -> CUDA / embedded sim.ptx
```

The bridge is transport-only. It does not parse the Native Messaging protocol, contain strategy/search logic, embed PTX, or replace the Linux host build identity.

## One-time setup

First build the Linux companion at the stable path you want Chrome to use:

```bash
~/.cargo/bin/cargo build \
  --manifest-path engine/Cargo.toml \
  --release \
  -p colonist-catan-native-host
```

Then run `scripts/install-gpu-wsl-bridge.ps1` in Windows PowerShell with:

- the exact unpacked Chrome extension ID;
- the trusted WSL distribution name;
- the absolute Linux path to `engine/target/release/colonist-assistant-gpu` in the stable checkout.

Example:

```powershell
& "\\wsl.localhost\Ubuntu-26.04\home\hamza\repo\colonist-assistant\scripts\install-gpu-wsl-bridge.ps1" `
  -ExtensionId cmigiicdpipphbcnebgaieeahfhlnnmb `
  -WslDistro Ubuntu-26.04 `
  -LinuxHostPath /home/hamza/repo/colonist-assistant/engine/target/release/colonist-assistant-gpu
```

The installer writes these stable Windows-side artifacts under `%LOCALAPPDATA%\ColonistAssistant`:

- `colonist-assistant-gpu-wsl-bridge.exe`;
- `gpu-wsl-bridge.conf`, containing only the trusted WSL distribution and Linux companion path;
- `io.colonist_assistant.gpu.json` with the exact `allowed_origins` extension origin.

It also writes the existing Chrome Native Messaging registry key:

```text
HKCU\Software\Google\Chrome\NativeMessagingHosts\io.colonist_assistant.gpu
```

The extension cannot choose a WSL distribution, executable path, shell command, or PTX payload. Those values come only from the local sidecar configuration written during setup.

## Normal GPU iteration

After setup, strategy/search changes stay entirely in WSL:

```text
edit Rust/CUDA
-> regenerate sim.ptx when sim.cu changes
-> cargo build --release -p colonist-catan-native-host
-> reload/reconnect the extension if needed
```

Do not rerun the Windows bridge installer for an ordinary Linux host rebuild. The bridge executes the configured Linux path each time Chrome opens the native host, so the next connection uses the newly built binary at that path.

The `hello` response still comes from the Linux `colonist-assistant-gpu`. Its existing runtime, protocol/state versions, engine revision, CUDA device, Git SHA/dirty state, build timestamp, and embedded PTX SHA-256 remain authoritative.

## Extension rebuilds and ID stability

No development-only manifest key is added. Rebuilding the contents of the same unpacked `dist` path does not require Native Messaging re-registration. If you deliberately load the extension from a different unpacked path and Chrome assigns a different ID, rerun the one-time bridge installer with that exact ID so `allowed_origins` remains exact.

## Existing native-Windows installer

`scripts/install-gpu-host.ps1` remains the native-Windows build/copy/install path. It is useful as a fallback when the strategy engine itself is intended to run as a Windows executable.

For WSL-first development, use `install-gpu-wsl-bridge.ps1`; it intentionally does not build or copy the strategy engine into Windows.
