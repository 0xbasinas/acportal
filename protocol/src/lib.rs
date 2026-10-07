//! Stable ACP v1 types and a lossless JSON-RPC boundary.
//! Unknown ACP payload fields remain in `Value` when forwarded or journaled.
pub use agent_client_protocol_schema::{ProtocolVersion, v1 as acp};
use anyhow::{Result, bail};
use serde_json::{Value, json};

pub const ACP_VERSION: u16 = 1;
pub const DEFAULT_MAX_FRAME_BYTES: usize = 1024 * 1024;

pub fn validate_message(value: &Value) -> Result<()> {
    let Some(object) = value.as_object() else {
        bail!("JSON-RPC message must be an object")
    };
    if object.get("jsonrpc") != Some(&json!("2.0")) {
        bail!("expected jsonrpc 2.0")
    }
    if let Some(id) = object.get("id")
        && !(id.is_string() || id.is_i64() || id.is_u64())
    {
        bail!("id must be a string or integer")
    }
    if let Some(method) = object.get("method") {
        if method.as_str().is_none_or(str::is_empty) {
            bail!("method must be a nonempty string")
        }
        if object.contains_key("result") || object.contains_key("error") {
            bail!("request contains response fields")
        }
        if let Some(params) = object.get("params")
            && !params.is_object()
            && !params.is_array()
        {
            bail!("params must be structured")
        }
    } else {
        if !object.contains_key("id") {
            bail!("response is missing id")
        }
        if object.contains_key("result") == object.contains_key("error") {
            bail!("response needs exactly one of result/error")
        }
        if let Some(error) = object.get("error")
            && (error.get("code").and_then(Value::as_i64).is_none()
                || error.get("message").and_then(Value::as_str).is_none())
        {
            bail!("malformed error response")
        }
    }
    Ok(())
}

pub fn request(id: u64, method: &str, params: Value) -> Value {
    json!({"jsonrpc":"2.0", "id":id, "method":method, "params":params})
}
pub fn response(id: Value, result: Value) -> Value {
    json!({"jsonrpc":"2.0", "id":id, "result":result})
}
pub fn error(id: Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc":"2.0", "id":id, "error":{"code":code,"message":message}})
}
pub fn initialize_params() -> Value {
    // No host FS or terminal execution is advertised until those services exist.
    json!({"protocolVersion":ACP_VERSION, "clientCapabilities":{},
        "clientInfo":{"name":"acpd","title":"ACP Portal","version":env!("CARGO_PKG_VERSION")}})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_ambiguous_or_invalid_envelopes() {
        for value in [
            json!([]),
            json!({"jsonrpc":"1.0","id":1,"result":{}}),
            json!({"jsonrpc":"2.0","id":true,"result":{}}),
            json!({"jsonrpc":"2.0","id":1,"result":{},"error":{}}),
            json!({"jsonrpc":"2.0","method":"x","result":{}}),
            json!({"jsonrpc":"2.0","id":1,"error":{"code":"oops"}}),
        ] {
            assert!(validate_message(&value).is_err(), "{value}");
        }
    }
    #[test]
    fn preserves_future_payloads_and_string_ids() {
        let value = json!({"jsonrpc":"2.0","id":"agent-1","method":"_future/x",
            "params":{"extra":{"unknown":[1,2,3]}},"_meta":{"vendor":true}});
        validate_message(&value).unwrap();
        assert_eq!(
            value,
            serde_json::from_str::<Value>(&value.to_string()).unwrap()
        );
    }
    #[test]
    fn initialization_matches_official_v1_schema() {
        let parsed: acp::InitializeRequest = serde_json::from_value(initialize_params()).unwrap();
        assert_eq!(parsed.protocol_version, ProtocolVersion::V1);
    }
}
