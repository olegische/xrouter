#[cfg(target_arch = "wasm32")]
mod bindings;
mod discovery;
mod error;
mod inference;
mod runtime;

#[cfg(target_arch = "wasm32")]
pub use bindings::WasmBrowserClient;
pub use discovery::BrowserModelDiscoveryClient;
pub use error::BrowserError;
pub use inference::{BrowserInferenceClient, BrowserProvider, DEFAULT_DEMO_PROMPT};
pub use runtime::BrowserProviderRuntime;
