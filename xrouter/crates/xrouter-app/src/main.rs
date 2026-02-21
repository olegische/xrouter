use std::net::SocketAddr;

use xrouter_app::{build_router, config::AppConfig, AppState};
use xrouter_observability::init_tracing;

#[tokio::main]
async fn main() {
    let _ = dotenvy::dotenv();
    init_tracing("xrouter-app");

    let config = AppConfig::from_env().expect("configuration must be valid");
    let state = AppState::from_config(&config);
    let app = build_router(state);
    let addr: SocketAddr =
        format!("{}:{}", config.host, config.port).parse().expect("socket address must be valid");

    let listener = tokio::net::TcpListener::bind(addr).await.expect("listener must bind");
    axum::serve(listener, app).await.expect("server must run");
}
