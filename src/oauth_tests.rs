use super::*;

fn query(url: &Url) -> BTreeMap<String, String> {
    url.query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect()
}

#[test]
fn first_sign_in_registers_with_the_dynamic_client() {
    let attempt = Attempt::new(None, 1455);
    let url = authorization_url(
        &Endpoints::default(),
        &attempt,
        "urn:uuid:host",
        Some("a@b.c"),
    );
    assert!(
        url.as_str()
            .starts_with("https://auth.openai.com/api/accounts/authorize?")
    );
    let q = query(&url);
    assert_eq!(q["client_id"], "dynamic_agent_client");
    assert_eq!(q["agent_name_hint"], "Gray");
    assert_eq!(q["ext_agent_host_id"], "urn:uuid:host");
    assert_eq!(q["response_type"], "code");
    assert_eq!(q["redirect_uri"], "http://127.0.0.1:1455/auth/callback");
    assert_eq!(
        q["scope"],
        "openid profile email offline_access resource.invoke chatgpt.tokens.use.direct"
    );
    assert_eq!(q["resource"], "https://api.openai.com/v1");
    assert_eq!(q["code_challenge_method"], "S256");
    assert_eq!(q["code_challenge"], pkce_challenge(&attempt.verifier));
    assert_eq!(q["state"], *attempt.state);
    assert_eq!(q["nonce"], *attempt.nonce);
    // A login hint only makes sense for a known registration.
    assert!(!q.contains_key("login_hint"));
}

#[test]
fn reauthorization_reuses_the_issued_client_without_a_name_hint() {
    let attempt = Attempt::new(Some("oaiapp_saved".into()), 54321);
    let url = authorization_url(
        &Endpoints::default(),
        &attempt,
        "urn:uuid:host",
        Some("a@b.c"),
    );
    let q = query(&url);
    assert_eq!(q["client_id"], "oaiapp_saved");
    assert!(!q.contains_key("agent_name_hint"));
    assert_eq!(q["login_hint"], "a@b.c");
    assert_eq!(q["ext_agent_host_id"], "urn:uuid:host");
    assert_eq!(q["redirect_uri"], "http://127.0.0.1:54321/auth/callback");
}

#[test]
fn attempts_never_share_state_nonce_or_verifier() {
    let a = Attempt::new(None, 1455);
    let b = Attempt::new(None, 1455);
    assert_ne!(*a.state, *b.state);
    assert_ne!(*a.nonce, *b.nonce);
    assert_ne!(*a.verifier, *b.verifier);
    assert_ne!(*a.state, *a.nonce);
}

#[test]
fn pkce_challenge_matches_rfc7636_example() {
    assert_eq!(
        pkce_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
        "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
    );
}

#[test]
fn only_the_callback_path_is_a_callback() {
    assert_eq!(parse_callback("/favicon.ico"), None);
    assert_eq!(parse_callback("/callback?code=x"), None);
    let cb = parse_callback("/auth/callback?code=c&state=s&client_id=oaiapp_1&scope=a+b").unwrap();
    assert_eq!(cb.code.as_deref(), Some("c"));
    assert_eq!(cb.states, ["s"]);
    assert_eq!(cb.client_id.as_deref(), Some("oaiapp_1"));
}

fn callback(attempt: &Attempt, extra: &str) -> Callback {
    parse_callback(&format!("/auth/callback?state={}{extra}", *attempt.state)).unwrap()
}

#[test]
fn registration_takes_the_issued_client() {
    let attempt = Attempt::new(None, 1455);
    let (code, client) = accept_callback(
        &attempt,
        &callback(&attempt, "&code=c&client_id=oaiapp_new"),
    )
    .unwrap();
    assert_eq!((code.as_str(), client.as_str()), ("c", "oaiapp_new"));
}

#[test]
fn registration_without_an_issued_client_is_incomplete() {
    let attempt = Attempt::new(None, 1455);
    for extra in ["&code=c", "&code=c&client_id=dynamic_agent_client"] {
        let err = accept_callback(&attempt, &callback(&attempt, extra)).unwrap_err();
        assert_eq!(err.code, "registration_incomplete", "{extra}");
    }
}

#[test]
fn reauthorization_keeps_the_saved_client_and_rejects_a_different_one() {
    let attempt = Attempt::new(Some("oaiapp_saved".into()), 1455);
    let (_, client) = accept_callback(&attempt, &callback(&attempt, "&code=c")).unwrap();
    assert_eq!(client, "oaiapp_saved");
    let err = accept_callback(
        &attempt,
        &callback(&attempt, "&code=c&client_id=oaiapp_other"),
    )
    .unwrap_err();
    assert_eq!(err.code, "client_mismatch");
}

#[test]
fn state_is_checked_before_anything_else() {
    let attempt = Attempt::new(None, 1455);
    let forged = parse_callback("/auth/callback?state=forged&error=access_denied").unwrap();
    assert_eq!(
        accept_callback(&attempt, &forged).unwrap_err().code,
        "invalid_state"
    );
    let doubled = parse_callback(&format!(
        "/auth/callback?state={0}&state={0}&code=c&client_id=oaiapp_1",
        *attempt.state
    ))
    .unwrap();
    assert_eq!(
        accept_callback(&attempt, &doubled).unwrap_err().code,
        "invalid_state"
    );
}

#[test]
fn declined_consent_is_access_denied() {
    let attempt = Attempt::new(None, 1455);
    let err = accept_callback(&attempt, &callback(&attempt, "&error=access_denied")).unwrap_err();
    assert_eq!(err.code, "access_denied");
}

