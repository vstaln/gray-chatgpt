use super::*;
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use std::collections::BTreeMap;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn endpoints(server: &MockServer) -> Endpoints {
    let base = server.uri();
    Endpoints {
        issuer: "https://auth.openai.com".into(),
        authorize: format!("{base}/authorize").parse().unwrap(),
        token: format!("{base}/token").parse().unwrap(),
        revoke: format!("{base}/revoke").parse().unwrap(),
        jwks: format!("{base}/jwks").parse().unwrap(),
        resource: format!("{base}/v1").parse().unwrap(),
    }
}

fn id_token(aud: &str, nonce: &str, sub: &str) -> String {
    let key =
        EncodingKey::from_rsa_pem(include_bytes!("../testdata/id-token-test-key.pem")).unwrap();
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some("test-kid".into());
    let now = oauth::now_secs();
    encode(
        &header,
        &serde_json::json!({"iss": "https://auth.openai.com", "aud": aud, "sub": sub,
            "email": "a@b.c", "nonce": nonce, "iat": now, "exp": now + 600}),
        &key,
    )
    .unwrap()
}

async fn mount_jwks(server: &MockServer) {
    let n = include_str!("../testdata/id-token-test-key.n").trim();
    Mock::given(method("GET"))
        .and(path("/jwks"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"keys": [{
                "kty": "RSA", "use": "sig", "alg": "RS256", "kid": "test-kid", "n": n, "e": "AQAB"
            }]})),
        )
        .mount(server)
        .await;
}

async fn mount_token(server: &MockServer, aud: &str, nonce: &str, sub: &str, scope: &str) {
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": "access-1", "refresh_token": "refresh-1",
            "id_token": id_token(aud, nonce, sub), "token_type": "Bearer",
            "expires_in": 3600, "scope": scope
        })))
        .mount(server)
        .await;
}

fn query(uri: &str) -> BTreeMap<String, String> {
    url::Url::parse(uri)
        .unwrap()
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect()
}

async fn wait(manager: &LoginManager, op: &str) -> ProviderAuthPoll {
    for _ in 0..100 {
        match manager.poll(op).await {
            ProviderAuthPoll::Pending { .. } => {
                tokio::time::sleep(std::time::Duration::from_millis(20)).await
            }
            done => return done,
        }
    }
    panic!("login never finished");
}

const FULL_SCOPE: &str =
    "chatgpt.tokens.use.direct email offline_access openid profile resource.invoke";

