use std::net::SocketAddr;

use tracing::info;
use xrouter_app::{AppState, build_router, config::AppConfig};
use xrouter_observability::init_observability;

#[tokio::main]
async fn main() {
    let _ = dotenvy::dotenv();
    init_observability("xrouter-app");

    let config = AppConfig::from_env().expect("configuration must be valid");
    info!(
        event = "app.starting",
        host = %config.host,
        port = config.port,
        openai_compatible_api = config.openai_compatible_api,
        provider_max_inflight = config.provider_max_inflight
    );
    let state = AppState::from_config(&config);
    let app = build_router(state);
    let addr: SocketAddr =
        format!("{}:{}", config.host, config.port).parse().expect("socket address must be valid");

    let listener = tokio::net::TcpListener::bind(addr).await.expect("listener must bind");
    axum::serve(listener, app).await.expect("server must run");
}
