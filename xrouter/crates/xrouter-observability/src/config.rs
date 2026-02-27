use std::{env, time::Duration};

use opentelemetry_otlp::Protocol;

use crate::exporters::{
    otlp::{TraceSinkConfig, parse_http_protocol, parse_trace_sinks_from_env},
    stdout::{LogExporterKind, parse_log_exporter_kind},
};

const DEFAULT_TRACE_TIMEOUT_MS: u64 = 3_000;

#[derive(Debug, Clone)]
pub struct ObservabilityConfig {
    pub log_level: String,
    pub log_span_events: bool,
    pub log_exporter: LogExporterKind,
    pub trace_enabled: bool,
    pub trace_http_protocol: Protocol,
    pub trace_timeout: Duration,
    pub trace_sinks: Vec<TraceSinkConfig>,
}

impl ObservabilityConfig {
    pub fn from_env() -> Self {
        let log_level = env::var("XR_LOG_LEVEL").unwrap_or_else(|_| "info".to_string());
        let log_span_events = env_truthy("XR_LOG_SPAN_EVENTS", false);
        let log_exporter = parse_log_exporter_kind(
            &env::var("XR_LOG_EXPORTER").unwrap_or_else(|_| "stdout".to_string()),
        );
        let trace_enabled = env_truthy("XR_TRACE_ENABLED", false);
        let trace_http_protocol = parse_http_protocol(
            &env::var("XR_OTEL_TRACE_HTTP_PROTOCOL").unwrap_or_else(|_| "binary".to_string()),
        );
        let trace_timeout = Duration::from_millis(
            env::var("XR_OTEL_TRACE_TIMEOUT_MS")
                .ok()
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(DEFAULT_TRACE_TIMEOUT_MS),
        );
        let trace_sinks = parse_trace_sinks_from_env(trace_enabled);

        Self {
            log_level,
            log_span_events,
            log_exporter,
            trace_enabled,
            trace_http_protocol,
            trace_timeout,
            trace_sinks,
        }
    }
}

fn env_truthy(var_name: &str, default: bool) -> bool {
    env::var(var_name)
        .ok()
        .map(|value| {
            let v = value.trim().to_ascii_lowercase();
            matches!(v.as_str(), "1" | "true" | "yes" | "on")
        })
        .unwrap_or(default)
}
