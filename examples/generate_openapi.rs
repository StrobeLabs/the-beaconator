/// Example program to generate OpenAPI specification without starting the server
///
/// Usage: cargo run --example generate_openapi > openapi.json

use rocket_okapi::{openapi_get_routes_spec, settings::OpenApiSettings};

// Import all route modules
use the_beaconator::routes;

fn main() {
    // Configure OpenAPI settings
    let openapi_settings = OpenApiSettings::new();

    // Generate OpenAPI specification for all routes
    let (_routes, openapi_spec) = openapi_get_routes_spec![
        openapi_settings:
        routes::info::index,
        routes::info::all_beacons,
        routes::beacon::create_beacon,
        routes::beacon::register_beacon,
        routes::beacon::create_perpcity_beacon,
        routes::beacon::batch_create_perpcity_beacon,
        routes::perp::deploy_perp_for_beacon_endpoint,
        routes::perp::batch_deploy_perps_for_beacons,
        routes::perp::deposit_liquidity_for_perp_endpoint,
        routes::perp::batch_deposit_liquidity_for_perps,
        routes::beacon::update_beacon,
        routes::beacon::batch_update_beacon,
        routes::wallet::fund_guest_wallet,
        routes::beacon::create_verifiable_beacon,
    ];

    // Serialize to pretty JSON
    let json = serde_json::to_string_pretty(&openapi_spec)
        .expect("Failed to serialize OpenAPI spec");

    // Print to stdout
    println!("{}", json);
}
