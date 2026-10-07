//! gray-chatgpt: a protocol-1.2 provider sidecar for gray.
//!
//! Signs in with ChatGPT (OpenAI's open-source plan-usage flow) and hands
//! gray an OAuth access token for the public Responses API. Gray makes the
//! inference requests itself; this process only logs in, refreshes, revokes
//! and lists models.

use std::io::Write;

use gray_chatgpt::login::LoginManager;
use gray_chatgpt::oauth::{self, Endpoints, TokenError};
use gray_chatgpt::state::StateFile;
use gray_chatgpt::{manifest, models, wire};
use gray_plugin::{
    ProviderModelsRequest, ProviderRefreshRequest, ProviderRevokeRequest, ProviderRpcError,
    ProviderRpcFailure,
};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
struct Request {
    id: Value,
    method: String,
    #[serde(default)]
    params: Option<Value>,
}

struct Ctx {
    logins: LoginManager,
    endpoints: Endpoints,
    http: reqwest::Client,
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    match std::env::args().nth(1).as_deref() {
        // `gray plugin install` registers sidecars by running `<bin> manifest`.
        Some("manifest") => {
            println!("{}", serde_json::to_string(&manifest::manifest())?);
            return Ok(());
        }
        Some("status") => return status(),
        Some("forget") => return forget(),
        Some("--version" | "-V") => {
            println!("gray-chatgpt {}", manifest::PLUGIN_VERSION);
            return Ok(());
        }
        Some("help" | "--help" | "-h") => {
            println!(
                "gray-chatgpt: Sign in with ChatGPT for gray\n\n\
                 Run with no arguments, gray talks to it over stdio.\n\n  \
                 status   show this install's ChatGPT registration\n  \
                 forget   drop the registration (switch account/workspace on next /connect)\n  \
                 manifest print the plugin manifest"
            );
            return Ok(());
        }
        _ => {}
    }
    serve().await
}

fn status() -> anyhow::Result<()> {
    let file = StateFile::default_location();
    let state = file.load();
    println!("state:     {}", file.path().display());
    println!(
        "host id:   {}",
        state.host_id.as_deref().unwrap_or("(none yet)")
    );
    match state.registration {
        Some(r) => {
            println!("client id: {}", r.client_id);
            println!(
                "account:   {}",
                r.email.as_deref().unwrap_or("(unverified)")
            );
        }
        None => println!("client id: (not registered; run /connect in gray)"),
    }
    Ok(())
}

fn forget() -> anyhow::Result<()> {
    let file = StateFile::default_location();
    file.forget_registration()?;
    println!(
        "Forgot the ChatGPT registration. The next /connect registers Gray again.\n\
         To cut off the old registration too, remove Gray under ChatGPT Settings → Apps."
    );
    Ok(())
}

async fn serve() -> anyhow::Result<()> {
    let http = oauth::http_client()?;
    let endpoints = Endpoints::default();
    let ctx = Ctx {
        logins: LoginManager::new(
            endpoints.clone(),
            StateFile::default_location(),
            http.clone(),
        ),
        endpoints,
        http,
    };
    let mut lines =
        tokio::io::AsyncBufReadExt::lines(tokio::io::BufReader::new(tokio::io::stdin()));
    let mut stdout = std::io::stdout();
    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }
        let Ok(request) = serde_json::from_str::<Request>(&line) else {
            continue;
        };
        let params = request.params.clone().unwrap_or_else(|| json!({}));
        let outcome = handle(&ctx, &request.method, params).await;
        // Protocol frames only: never log tokens or upstream bodies.
        let frame = serde_json::to_string(&wire::reply(request.id, outcome))?;
        writeln!(stdout, "{frame}")?;
        stdout.flush()?;
        if request.method == "plugin/shutdown" {
            return Ok(());
        }
    }
    Ok(())
}

