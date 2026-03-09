use std::{sync::Arc, time::Duration};

use serde::Deserialize;
use tracing::warn;
use ureq::rustls::{
    self,
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    pki_types::{CertificateDer, ServerName, UnixTime},
};
use xrouter_clients_openai::models::{
    OpenRouterModelsResponse, ProviderModelsResponse, XrouterProviderModelsResponse,
    extract_provider_model_ids, map_openrouter_models, map_xrouter_models,
};
use xrouter_core::ModelDescriptor;

use crate::config;

#[derive(Debug, Deserialize)]
struct GigachatOauthResponse {
    #[serde(alias = "tok")]
    access_token: String,
}

const GIGACHAT_OAUTH_URL: &str = "https://ngw.devices.sberbank.ru:9443/api/v2/oauth";
const GIGACHAT_SCOPE: &str = "GIGACHAT_API_PERS";

#[derive(Debug)]
struct AcceptAllCerts;

impl ServerCertVerifier for AcceptAllCerts {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
            rustls::SignatureScheme::ECDSA_NISTP521_SHA512,
            rustls::SignatureScheme::ED25519,
            rustls::SignatureScheme::ED448,
            rustls::SignatureScheme::RSA_PSS_SHA256,
            rustls::SignatureScheme::RSA_PSS_SHA384,
            rustls::SignatureScheme::RSA_PSS_SHA512,
            rustls::SignatureScheme::RSA_PKCS1_SHA256,
            rustls::SignatureScheme::RSA_PKCS1_SHA384,
            rustls::SignatureScheme::RSA_PKCS1_SHA512,
        ]
    }
}

fn gigachat_ureq_agent(connect_timeout_seconds: u64, insecure_tls: bool) -> ureq::Agent {
    let builder =
        ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(connect_timeout_seconds));
    if insecure_tls {
        let tls_config = rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(AcceptAllCerts))
            .with_no_client_auth();
        builder.tls_config(Arc::new(tls_config)).build()
    } else {
        builder.build()
    }
}

pub(crate) fn fetch_openrouter_models(
    provider_config: &config::ProviderConfig,
    supported_ids: &[String],
    connect_timeout_seconds: u64,
) -> Option<Vec<ModelDescriptor>> {
    let base_url = provider_config
        .base_url
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("https://openrouter.ai/api/v1")
        .trim_end_matches('/')
        .to_string();
    if base_url.is_empty() {
        return None;
    }

    let url = format!("{base_url}/models");
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(connect_timeout_seconds))
        .build();
    let mut request = agent.get(url.as_str()).set("Accept", "application/json");
    if let Some(api_key) = provider_config.api_key.as_deref() {
        request = request.set("Authorization", &format!("Bearer {api_key}"));
    }

    let response = request.call();
    let payload = match response {
        Ok(ok) => match ok.into_json::<OpenRouterModelsResponse>() {
            Ok(payload) => payload,
            Err(err) => {
                warn!(
                    event = "openrouter.models.fetch.failed",
                    reason = "invalid_json",
                    error = %err
                );
                return None;
            }
        },
        Err(err) => {
            warn!(
                event = "openrouter.models.fetch.failed",
                reason = "request_failed",
                error = %err
            );
            return None;
        }
    };

    Some(map_openrouter_models(payload, supported_ids))
}

pub(crate) fn fetch_provider_model_ids(
    provider_name: &str,
    provider_config: &config::ProviderConfig,
    connect_timeout_seconds: u64,
    gigachat_insecure_tls: bool,
) -> Option<Vec<String>> {
    if provider_name == "gigachat" {
        return fetch_gigachat_model_ids(
            provider_config,
            connect_timeout_seconds,
            gigachat_insecure_tls,
        );
    }

    let base_url = provider_config
        .base_url
        .as_deref()
        .filter(|value| !value.trim().is_empty())?
        .trim_end_matches('/')
        .to_string();
    if base_url.is_empty() {
        return None;
    }

    let url = format!("{base_url}/models");
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(connect_timeout_seconds))
        .build();
    let mut request = agent.get(url.as_str()).set("Accept", "application/json");
    if let Some(api_key) = provider_config.api_key.as_deref().filter(|v| !v.trim().is_empty()) {
        request = request.set("Authorization", &format!("Bearer {api_key}"));
    }
    if provider_name == "yandex"
        && let Some(project) = provider_config.project.as_deref().filter(|v| !v.trim().is_empty())
    {
        request = request.set("OpenAI-Project", project);
    }

    match request.call() {
        Ok(ok) => match ok.into_json::<ProviderModelsResponse>() {
            Ok(payload) => Some(extract_provider_model_ids(payload)),
            Err(err) => {
                warn!(
                    event = "provider.models.fetch.failed",
                    provider = %provider_name,
                    reason = "invalid_json",
                    error = %err
                );
                None
            }
        },
        Err(err) => {
            warn!(
                event = "provider.models.fetch.failed",
                provider = %provider_name,
                reason = "request_failed",
                error = %err
            );
            None
        }
    }
}

