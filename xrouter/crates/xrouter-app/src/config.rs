use std::collections::HashMap;
use std::env;

pub const DEFAULT_OPENROUTER_SUPPORTED_MODELS: &[&str] = &[
    "anthropic/claude-haiku-4.5",
    "anthropic/claude-opus-4.5",
    "anthropic/claude-opus-4.6",
    "anthropic/claude-sonnet-4.5",
    "anthropic/claude-sonnet-4.6",
    "deepseek/deepseek-r1",
    "deepseek/deepseek-r1-0528",
    "deepseek/deepseek-r1-0528:free",
    "deepseek/deepseek-v3.2",
    "deepseek/deepseek-v3.2-exp",
    "deepseek/deepseek-v3.2-speciale",
    "google/gemini-2.5-flash",
    "google/gemini-2.5-flash-image",
    "google/gemini-2.5-flash-lite",
    "google/gemini-2.5-flash-lite-preview-09-2025",
    "google/gemini-2.5-pro",
    "google/gemini-2.5-pro-preview",
    "google/gemini-2.5-pro-preview-05-06",
    "google/gemini-3-flash-preview",
    "google/gemini-3-pro-image-preview",
    "google/gemini-3-pro-preview",
    "google/gemini-3.1-pro-preview",
    "minimax/minimax-m2",
    "minimax/minimax-m2-her",
    "minimax/minimax-m2.1",
    "minimax/minimax-m2.5",
    "moonshotai/kimi-k2",
    "moonshotai/kimi-k2-0905",
    "moonshotai/kimi-k2-0905:exacto",
    "moonshotai/kimi-k2-thinking",
    "moonshotai/kimi-k2.5",
    "openai/gpt-5.2",
    "openai/gpt-5.2-chat",
    "openai/gpt-5.2-codex",
    "openai/gpt-5.2-pro",
    "x-ai/grok-4",
    "x-ai/grok-4-fast",
    "x-ai/grok-4.1-fast",
    "z-ai/glm-4.7",
    "z-ai/glm-4.7-flash",
    "z-ai/glm-5",
];
pub const DEFAULT_GIGACHAT_SUPPORTED_MODELS: &[&str] =
    &["gigachat/GigaChat-2", "gigachat/GigaChat-2-Max", "gigachat/GigaChat-2-Pro"];

