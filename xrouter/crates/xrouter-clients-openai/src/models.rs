use std::collections::{HashMap, HashSet};

use serde::Deserialize;
use xrouter_core::ModelDescriptor;

#[derive(Debug, Deserialize)]
pub struct OpenRouterModelsResponse {
    #[serde(default)]
    pub data: Vec<OpenRouterModelData>,
}

#[derive(Debug, Deserialize)]
pub struct OpenRouterModelData {
    pub id: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub context_length: u32,
    #[serde(default)]
    pub architecture: OpenRouterArchitecture,
    #[serde(default)]
    pub top_provider: OpenRouterTopProvider,
}

#[derive(Debug, Deserialize)]
pub struct OpenRouterArchitecture {
    #[serde(default = "default_modality")]
    pub modality: String,
    #[serde(default)]
    pub tokenizer: Option<String>,
    #[serde(default)]
    pub instruct_type: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct OpenRouterTopProvider {
    pub context_length: Option<u32>,
    pub max_completion_tokens: Option<u32>,
    pub is_moderated: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct ProviderModelsResponse {
    #[serde(default)]
    pub data: Vec<ProviderModelEntry>,
}

#[derive(Debug, Deserialize)]
pub struct ProviderModelEntry {
    pub id: String,
}

#[derive(Debug, Deserialize)]
pub struct XrouterProviderModelsResponse {
    #[serde(default)]
    pub data: Vec<XrouterProviderModelEntry>,
}

#[derive(Debug, Default, Deserialize)]
pub struct XrouterProviderModelEntry {
    pub id: String,
    #[serde(default)]
    pub context_length: u32,
    #[serde(default)]
    pub max_model_len: u32,
    #[serde(default)]
    pub metadata: XrouterProviderModelMetadata,
}

#[derive(Debug, Default, Deserialize)]
pub struct XrouterProviderModelMetadata {
    #[serde(default)]
    pub company: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub endpoints: Vec<XrouterProviderEndpoint>,
    #[serde(default, rename = "type")]
    pub model_type: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct XrouterProviderEndpoint {
    #[serde(default)]
    pub path: String,
}

pub fn map_openrouter_models(
    payload: OpenRouterModelsResponse,
    supported_ids: &[String],
) -> Vec<ModelDescriptor> {
    let supported = supported_ids.iter().cloned().collect::<HashSet<_>>();
    payload
        .data
        .into_iter()
        .filter(|model| supported.contains(&model.id))
        .map(|model| {
            let context_length = if model.context_length > 0 { model.context_length } else { 4096 };
            let top_context_length = model.top_provider.context_length.unwrap_or(context_length);
            let max_completion_tokens = model.top_provider.max_completion_tokens.unwrap_or(4096);
            ModelDescriptor {
                id: model.id.clone(),
                provider: "openrouter".to_string(),
                description: if model.description.is_empty() {
                    format!("{} via OpenRouter", model.id)
                } else {
                    model.description
                },
                context_length,
                tokenizer: model.architecture.tokenizer.unwrap_or_else(|| {
                    if model.id.contains("anthropic/") {
                        "anthropic".to_string()
                    } else if model.id.contains("google/") {
                        "google".to_string()
                    } else {
                        "unknown".to_string()
                    }
                }),
                instruct_type: model
                    .architecture
                    .instruct_type
                    .unwrap_or_else(|| "none".to_string()),
                modality: model.architecture.modality,
                top_provider_context_length: top_context_length,
                is_moderated: model.top_provider.is_moderated.unwrap_or(true),
                max_completion_tokens,
            }
        })
        .collect::<Vec<_>>()
}

pub fn fallback_openrouter_models(model_ids: &[String]) -> Vec<ModelDescriptor> {
    model_ids
        .iter()
        .map(|id| ModelDescriptor {
            id: id.clone(),
            provider: "openrouter".to_string(),
            description: format!("{id} via OpenRouter"),
            context_length: 128_000,
            tokenizer: if id.contains("anthropic/") {
                "anthropic".to_string()
            } else if id.contains("google/") {
                "google".to_string()
            } else {
                "unknown".to_string()
            },
            instruct_type: "none".to_string(),
            modality: "text->text".to_string(),
            top_provider_context_length: 128_000,
            is_moderated: true,
            max_completion_tokens: 16_384,
        })
        .collect()
}

pub fn extract_provider_model_ids(payload: ProviderModelsResponse) -> Vec<String> {
    payload.data.into_iter().map(|entry| entry.id).filter(|id| !id.trim().is_empty()).collect()
}

pub fn map_xrouter_models(payload: XrouterProviderModelsResponse) -> Vec<ModelDescriptor> {
    payload
        .data
        .into_iter()
        .filter(xrouter_supports_chat_generation)
        .map(|model| {
            let context_length = if model.context_length > 0 {
                model.context_length
            } else if model.max_model_len > 0 {
                model.max_model_len
            } else {
                128_000
            };
            let company = model.metadata.company.unwrap_or_else(|| "xrouter".to_string());
            let name = model.metadata.name.unwrap_or_else(|| model.id.clone());
            let model_type = model.metadata.model_type.unwrap_or_else(|| "llm".to_string());

            ModelDescriptor {
                id: model.id,
                provider: "xrouter".to_string(),
                description: format!("{name} via xrouter ({company})"),
                context_length,
                tokenizer: "unknown".to_string(),
                instruct_type: "none".to_string(),
                modality: map_xrouter_modality(&model_type),
                top_provider_context_length: context_length,
                is_moderated: true,
                max_completion_tokens: 8_192,
            }
        })
        .collect()
}

pub fn build_models_from_registry(
    provider: &str,
    provider_model_ids: &[String],
    registry_seed: &[ModelDescriptor],
) -> Vec<ModelDescriptor> {
    let registry = registry_seed
        .iter()
        .filter(|model| model.provider == provider)
        .map(|model| (model.id.clone(), model.clone()))
        .collect::<HashMap<_, _>>();

    provider_model_ids
        .iter()
        .map(|id| {
            if let Some(template) = registry.get(id) {
                let mut model = template.clone();
                model.id = id.clone();
                model
            } else if provider == "zai" {
                zai_fallback_model_descriptor(id)
            } else if provider == "yandex" {
                yandex_fallback_model_descriptor(id)
            } else {
                ModelDescriptor {
                    id: id.clone(),
                    provider: provider.to_string(),
                    description: format!("{id} via {provider}"),
                    context_length: 128_000,
                    tokenizer: "unknown".to_string(),
                    instruct_type: "none".to_string(),
                    modality: "text->text".to_string(),
                    top_provider_context_length: 128_000,
                    is_moderated: true,
                    max_completion_tokens: 8_192,
                }
            }
        })
        .collect()
}

fn default_modality() -> String {
    "text->text".to_string()
}

impl Default for OpenRouterArchitecture {
    fn default() -> Self {
        Self { modality: default_modality(), tokenizer: None, instruct_type: None }
    }
}

fn xrouter_supports_chat_generation(model: &XrouterProviderModelEntry) -> bool {
    model.metadata.endpoints.iter().any(|endpoint| {
        endpoint.path == "/v1/chat/completions" || endpoint.path == "/v1/completions"
    })
}

fn map_xrouter_modality(model_type: &str) -> String {
    match model_type {
        "image+text-to-text" => "image+text->text".to_string(),
        "audio-to-text" => "audio->text".to_string(),
        _ => "text->text".to_string(),
    }
}

fn zai_fallback_model_descriptor(id: &str) -> ModelDescriptor {
    let (context_length, max_completion_tokens, description) = match id {
        "glm-4.5" => (
            128_000,
            98_304,
            "GLM-4.5 is Z.AI's flagship general model focused on strong coding, reasoning, and long-context agent workflows.".to_string(),
        ),
        "glm-4.5-air" => (
            128_000,
            98_304,
            "GLM-4.5-Air is a lighter GLM-4.5 variant aimed at lower-latency interactive and agent tasks.".to_string(),
        ),
        "glm-4.6" => (
            200_000,
            128_000,
            "GLM-4.6 extends GLM with larger context and output budgets for long-horizon reasoning and implementation tasks.".to_string(),
        ),
        "glm-4.7" => (
            200_000,
            128_000,
            "GLM-4.7 improves stability for multi-step execution, coding, and structured planning over prior GLM generations.".to_string(),
        ),
        "glm-5" => (
            200_000,
            128_000,
            "GLM-5 is Z.AI's latest high-capacity model for complex systems design, agent orchestration, and long-context coding work.".to_string(),
        ),
        _ => (128_000, 8_192, format!("{id} via zai")),
    };

    ModelDescriptor {
        id: id.to_string(),
        provider: "zai".to_string(),
        description,
        context_length,
        tokenizer: "unknown".to_string(),
        instruct_type: "none".to_string(),
        modality: "text->text".to_string(),
        top_provider_context_length: context_length,
        is_moderated: true,
        max_completion_tokens,
    }
}

fn yandex_fallback_model_descriptor(id: &str) -> ModelDescriptor {
    ModelDescriptor {
        id: id.to_string(),
        provider: "yandex".to_string(),
        description: format!("{id} via yandex"),
        context_length: 32_768,
        tokenizer: "unknown".to_string(),
        instruct_type: "none".to_string(),
        modality: "text->text".to_string(),
        top_provider_context_length: 32_768,
        is_moderated: true,
        max_completion_tokens: 8_192,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        OpenRouterModelsResponse, XrouterProviderModelsResponse, build_models_from_registry,
        map_openrouter_models, map_xrouter_models,
    };
    use serde_json::json;

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
}