#[tokio::test]
async fn first_sign_in_registers_and_returns_the_credential() {
    let server = MockServer::start().await;
    mount_jwks(&server).await;
    let dir = tempfile::tempdir().unwrap();
    let state = StateFile::at(dir.path().join("state.json"));
    let manager = LoginManager::new(
        endpoints(&server),
        state.clone(),
        oauth::http_client().unwrap(),
    );

    let start = manager.start().await.unwrap();
    let q = query(&start.verification_uri);
    assert_eq!(q["client_id"], "dynamic_agent_client");
    assert_eq!(q["ext_agent_host_id"], state.host_id().unwrap());
    mount_token(&server, "oaiapp_new", &q["nonce"], "user-1", FULL_SCOPE).await;

    let http = reqwest::Client::new();
    // A stray request first: the listener must keep waiting.
    let callback = url::Url::parse(&q["redirect_uri"]).unwrap();
    let favicon = format!("http://127.0.0.1:{}/favicon.ico", callback.port().unwrap());
    assert_eq!(http.get(&favicon).send().await.unwrap().status(), 404);
    let response = http
        .get(format!(
            "{}?code=c1&state={}&client_id=oaiapp_new&scope=openid",
            q["redirect_uri"], q["state"]
        ))
        .send()
        .await
        .unwrap();
    let header = |name: &str| {
        response
            .headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_string()
    };
    assert_eq!(header("content-type"), "text/html; charset=utf-8");
    assert!(header("content-security-policy").starts_with("default-src 'none'"));
    assert_eq!(header("referrer-policy"), "no-referrer");
    let page = response.text().await.unwrap();
    assert!(page.contains("Signed in with ChatGPT"), "{page}");
    assert!(
        page.contains(r#"aria-label="Gray""#),
        "Gray mark on the page"
    );

    let ProviderAuthPoll::Completed(material) = wait(&manager, &start.operation_id).await else {
        panic!("login did not complete");
    };
    assert_eq!(material.secrets.get("access_token"), Some("access-1"));
    assert_eq!(material.secrets.get("refresh_token"), Some("refresh-1"));
    assert_eq!(material.metadata["client_id"], "oaiapp_new");
    let saved = state.registration().unwrap();
    assert_eq!(saved.client_id, "oaiapp_new");
    assert_eq!(saved.subject.as_deref(), Some("user-1"));
    assert_eq!(saved.email.as_deref(), Some("a@b.c"));
}

#[tokio::test]
async fn identity_only_grant_is_refused() {
    let server = MockServer::start().await;
    mount_jwks(&server).await;
    let dir = tempfile::tempdir().unwrap();
    let state = StateFile::at(dir.path().join("state.json"));
    let manager = LoginManager::new(
        endpoints(&server),
        state.clone(),
        oauth::http_client().unwrap(),
    );
    let start = manager.start().await.unwrap();
    let q = query(&start.verification_uri);
    mount_token(
        &server,
        "oaiapp_new",
        &q["nonce"],
        "user-1",
        "openid profile email",
    )
    .await;
    reqwest::get(format!(
        "{}?code=c1&state={}&client_id=oaiapp_new",
        q["redirect_uri"], q["state"]
    ))
    .await
    .unwrap();
    let ProviderAuthPoll::Failed(failure) = wait(&manager, &start.operation_id).await else {
        panic!("identity-only login must fail");
    };
    assert_eq!(failure.code, "plan_usage_not_granted");
    // The registration survives for the retry that grants plan usage.
    assert_eq!(state.registration().unwrap().client_id, "oaiapp_new");
}

#[tokio::test]
async fn returning_sign_in_reuses_the_client_and_rejects_another_account() {
    let server = MockServer::start().await;
    mount_jwks(&server).await;
    let dir = tempfile::tempdir().unwrap();
    let state = StateFile::at(dir.path().join("state.json"));
    state
        .set_registration(Registration {
            client_id: "oaiapp_saved".into(),
            subject: Some("user-1".into()),
            email: Some("a@b.c".into()),
        })
        .unwrap();
    let manager = LoginManager::new(
        endpoints(&server),
        state.clone(),
        oauth::http_client().unwrap(),
    );
    let start = manager.start().await.unwrap();
    let q = query(&start.verification_uri);
    assert_eq!(q["client_id"], "oaiapp_saved");
    assert_eq!(q["login_hint"], "a@b.c");
    assert!(!q.contains_key("agent_name_hint"));
    mount_token(&server, "oaiapp_saved", &q["nonce"], "user-2", FULL_SCOPE).await;
    reqwest::get(format!(
        "{}?code=c1&state={}",
        q["redirect_uri"], q["state"]
    ))
    .await
    .unwrap();
    let ProviderAuthPoll::Failed(failure) = wait(&manager, &start.operation_id).await else {
        panic!("a different account must not replace the registration");
    };
    assert_eq!(failure.code, "account_changed");
    assert_eq!(
        state.registration().unwrap().subject.as_deref(),
        Some("user-1")
    );
}

#[tokio::test]
async fn cancel_releases_the_operation() {
    let server = MockServer::start().await;
    let dir = tempfile::tempdir().unwrap();
    let manager = LoginManager::new(
        endpoints(&server),
        StateFile::at(dir.path().join("state.json")),
        oauth::http_client().unwrap(),
    );
    let start = manager.start().await.unwrap();
    manager.cancel(&start.operation_id).await;
    assert!(matches!(
        manager.poll(&start.operation_id).await,
        ProviderAuthPoll::OperationLost
    ));
}
