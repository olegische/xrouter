use async_trait::async_trait;
#[cfg(target_arch = "wasm32")]
use js_sys::{Reflect, Uint8Array};
use serde_json::Value;
#[cfg(target_arch = "wasm32")]
use std::cell::RefCell;
#[cfg(target_arch = "wasm32")]
use std::collections::HashMap;
#[cfg(target_arch = "wasm32")]
use std::thread_local;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsValue;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen_futures::JsFuture;
#[cfg(target_arch = "wasm32")]
use web_sys::{
    AbortController, Headers, ReadableStreamDefaultReader, Request, RequestInit, Response, Window,
};
#[cfg(target_arch = "wasm32")]
use xrouter_contracts::ResponseEvent;
use xrouter_core::{CoreError, ProviderOutcome, ResponseEventSink};

use crate::error::BrowserError;
#[cfg(target_arch = "wasm32")]
use xrouter_clients_openai::parser::{
    ChatCompletionsResponse, ResponsesApiResponse, drain_sse_frames, extract_chat_delta_chunks,
    extract_chat_reasoning_delta, extract_responses_text_delta, map_chat_completion_response,
    map_chat_completion_stream_text, map_responses_api_response, map_responses_stream_text,
};
use xrouter_clients_openai::runtime::ProviderRuntime;

#[derive(Debug, Clone)]
pub struct BrowserProviderRuntime {
    base_url: Option<String>,
    api_key: Option<String>,
}

#[cfg(target_arch = "wasm32")]
#[derive(Debug)]
pub(crate) struct BrowserHttpResponse {
    pub(crate) body: String,
}

#[cfg(target_arch = "wasm32")]
thread_local! {
    static ACTIVE_REQUESTS: RefCell<HashMap<String, AbortController>> = RefCell::new(HashMap::new());
}

#[cfg(target_arch = "wasm32")]
struct ActiveRequestGuard {
    request_id: String,
}

#[cfg(target_arch = "wasm32")]
impl Drop for ActiveRequestGuard {
    fn drop(&mut self) {
        ACTIVE_REQUESTS.with(|requests: &RefCell<HashMap<String, AbortController>>| {
            requests.borrow_mut().remove(&self.request_id);
        });
    }
}

impl BrowserProviderRuntime {
    pub fn new(
        _provider_id: impl Into<String>,
        base_url: Option<String>,
        api_key: Option<String>,
    ) -> Self {
        Self { base_url, api_key }
    }

    fn api_key_ref(&self) -> Option<&str> {
        self.api_key.as_deref().filter(|value| !value.trim().is_empty())
    }

