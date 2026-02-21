use std::collections::HashMap;
use std::env;

#[derive(Debug, Clone)]
pub struct ProviderConfig {
    pub enabled: bool,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub host: String,
    pub port: u16,
    pub billing_enabled: bool,
    pub providers: HashMap<String, ProviderConfig>,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("invalid XR_PORT value: {0}")]
    InvalidPort(String),
    #[error("invalid XR_BILLING_ENABLED value: {0}")]
    InvalidBool(String),
}

impl AppConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        let host = env::var("XR_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());

        let port_raw = env::var("XR_PORT").unwrap_or_else(|_| "3000".to_string());
        let port =
            port_raw.parse::<u16>().map_err(|_| ConfigError::InvalidPort(port_raw.clone()))?;

        let billing_raw = env::var("XR_BILLING_ENABLED").unwrap_or_else(|_| "false".to_string());
        let billing_enabled = parse_bool(&billing_raw)
            .ok_or_else(|| ConfigError::InvalidBool(billing_raw.clone()))?;

        let providers = [
            provider_from_env("openai", "OPENAI"),
            provider_from_env("openrouter", "OPENROUTER"),
            provider_from_env("deepseek", "DEEPSEEK"),
            provider_from_env("gigachat", "GIGACHAT"),
            provider_from_env("yandex", "YANDEX"),
            provider_from_env("ollama", "OLLAMA"),
            provider_from_env("zai", "ZAI"),
            provider_from_env("agents", "AGENTS"),
            provider_from_env("xrouter", "XROUTER"),
        ]
        .into_iter()
        .collect::<HashMap<_, _>>();

        Ok(Self { host, port, billing_enabled, providers })
    }

    pub fn for_tests() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 3000,
            billing_enabled: false,
            providers: [
                (
                    "openai".to_string(),
                    ProviderConfig { enabled: true, api_key: None, base_url: None },
                ),
                (
                    "openrouter".to_string(),
                    ProviderConfig { enabled: true, api_key: None, base_url: None },
                ),
                (
                    "deepseek".to_string(),
                    ProviderConfig { enabled: true, api_key: None, base_url: None },
                ),
                (
                    "gigachat".to_string(),
                    ProviderConfig { enabled: true, api_key: None, base_url: None },
                ),
                (
                    "yandex".to_string(),
                    ProviderConfig { enabled: true, api_key: None, base_url: None },
                ),
                (
                    "ollama".to_string(),
                    ProviderConfig { enabled: true, api_key: None, base_url: None },
                ),
                (
                    "zai".to_string(),
                    ProviderConfig { enabled: true, api_key: None, base_url: None },
                ),
                (
                    "agents".to_string(),
                    ProviderConfig { enabled: true, api_key: None, base_url: None },
                ),
                (
                    "xrouter".to_string(),
                    ProviderConfig { enabled: true, api_key: None, base_url: None },
                ),
            ]
            .into_iter()
            .collect(),
        }
    }
}

fn provider_from_env(name: &str, prefix: &str) -> (String, ProviderConfig) {
    let enabled_var = format!("{prefix}_ENABLED");
    let enabled = env::var(enabled_var).ok().and_then(|v| parse_bool(&v)).unwrap_or(true);

    let api_key_var = format!("{prefix}_API_KEY");
    let base_url_var = format!("{prefix}_BASE_URL");

    let api_key = env::var(api_key_var).ok().filter(|v| !v.trim().is_empty());
    let base_url = env::var(base_url_var).ok().filter(|v| !v.trim().is_empty());

    (name.to_string(), ProviderConfig { enabled, api_key, base_url })
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}
