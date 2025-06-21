use the_beaconator::create_rocket;

#[rocket::launch]
async fn rocket() -> _ {
    tracing_subscriber::fmt::init();
    tracing::info!("Starting the Beaconator server...");

    let dsn = std::env::var("SENTRY_DSN")
        .ok()
        .and_then(|s| s.parse().ok());
    let _sentry = sentry::init(sentry::ClientOptions {
        dsn,
        release: sentry::release_name!(),
        ..Default::default()
    });

    create_rocket().await
}