async fn handle(ctx: &Ctx, method: &str, params: Value) -> Result<Value, ProviderRpcError> {
    match method {
        "plugin/manifest" => Ok(serde_json::to_value(manifest::manifest()).unwrap()),
        "provider/auth/start" => {
            ensure_provider(
                str_param(&params, "provider"),
                str_param(&params, "auth_method"),
            )?;
            Ok(serde_json::to_value(ctx.logins.start().await?).unwrap())
        }
        "provider/auth/poll" => {
            // Gray sends only the operation id here.
            Ok(wire::poll_value(
                ctx.logins.poll(str_param(&params, "operation_id")).await,
            ))
        }
        "provider/auth/cancel" => {
            ctx.logins.cancel(str_param(&params, "operation_id")).await;
            Ok(json!({}))
        }
        "provider/auth/refresh" => refresh(ctx, params).await,
        "provider/auth/revoke" => revoke(ctx, params).await,
        "provider/models" => {
            let request: ProviderModelsRequest =
                serde_json::from_value(params).map_err(|_| protocol("models request"))?;
            ensure_provider(&request.provider, &request.auth_method)?;
            let access = request
                .credential
                .credential
                .secrets
                .get("access_token")
                .ok_or_else(|| protocol("credential has no access token"))?;
            let catalog = models::fetch(&ctx.http, &ctx.endpoints, access).await?;
            Ok(serde_json::to_value(catalog).unwrap())
        }
        "plugin/shutdown" => Ok(json!({})),
        _ => Err(protocol("unknown method")),
    }
}

async fn refresh(ctx: &Ctx, params: Value) -> Result<Value, ProviderRpcError> {
    let request: ProviderRefreshRequest =
        serde_json::from_value(params).map_err(|_| protocol("refresh request"))?;
    ensure_provider(&request.provider, &request.auth_method)?;
    let current = &request.credential.credential;
    // Without its issued client the token set cannot be renewed: sign in again.
    let (Some(client_id), Some(refresh_token)) = (
        current.metadata.get("client_id"),
        current.secrets.get("refresh_token"),
    ) else {
        return Err(ProviderRpcError::Rpc(oauth::failure(
            "invalid_grant",
            "Stored ChatGPT sign-in is incomplete; sign in again.",
        )));
    };
    let tokens = oauth::refresh(&ctx.http, &ctx.endpoints, client_id, refresh_token)
        .await
        .map_err(oauth::refresh_failure)?;
    let material = oauth::credential(
        &tokens,
        client_id,
        current
            .metadata
            .get("subject")
            .map(String::as_str)
            .unwrap_or_default(),
        current.metadata.get("email").map(String::as_str),
        Some(refresh_token),
        current.metadata.get("scope").map(String::as_str),
    )
    .map_err(ProviderRpcError::Rpc)?;
    Ok(serde_json::to_value(material).unwrap())
}

async fn revoke(ctx: &Ctx, params: Value) -> Result<Value, ProviderRpcError> {
    let request: ProviderRevokeRequest =
        serde_json::from_value(params).map_err(|_| protocol("revoke request"))?;
    ensure_provider(&request.provider, &request.auth_method)?;
    let current = &request.credential.credential;
    let (Some(client_id), Some(refresh_token)) = (
        current.metadata.get("client_id"),
        current.secrets.get("refresh_token"),
    ) else {
        return Ok(json!({"status": "revoked"}));
    };
    // Retry transient failures briefly; the registration itself is kept so
    // a later sign-in reuses the same client.
    for delay_ms in [0u64, 500, 1500] {
        tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
        match oauth::revoke(&ctx.http, &ctx.endpoints, client_id, refresh_token).await {
            Ok(()) => return Ok(json!({"status": "revoked"})),
            Err(TokenError::Unavailable) => continue,
            Err(TokenError::Rejected(_)) => break,
        }
    }
    Err(ProviderRpcError::Rpc(ProviderRpcFailure {
        code: "revocation_unconfirmed".to_string(),
        message: "Signed out locally, but OpenAI did not confirm revocation. \
                  Remove Gray under ChatGPT Settings → Apps to cut it off."
            .to_string(),
        retryable: true,
        terminal: false,
    }))
}

fn str_param<'a>(params: &'a Value, key: &str) -> &'a str {
    params.get(key).and_then(Value::as_str).unwrap_or_default()
}

fn ensure_provider(provider: &str, auth_method: &str) -> Result<(), ProviderRpcError> {
    if provider == manifest::PROVIDER_ID && auth_method == manifest::AUTH_METHOD_ID {
        Ok(())
    } else {
        Err(protocol("unknown provider or auth method"))
    }
}

fn protocol(message: &str) -> ProviderRpcError {
    ProviderRpcError::Protocol(message.to_string())
}
