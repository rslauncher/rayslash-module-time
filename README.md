# RaySlash Time

Official RaySlash module for queries such as `time in Tokyo` or `time in São Paulo`.

The module resolves places with the free Open-Meteo geocoding API, caches normalized place/timezone metadata for seven days, then calculates current local time inside the sandbox from the host-provided Unix timestamp. A stale cached place can be used when the network is unavailable. It receives no WASI access.

## Development

Install Rust 1.92.0 and `cargo-component` 0.21.1, then run:

```sh
cargo test --all-targets
cargo component build --release --target wasm32-unknown-unknown
```

The module is licensed under MIT. Open-Meteo data use is subject to its attribution and licensing terms.
