use thiserror::Error;

#[derive(Debug, Error)]
pub enum BrowserError {
    #[error("browser runtime is only available on wasm32")]
    UnsupportedPlatform,
    #[error("request build failed: {0}")]
    InvalidRequest(&'static str),
    #[error("provider `{0}` is not supported in the browser runtime yet")]
    UnsupportedProvider(String),
    #[error("browser window is not available")]
    MissingWindow,
    #[error("browser fetch failed: {0}")]
    Fetch(String),
    #[error("provider responded with HTTP {status}: {body}")]
    HttpStatus { status: u16, body: String },
    #[error("response text read failed: {0}")]
    ResponseBody(String),
    #[error("response parse failed: {0}")]
    Parse(String),
}
