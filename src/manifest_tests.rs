use super::*;

#[test]
fn declaration_passes_host_validation() {
    let value = serde_json::to_value(provider()).unwrap();
    let decl = ProviderDecl::from_value(&value).expect("host accepts the declaration");
    assert_eq!(
        decl.transport.base_url.as_str(),
        "https://api.openai.com/v1"
    );
    assert!(!decl.transport.request.store);
    assert!(!decl.transport.request.previous_response_id);
    assert!(!decl.transport.request.warm_replay);
}

#[test]
fn direct_transport_never_asks_for_a_relay() {
    let ops = &provider().auth_methods[0].operations;
    assert!(!ops.iter().any(|op| op == "chat"), "{ops:?}");
    assert!(ops.iter().any(|op| op == "refresh"), "{ops:?}");
}

#[test]
fn manifest_claims_provider_protocol_and_capability() {
    let value = serde_json::to_value(manifest()).unwrap();
    assert_eq!(value["protocol"], "1.2");
    assert_eq!(
        value["capabilities"],
        serde_json::json!(["provider.credentials"])
    );
    assert_eq!(value["providers"][0]["id"], PROVIDER_ID);
}
