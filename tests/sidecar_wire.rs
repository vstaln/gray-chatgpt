//! The real binary through gray's own sidecar client (`SidecarPlugin`): the
//! exact params gray sends and the reply shapes it accepts. Nothing here
//! reaches OpenAI — `auth/start` only builds a URL, and the refresh case
//! fails before any request.

use std::time::Duration;

use gray_core::credential::{CredentialEnvelope, CredentialMaterial, SecretMap};
use gray_plugin::{ProviderAuthPoll, ProviderRefreshRequest, ProviderRpcError, SidecarPlugin};

async fn spawn(home: &std::path::Path) -> SidecarPlugin {
    let plugin = SidecarPlugin::spawn(vec![
        "env".into(),
        format!("GRAY_HOME={}", home.display()),
        env!("CARGO_BIN_EXE_gray-chatgpt").into(),
    ])
    .await
    .expect("sidecar spawns and answers plugin/manifest");
    // What `gray plugin install` consent grants.
    plugin.set_capabilities(vec![gray_plugin::PROVIDER_CREDENTIALS.into()]);
    plugin
}

#[tokio::test]
async fn login_start_poll_and_cancel_over_the_real_wire() {
    let home = tempfile::tempdir().unwrap();
    let plugin = spawn(home.path()).await;
    let start = plugin
        .provider_auth_start("chatgpt", "chatgpt-plan")
        .await
        .expect("auth/start");
    assert!(
        start
            .verification_uri
            .starts_with("https://auth.openai.com/api/accounts/authorize?"),
    );
    // gray polls with the operation id alone.
    let poll = plugin
        .provider_auth_poll(&start.operation_id)
        .await
        .expect("auth/poll parses");
    assert!(matches!(poll, ProviderAuthPoll::Pending { .. }), "{poll:?}");
    plugin
        .provider_auth_cancel(&start.operation_id)
        .await
        .expect("auth/cancel");
    let poll = plugin
        .provider_auth_poll(&start.operation_id)
        .await
        .unwrap();
    assert!(matches!(poll, ProviderAuthPoll::OperationLost), "{poll:?}");
    // The host id was persisted under GRAY_HOME before the first sign-in.
    assert!(home.path().join("chatgpt/state.json").exists());
    plugin.shutdown(Duration::from_secs(2)).await;
}

#[tokio::test]
async fn refresh_failures_reach_gray_as_typed_errors() {
    let home = tempfile::tempdir().unwrap();
    let plugin = spawn(home.path()).await;
    // A credential without its issued client cannot be renewed.
    let material = CredentialMaterial {
        secrets: SecretMap::from_iter([("access_token", "a"), ("refresh_token", "r")]),
        metadata: Default::default(),
        expires_at: Some(1),
    };
    let request = ProviderRefreshRequest {
        provider: "chatgpt".into(),
        auth_method: "chatgpt-plan".into(),
        profile_binding: "sha256:test".into(),
        credential: CredentialEnvelope::new(
            "chatgpt",
            "chatgpt",
            "chatgpt-plan",
            "sha256:test",
            material,
        )
        .unwrap(),
    };
    match plugin.provider_auth_refresh(&request).await {
        Err(ProviderRpcError::Rpc(failure)) => {
            assert_eq!(failure.code, "invalid_grant");
            assert!(failure.terminal);
        }
        other => panic!("expected a typed invalid_grant, got {other:?}"),
    }
    plugin.shutdown(Duration::from_secs(2)).await;
}
