mod clients;
pub mod model_discovery;
pub mod models;
pub mod parser;
pub mod protocol;
pub mod runtime;
#[cfg(not(target_arch = "wasm32"))]
mod transport;

#[cfg(not(target_arch = "wasm32"))]
pub use clients::GigachatClient;
#[cfg(not(target_arch = "wasm32"))]
pub use clients::YandexResponsesClient;
pub use clients::{
    DeepSeekClient, MockProviderClient, OpenAiClient, OpenRouterClient, XrouterClient, ZaiClient,
};
#[cfg(not(target_arch = "wasm32"))]
pub use transport::{build_http_client, build_http_client_insecure_tls};
