use super::*;
use gray_plugin::ProviderRpcFailure;

#[test]
fn errors_ride_inside_result_with_the_full_failure_shape() {
    let frame = reply(
        json!(7),
        Err(ProviderRpcError::Rpc(ProviderRpcFailure {
            code: "invalid_grant".into(),
            message: "sign in again".into(),
            retryable: false,
            terminal: true,
        })),
    );
    assert_eq!(frame["id"], 7);
    assert!(frame.get("error").is_none(), "{frame}");
    let failure: ProviderRpcFailure =
        serde_json::from_value(frame["result"]["error"].clone()).expect("host-parseable failure");
    assert_eq!(failure.code, "invalid_grant");
    assert!(failure.terminal);
}

#[test]
fn every_error_kind_is_host_parseable() {
    for error in [
        ProviderRpcError::Protocol("p".into()),
        ProviderRpcError::Unavailable("u".into()),
        ProviderRpcError::CapabilityMissing("c".into()),
    ] {
        let value = error_value(error);
        serde_json::from_value::<ProviderRpcFailure>(value.clone())
            .unwrap_or_else(|e| panic!("{value}: {e}"));
    }
}

#[test]
fn success_is_the_bare_result() {
    let frame = reply(json!("a"), Ok(json!({"state": "pending"})));
    assert_eq!(frame, json!({"id": "a", "result": {"state": "pending"}}));
}
