use std::collections::HashSet;
use std::io::{self, Read, Write};
use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}, mpsc};
use std::thread;

use colonist_catan_wasm::{
    NATIVE_GPU_PROTOCOL_VERSION, NATIVE_GPU_STATE_SCHEMA_VERSION, NativeGpuSearchEngine,
    engine_version,
};
use serde::Deserialize;
use serde_json::{Value, json};

const MAX_INBOUND_BYTES: usize = 64 * 1024 * 1024;
const MAX_OUTBOUND_BYTES: usize = 1024 * 1024;

fn native_build_identity() -> Value {
    json!({
        "gitSha": env!("COLONIST_NATIVE_HOST_GIT_SHA"),
        "dirty": env!("COLONIST_NATIVE_HOST_DIRTY") == "1",
        "builtAtUnixMs": env!("COLONIST_NATIVE_HOST_BUILT_AT_UNIX_MS")
            .parse::<u64>()
            .expect("build timestamp must be a u64"),
        "ptxSha256": env!("COLONIST_NATIVE_HOST_PTX_SHA256"),
    })
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
enum HostRequest {
    Hello {
        id: u64,
        #[serde(rename = "protocolVersion")]
        protocol_version: Option<u32>,
        #[serde(rename = "stateSchemaVersion")]
        state_schema_version: Option<u32>,
    },
    Analyze {
        id: u64,
        request: Value,
    },
    Cancel {
        id: u64,
    },
}

enum Inbound {
    Request(HostRequest),
    Invalid(String),
}

fn read_message(reader: &mut impl Read) -> io::Result<Option<Value>> {
    let mut length = [0u8; 4];
    match reader.read_exact(&mut length) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(error) => return Err(error),
    }
    let length = u32::from_ne_bytes(length) as usize;
    if length > MAX_INBOUND_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("native message exceeds {MAX_INBOUND_BYTES} bytes"),
        ));
    }
    let mut payload = vec![0u8; length];
    reader.read_exact(&mut payload)?;
    serde_json::from_slice(&payload)
        .map(Some)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn write_message(writer: &mut impl Write, value: &Value) -> io::Result<()> {
    let payload = serde_json::to_vec(value)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    if payload.len() > MAX_OUTBOUND_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("native response exceeds {MAX_OUTBOUND_BYTES} bytes"),
        ));
    }
    let length = u32::try_from(payload.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "native response too large"))?;
    writer.write_all(&length.to_ne_bytes())?;
    writer.write_all(&payload)?;
    writer.flush()
}

fn main() -> io::Result<()> {
    let cancelled = Arc::new(Mutex::new(HashSet::<u64>::new()));
    let shutdown = Arc::new(AtomicBool::new(false));
    let (sender, receiver) = mpsc::channel::<Inbound>();
    let reader_cancelled = Arc::clone(&cancelled);
    let reader_shutdown = Arc::clone(&shutdown);
    let reader = thread::spawn(move || -> io::Result<()> {
        let stdin = io::stdin();
        let mut stdin = stdin.lock();
        while let Some(value) = read_message(&mut stdin)? {
            match serde_json::from_value::<HostRequest>(value) {
                Ok(HostRequest::Cancel { id }) => {
                    reader_cancelled
                        .lock()
                        .map_err(|_| io::Error::other("native cancellation lock poisoned"))?
                        .insert(id);
                }
                Ok(request) => {
                    if sender.send(Inbound::Request(request)).is_err() {
                        break;
                    }
                }
                Err(error) => {
                    if sender
                        .send(Inbound::Invalid(format!("invalid native request: {error}")))
                        .is_err()
                    {
                        break;
                    }
                }
            }
        }
        reader_shutdown.store(true, Ordering::Release);
        Ok(())
    });

    let mut stdout = io::stdout().lock();
    let mut engine = NativeGpuSearchEngine::new().map_err(|error| {
        eprintln!("[Colonist Assistant GPU] initialization failed: {error}");
        error
    });

    while let Ok(inbound) = receiver.recv() {
        let response = match inbound {
            Inbound::Invalid(error) => json!({ "id": 0, "error": error }),
            Inbound::Request(HostRequest::Hello {
                id,
                protocol_version,
                state_schema_version,
            }) => {
                if protocol_version != Some(NATIVE_GPU_PROTOCOL_VERSION)
                    || state_schema_version != Some(NATIVE_GPU_STATE_SCHEMA_VERSION)
                {
                    json!({
                        "id": id,
                        "error": format!(
                            "GPU companion protocol mismatch: extension protocol/state {:?}/{:?}, host {}/{}",
                            protocol_version,
                            state_schema_version,
                            NATIVE_GPU_PROTOCOL_VERSION,
                            NATIVE_GPU_STATE_SCHEMA_VERSION,
                        )
                    })
                } else {
                    match engine.as_ref() {
                        Ok(engine) => json!({
                            "id": id,
                            "runtime": "gpu-native",
                            "protocolVersion": NATIVE_GPU_PROTOCOL_VERSION,
                            "stateSchemaVersion": NATIVE_GPU_STATE_SCHEMA_VERSION,
                            "engineRevision": engine_version(),
                            "build": native_build_identity(),
                            "device": engine.device_identity(),
                        }),
                        Err(error) => json!({ "id": id, "error": error }),
                    }
                }
            }
            Inbound::Request(HostRequest::Analyze { id, request }) => {
                let result = match engine.as_mut() {
                    Ok(engine) => engine.analyze_json_controlled(request, || {
                        if shutdown.load(Ordering::Acquire) {
                            return true;
                        }
                        cancelled
                            .lock()
                            .map_or(true, |ids| ids.contains(&id))
                    }),
                    Err(error) => Err(error.clone()),
                };
                if let Ok(mut ids) = cancelled.lock() {
                    ids.remove(&id);
                }
                match result {
                    Ok(response) => json!({ "id": id, "response": response }),
                    Err(error) => json!({ "id": id, "error": error }),
                }
            }
            Inbound::Request(HostRequest::Cancel { .. }) => unreachable!("cancel is consumed by the reader thread"),
        };
        write_message(&mut stdout, &response)?;
    }

    shutdown.store(true, Ordering::Release);
    match reader.join() {
        Ok(result) => result,
        Err(_) => Err(io::Error::other("native reader thread panicked")),
    }
}
