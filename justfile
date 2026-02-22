set shell := ["bash", "-cu"]
set working-directory := "xrouter"
set positional-arguments := true

help:
    just -l

fmt:
    cargo fmt --all

check:
    cargo check --workspace

clippy:
    cargo clippy --all-targets --all-features -- -D warnings

test:
    cargo test --all-features

run:
    cargo run -p xrouter-app

run-billing:
    cargo run -p xrouter-app --features billing

dev:
    ./dev.sh

models provider:
    ./scripts/check_models.sh {{ provider }}

smoke provider:
    ./scripts/smoke_provider.sh {{ provider }}

smoke-stream provider:
    ./scripts/smoke_provider.sh {{ provider }} stream

smoke-reasoner:
    ./scripts/smoke_provider.sh deepseek-reasoner

smoke-reasoner-stream:
    ./scripts/smoke_provider.sh deepseek-reasoner stream
