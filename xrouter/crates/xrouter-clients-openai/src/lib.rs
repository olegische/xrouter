#[cfg(not(target_arch = "wasm32"))]
mod clients;
pub mod parser;
pub mod protocol;
#[cfg(not(target_arch = "wasm32"))]
mod transport;

#[cfg(not(target_arch = "wasm32"))]
pub use clients::{
    DeepSeekClient, GigachatClient, MockProviderClient, OpenAiClient, OpenRouterClient,
    XrouterClient, YandexResponsesClient, ZaiClient,
};
#[cfg(not(target_arch = "wasm32"))]
pub use transport::{build_http_client, build_http_client_insecure_tls};
