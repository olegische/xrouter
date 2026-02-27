use std::{env, sync::OnceLock};

use opentelemetry::{KeyValue, global, trace::TracerProvider};
use opentelemetry_sdk::{Resource, propagation::TraceContextPropagator, trace::SdkTracerProvider};
use tracing::{info, warn};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

mod config;
mod exporters;
mod preflight;

use config::ObservabilityConfig;
use exporters::{
    otlp::build_trace_exporters,
    stdout::{LogExporterKind, span_events_mask},
};
use preflight::preflight_trace_endpoints;

static TRACER_PROVIDER: OnceLock<SdkTracerProvider> = OnceLock::new();

pub fn init_observability(service_name: &str) {
    let config = ObservabilityConfig::from_env();

    let fallback_filter = format!(
        "{level},xrouter_app={level},xrouter_core={level},xrouter_clients_openai={level}",
        level = config.log_level
    );
    let env_filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new(fallback_filter))
        .unwrap_or_else(|_| EnvFilter::new("info"));

    let fmt_layer = if matches!(config.log_exporter, LogExporterKind::Stdout) {
        Some(
            tracing_subscriber::fmt::layer()
                .compact()
                .with_target(true)
                .with_writer(std::io::stdout)
                .with_span_events(span_events_mask(config.log_span_events)),
        )
    } else {
        None
    };

    let preflights = if config.trace_enabled {
        preflight_trace_endpoints(&config.trace_sinks)
    } else {
        Vec::new()
    };

    let telemetry_layer = if config.trace_enabled {
        let tracer = build_tracer(service_name, &config);
        Some(tracing_opentelemetry::layer().with_tracer(tracer))
    } else {
        None
    };

    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt_layer)
        .with(telemetry_layer)
        .try_init()
        .ok();

    if config.trace_enabled {
        info!(
            event = "observability.trace.configured",
            sink_count = config.trace_sinks.len(),
            log_exporter = match config.log_exporter {
                LogExporterKind::Stdout => "stdout",
                LogExporterKind::None => "none",
            }
        );
    }

    for result in preflights {
        if result.reachable {
            info!(
                event = "observability.trace.preflight.ok",
                exporter = result.exporter,
                trace_endpoint = result.endpoint,
                connect_addr = result.connect_addr
            );
        } else {
            warn!(
                event = "observability.trace.preflight.failed",
                exporter = result.exporter,
                trace_endpoint = result.endpoint,
                connect_addr = result.connect_addr,
                error = result.error.unwrap_or_else(|| "unknown preflight error".to_string()),
                "trace sink is unavailable; continuing in soft mode"
            );
        }
    }
}

pub fn init_tracing(service_name: &str) {
    init_observability(service_name);
}

fn build_tracer(
    service_name: &str,
    config: &ObservabilityConfig,
) -> opentelemetry_sdk::trace::Tracer {
    let provider = TRACER_PROVIDER.get_or_init(|| init_tracer_provider(service_name, config));
    provider.tracer(service_name.to_string())
}

fn init_tracer_provider(service_name: &str, config: &ObservabilityConfig) -> SdkTracerProvider {
    let mut provider_builder =
        SdkTracerProvider::builder().with_resource(default_resource(service_name));

    for exporter in
        build_trace_exporters(&config.trace_sinks, config.trace_timeout, config.trace_http_protocol)
    {
        provider_builder = provider_builder.with_batch_exporter(exporter);
    }

    let provider = provider_builder.build();
    global::set_tracer_provider(provider.clone());
    global::set_text_map_propagator(TraceContextPropagator::new());
    provider
}

fn default_resource(service_name: &str) -> Resource {
    Resource::builder()
        .with_attributes(vec![
            KeyValue::new("service.name", service_name.to_string()),
            KeyValue::new("service.version", env!("CARGO_PKG_VERSION").to_string()),
            KeyValue::new(
                "deployment.environment",
                env::var("XR_ENVIRONMENT").unwrap_or_else(|_| "dev".to_string()),
            ),
        ])
        .build()
}
