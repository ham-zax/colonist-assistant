# Windows Colonist GPU Runtime

Windows Chrome and Edge use one permanent Native Messaging runtime while the current strategy engine remains in WSL.

```text
Chrome / Edge extension
  -> io.colonist_assistant.gpu
  -> Colonist GPU Runtime (Windows)
  -> wsl.exe --exec
  -> current WSL colonist-assistant-gpu
  -> CUDA / embedded sim.ptx
```

The Windows runtime is a stable browser-to-GPU broker. It contains no Catan strategy, search policy, evaluation logic, engine revision, CUDA policy, or PTX. It forwards Native Messaging stdin/stdout bytes without parsing or rewriting the protocol and starts the locally configured WSL companion for each browser connection.

## One-time setup

First build the Linux companion at the stable path the runtime should execute:

```bash
~/.cargo/bin/cargo build \
  --manifest-path engine/Cargo.toml \
  --release \
  -p colonist-catan-native-host
```

Then run `scripts/install-gpu-runtime.ps1` in Windows PowerShell with:

- every exact Chrome/Edge extension ID that should be authorized;
- the trusted WSL distribution name;
- the absolute Linux path to `engine/target/release/colonist-assistant-gpu` in the stable checkout.

If Chrome and Edge use different extension IDs, pass both. If they use the same ID, pass it once.

```powershell
& "\\wsl.localhost\Ubuntu-26.04\home\hamza\repo\colonist-assistant\scripts\install-gpu-runtime.ps1" `
  -ExtensionIds cmigiicdpipphbcnebgaieeahfhlnnmb `
  -WslDistro Ubuntu-26.04 `
  -LinuxHostPath /home/hamza/repo/colonist-assistant/engine/target/release/colonist-assistant-gpu
```

The installer writes these stable Windows-side artifacts under `%LOCALAPPDATA%\ColonistAssistant`:

- `colonist-gpu-runtime.exe`;
- `gpu-runtime.conf`, containing only the trusted WSL distribution and Linux companion path;
- `io.colonist_assistant.gpu.json`, whose `allowed_origins` contains only the exact supplied extension IDs.

The same manifest is registered for both browsers:

```text
HKCU\Software\Google\Chrome\NativeMessagingHosts\io.colonist_assistant.gpu
HKCU\Software\Microsoft\Edge\NativeMessagingHosts\io.colonist_assistant.gpu
```

The extension cannot choose a WSL distribution, executable path, shell command, PTX payload, or native code. Those values come only from the local runtime configuration written during setup. If the configured WSL distribution or binary is missing, the runtime fails rather than falling back to another strategy executable.

## Normal GPU iteration

After setup, strategy/search changes stay entirely in WSL:

```text
edit Rust/CUDA
-> regenerate sim.ptx when sim.cu changes
-> cargo build --release -p colonist-catan-native-host
-> reconnect/reload the extension if needed
```

Do not rerun the Windows runtime installer for an ordinary Linux host rebuild. Each new Native Messaging connection executes the configured Linux path, so the next connection uses the newly built binary at that path.

The `hello` response still comes from the Linux `colonist-assistant-gpu`. Its existing runtime, protocol/state versions, engine revision, CUDA device, Git SHA/dirty state, build timestamp, and embedded PTX SHA-256 remain authoritative. The Windows runtime has no strategy-engine identity in that protocol.

## Extension rebuilds and exact origins

No development-only manifest key is added. Rebuilding the contents of the same unpacked `dist` path does not require Native Messaging re-registration. If Chrome or Edge is deliberately loaded from a path that produces a different extension ID, rerun the one-time runtime installer with the complete exact ID set. Wildcard origins are not used.

## Windows strategy ownership

The former `scripts/install-gpu-host.ps1` Windows-native strategy installation path is retired. The repository had no demonstrated release or fallback consumer for maintaining a second Windows copy of the Catan/CUDA strategy engine. On Windows, the permanent Colonist GPU Runtime is the browser-facing component and WSL is the trusted strategy/GPU backend.
