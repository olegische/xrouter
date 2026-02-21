use opentelemetry::trace::TracerProvider;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

pub fn init_tracing(service_name: &str) {
    let telemetry_layer = tracing_opentelemetry::layer().with_tracer(
        opentelemetry::trace::noop::NoopTracerProvider::new().tracer(service_name.to_string()),
    );

    tracing_subscriber::registry()
        .with(EnvFilter::from_default_env())
        .with(tracing_subscriber::fmt::layer())
        .with(telemetry_layer)
        .try_init()
        .ok();
}
