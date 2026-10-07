use super::*;

#[test]
fn keeps_listed_models_in_server_order() {
    let catalog = parse(&serde_json::json!({"models": [
        {"slug": "gpt-b", "display_name": "GPT B", "visibility": "list", "context_window": 400000},
        {"slug": "gpt-hidden", "display_name": "Hidden", "visibility": "hide"},
        {"slug": "gpt-a", "visibility": "list",
         "supported_reasoning_levels": [{"effort": "low"}, {"effort": "high"}]},
        {"slug": "gpt-b", "display_name": "dup", "visibility": "list"},
        {"display_name": "no slug", "visibility": "list"}
    ]}));
    let ids: Vec<_> = catalog.models.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids, ["gpt-b", "gpt-a"]);
    assert_eq!(catalog.models[0].name, "GPT B");
    assert_eq!(catalog.models[0].context_window, Some(400000));
    assert_eq!(catalog.models[1].name, "gpt-a");
    assert_eq!(catalog.models[1].reasoning_efforts, ["low", "high"]);
}

#[test]
fn unexpected_shapes_yield_an_empty_catalog() {
    assert!(
        parse(&serde_json::json!({"object": "list"}))
            .models
            .is_empty()
    );
    assert!(parse(&serde_json::json!([])).models.is_empty());
}

#[tokio::test]
async fn real_sized_catalog_is_not_truncated() {
    use wiremock::matchers::{header, method, path};
    // The live catalog is ~600 KB: per-model instructions and tool specs.
    let padding = "x".repeat(90 * 1024);
    let models: Vec<_> = (0..10)
        .map(|i| {
            serde_json::json!({"slug": format!("gpt-{i}"), "display_name": format!("GPT {i}"),
                "visibility": "list", "base_instructions": padding})
        })
        .collect();
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(method("GET"))
        .and(path("/v1/models"))
        .and(header("authorization", "Bearer token"))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({ "models": models })),
        )
        .mount(&server)
        .await;
    let endpoints = crate::oauth::Endpoints {
        resource: format!("{}/v1", server.uri()).parse().unwrap(),
        ..Default::default()
    };
    let catalog = fetch(&crate::oauth::http_client().unwrap(), &endpoints, "token")
        .await
        .unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(catalog.models.len(), 10);
}
