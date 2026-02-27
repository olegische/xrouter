use std::{env, time::Duration};

use opentelemetry_otlp::{Protocol, SpanExporter, WithExportConfig};

const DEFAULT_OTEL_TRACE_GRPC_ENDPOINT: &str = "http://127.0.0.1:4317";
const DEFAULT_OTEL_TRACE_HTTP_ENDPOINT: &str = "http://127.0.0.1:4318/v1/traces";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraceExporterKind {
    OtlpGrpc,
    OtlpHttp,
}

impl TraceExporterKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OtlpGrpc => "otlp_grpc",
            Self::OtlpHttp => "otlp_http",
        }
    }

    pub fn default_endpoint(self) -> &'static str {
        match self {
            Self::OtlpGrpc => DEFAULT_OTEL_TRACE_GRPC_ENDPOINT,
            Self::OtlpHttp => DEFAULT_OTEL_TRACE_HTTP_ENDPOINT,
        }
    }

    pub fn default_port(self) -> u16 {
        match self {
            Self::OtlpGrpc => 4317,
            Self::OtlpHttp => 4318,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TraceSinkConfig {
    pub kind: TraceExporterKind,
    pub endpoint: String,
}

pub fn parse_trace_exporter_kind(raw: &str) -> Option<TraceExporterKind> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "otlp_grpc" | "otlp-grpc" => Some(TraceExporterKind::OtlpGrpc),
        "otlp_http" | "otlp-http" => Some(TraceExporterKind::OtlpHttp),
        _ => None,
    }
}

pub fn parse_http_protocol(raw: &str) -> Protocol {
    match raw.trim().to_ascii_lowercase().as_str() {
        "json" => Protocol::HttpJson,
        _ => Protocol::HttpBinary,
    }
}

pub fn parse_trace_sinks_spec(spec: &str) -> (Vec<TraceSinkConfig>, Vec<String>) {
    let mut sinks = Vec::new();
    let mut invalid = Vec::new();

    for token in spec.split(',') {
        let token = token.trim();
        if token.is_empty() {
            continue;
        }
        let Some((kind_raw, endpoint_raw)) = token.split_once('=') else {
            invalid.push(format!("missing '=' in '{token}'"));
            continue;
        };
        let Some(kind) = parse_trace_exporter_kind(kind_raw) else {
            invalid.push(format!("unknown exporter kind '{kind_raw}'"));
            continue;
        };
        let endpoint = endpoint_raw.trim();
        if endpoint.is_empty() {
            invalid.push(format!("empty endpoint for '{kind_raw}'"));
            continue;
        }
        sinks.push(TraceSinkConfig { kind, endpoint: endpoint.to_string() });
    }

    (sinks, invalid)
}

pub fn parse_trace_sinks_from_env(trace_enabled: bool) -> Vec<TraceSinkConfig> {
    if !trace_enabled {
        return Vec::new();
    }

    if let Some(spec) = env::var("XR_OTEL_TRACE_EXPORTERS")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
    {
        let (sinks, invalid) = parse_trace_sinks_spec(&spec);
        for reason in invalid {
            eprintln!(
                "xrouter: ignoring invalid XR_OTEL_TRACE_EXPORTERS entry ({reason}); expected <otlp_grpc|otlp_http>=<endpoint>"
            );
        }
        if !sinks.is_empty() {
            return sinks;
        }
        eprintln!(
            "xrouter: XR_OTEL_TRACE_EXPORTERS provided but yielded no valid sinks; falling back to legacy trace exporter env"
        );
    }

    let kind = env::var("XR_OTEL_TRACE_EXPORTER")
        .ok()
        .and_then(|v| parse_trace_exporter_kind(&v))
        .unwrap_or(TraceExporterKind::OtlpGrpc);
    let endpoint = env::var("XR_OTEL_TRACE_ENDPOINT")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| kind.default_endpoint().to_string());

    vec![TraceSinkConfig { kind, endpoint }]
}

pub fn build_trace_exporters(
    trace_sinks: &[TraceSinkConfig],
    trace_timeout: Duration,
    trace_http_protocol: Protocol,
) -> Vec<SpanExporter> {
    let mut exporters = Vec::new();

    for sink in trace_sinks {
        let built = match sink.kind {
            TraceExporterKind::OtlpGrpc => SpanExporter::builder()
                .with_tonic()
                .with_endpoint(sink.endpoint.clone())
                .with_timeout(trace_timeout)
                .build(),
            TraceExporterKind::OtlpHttp => SpanExporter::builder()
                .with_http()
                .with_endpoint(sink.endpoint.clone())
                .with_timeout(trace_timeout)
                .with_protocol(trace_http_protocol)
                .build(),
        };

        match built {
            Ok(exporter) => exporters.push(exporter),
            Err(error) => {
                eprintln!(
                    "xrouter: failed to initialize OTLP trace exporter (kind={}, endpoint={}): {error}. continuing in soft mode.",
                    sink.kind.as_str(),
                    sink.endpoint
                );
            }
        }
    }

    exporters
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_trace_sinks_spec_parses_valid_entries() {
        let (sinks, invalid) = parse_trace_sinks_spec(
            "otlp_grpc=http://127.0.0.1:4317,otlp_http=http://127.0.0.1:4318/v1/traces",
        );
        assert!(invalid.is_empty());
        assert_eq!(sinks.len(), 2);
        assert_eq!(sinks[0].kind, TraceExporterKind::OtlpGrpc);
        assert_eq!(sinks[1].kind, TraceExporterKind::OtlpHttp);
    }

    #[test]
    fn parse_trace_sinks_spec_collects_invalid_entries() {
        let (sinks, invalid) =
            parse_trace_sinks_spec("otlp_grpc=,unknown=http://x,broken_entry_without_equals");
        assert!(sinks.is_empty());
        assert_eq!(invalid.len(), 3);
    }
}
