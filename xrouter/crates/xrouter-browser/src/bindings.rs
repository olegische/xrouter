use async_trait::async_trait;
use js_sys::Function;
use serde::Serialize;
use serde_wasm_bindgen::to_value;
use wasm_bindgen::JsValue;
use wasm_bindgen::prelude::wasm_bindgen;
use xrouter_contracts::ResponseEvent;
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
        let model_ids = client
            .fetch_provider_model_ids(
                self.provider.as_str(),
                self.base_url.as_deref(),
                self.api_key.as_deref(),
                None,
            )
            .await
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

    #[wasm_bindgen]
    pub fn cancel(&self, request_id: String) -> Result<(), JsValue> {
        self.inference.cancel(&request_id).map_err(|error| JsValue::from_str(&error.to_string()))
    }
}
