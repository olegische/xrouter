mod app_state;
pub mod config;
mod http;
mod startup;
pub use app_state::AppState;
pub use http::docs::build_router;

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use axum::{
        body::{Body, to_bytes},
        http::{HeaderMap, Request, StatusCode},
        response::Response,
    };
    use serde_json::{Map, Value, json};
    use tower::ServiceExt;

    use crate::startup::model_catalog::{
        OpenRouterModelsResponse, XrouterProviderModelsResponse, build_models_from_registry,
        fetch_openrouter_models, map_openrouter_models, map_xrouter_models,
    };
    use crate::{AppState, build_router, http::errors::error_response};
    use xrouter_core::CoreError;

    #[derive(Debug)]
    struct AppFixture<'a> {
        name: &'a str,
        method: &'a str,
        path: &'a str,
        body: Option<&'a str>,
    }

    impl<'a> AppFixture<'a> {
        fn parse(raw: &'a str) -> Self {
            let mut fixture = Self { name: "unnamed", method: "GET", path: "/health", body: None };

            for line in raw.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                let Some((key, value)) = line.split_once('=') else {
                    continue;
                };
                let key = key.trim();
                let value = value.trim();

                match key {
                    "name" => fixture.name = value,
                    "method" => fixture.method = value,
                    "path" => fixture.path = value,
                    "body" => fixture.body = Some(value),
                    other => panic!("unsupported fixture key: {other}"),
                }
            }

            fixture
        }
    }

    #[test]
    fn map_openrouter_models_uses_provider_payload_fields() {
        let payload: OpenRouterModelsResponse = serde_json::from_value(json!({
            "data": [{
                "id": "openai/gpt-5.2",
                "description": "OpenAI GPT-5.2 via OpenRouter",
                "context_length": 222000,
                "architecture": {"modality": "text->text"},
                "top_provider": {
                    "context_length": 210000,
                    "max_completion_tokens": 12345,
                    "is_moderated": false
                }
            }, {
                "id": "ignore/me",
                "description": "ignored",
                "context_length": 1
            }]
        }))
        .expect("payload must deserialize");

        let models = map_openrouter_models(payload, &["openai/gpt-5.2".to_string()]);
        assert_eq!(models.len(), 1);
        let model = &models[0];
        assert_eq!(model.id, "openai/gpt-5.2");
        assert_eq!(model.description, "OpenAI GPT-5.2 via OpenRouter");
        assert_eq!(model.context_length, 222000);
        assert_eq!(model.top_provider_context_length, 210000);
        assert_eq!(model.max_completion_tokens, 12345);
        assert!(!model.is_moderated);
        assert_eq!(model.modality, "text->text");
        assert_eq!(model.tokenizer, "unknown");
        assert_eq!(model.instruct_type, "none");
    }

    #[test]
    fn fetch_openrouter_models_returns_none_when_request_fails() {
        let provider = crate::config::ProviderConfig {
            enabled: true,
            api_key: None,
            base_url: Some("http://127.0.0.1:0".to_string()),
            project: None,
        };
        let models = fetch_openrouter_models(&provider, &["openai/gpt-5.2".to_string()], 1);
        assert!(models.is_none());
    }

    #[test]
    fn build_models_from_registry_uses_seed_and_fallback_for_unknown_ids() {
        let seed = xrouter_core::default_model_catalog();
        let ids = vec!["glm-4.5".to_string(), "glm-4.6".to_string(), "glm-5".to_string()];
        let models = build_models_from_registry("zai", &ids, &seed);
        assert_eq!(models.len(), 3);
        assert_eq!(models[0].id, "glm-4.5");
        assert_eq!(models[0].provider, "zai");
        assert_eq!(models[1].id, "glm-4.6");
        assert_eq!(models[1].provider, "zai");
        assert_eq!(models[1].max_completion_tokens, 128_000);
        assert_eq!(models[1].context_length, 200_000);
        assert_eq!(models[2].id, "glm-5");
        assert_eq!(models[2].max_completion_tokens, 128_000);
    }

    #[test]
    fn map_xrouter_models_filters_non_chat_models_and_hardcodes_missing_fields() {
        let payload: XrouterProviderModelsResponse = serde_json::from_value(json!({
            "data": [
                {
                    "id": "Qwen/Qwen3-235B-A22B-Instruct-2507",
                    "context_length": 262144,
                    "metadata": {
                        "company": "Qwen",
                        "name": "Qwen3-235B-A22B-Instruct-2507",
                        "type": "llm",
                        "endpoints": [{"path": "/v1/chat/completions"}]
                    }
                },
                {
                    "id": "Qwen/Qwen3-Embedding-0.6B",
                    "context_length": 32768,
                    "metadata": {
                        "company": "Qwen",
                        "name": "Qwen3-Embedding-0.6B",
                        "type": "embedder",
                        "endpoints": [{"path": "/v1/embeddings"}]
                    }
                }
            ]
        }))
        .expect("payload must deserialize");

        let models = map_xrouter_models(payload);
        assert_eq!(models.len(), 1);
        let model = &models[0];
        assert_eq!(model.id, "Qwen/Qwen3-235B-A22B-Instruct-2507");
        assert_eq!(model.provider, "xrouter");
        assert_eq!(model.description, "Qwen3-235B-A22B-Instruct-2507 via xrouter (Qwen)");
        assert_eq!(model.context_length, 262_144);
        assert_eq!(model.top_provider_context_length, 262_144);
        assert_eq!(model.max_completion_tokens, 8_192);
        assert_eq!(model.modality, "text->text");
        assert_eq!(model.tokenizer, "unknown");
        assert_eq!(model.instruct_type, "none");
        assert!(model.is_moderated);
    }

    fn assert_snapshot(name: &str, actual: &str, expected: &str) {
        let actual = actual.trim();
        let expected = expected.trim();
        assert_eq!(
            actual, expected,
            "snapshot mismatch for fixture `{name}`\n\nactual:\n{actual}\n\nexpected:\n{expected}"
        );
    }

    fn normalize_json(mut value: Value) -> Value {
        fn walk(value: &mut Value) {
            match value {
                Value::Object(map) => {
                    for (key, child) in map.iter_mut() {
                        if key == "id" {
                            *child = Value::String("<id>".to_string());
                        } else {
                            walk(child);
                        }
                    }
                }
                Value::Array(items) => {
                    for item in items {
                        walk(item);
                    }
                }
                _ => {}
            }
        }

        walk(&mut value);
        value
    }

    fn summarize_json(value: Value) -> String {
        let value = normalize_json(value);
        let Some(obj) = value.as_object() else {
            return format!("json={}", value);
        };

        if let Some(status) = obj.get("status").and_then(Value::as_str)
            && obj.len() == 1
        {
            return format!("json.status={status}");
        }

        if let Some(error) = obj.get("error").and_then(Value::as_str) {
            return format!("json.error={error}");
        }

        if let Some(data) = obj.get("data").and_then(Value::as_array) {
            let first_id = data
                .first()
                .and_then(Value::as_object)
                .and_then(|it| it.get("id"))
                .and_then(Value::as_str)
                .unwrap_or("<none>");
            return format!("json.data_len={}\njson.first_id={first_id}", data.len());
        }

        if let Some(output) = obj.get("output").and_then(Value::as_array) {
            let output_text = output
                .iter()
                .find_map(|item| {
                    item.as_object()
                        .filter(|map| map.get("type").and_then(Value::as_str) == Some("message"))
                        .and_then(|map| map.get("content"))
                        .and_then(Value::as_array)
                        .and_then(|arr| arr.first())
                        .and_then(Value::as_object)
                        .and_then(|part| part.get("text"))
                        .and_then(Value::as_str)
                })
                .unwrap_or("");
            let status = obj.get("status").and_then(Value::as_str).unwrap_or("<none>");
            let usage_total = obj
                .get("usage")
                .and_then(Value::as_object)
                .and_then(|usage| usage.get("total_tokens"))
                .and_then(Value::as_u64)
                .unwrap_or(0);
            return format!(
                "json.status={status}\njson.output_text={}\njson.usage_total={usage_total}",
                output_text.trim_end()
            );
        }

        if obj.get("object").and_then(Value::as_str) == Some("chat.completion") {
            let content = obj
                .get("choices")
                .and_then(Value::as_array)
                .and_then(|choices| choices.first())
                .and_then(Value::as_object)
                .and_then(|choice| choice.get("message"))
                .and_then(Value::as_object)
                .and_then(|message| message.get("content"))
                .and_then(Value::as_str)
                .unwrap_or("");
            return format!("json.object=chat.completion\njson.choice0={}", content.trim_end());
        }

        let ordered = to_ordered_json(obj);
        format!("json={ordered}")
    }

    fn to_ordered_json(map: &Map<String, Value>) -> Value {
        let mut ordered = BTreeMap::new();
        for (k, v) in map {
            ordered.insert(k.clone(), v.clone());
        }
        serde_json::to_value(ordered).expect("ordered json serialization must succeed")
    }

    fn summarize_text(body: &str) -> String {
        if !body.contains("response.created")
            && !body.contains("response.completed")
            && !body.contains("[DONE]")
        {
            return format!("text.body={}", body.trim());
        }
        format!(
            "text.has_response_created={}\ntext.has_response_completed={}\ntext.has_done_marker={}",
            body.contains("response.created"),
            body.contains("response.completed"),
            body.contains("[DONE]")
        )
    }

    async fn snapshot_response(response: Response) -> String {
        let status = response.status().as_u16();
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body read must succeed");
        let body = String::from_utf8_lossy(&body).to_string();
        let summary = match serde_json::from_str::<Value>(&body) {
            Ok(value) => summarize_json(value),
            Err(_) => summarize_text(&body),
        };
        format!("status={status}\n{summary}")
    }

    fn test_app_state(openai_compatible_api: bool) -> AppState {
        let mut config = crate::config::AppConfig::for_tests();
        config.openai_compatible_api = openai_compatible_api;
        AppState::from_config(&config)
    }

    async fn check_fixture(
        raw_fixture: &str,
        expected_snapshot: &str,
        openai_compatible_api: bool,
    ) {
        let fixture = AppFixture::parse(raw_fixture);
        let app = build_router(test_app_state(openai_compatible_api));

        let mut builder = Request::builder().method(fixture.method).uri(fixture.path);

        let request_body = if let Some(body) = fixture.body {
            builder = builder.header("content-type", "application/json");
            Body::from(body.to_string())
        } else {
            Body::empty()
        };

        let response = app
            .oneshot(builder.body(request_body).expect("request must build"))
            .await
            .expect("request must complete");

        if expected_snapshot.contains("status=200") {
            assert_eq!(response.status(), StatusCode::OK);
        }

        let actual_snapshot = snapshot_response(response).await;
        assert_snapshot(fixture.name, &actual_snapshot, expected_snapshot);
    }

    #[tokio::test]
    async fn app_route_fixtures() {
        let fixtures = [
            (
                r#"
name=health
method=GET
path=/health
"#,
                r#"
status=200
json.status=healthy
"#,
            ),
            (
                r#"
name=models_xrouter
method=GET
path=/api/v1/models
"#,
                r#"
status=200
json.data_len=53
json.first_id=<id>
"#,
            ),
            (
                r#"
name=responses_success
method=POST
path=/api/v1/responses
body={"model":"openrouter/anthropic/claude-3.5-sonnet","input":"hello world","stream":false}
"#,
                r#"
status=200
json.status=completed
json.output_text=[openrouter] hello world
json.usage_total=4
"#,
            ),
            (
                r#"
name=responses_validation_error
method=POST
path=/api/v1/responses
body={"model":"gpt-4.1-mini","input":"","stream":false}
"#,
                r#"
status=400
json.error=validation failed: input must not be empty
"#,
            ),
            (
                r#"
name=responses_array_input_message
method=POST
path=/api/v1/responses
body={"model":"deepseek/deepseek-chat","input":[{"role":"user","content":"hi"}],"stream":false}
"#,
                r#"
status=200
json.status=completed
json.output_text=[deepseek] user:hi
json.usage_total=2
"#,
            ),
            (
                r#"
name=responses_invalid_json_shape
method=POST
path=/api/v1/responses
body={"model":"deepseek/deepseek-chat","input":[1],"stream":false}
"#,
                r#"
status=422
json.error=invalid request body
"#,
            ),
            (
                r#"
name=chat_adapter_success
method=POST
path=/api/v1/chat/completions
body={"model":"gigachat/GigaChat-2-Max","messages":[{"role":"user","content":"hello world"}],"stream":false}
"#,
                r#"
status=200
json.object=chat.completion
json.choice0=[gigachat] user:hello world
"#,
            ),
            (
                r#"
name=responses_stream
method=POST
path=/api/v1/responses
body={"model":"gpt-4.1-mini","input":"hello world","stream":true}
"#,
                r#"
status=200
text.has_response_created=true
text.has_response_completed=true
text.has_done_marker=false
"#,
            ),
            (
                r#"
name=openai_paths_disabled_by_default
method=GET
path=/v1/models
"#,
                r#"
status=404
text.body=
"#,
            ),
        ];

        for (fixture, expected) in fixtures {
            check_fixture(fixture, expected, false).await;
        }
    }

    #[tokio::test]
    async fn app_openai_compatible_paths_fixtures() {
        let fixtures = [
            (
                r#"
name=openai_compatible_models_path
method=GET
path=/v1/models
"#,
                r#"
status=200
json.data_len=53
json.first_id=<id>
"#,
            ),
            (
                r#"
name=xrouter_paths_disabled_in_openai_mode
method=GET
path=/api/v1/models
"#,
                r#"
status=404
text.body=
"#,
            ),
        ];

        for (fixture, expected) in fixtures {
            check_fixture(fixture, expected, true).await;
        }
    }

    #[tokio::test]
    async fn app_models_empty_when_all_providers_disabled() {
        let mut config = crate::config::AppConfig::for_tests();
        for provider in config.providers.values_mut() {
            provider.enabled = false;
        }

        let app = build_router(AppState::from_config(&config));
        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/v1/models")
                    .body(Body::empty())
                    .expect("request must build"),
            )
            .await
            .expect("request must complete");

        assert_eq!(response.status(), StatusCode::OK);
        let snapshot = snapshot_response(response).await;
        assert_snapshot(
            "models_empty_when_all_providers_disabled",
            &snapshot,
            r#"
status=200
json.data_len=0
json.first_id=<none>
"#,
        );
    }

    #[tokio::test]
    async fn responses_non_stream_uses_resp_id_prefix() {
        let app = build_router(test_app_state(false));
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/responses")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"model":"deepseek/deepseek-chat","input":"hello","stream":false}"#,
                    ))
                    .expect("request must build"),
            )
            .await
            .expect("request must complete");

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body read must succeed");
        let payload: Value =
            serde_json::from_slice(&body).expect("response body must be valid json");
        let id = payload.get("id").and_then(Value::as_str).expect("id must be present");
        assert!(id.starts_with("resp_"), "unexpected id: {id}");
    }

    #[tokio::test]
    async fn chat_non_stream_uses_chatcmpl_id_prefix() {
        let app = build_router(test_app_state(false));
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/chat/completions")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"model":"deepseek/deepseek-chat","messages":[{"role":"user","content":"hello"}],"stream":false}"#,
                    ))
                    .expect("request must build"),
            )
            .await
            .expect("request must complete");

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body read must succeed");
        let payload: Value =
            serde_json::from_slice(&body).expect("response body must be valid json");
        let id = payload.get("id").and_then(Value::as_str).expect("id must be present");
        assert!(id.starts_with("chatcmpl_"), "unexpected id: {id}");
    }

    #[tokio::test]
    async fn chat_stream_emits_chatcmpl_id_and_done_marker() {
        let app = build_router(test_app_state(false));
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/chat/completions")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"model":"deepseek/deepseek-chat","messages":[{"role":"user","content":"hello world"}],"stream":true}"#,
                    ))
                    .expect("request must build"),
            )
            .await
            .expect("request must complete");

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body read must succeed");
        let payload = String::from_utf8_lossy(&body);
        assert!(payload.contains("\"id\":\"chatcmpl_"), "expected chatcmpl id in stream payload");
        assert!(payload.contains("[DONE]"), "expected done marker in stream payload");
    }

    #[tokio::test]
    async fn responses_tool_call_sets_finish_reason_and_tool_call_id() {
        let app = build_router(test_app_state(false));
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/responses")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"model":"deepseek/deepseek-chat","input":"TOOL_CALL:get_weather:{\"location\":\"New York\"}","stream":false}"#,
                    ))
                    .expect("request must build"),
            )
            .await
            .expect("request must complete");

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body read must succeed");
        let payload: Value =
            serde_json::from_slice(&body).expect("response body must be valid json");
        assert_eq!(payload.get("finish_reason").and_then(Value::as_str), Some("tool_calls"));
        let tool_call_id = payload
            .get("output")
            .and_then(Value::as_array)
            .and_then(|arr| {
                arr.iter().find(|item| {
                    item.as_object().and_then(|obj| obj.get("type")).and_then(Value::as_str)
                        == Some("function_call")
                })
            })
            .and_then(Value::as_object)
            .and_then(|obj| obj.get("call_id"))
            .and_then(Value::as_str)
            .unwrap_or("");
        assert!(tool_call_id.starts_with("call_"), "unexpected tool_call id: {tool_call_id}");
    }

    #[tokio::test]
    async fn responses_stream_emits_output_item_done_for_function_call() {
        let app = build_router(test_app_state(false));
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/responses")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"model":"deepseek/deepseek-chat","input":"TOOL_CALL:get_weather:{\"location\":\"New York\"}","stream":true}"#,
                    ))
                    .expect("request must build"),
            )
            .await
            .expect("request must complete");

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body read must succeed");
        let payload = String::from_utf8_lossy(&body);
        let mut function_call_done = false;
        for event_block in payload.split("\n\n") {
            if !event_block.contains("event: response.output_item.done") {
                continue;
            }
            let Some(data_line) = event_block.lines().find(|line| line.starts_with("data: "))
            else {
                continue;
            };
            let data = data_line.trim_start_matches("data: ");
            let Ok(value) = serde_json::from_str::<Value>(data) else {
                continue;
            };
            let item_type = value
                .get("item")
                .and_then(Value::as_object)
                .and_then(|item| item.get("type"))
                .and_then(Value::as_str);
            if item_type == Some("function_call") {
                function_call_done = true;
                break;
            }
        }
        assert!(
            function_call_done,
            "expected response.output_item.done with item.type=function_call, payload={payload}"
        );
        assert!(
            payload.contains("event: response.completed"),
            "stream must end with response.completed"
        );
    }

    #[tokio::test]
    async fn chat_non_stream_maps_tool_call_to_choice_message() {
        let app = build_router(test_app_state(false));
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/chat/completions")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"model":"deepseek/deepseek-chat","messages":[{"role":"user","content":"TOOL_CALL:get_weather:{\"location\":\"New York\"}"}],"stream":false}"#,
                    ))
                    .expect("request must build"),
            )
            .await
            .expect("request must complete");

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body read must succeed");
        let payload: Value =
            serde_json::from_slice(&body).expect("response body must be valid json");
        assert_eq!(
            payload
                .get("choices")
                .and_then(Value::as_array)
                .and_then(|arr| arr.first())
                .and_then(Value::as_object)
                .and_then(|choice| choice.get("finish_reason"))
                .and_then(Value::as_str),
            Some("tool_calls")
        );
        let tool_call_id = payload
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|arr| arr.first())
            .and_then(Value::as_object)
            .and_then(|choice| choice.get("message"))
            .and_then(Value::as_object)
            .and_then(|message| message.get("tool_calls"))
            .and_then(Value::as_array)
            .and_then(|arr| arr.first())
            .and_then(Value::as_object)
            .and_then(|obj| obj.get("id"))
            .and_then(Value::as_str)
            .unwrap_or("");
        assert!(tool_call_id.starts_with("call_"), "unexpected tool_call id: {tool_call_id}");
    }

    #[tokio::test]
    async fn responses_reasoner_model_returns_reasoning_field() {
        let app = build_router(test_app_state(false));
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/responses")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"model":"deepseek/deepseek-reasoner","input":"Solve 2+2 briefly","stream":false}"#,
                    ))
                    .expect("request must build"),
            )
            .await
            .expect("request must complete");

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body read must succeed");
        let payload: Value =
            serde_json::from_slice(&body).expect("response body must be valid json");
        let reasoning = payload
            .get("output")
            .and_then(Value::as_array)
            .and_then(|arr| {
                arr.iter().find(|item| {
                    item.as_object().and_then(|obj| obj.get("type")).and_then(Value::as_str)
                        == Some("reasoning")
                })
            })
            .and_then(Value::as_object)
            .and_then(|obj| obj.get("summary"))
            .and_then(Value::as_array)
            .and_then(|arr| arr.first())
            .and_then(Value::as_object)
            .and_then(|obj| obj.get("text"))
            .and_then(Value::as_str)
            .unwrap_or("");
        assert!(!reasoning.is_empty(), "expected reasoning for deepseek-reasoner");
    }

    #[tokio::test]
    async fn chat_reasoner_model_maps_reasoning_to_message_field() {
        let app = build_router(test_app_state(false));
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/chat/completions")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"model":"deepseek/deepseek-reasoner","messages":[{"role":"user","content":"Solve 2+2 briefly"}],"stream":false}"#,
                    ))
                    .expect("request must build"),
            )
            .await
            .expect("request must complete");

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body read must succeed");
        let payload: Value =
            serde_json::from_slice(&body).expect("response body must be valid json");
        let reasoning = payload
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|arr| arr.first())
            .and_then(Value::as_object)
            .and_then(|choice| choice.get("message"))
            .and_then(Value::as_object)
            .and_then(|message| message.get("reasoning"))
            .and_then(Value::as_str)
            .unwrap_or("");
        assert!(!reasoning.is_empty(), "expected reasoning in chat message for reasoner model");
    }

    #[test]
    fn parse_bearer_token_accepts_case_insensitive_scheme() {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            "bEaReR test-token".parse().expect("header value must parse"),
        );
        assert_eq!(crate::http::auth::parse_bearer_token(&headers).as_deref(), Some("test-token"));
    }

    #[test]
    fn resolve_byok_bearer_requires_authorization_header() {
        let headers = HeaderMap::new();
        let result =
            crate::http::auth::resolve_byok_bearer(&headers, true, "deepseek", "/api/v1/responses");
        assert!(matches!(result, Err(CoreError::Validation(_))));
    }

    #[test]
    fn resolve_byok_bearer_rejects_yandex_provider() {
        let headers = HeaderMap::new();
        let result =
            crate::http::auth::resolve_byok_bearer(&headers, true, "yandex", "/api/v1/responses");
        assert!(matches!(result, Err(CoreError::Validation(_))));
    }

    #[tokio::test]
    async fn byok_enabled_requires_bearer_header() {
        let mut config = crate::config::AppConfig::for_tests();
        config.byok_enabled = true;
        let app = build_router(AppState::from_config(&config));
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/responses")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"model":"deepseek/deepseek-chat","input":"hello","stream":false}"#,
                    ))
                    .expect("request must build"),
            )
            .await
            .expect("request must complete");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn byok_enabled_accepts_bearer_header() {
        let mut config = crate::config::AppConfig::for_tests();
        config.byok_enabled = true;
        let app = build_router(AppState::from_config(&config));
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/responses")
                    .header("content-type", "application/json")
                    .header(axum::http::header::AUTHORIZATION, "Bearer test-token")
                    .body(Body::from(
                        r#"{"model":"deepseek/deepseek-chat","input":"hello","stream":false}"#,
                    ))
                    .expect("request must build"),
            )
            .await
            .expect("request must complete");

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[test]
    fn error_response_returns_429_for_provider_overload() {
        let response = error_response(CoreError::Provider(
            "provider overloaded: max in-flight limit reached for deepseek".to_string(),
        ));
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    #[test]
    fn error_response_keeps_400_for_regular_provider_error() {
        let response =
            error_response(CoreError::Provider("provider request failed: timeout".to_string()));
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}
