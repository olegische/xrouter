use tracing_subscriber::fmt::format::FmtSpan;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogExporterKind {
    Stdout,
    None,
}

pub fn parse_log_exporter_kind(raw: &str) -> LogExporterKind {
    match raw.trim().to_ascii_lowercase().as_str() {
        "none" => LogExporterKind::None,
        _ => LogExporterKind::Stdout,
    }
}

pub fn span_events_mask(log_span_events: bool) -> FmtSpan {
    if log_span_events { FmtSpan::NEW | FmtSpan::CLOSE } else { FmtSpan::NONE }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_log_exporter_kind_defaults_to_stdout() {
        assert_eq!(parse_log_exporter_kind(""), LogExporterKind::Stdout);
        assert_eq!(parse_log_exporter_kind("stdout"), LogExporterKind::Stdout);
    }

    #[test]
    fn parse_log_exporter_kind_accepts_none() {
        assert_eq!(parse_log_exporter_kind("none"), LogExporterKind::None);
    }
}
