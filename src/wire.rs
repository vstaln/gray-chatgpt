//! Reply frames for gray's sidecar wire.
//!
//! Gray routes a reply by `id` and keeps only its `result` (a top-level
//! JSON-RPC `error` is dropped); a provider failure is read from
//! `result.error` and must carry the full failure shape gray parses
//! (`code`, `message`, `retryable`, `terminal`).

use gray_plugin::{ProviderAuthPoll, ProviderRpcError};
use serde_json::{Value, json};

pub fn reply(id: Value, outcome: Result<Value, ProviderRpcError>) -> Value {
    let result = match outcome {
        Ok(result) => result,
        Err(error) => json!({"error": error_value(error)}),
    };
    json!({"id": id, "result": result})
}

pub fn error_value(error: ProviderRpcError) -> Value {
    let (code, message, retryable, terminal) = match error {
        ProviderRpcError::Rpc(f) => (f.code, f.message, f.retryable, f.terminal),
        ProviderRpcError::Protocol(m) => ("protocol".to_string(), m, false, true),
        ProviderRpcError::Unavailable(m) => ("unavailable".to_string(), m, true, false),
        ProviderRpcError::CapabilityMissing(m) => {
            ("capability_missing".to_string(), m, false, true)
        }
    };
    json!({"code": code, "message": message, "retryable": retryable, "terminal": terminal})
}

pub fn poll_value(poll: ProviderAuthPoll) -> Value {
    match poll {
        ProviderAuthPoll::Pending { retry_after_ms } => {
            json!({"state": "pending", "retry_after_ms": retry_after_ms})
        }
        ProviderAuthPoll::Completed(credential) => {
            json!({"state": "completed", "credential": credential})
        }
        ProviderAuthPoll::Failed(failure) => json!({"state": "failed", "error": failure}),
        ProviderAuthPoll::Cancelled => json!({"state": "cancelled"}),
        ProviderAuthPoll::OperationLost => json!({"state": "operation_lost"}),
    }
}

#[path = "wire_tests.rs"]
#[cfg(test)]
mod tests;