    fn base_url(&self) -> Result<&str, CoreError> {
        self.base_url
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| CoreError::Provider("provider base_url is not configured".to_string()))
    }

    fn provider_error(message: impl Into<String>) -> CoreError {
        CoreError::Provider(message.into())
    }

    pub fn cancel(&self, request_id: &str) -> Result<(), BrowserError> {
        #[cfg(target_arch = "wasm32")]
        {
            ACTIVE_REQUESTS.with(|requests: &RefCell<HashMap<String, AbortController>>| {
                if let Some(controller) = requests.borrow().get(request_id) {
                    controller.abort();
                }
            });
            Ok(())
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = request_id;
            Ok(())
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn map_stream_parse_failure(
        all_chunks: &[String],
        error: CoreError,
    ) -> Result<ProviderOutcome, CoreError> {
        if all_chunks.is_empty() {
            return Err(error);
        }
        let output_tokens =
            all_chunks.iter().map(|chunk| chunk.split_whitespace().count() as u32).sum::<u32>();
        Ok(ProviderOutcome {
            chunks: all_chunks.to_vec(),
            output_tokens,
            reasoning: None,
            reasoning_details: None,
            tool_calls: None,
            emitted_live: false,
        })
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl ProviderRuntime for BrowserProviderRuntime {
    fn api_key(&self) -> Option<String> {
        self.api_key_ref().map(ToString::to_string)
    }

    fn build_url(&self, path: &str) -> Result<String, CoreError> {
        let base_url = self.base_url()?.trim_end_matches('/');
        Ok(format!("{base_url}/{}", path.trim_start_matches('/')))
    }

    async fn post_chat_completions_stream(
        &self,
        request_id: &str,
        url: &str,
        payload: &Value,
        bearer_override: Option<&str>,
        extra_headers: &[(String, String)],
        sender: Option<&dyn ResponseEventSink>,
    ) -> Result<ProviderOutcome, CoreError> {
        post_chat_completions_stream_impl(
            self.api_key_ref(),
            request_id,
            url,
            payload,
            bearer_override,
            extra_headers,
            sender,
        )
        .await
    }

    async fn post_responses_stream(
        &self,
        request_id: &str,
        url: &str,
        payload: &Value,
        bearer_override: Option<&str>,
        extra_headers: &[(String, String)],
        sender: Option<&dyn ResponseEventSink>,
    ) -> Result<ProviderOutcome, CoreError> {
        post_responses_stream_impl(
            self.api_key_ref(),
            request_id,
            url,
            payload,
            bearer_override,
            extra_headers,
            sender,
        )
        .await
    }

    async fn post_form_json(
        &self,
        _url: &str,
        _form_fields: &[(String, String)],
        _headers: &[(String, String)],
    ) -> Result<Value, CoreError> {
        Err(Self::provider_error("browser form-post runtime is not implemented yet"))
    }
}

async fn post_chat_completions_stream_impl(
    api_key: Option<&str>,
    request_id: &str,
    url: &str,
    payload: &Value,
    bearer_override: Option<&str>,
    extra_headers: &[(String, String)],
    sender: Option<&dyn ResponseEventSink>,
) -> Result<ProviderOutcome, CoreError> {
    #[cfg(target_arch = "wasm32")]
    {
        let response =
            fetch_post(request_id, url, payload, bearer_override.or(api_key), extra_headers)
                .await?;
        if response.content_type.as_deref().is_some_and(|value| value.contains("application/json"))
        {
            let payload =
                serde_json::from_str::<ChatCompletionsResponse>(&response.body).map_err(|err| {
                    BrowserProviderRuntime::provider_error(format!(
                        "provider response parse failed: {err}"
                    ))
                })?;
            return map_chat_completion_response(payload);
        }

        let mut all_chunks = Vec::<String>::new();
        let mut parse_buffer = String::new();
        let mut full_body = String::new();
        let mut reader = stream_response_reader(response.response)?;

        while let Some(chunk) = read_reader_chunk(request_id, &mut reader).await? {
            let chunk = chunk.replace('\r', "");
            parse_buffer.push_str(&chunk);
            full_body.push_str(&chunk);
            for frame in drain_sse_frames(&mut parse_buffer, false) {
                for delta in extract_chat_delta_chunks(&frame, request_id)? {
                    if let Some(tx) = sender {
                        tx.send(Ok(ResponseEvent::OutputTextDelta {
                            id: request_id.to_string(),
                            delta: delta.clone(),
                        }))
                        .await;
                    }
                    all_chunks.push(delta);
                }
                if let Some(reasoning_delta) = extract_chat_reasoning_delta(&frame, request_id)?
                    && let Some(tx) = sender
                {
                    tx.send(Ok(ResponseEvent::ReasoningDelta {
                        id: request_id.to_string(),
                        delta: reasoning_delta,
                    }))
                    .await;
                }
            }
        }

        for frame in drain_sse_frames(&mut parse_buffer, true) {
            for delta in extract_chat_delta_chunks(&frame, request_id)? {
                if let Some(tx) = sender {
                    tx.send(Ok(ResponseEvent::OutputTextDelta {
                        id: request_id.to_string(),
                        delta: delta.clone(),
                    }))
                    .await;
                }
                all_chunks.push(delta);
            }
            if let Some(reasoning_delta) = extract_chat_reasoning_delta(&frame, request_id)?
                && let Some(tx) = sender
            {
                tx.send(Ok(ResponseEvent::ReasoningDelta {
                    id: request_id.to_string(),
                    delta: reasoning_delta,
                }))
                .await;
            }
        }

        let mut outcome = match map_chat_completion_stream_text(&full_body) {
            Ok(parsed) => parsed,
            Err(error) => BrowserProviderRuntime::map_stream_parse_failure(&all_chunks, error)?,
        };
        if !all_chunks.is_empty() {
            outcome.chunks = all_chunks;
        }
        outcome.emitted_live = sender.is_some();
        Ok(outcome)
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (api_key, request_id, url, payload, bearer_override, extra_headers, sender);
        Err(BrowserProviderRuntime::provider_error(BrowserError::UnsupportedPlatform.to_string()))
    }
}

async fn post_responses_stream_impl(
    api_key: Option<&str>,
    request_id: &str,
    url: &str,
    payload: &Value,
    bearer_override: Option<&str>,
    extra_headers: &[(String, String)],
    sender: Option<&dyn ResponseEventSink>,
) -> Result<ProviderOutcome, CoreError> {
    #[cfg(target_arch = "wasm32")]
    {
        let response =
            fetch_post(request_id, url, payload, bearer_override.or(api_key), extra_headers)
                .await?;
        if response.content_type.as_deref().is_some_and(|value| value.contains("application/json"))
        {
            let payload =
                serde_json::from_str::<ResponsesApiResponse>(&response.body).map_err(|err| {
                    BrowserProviderRuntime::provider_error(format!(
                        "provider response parse failed: {err}"
                    ))
                })?;
            return map_responses_api_response(payload);
        }

        let mut all_chunks = Vec::<String>::new();
        let mut parse_buffer = String::new();
        let mut full_body = String::new();
        let mut reader = stream_response_reader(response.response)?;

        while let Some(chunk) = read_reader_chunk(request_id, &mut reader).await? {
            let chunk = chunk.replace('\r', "");
            parse_buffer.push_str(&chunk);
            full_body.push_str(&chunk);
            for frame in drain_sse_frames(&mut parse_buffer, false) {
                if let Some(delta) = extract_responses_text_delta(&frame)? {
                    if let Some(tx) = sender {
                        tx.send(Ok(ResponseEvent::OutputTextDelta {
                            id: request_id.to_string(),
                            delta: delta.clone(),
                        }))
                        .await;
                    }
                    all_chunks.push(delta);
                }
            }
        }

        for frame in drain_sse_frames(&mut parse_buffer, true) {
            if let Some(delta) = extract_responses_text_delta(&frame)? {
                if let Some(tx) = sender {
                    tx.send(Ok(ResponseEvent::OutputTextDelta {
                        id: request_id.to_string(),
                        delta: delta.clone(),
                    }))
                    .await;
                }
                all_chunks.push(delta);
            }
        }

        let mut outcome = match map_responses_stream_text(&full_body) {
            Ok(parsed) => parsed,
            Err(error) => BrowserProviderRuntime::map_stream_parse_failure(&all_chunks, error)?,
        };
        if !all_chunks.is_empty() {
            outcome.chunks = all_chunks;
        }
        outcome.emitted_live = sender.is_some();
        Ok(outcome)
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (api_key, request_id, url, payload, bearer_override, extra_headers, sender);
        Err(BrowserProviderRuntime::provider_error(BrowserError::UnsupportedPlatform.to_string()))
    }
}

#[cfg(target_arch = "wasm32")]
pub(crate) struct WasmFetchResponse {
    pub(crate) response: Response,
    pub(crate) content_type: Option<String>,
    pub(crate) body: String,
    _guard: Option<ActiveRequestGuard>,
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn fetch_get_text(
    request: &xrouter_clients_openai::model_discovery::HttpJsonRequest,
) -> Result<BrowserHttpResponse, BrowserError> {
    let response =
        send_request("model-discovery", "GET", &request.url, None, None, &request.headers).await?;
    Ok(BrowserHttpResponse { body: response.body })
}

#[cfg(target_arch = "wasm32")]
async fn fetch_post(
    request_id: &str,
    url: &str,
    payload: &Value,
    bearer: Option<&str>,
    extra_headers: &[(String, String)],
) -> Result<WasmFetchResponse, CoreError> {
    let body = serde_json::to_string(payload).map_err(|err| {
        BrowserProviderRuntime::provider_error(format!("request serialization failed: {err}"))
    })?;
    send_request(request_id, "POST", url, Some(&body), bearer, extra_headers)
        .await
        .map_err(|error| BrowserProviderRuntime::provider_error(error.to_string()))
}

#[cfg(target_arch = "wasm32")]
async fn send_request(
    request_id: &str,
    method: &str,
    url: &str,
    body: Option<&str>,
    bearer: Option<&str>,
    extra_headers: &[(String, String)],
) -> Result<WasmFetchResponse, BrowserError> {
    let window: Window = web_sys::window().ok_or(BrowserError::MissingWindow)?;
    let init = RequestInit::new();
    init.set_method(method);
    let controller = if method == "POST" {
        let controller =
            AbortController::new().map_err(|err| BrowserError::Fetch(format!("{err:?}")))?;
        init.set_signal(Some(&controller.signal()));
        Some(controller)
    } else {
        None
    };
    let guard = controller
        .as_ref()
        .map(|controller| register_active_request(request_id, controller))
        .transpose()?;
    if let Some(body) = body {
        init.set_body(&JsValue::from_str(body));
    }

    let request = Request::new_with_str_and_init(url, &init)
        .map_err(|err| BrowserError::Fetch(format!("{err:?}")))?;
    let headers: Headers = request.headers();
    if body.is_some() {
        headers
            .set("Content-Type", "application/json")
            .map_err(|err| BrowserError::Fetch(format!("{err:?}")))?;
    }
    if let Some(token) = bearer.filter(|value| !value.trim().is_empty()) {
        headers
            .set("Authorization", &format!("Bearer {token}"))
            .map_err(|err| BrowserError::Fetch(format!("{err:?}")))?;
    }
    for (name, value) in extra_headers {
        headers.set(name, value).map_err(|err| BrowserError::Fetch(format!("{err:?}")))?;
    }

    let response = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|err| map_fetch_error(request_id, err))?;
    let response: Response = response.dyn_into().map_err(|err| map_fetch_error(request_id, err))?;
    let content_type =
        response.headers().get("content-type").map_err(|err| map_fetch_error(request_id, err))?;

    if !response.ok() {
        let body = read_response_text(&response).await?;
        return Err(BrowserError::HttpStatus { status: response.status(), body });
    }

    let body = if content_type.as_deref().is_some_and(|value| value.contains("application/json")) {
        read_response_text(&response).await?
    } else {
        String::new()
    };
    Ok(WasmFetchResponse { response, content_type, body, _guard: guard })
}

#[cfg(target_arch = "wasm32")]
async fn read_response_text(response: &Response) -> Result<String, BrowserError> {
    let body = JsFuture::from(
        response.text().map_err(|err| BrowserError::ResponseBody(format!("{err:?}")))?,
    )
    .await
    .map_err(|err| BrowserError::ResponseBody(format!("{err:?}")))?;
    body.as_string()
        .ok_or_else(|| BrowserError::ResponseBody("response text is not a string".to_string()))
}

#[cfg(target_arch = "wasm32")]
fn stream_response_reader(response: Response) -> Result<ReadableStreamDefaultReader, CoreError> {
    let stream = response.body().ok_or_else(|| {
        BrowserProviderRuntime::provider_error("provider response body stream is not available")
    })?;
    stream.get_reader().dyn_into::<ReadableStreamDefaultReader>().map_err(|err| {
        BrowserProviderRuntime::provider_error(format!("stream reader init failed: {err:?}"))
    })
}

#[cfg(target_arch = "wasm32")]
async fn read_reader_chunk(
    request_id: &str,
    reader: &mut ReadableStreamDefaultReader,
) -> Result<Option<String>, CoreError> {
    let result = JsFuture::from(reader.read()).await.map_err(|err| {
        BrowserProviderRuntime::provider_error(
            map_browser_error(&err, request_id, "provider stream read failed").to_string(),
        )
    })?;
    let done = Reflect::get(&result, &JsValue::from_str("done"))
        .map_err(|err| {
            BrowserProviderRuntime::provider_error(format!("stream read decode failed: {err:?}"))
        })?
        .as_bool()
        .unwrap_or(false);
    if done {
        return Ok(None);
    }
    let value = Reflect::get(&result, &JsValue::from_str("value")).map_err(|err| {
        BrowserProviderRuntime::provider_error(format!("stream value decode failed: {err:?}"))
    })?;
    let bytes = Uint8Array::new(&value);
    let mut out = vec![0u8; bytes.length() as usize];
    bytes.copy_to(&mut out);
    Ok(Some(String::from_utf8_lossy(&out).into_owned()))
}

#[cfg(target_arch = "wasm32")]
fn register_active_request(
    request_id: &str,
    controller: &AbortController,
) -> Result<ActiveRequestGuard, BrowserError> {
    ACTIVE_REQUESTS.with(|requests: &RefCell<HashMap<String, AbortController>>| {
        let mut requests = requests.borrow_mut();
        if requests.contains_key(request_id) {
            return Err(BrowserError::RequestConflict(request_id.to_string()));
        }
        requests.insert(request_id.to_string(), controller.clone());
        Ok(ActiveRequestGuard { request_id: request_id.to_string() })
    })
}

#[cfg(target_arch = "wasm32")]
fn map_fetch_error(request_id: &str, err: JsValue) -> BrowserError {
    map_browser_error(&err, request_id, "browser fetch failed")
}

#[cfg(target_arch = "wasm32")]
fn map_browser_error(err: &JsValue, request_id: &str, fallback: &str) -> BrowserError {
    if is_abort_error(err) {
        BrowserError::Canceled(request_id.to_string())
    } else {
        BrowserError::Fetch(format!("{fallback}: {err:?}"))
    }
}

#[cfg(target_arch = "wasm32")]
fn is_abort_error(err: &JsValue) -> bool {
    Reflect::get(err, &JsValue::from_str("name"))
        .ok()
        .and_then(|value| value.as_string())
        .is_some_and(|name| name == "AbortError")
}

#[cfg(test)]
mod tests {
    use xrouter_core::CoreError;

    use super::BrowserProviderRuntime;
    use xrouter_clients_openai::runtime::ProviderRuntime;

    #[test]
    fn build_url_joins_base_and_path() {
        let runtime = BrowserProviderRuntime::new(
            "deepseek",
            Some("https://api.deepseek.com/".to_string()),
            Some("test".to_string()),
        );
        let url = ProviderRuntime::build_url(&runtime, "/chat/completions").expect("url");
        assert_eq!(url, "https://api.deepseek.com/chat/completions");
    }

    #[test]
    fn native_runtime_reports_unsupported_platform() {
        let runtime = BrowserProviderRuntime::new(
            "deepseek",
            Some("https://api.deepseek.com".to_string()),
            Some("test".to_string()),
        );
        let payload = serde_json::json!({ "model": "deepseek-chat" });
        let result = futures::executor::block_on(ProviderRuntime::post_chat_completions_stream(
            &runtime,
            "request",
            "https://api.deepseek.com/chat/completions",
            &payload,
            None,
            &[],
            None,
        ));
        assert!(matches!(result, Err(CoreError::Provider(message)) if message.contains("wasm32")));
    }

    #[test]
    fn native_cancel_is_idempotent() {
        let runtime = BrowserProviderRuntime::new(
            "deepseek",
            Some("https://api.deepseek.com".to_string()),
            Some("test".to_string()),
        );
        runtime.cancel("request-1").expect("cancel should be idempotent");
    }
}
