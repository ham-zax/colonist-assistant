use std::io::{self, Read, Write};

use colonist_catan_wasm::{
    NATIVE_GPU_PROTOCOL_VERSION, NATIVE_GPU_STATE_SCHEMA_VERSION, NativeGpuSearchEngine,
    engine_version,
};
use serde::Deserialize;
use serde_json::{Value, json};

const MAX_INBOUND_BYTES: usize = 64 * 1024 * 1024;
const MAX_OUTBOUND_BYTES: usize = 1024 * 1024;

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
    let mut stdin = io::stdin().lock();
    let mut stdout = io::stdout().lock();
    let mut engine = NativeGpuSearchEngine::new().map_err(|error| {
        eprintln!("[Colonist Assistant GPU] initialization failed: {error}");
        error
    });

    while let Some(value) = read_message(&mut stdin)? {
        let request = match serde_json::from_value::<HostRequest>(value) {
            Ok(request) => request,
            Err(error) => {
                write_message(
                    &mut stdout,
                    &json!({ "id": 0, "error": format!("invalid native request: {error}") }),
                )?;
                continue;
            }
        };
        let response = match request {
            HostRequest::Hello {
                id,
                protocol_version,
                state_schema_version,
            } => {
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
                            "device": engine.device_identity(),
                        }),
                        Err(error) => json!({ "id": id, "error": error }),
                    }
                }
            }
            HostRequest::Analyze { id, request } => match engine.as_mut() {
                Ok(engine) => match engine.analyze_json(request) {
                    Ok(response) => json!({ "id": id, "response": response }),
                    Err(error) => json!({ "id": id, "error": error }),
                },
                Err(error) => json!({ "id": id, "error": error }),
            },
        };
        write_message(&mut stdout, &response)?;
    }
    Ok(())
}
