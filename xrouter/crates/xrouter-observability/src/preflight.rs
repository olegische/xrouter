use std::{
    net::{TcpStream, ToSocketAddrs},
    time::Duration,
};

use crate::exporters::otlp::{TraceExporterKind, TraceSinkConfig};

const DEFAULT_TRACE_PREFLIGHT_TIMEOUT_MS: u64 = 1_200;

#[derive(Debug, Clone)]
pub struct TraceEndpointPreflight {
    pub exporter: &'static str,
    pub endpoint: String,
    pub connect_addr: String,
    pub reachable: bool,
    pub error: Option<String>,
}

pub fn preflight_trace_endpoints(sinks: &[TraceSinkConfig]) -> Vec<TraceEndpointPreflight> {
    sinks
        .iter()
        .map(|sink| {
            let connect_addr = endpoint_to_connect_addr(&sink.endpoint, sink.kind)
                .unwrap_or_else(|| sink.endpoint.clone());
            let socket_addr = connect_addr
                .to_socket_addrs()
                .ok()
                .and_then(|mut addrs| addrs.next())
                .unwrap_or_else(|| "127.0.0.1:4317".parse().expect("static socket addr"));
            match TcpStream::connect_timeout(
                &socket_addr,
                Duration::from_millis(DEFAULT_TRACE_PREFLIGHT_TIMEOUT_MS),
            ) {
                Ok(_) => TraceEndpointPreflight {
                    exporter: sink.kind.as_str(),
                    endpoint: sink.endpoint.clone(),
                    connect_addr,
                    reachable: true,
                    error: None,
                },
                Err(err) => TraceEndpointPreflight {
                    exporter: sink.kind.as_str(),
                    endpoint: sink.endpoint.clone(),
                    connect_addr,
                    reachable: false,
                    error: Some(err.to_string()),
                },
            }
        })
        .collect()
}

pub fn endpoint_to_connect_addr(endpoint: &str, kind: TraceExporterKind) -> Option<String> {
    let trimmed = endpoint.trim();
    if trimmed.is_empty() {
        return None;
    }
    let without_scheme = trimmed
        .strip_prefix("http://")
        .or_else(|| trimmed.strip_prefix("https://"))
        .unwrap_or(trimmed);
    let authority = without_scheme.split('/').next().unwrap_or_default();
    if authority.is_empty() {
        return None;
    }
    if authority.contains(':') {
        return Some(authority.to_string());
    }
    Some(format!("{authority}:{}", kind.default_port()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_to_connect_addr_applies_default_ports_by_exporter_kind() {
        let grpc_addr =
            endpoint_to_connect_addr("http://collector.local", TraceExporterKind::OtlpGrpc)
                .expect("grpc connect addr");
        let http_addr =
            endpoint_to_connect_addr("http://collector.local", TraceExporterKind::OtlpHttp)
                .expect("http connect addr");
        assert_eq!(grpc_addr, "collector.local:4317");
        assert_eq!(http_addr, "collector.local:4318");
    }
}
