use std::env;

use opentelemetry::trace::TracerProvider;
use tracing_subscriber::{
    EnvFilter, fmt::format::FmtSpan, layer::SubscriberExt, util::SubscriberInitExt,
};

fn env_truthy(var_name: &str, default: bool) -> bool {
    env::var(var_name)
        .ok()
        .map(|value| {
            let v = value.trim().to_ascii_lowercase();
            matches!(v.as_str(), "1" | "true" | "yes" | "on")
        })
        .unwrap_or(default)
}

pub fn init_observability(service_name: &str) {
    let default_level = env::var("XR_LOG_LEVEL").unwrap_or_else(|_| "info".to_string());
    let fallback_filter = format!(
        "{level},xrouter_app={level},xrouter_core={level},xrouter_clients_openai={level}",
        level = default_level
    );
    let env_filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new(fallback_filter))
        .unwrap_or_else(|_| EnvFilter::new("info"));
    let log_span_events = env_truthy("XR_LOG_SPAN_EVENTS", false);
    let fmt_layer = tracing_subscriber::fmt::layer()
        .compact()
        .with_target(true)
        .with_writer(std::io::stdout)
        .with_span_events(if log_span_events {
            FmtSpan::NEW | FmtSpan::CLOSE
        } else {
            FmtSpan::NONE
        });
    let telemetry_layer = if env_truthy("XR_TRACE_ENABLED", false) {
        Some(tracing_opentelemetry::layer().with_tracer(
            opentelemetry::trace::noop::NoopTracerProvider::new().tracer(service_name.to_string()),
        ))
    } else {
        None
    };

    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt_layer)
        .with(telemetry_layer)
        .try_init()
        .ok();
}

pub fn init_tracing(service_name: &str) {
    init_observability(service_name);
}