#[test]
fn refresh_rejections_map_onto_the_host_contract() {
    let code = |e: ProviderRpcError| match e {
        ProviderRpcError::Rpc(f) => (f.code, f.terminal),
        other => panic!("{other:?}"),
    };
    for dead in [
        "invalid_grant",
        "invalid_refresh_token",
        "token_expired",
        "refresh_token_expired",
        "refresh_token_invalidated",
        "invalid_client",
    ] {
        assert_eq!(
            code(refresh_failure(TokenError::Rejected(dead.into()))),
            ("invalid_grant".to_string(), true),
            "{dead}"
        );
    }
    assert_eq!(
        code(refresh_failure(TokenError::Rejected(
            "refresh_token_reused".into()
        ))),
        ("refresh_reuse".to_string(), true)
    );
    // Offline is not a dead login: gray keeps the token set.
    assert_eq!(
        code(refresh_failure(TokenError::Unavailable)),
        ("unavailable".to_string(), false)
    );
}

fn tokens(refresh: Option<&str>, scope: Option<&str>) -> TokenResponse {
    TokenResponse {
        access_token: "access".into(),
        refresh_token: refresh.map(str::to_string),
        id_token: None,
        expires_in: 3600,
        scope: scope.map(str::to_string),
    }
}

#[test]
fn credential_carries_the_registration_and_expiry() {
    let material = credential(
        &tokens(Some("refresh"), Some("openid chatgpt.tokens.use.direct")),
        "oaiapp_1",
        "sub_1",
        Some("a@b.c"),
        None,
        None,
    )
    .unwrap();
    assert_eq!(material.secrets.get("access_token"), Some("access"));
    assert_eq!(material.secrets.get("refresh_token"), Some("refresh"));
    assert_eq!(material.metadata["client_id"], "oaiapp_1");
    assert_eq!(material.metadata["subject"], "sub_1");
    assert_eq!(material.metadata["email"], "a@b.c");
    let expires = material.expires_at.unwrap();
    assert!(expires >= now_secs() + 3590 && expires <= now_secs() + 3600);
}

#[test]
fn refresh_without_rotation_or_scope_keeps_the_previous_values() {
    let material = credential(
        &tokens(None, None),
        "oaiapp_1",
        "sub_1",
        None,
        Some("old-refresh"),
        Some("openid chatgpt.tokens.use.direct"),
    )
    .unwrap();
    assert_eq!(material.secrets.get("refresh_token"), Some("old-refresh"));
    assert_eq!(
        material.metadata["scope"],
        "openid chatgpt.tokens.use.direct"
    );
}

fn mock_endpoints(server: &wiremock::MockServer) -> Endpoints {
    let base = server.uri();
    Endpoints {
        issuer: base.clone(),
        authorize: format!("{base}/authorize").parse().unwrap(),
        token: format!("{base}/token").parse().unwrap(),
        revoke: format!("{base}/revoke").parse().unwrap(),
        jwks: format!("{base}/jwks").parse().unwrap(),
        resource: format!("{base}/v1").parse().unwrap(),
    }
}

#[tokio::test]
async fn code_exchange_sends_the_spec_form() {
    use wiremock::matchers::{body_string_contains, method, path};
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(method("POST"))
        .and(path("/token"))
        .and(body_string_contains("grant_type=authorization_code"))
        .and(body_string_contains("client_id=oaiapp_new"))
        .and(body_string_contains("code=the-code"))
        .and(body_string_contains(
            "resource=https%3A%2F%2Fapi.openai.com%2Fv1",
        ))
        .and(body_string_contains(
            "redirect_uri=http%3A%2F%2F127.0.0.1%3A1455%2Fauth%2Fcallback",
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "a", "refresh_token": "r", "id_token": "i",
                "token_type": "Bearer", "expires_in": 3600,
                "scope": "openid chatgpt.tokens.use.direct"
            })),
        )
        .expect(1)
        .mount(&server)
        .await;
    let attempt = Attempt::new(None, 1455);
    let tokens = exchange_code(
        &http_client().unwrap(),
        &mock_endpoints(&server),
        &attempt,
        "the-code",
        "oaiapp_new",
    )
    .await
    .unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(tokens.access_token, "a");
    assert!(tokens.scopes().contains(&PLAN_USAGE_SCOPE));
    let received = server.received_requests().await.unwrap();
    let body = String::from_utf8_lossy(&received[0].body);
    assert!(
        body.contains(&format!("code_verifier={}", *attempt.verifier)),
        "{body}"
    );
}

#[tokio::test]
async fn refresh_reads_the_oauth_error_code_not_the_body() {
    use wiremock::matchers::{method, path};
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(wiremock::ResponseTemplate::new(400).set_body_json(
            serde_json::json!({"error": "refresh_token_reused", "error_description": "secret-ish"}),
        ))
        .mount(&server)
        .await;
    let err = refresh(
        &http_client().unwrap(),
        &mock_endpoints(&server),
        "oaiapp_1",
        "r",
    )
    .await
    .err()
    .unwrap();
    assert_eq!(err, TokenError::Rejected("refresh_token_reused".into()));
}

#[tokio::test]
async fn server_errors_are_transient() {
    use wiremock::matchers::method;
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(method("POST"))
        .respond_with(wiremock::ResponseTemplate::new(503))
        .mount(&server)
        .await;
    let endpoints = mock_endpoints(&server);
    let http = http_client().unwrap();
    assert_eq!(
        refresh(&http, &endpoints, "oaiapp_1", "r")
            .await
            .err()
            .unwrap(),
        TokenError::Unavailable
    );
    assert_eq!(
        revoke(&http, &endpoints, "oaiapp_1", "r")
            .await
            .unwrap_err(),
        TokenError::Unavailable
    );
}
