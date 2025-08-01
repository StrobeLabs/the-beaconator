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

    // Install panic handler to log panics
    std::panic::set_hook(Box::new(|panic_info| {
        tracing::error!("PANIC occurred: {:?}", panic_info);
        if let Some(location) = panic_info.location() {
            tracing::error!(
                "Panic location: {}:{}:{}",
                location.file(),
                location.line(),
                location.column()
            );
        }
        sentry::capture_message(&format!("Panic: {panic_info:?}"), sentry::Level::Fatal);
    }));

    create_rocket().await
}