pub(crate) fn fetch_xrouter_models(
    provider_config: &config::ProviderConfig,
    connect_timeout_seconds: u64,
) -> Option<Vec<ModelDescriptor>> {
    let base_url = provider_config
        .base_url
        .as_deref()
        .filter(|value| !value.trim().is_empty())?
        .trim_end_matches('/')
        .to_string();
    if base_url.is_empty() {
        return None;
    }

    let url = format!("{base_url}/models");
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(connect_timeout_seconds))
        .build();
    let mut request = agent.get(url.as_str()).set("Accept", "application/json");
    if let Some(api_key) = provider_config.api_key.as_deref().filter(|v| !v.trim().is_empty()) {
        request = request.set("Authorization", &format!("Bearer {api_key}"));
    }

    match request.call() {
        Ok(ok) => match ok.into_json::<XrouterProviderModelsResponse>() {
            Ok(payload) => Some(map_xrouter_models(payload)),
            Err(err) => {
                warn!(
                    event = "xrouter.models.fetch.failed",
                    reason = "invalid_json",
                    error = %err
                );
                None
            }
        },
        Err(err) => {
            warn!(event = "xrouter.models.fetch.failed", reason = "request_failed", error = %err);
            None
        }
    }
}

fn fetch_gigachat_access_token(
    provider_config: &config::ProviderConfig,
    connect_timeout_seconds: u64,
    insecure_tls: bool,
) -> Option<String> {
    let api_key = provider_config.api_key.as_deref().filter(|v| !v.trim().is_empty())?;
    let agent = gigachat_ureq_agent(connect_timeout_seconds, insecure_tls);
    let request_id = uuid::Uuid::new_v4().to_string();
    let response = agent
        .post(GIGACHAT_OAUTH_URL)
        .set("Accept", "application/json")
        .set("Authorization", &format!("Bearer {api_key}"))
        .set("RqUID", &request_id)
        .set("Content-Type", "application/x-www-form-urlencoded")
        .send_form(&[("scope", GIGACHAT_SCOPE)]);

    match response {
        Ok(ok) => match ok.into_json::<GigachatOauthResponse>() {
            Ok(payload) => Some(payload.access_token),
            Err(err) => {
                warn!(
                    event = "provider.oauth.fetch.failed",
                    provider = "gigachat",
                    reason = "invalid_json",
                    error = %err
                );
                None
            }
        },
        Err(err) => {
            warn!(
                event = "provider.oauth.fetch.failed",
                provider = "gigachat",
                reason = "request_failed",
                error = %err
            );
            None
        }
    }
}

fn fetch_gigachat_model_ids(
    provider_config: &config::ProviderConfig,
    connect_timeout_seconds: u64,
    insecure_tls: bool,
) -> Option<Vec<String>> {
    let base_url = provider_config
        .base_url
        .as_deref()
        .filter(|value| !value.trim().is_empty())?
        .trim_end_matches('/')
        .to_string();
    let access_token =
        fetch_gigachat_access_token(provider_config, connect_timeout_seconds, insecure_tls)?;
    let agent = gigachat_ureq_agent(connect_timeout_seconds, insecure_tls);
    let url = format!("{base_url}/models");
    let response = agent
        .get(url.as_str())
        .set("Accept", "application/json")
        .set("Authorization", &format!("Bearer {access_token}"))
        .call();

    match response {
        Ok(ok) => match ok.into_json::<ProviderModelsResponse>() {
            Ok(payload) => Some(extract_provider_model_ids(payload)),
            Err(err) => {
                warn!(
                    event = "provider.models.fetch.failed",
                    provider = "gigachat",
                    reason = "invalid_json",
                    error = %err
                );
                None
            }
        },
        Err(err) => {
            warn!(
                event = "provider.models.fetch.failed",
                provider = "gigachat",
                reason = "request_failed",
                error = %err
            );
            None
        }
    }
}
