use std::{sync::Arc, time::Duration};

use serde::Deserialize;
use tracing::warn;
use ureq::rustls::{
    self,
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    pki_types::{CertificateDer, ServerName, UnixTime},
};
use xrouter_clients_openai::model_discovery::{
    HttpFormRequest, HttpJsonRequest, build_gigachat_models_request, build_gigachat_oauth_request,
    build_openrouter_models_request, build_provider_models_request, build_xrouter_models_request,
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
    let request = build_openrouter_models_request(
        provider_config.base_url.as_deref(),
        provider_config.api_key.as_deref(),
    )?;
    let payload = fetch_json::<OpenRouterModelsResponse>(
        request,
        connect_timeout_seconds,
        "openrouter.models.fetch.failed",
        None,
    )?;

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

    let request = build_provider_models_request(
        provider_name,
        provider_config.base_url.as_deref(),
        provider_config.api_key.as_deref(),
        provider_config.project.as_deref(),
    )?;
    let payload = fetch_json::<ProviderModelsResponse>(
        request,
        connect_timeout_seconds,
        "provider.models.fetch.failed",
        Some(provider_name),
    )?;
    Some(extract_provider_model_ids(payload))
}

pub(crate) fn fetch_xrouter_models(
    provider_config: &config::ProviderConfig,
    connect_timeout_seconds: u64,
) -> Option<Vec<ModelDescriptor>> {
    let request = build_xrouter_models_request(
        provider_config.base_url.as_deref(),
        provider_config.api_key.as_deref(),
    )?;
    let payload = fetch_json::<XrouterProviderModelsResponse>(
        request,
        connect_timeout_seconds,
        "xrouter.models.fetch.failed",
        None,
    )?;
    Some(map_xrouter_models(payload))
}

fn fetch_gigachat_access_token(
    provider_config: &config::ProviderConfig,
    connect_timeout_seconds: u64,
    insecure_tls: bool,
) -> Option<String> {
    let api_key = provider_config.api_key.as_deref().filter(|v| !v.trim().is_empty())?;
    let agent = gigachat_ureq_agent(connect_timeout_seconds, insecure_tls);
    let request_id = uuid::Uuid::new_v4().to_string();
    let request = build_gigachat_oauth_request(api_key, &request_id, GIGACHAT_SCOPE);
    fetch_form_json::<GigachatOauthResponse>(
        &agent,
        request,
        "provider.oauth.fetch.failed",
        Some("gigachat"),
    )
    .map(|payload| payload.access_token)
}

fn fetch_gigachat_model_ids(
    provider_config: &config::ProviderConfig,
    connect_timeout_seconds: u64,
    insecure_tls: bool,
) -> Option<Vec<String>> {
    let access_token =
        fetch_gigachat_access_token(provider_config, connect_timeout_seconds, insecure_tls)?;
    let agent = gigachat_ureq_agent(connect_timeout_seconds, insecure_tls);
    let request =
        build_gigachat_models_request(provider_config.base_url.as_deref(), &access_token)?;
    let payload = fetch_json_with_agent::<ProviderModelsResponse>(
        &agent,
        request,
        "provider.models.fetch.failed",
        Some("gigachat"),
    )?;
    Some(extract_provider_model_ids(payload))
}

fn fetch_json<T: serde::de::DeserializeOwned>(
    request: HttpJsonRequest,
    connect_timeout_seconds: u64,
    event: &'static str,
    provider: Option<&str>,
) -> Option<T> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(connect_timeout_seconds))
        .build();
    fetch_json_with_agent(&agent, request, event, provider)
}

fn fetch_json_with_agent<T: serde::de::DeserializeOwned>(
    agent: &ureq::Agent,
    request: HttpJsonRequest,
    event: &'static str,
    provider: Option<&str>,
) -> Option<T> {
    let mut call = agent.get(request.url.as_str());
    for (name, value) in &request.headers {
        call = call.set(name, value);
    }
    match call.call() {
        Ok(ok) => match ok.into_json::<T>() {
            Ok(payload) => Some(payload),
            Err(err) => {
                log_fetch_failure(event, provider, "invalid_json", &err.to_string());
                None
            }
        },
        Err(err) => {
            log_fetch_failure(event, provider, "request_failed", &err.to_string());
            None
        }
    }
}

fn fetch_form_json<T: serde::de::DeserializeOwned>(
    agent: &ureq::Agent,
    request: HttpFormRequest,
    event: &'static str,
    provider: Option<&str>,
) -> Option<T> {
    let mut call = agent.post(request.url.as_str());
    for (name, value) in &request.headers {
        call = call.set(name, value);
    }
    let form_fields = request
        .form_fields
        .iter()
        .map(|(name, value)| (name.as_str(), value.as_str()))
        .collect::<Vec<_>>();
    match call.send_form(&form_fields) {
        Ok(ok) => match ok.into_json::<T>() {
            Ok(payload) => Some(payload),
            Err(err) => {
                log_fetch_failure(event, provider, "invalid_json", &err.to_string());
                None
            }
        },
        Err(err) => {
            log_fetch_failure(event, provider, "request_failed", &err.to_string());
            None
        }
    }
}

fn log_fetch_failure(
    event: &'static str,
    provider: Option<&str>,
    reason: &'static str,
    error: &str,
) {
    if let Some(provider) = provider {
        warn!(event = event, provider = provider, reason = reason, error = %error);
    } else {
        warn!(event = event, reason = reason, error = %error);
    }
}
