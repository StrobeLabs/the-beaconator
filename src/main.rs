use the_beaconator::create_rocket;

#[rocket::launch]
async fn rocket() -> _ {
    // Pin the process-level rustls CryptoProvider BEFORE anything opens a TLS
    // connection. The dependency tree carries rustls via both redis
    // (tls-rustls, for ElastiCache rediss://) and reqwest (rustls-tls), and
    // rustls 0.23 panics at the first TLS handshake when it cannot infer
    // exactly one provider. Ignore the Err case: it means a provider is
    // already installed, which is the desired end state.
    let _ = rustls::crypto::ring::default_provider().install_default();

    // Initialize logging first with environment variable support
    use tracing_subscriber::{EnvFilter, fmt};

    // Set up logging with RUST_LOG environment variable support
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,the_beaconator=info,rocket=warn"));

    fmt()
        .with_env_filter(filter)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true)
        .init();

    tracing::info!("Starting the Beaconator server...");

    // Environment check — presence only, never values. The full audit (with shape /
    // length checks for every var) runs inside `create_rocket()` via `audit_environment`,
    // which emits ERROR lines per problem and a one-line summary.
    tracing::info!("Environment check:");
    for key in ["RUST_LOG", "ENV"] {
        tracing::info!(
            "  - {key}: {}",
            std::env::var(key).map(|_| "Set").unwrap_or("Not set")
        );
    }

    // Install panic handler to log panics.
    //
    // The previous version logged `panic_info` via Debug, which only prints
    // `PanicHookInfo { payload: Any { .. }, location: ... }` — the actual panic message
    // never made it to the logs. We now downcast `payload()` to recover the message
    // string that `panic!("...")` and `.expect("...")` write.
    std::panic::set_hook(Box::new(|panic_info| {
        let payload = panic_info.payload();
        let message = payload
            .downcast_ref::<&'static str>()
            .copied()
            .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
            .unwrap_or("(non-string panic payload — payload was not &str or String)");

        let location_str = panic_info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "<unknown location>".to_string());

        tracing::error!("PANIC at {}: {}", location_str, message);
    }));

    create_rocket().await
}
