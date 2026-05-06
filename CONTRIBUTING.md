# Contributing

## Prerequisites
- Rust stable toolchain
- Ollama running locally for manual testing

## Development
- `cargo test`
- `cargo clippy -- -D warnings`
- `cargo build --release`

## Scope
- Keep the binary cross-platform.
- Preserve the plan -> approve -> output -> explanation flow.
- Do not add telemetry, cloud dependencies, or non-Rust runtime requirements.