#[derive(Debug, Clone)]
pub struct ProviderConfig {
    pub enabled: bool,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub project: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub host: String,
    pub port: u16,
    pub billing_enabled: bool,
    pub openai_compatible_api: bool,
    pub provider_timeout_seconds: u64,
    pub provider_max_inflight: usize,
    pub gigachat_insecure_tls: bool,
    pub openrouter_supported_models: Vec<String>,
    pub gigachat_supported_models: Vec<String>,
    pub providers: HashMap<String, ProviderConfig>,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("invalid XR_PORT value: {0}")]
    InvalidPort(String),
    #[error("invalid XR_BILLING_ENABLED value: {0}")]
    InvalidBool(String),
    #[error("invalid ENABLE_OPENAI_COMPATIBLE_API value: {0}")]
    InvalidOpenAiCompatibleApiBool(String),
    #[error("invalid XR_PROVIDER_TIMEOUT value: {0}")]
    InvalidProviderConnectTimeout(String),
    #[error("invalid XR_PROVIDER_MAX_INFLIGHT value: {0}")]
    InvalidProviderMaxInflight(String),
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

        let openai_compatible_raw =
            env::var("ENABLE_OPENAI_COMPATIBLE_API").unwrap_or_else(|_| "false".to_string());
        let openai_compatible_api = parse_bool(&openai_compatible_raw).ok_or_else(|| {
            ConfigError::InvalidOpenAiCompatibleApiBool(openai_compatible_raw.clone())
        })?;
        let provider_timeout_raw =
            env::var("XR_PROVIDER_TIMEOUT").unwrap_or_else(|_| "15".to_string());
        let provider_timeout_seconds = provider_timeout_raw.parse::<u64>().map_err(|_| {
            ConfigError::InvalidProviderConnectTimeout(provider_timeout_raw.clone())
        })?;
        let provider_max_inflight_raw =
            env::var("XR_PROVIDER_MAX_INFLIGHT").unwrap_or_else(|_| "100".to_string());
        let provider_max_inflight = parse_positive_usize(&provider_max_inflight_raw)
            .ok_or(ConfigError::InvalidProviderMaxInflight(provider_max_inflight_raw))?;
        let gigachat_insecure_tls =
            env::var("GIGACHAT_INSECURE_TLS").ok().and_then(|v| parse_bool(&v)).unwrap_or(false);
        let openrouter_supported_models = parse_string_list_env(
            "OPENROUTER_SUPPORTED_MODELS",
            DEFAULT_OPENROUTER_SUPPORTED_MODELS,
        );
        let gigachat_supported_models =
            parse_string_list_env("GIGACHAT_SUPPORTED_MODELS", DEFAULT_GIGACHAT_SUPPORTED_MODELS);

        let providers = [
            provider_from_env("openrouter", "OPENROUTER"),
            provider_from_env("deepseek", "DEEPSEEK"),
            provider_from_env("gigachat", "GIGACHAT"),
            provider_from_env("yandex", "YANDEX"),
            provider_from_env("ollama", "OLLAMA"),
            provider_from_env("zai", "ZAI"),
            provider_from_env("xrouter", "XROUTER"),
        ]
        .into_iter()
        .collect::<HashMap<_, _>>();

        Ok(Self {
            host,
            port,
            billing_enabled,
            openai_compatible_api,
            provider_timeout_seconds,
            provider_max_inflight,
            gigachat_insecure_tls,
            openrouter_supported_models,
            gigachat_supported_models,
            providers,
        })
    }

    pub fn for_tests() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 3000,
            billing_enabled: false,
            openai_compatible_api: false,
            provider_timeout_seconds: 15,
            provider_max_inflight: 100,
            gigachat_insecure_tls: false,
            openrouter_supported_models: DEFAULT_OPENROUTER_SUPPORTED_MODELS
                .iter()
                .map(|model| (*model).to_string())
                .collect(),
            gigachat_supported_models: DEFAULT_GIGACHAT_SUPPORTED_MODELS
                .iter()
                .map(|model| (*model).to_string())
                .collect(),
            providers: [
                (
                    "openrouter".to_string(),
                    ProviderConfig { enabled: true, api_key: None, base_url: None, project: None },
                ),
                (
                    "deepseek".to_string(),
                    ProviderConfig { enabled: true, api_key: None, base_url: None, project: None },
                ),
                (
                    "gigachat".to_string(),
                    ProviderConfig { enabled: true, api_key: None, base_url: None, project: None },
                ),
                (
                    "yandex".to_string(),
                    ProviderConfig { enabled: true, api_key: None, base_url: None, project: None },
                ),
                (
                    "ollama".to_string(),
                    ProviderConfig { enabled: true, api_key: None, base_url: None, project: None },
                ),
                (
                    "zai".to_string(),
                    ProviderConfig { enabled: true, api_key: None, base_url: None, project: None },
                ),
                (
                    "xrouter".to_string(),
                    ProviderConfig { enabled: true, api_key: None, base_url: None, project: None },
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
    let project_var = format!("{prefix}_PROJECT");

    let api_key = env::var(api_key_var).ok().filter(|v| !v.trim().is_empty());
    let base_url = env::var(base_url_var)
        .ok()
        .filter(|v| !v.trim().is_empty())
        .or_else(|| default_provider_base_url(name).map(ToString::to_string));
    let project = if name == "yandex" {
        env::var("YANDEX_FOLDER_ID")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .or_else(|| env::var(project_var).ok().filter(|v| !v.trim().is_empty()))
    } else {
        env::var(project_var).ok().filter(|v| !v.trim().is_empty())
    };

    (name.to_string(), ProviderConfig { enabled, api_key, base_url, project })
}

fn default_provider_base_url(provider: &str) -> Option<&'static str> {
    match provider {
        "deepseek" => Some("https://api.deepseek.com"),
        "openrouter" => Some("https://openrouter.ai/api/v1"),
        "gigachat" => Some("https://gigachat.devices.sberbank.ru/api/v1"),
        "zai" => Some("https://api.z.ai/api/paas/v4"),
        "yandex" => Some("https://ai.api.cloud.yandex.net/v1"),
        _ => None,
    }
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn parse_positive_usize(value: &str) -> Option<usize> {
    let parsed = value.trim().parse::<usize>().ok()?;
    if parsed == 0 { None } else { Some(parsed) }
}

fn parse_string_list_env(var_name: &str, default: &[&str]) -> Vec<String> {
    let Some(raw) = env::var(var_name).ok() else {
        return default.iter().map(|value| (*value).to_string()).collect();
    };
    parse_string_list(raw.trim(), default)
}

fn parse_string_list(trimmed: &str, default: &[&str]) -> Vec<String> {
    let fallback = || default.iter().map(|value| (*value).to_string()).collect::<Vec<_>>();
    if trimmed.is_empty() {
        return fallback();
    }
    if trimmed.starts_with('[') {
        if let Ok(parsed) = serde_json::from_str::<Vec<String>>(trimmed) {
            return parsed
                .into_iter()
                .map(|item| item.trim().to_string())
                .filter(|item| !item.is_empty())
                .collect();
        }
        return fallback();
    }

    let parsed = trimmed
        .split(',')
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect::<Vec<_>>();
    if parsed.is_empty() { fallback() } else { parsed }
}

#[cfg(test)]
mod tests {
    use super::{DEFAULT_OPENROUTER_SUPPORTED_MODELS, parse_positive_usize, parse_string_list};

    #[test]
    fn parse_string_list_accepts_json_array() {
        let parsed = parse_string_list(r#"["openai/gpt-5.2","anthropic/claude-sonnet-4.6"]"#, &[]);
        assert_eq!(parsed, vec!["openai/gpt-5.2", "anthropic/claude-sonnet-4.6"]);
    }

    #[test]
    fn parse_string_list_falls_back_on_invalid_json() {
        let parsed = parse_string_list("[not-json]", DEFAULT_OPENROUTER_SUPPORTED_MODELS);
        assert_eq!(parsed.len(), DEFAULT_OPENROUTER_SUPPORTED_MODELS.len());
        assert_eq!(parsed.first().map(String::as_str), Some("anthropic/claude-haiku-4.5"));
    }

    #[test]
    fn parse_positive_usize_accepts_positive_values() {
        assert_eq!(parse_positive_usize("100"), Some(100));
        assert_eq!(parse_positive_usize(" 7 "), Some(7));
    }

    #[test]
    fn parse_positive_usize_rejects_zero_and_invalid() {
        assert_eq!(parse_positive_usize("0"), None);
        assert_eq!(parse_positive_usize("abc"), None);
    }
}
