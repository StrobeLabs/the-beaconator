use the_beaconator::create_rocket;

#[rocket::launch]
async fn rocket() -> _ {
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

    // Check environment setup
    tracing::info!("Environment check:");
    tracing::info!("  - RUST_LOG: {:?}", std::env::var("RUST_LOG"));
    tracing::info!("  - ENV: {:?}", std::env::var("ENV"));
    tracing::info!(
        "  - SENTRY_DSN: {}",
        std::env::var("SENTRY_DSN")
            .map(|_| "Set")
            .unwrap_or("Not set")
    );

    let dsn = std::env::var("SENTRY_DSN")
        .ok()
        .and_then(|s| s.parse().ok());

    let _sentry = if dsn.is_some() {
        tracing::info!("Initializing Sentry error tracking");
        Some(sentry::init(sentry::ClientOptions {
            dsn,
            release: sentry::release_name!(),
            traces_sample_rate: 1.0, // Capture all traces for debugging
            ..Default::default()
        }))
    } else {
        tracing::warn!("Sentry DSN not configured, error tracking disabled");
        None
    };

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

        // Sentry capture: include both the message and the location in a single string so the
        // grouped issue title is readable in the dashboard.
        let sentry_msg = format!("Panic at {location_str}: {message}");
        let _ = std::panic::catch_unwind(|| {
            sentry::capture_message(&sentry_msg, sentry::Level::Fatal);
        });
    }));

    create_rocket().await
}
