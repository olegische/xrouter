use async_trait::async_trait;
use js_sys::Function;
use serde::{Deserialize, Serialize};
use serde_wasm_bindgen::{from_value, to_value};
use std::collections::HashMap;
use wasm_bindgen::JsValue;
use wasm_bindgen::prelude::wasm_bindgen;
use xrouter_contracts::{ResponseEvent, ResponsesRequest};
use xrouter_core::{CoreError, ResponseEventSink};

use crate::{
    BrowserInferenceClient, BrowserModelDiscoveryClient, BrowserProvider, DEFAULT_DEMO_PROMPT,
};

#[derive(Debug, Serialize)]
struct BrowserRunResult {
    request_id: String,
    text: String,
    output_tokens: u32,
    reasoning: Option<String>,
    emitted_live: bool,
}

#[derive(Debug, Deserialize)]
struct BrowserResponsesStreamRequest {
    #[serde(flatten)]
    request: ResponsesRequest,
    #[serde(default)]
    headers: HashMap<String, String>,
}

struct JsCallbackSink {
    request_id: String,
    callback: Function,
}

impl JsCallbackSink {
    fn new(request_id: String, callback: Function) -> Self {
        Self { request_id, callback }
    }

    fn emit(&self, event: ResponseEvent) {
        if let Ok(value) = to_value(&event) {
            let _ = self.callback.call1(&JsValue::NULL, &value);
        }
    }
}

#[async_trait(?Send)]
impl ResponseEventSink for JsCallbackSink {
    async fn send(&self, event: Result<ResponseEvent, CoreError>) {
        match event {
            Ok(event) => self.emit(event),
            Err(error) => self.emit(ResponseEvent::ResponseError {
                id: self.request_id.clone(),
                message: error.to_string(),
            }),
        }
    }
}

#[wasm_bindgen]
pub struct WasmBrowserClient {
    provider: BrowserProvider,
    base_url: Option<String>,
    api_key: Option<String>,
    inference: BrowserInferenceClient,
}

#[wasm_bindgen]
impl WasmBrowserClient {
    #[wasm_bindgen(constructor)]
    pub fn new(
        provider: String,
        base_url: Option<String>,
        api_key: Option<String>,
    ) -> Result<WasmBrowserClient, JsValue> {
        let provider = BrowserProvider::try_from(provider.as_str())
            .map_err(|error| JsValue::from_str(&error.to_string()))?;
        let inference = BrowserInferenceClient::new(provider, base_url.clone(), api_key.clone());
        Ok(Self { provider, base_url, api_key, inference })
    }

    #[wasm_bindgen(js_name = fetchModelIds)]
    pub async fn fetch_model_ids(&self) -> Result<JsValue, JsValue> {
        let client = BrowserModelDiscoveryClient::new();
        let model_ids = match self.provider {
            BrowserProvider::OpenRouter => {
                client
                    .fetch_openrouter_model_ids(self.base_url.as_deref(), self.api_key.as_deref())
                    .await
            }
            _ => {
                client
                    .fetch_provider_model_ids(
                        self.provider.as_str(),
                        self.base_url.as_deref(),
                        self.api_key.as_deref(),
                        None,
                    )
                    .await
            }
        }
        .map_err(|error| JsValue::from_str(&error.to_string()))?;
        to_value(&model_ids).map_err(|error| JsValue::from_str(&error.to_string()))
    }

    #[wasm_bindgen(js_name = runDemoPromptStream)]
    pub async fn run_demo_prompt_stream(
        &self,
        request_id: String,
        model: String,
        on_event: Function,
    ) -> Result<JsValue, JsValue> {
        self.run_text_stream(request_id, model, DEFAULT_DEMO_PROMPT.to_string(), on_event).await
    }

    #[wasm_bindgen(js_name = runTextStream)]
    pub async fn run_text_stream(
        &self,
        request_id: String,
        model: String,
        input: String,
        on_event: Function,
    ) -> Result<JsValue, JsValue> {
        let sink = JsCallbackSink::new(request_id.clone(), on_event);
        let outcome = self
            .inference
            .generate_text_stream(&request_id, &model, &input, Some(&sink))
            .await
            .map_err(|error| JsValue::from_str(&error.to_string()))?;
        let result = BrowserRunResult {
            request_id,
            text: outcome.chunks.join(""),
            output_tokens: outcome.output_tokens,
            reasoning: outcome.reasoning,
            emitted_live: outcome.emitted_live,
        };
        to_value(&result).map_err(|error| JsValue::from_str(&error.to_string()))
    }

    #[wasm_bindgen(js_name = runResponsesStream)]
    pub async fn run_responses_stream(
        &self,
        request_id: String,
        request: JsValue,
        on_event: Function,
    ) -> Result<JsValue, JsValue> {
        let request: BrowserResponsesStreamRequest =
            from_value(request).map_err(|error| JsValue::from_str(&error.to_string()))?;
        let sink = JsCallbackSink::new(request_id.clone(), on_event);
        let response = self
            .inference
            .generate_responses_stream_with_headers(
                &request_id,
                &request.request,
                Some(&request.headers),
                Some(&sink),
            )
            .await
            .map_err(|error| JsValue::from_str(&error.to_string()))?;
        to_value(&response).map_err(|error| JsValue::from_str(&error.to_string()))
    }

    #[wasm_bindgen]
    pub fn cancel(&self, request_id: String) -> Result<(), JsValue> {
        self.inference.cancel(&request_id).map_err(|error| JsValue::from_str(&error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::BrowserResponsesStreamRequest;

    #[test]
    fn browser_responses_stream_request_accepts_legacy_payload_without_headers() {
        let request: BrowserResponsesStreamRequest = serde_json::from_value(json!({
            "model": "openrouter/openai/gpt-5-mini",
            "input": "hello",
            "stream": true
        }))
        .expect("legacy browser request should deserialize");

        assert_eq!(request.request.model, "openrouter/openai/gpt-5-mini");
        assert!(request.headers.is_empty());
    }

    #[test]
    fn browser_responses_stream_request_accepts_optional_headers() {
        let request: BrowserResponsesStreamRequest = serde_json::from_value(json!({
            "model": "openrouter/openai/gpt-5-mini",
            "input": "hello",
            "stream": true,
            "headers": {
                "HTTP-Referer": "https://xcodex.chat",
                "X-OpenRouter-Title": "XCodex"
            }
        }))
        .expect("browser request with headers should deserialize");

        assert_eq!(
            request.headers.get("HTTP-Referer").map(String::as_str),
            Some("https://xcodex.chat")
        );
        assert_eq!(request.headers.get("X-OpenRouter-Title").map(String::as_str), Some("XCodex"));
    }
}
