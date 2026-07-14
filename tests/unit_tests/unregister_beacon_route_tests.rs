use alloy::primitives::Address;
use rocket::State;
use rocket::http::Status;
use rocket::serde::json::Json;
use std::str::FromStr;

use the_beaconator::guards::ApiToken;
use the_beaconator::models::UnregisterBeaconRequest;
use the_beaconator::routes::beacon::unregister_beacon;

#[tokio::test]
async fn test_unregister_beacon_invalid_beacon_address() {
    let app_state = crate::test_utils::create_simple_test_app_state().await;
    let state = State::from(&app_state);
    let token = ApiToken("test_token".to_string());

    let request = Json(UnregisterBeaconRequest {
        beacon_address: "invalid_address".to_string(),
        registry_address: Some("0x1234567890123456789012345678901234567890".to_string()),
    });

    let result = unregister_beacon(request, token, state).await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), Status::BadRequest);
}

#[tokio::test]
async fn test_unregister_beacon_invalid_registry_address() {
    let app_state = crate::test_utils::create_simple_test_app_state().await;
    let state = State::from(&app_state);
    let token = ApiToken("test_token".to_string());

    let request = Json(UnregisterBeaconRequest {
        beacon_address: "0x1234567890123456789012345678901234567890".to_string(),
        registry_address: Some("not_an_address".to_string()),
    });

    let result = unregister_beacon(request, token, state).await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), Status::BadRequest);
}

#[tokio::test]
async fn test_unregister_beacon_beacon_without_0x_prefix() {
    let app_state = crate::test_utils::create_simple_test_app_state().await;
    let state = State::from(&app_state);
    let token = ApiToken("test_token".to_string());

    let request = Json(UnregisterBeaconRequest {
        beacon_address: "1234567890123456789012345678901234567890".to_string(),
        registry_address: Some("0x1234567890123456789012345678901234567890".to_string()),
    });

    let result = unregister_beacon(request, token, state).await;
    assert_eq!(result.unwrap_err(), Status::BadRequest);
}

#[tokio::test]
async fn test_unregister_beacon_registry_without_0x_prefix() {
    let app_state = crate::test_utils::create_simple_test_app_state().await;
    let state = State::from(&app_state);
    let token = ApiToken("test_token".to_string());

    let request = Json(UnregisterBeaconRequest {
        beacon_address: "0x1234567890123456789012345678901234567890".to_string(),
        registry_address: Some("1234567890123456789012345678901234567890".to_string()),
    });

    let result = unregister_beacon(request, token, state).await;
    assert_eq!(result.unwrap_err(), Status::BadRequest);
}

#[tokio::test]
async fn test_unregister_beacon_too_short_address() {
    let app_state = crate::test_utils::create_simple_test_app_state().await;
    let state = State::from(&app_state);
    let token = ApiToken("test_token".to_string());

    let request = Json(UnregisterBeaconRequest {
        beacon_address: "0x1234".to_string(),
        registry_address: Some("0x1234567890123456789012345678901234567890".to_string()),
    });

    let result = unregister_beacon(request, token, state).await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), Status::BadRequest);
}

#[tokio::test]
async fn test_unregister_beacon_too_long_address() {
    let app_state = crate::test_utils::create_simple_test_app_state().await;
    let state = State::from(&app_state);
    let token = ApiToken("test_token".to_string());

    let request = Json(UnregisterBeaconRequest {
        beacon_address: "0x12345678901234567890123456789012345678901".to_string(), // 41 chars
        registry_address: Some("0x1234567890123456789012345678901234567890".to_string()),
    });

    let result = unregister_beacon(request, token, state).await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), Status::BadRequest);
}

// The unregister flow uses a STRICT registration check, so a provider failure surfaces as an
// error (HTTP 500) instead of being silently reported as "already unregistered". This mirrors
// the register route's network-failure behavior and prevents a transient RPC blip from making
// us skip a real removal while reporting success.
#[tokio::test]
async fn test_unregister_beacon_provider_failure_is_error() {
    let mock_provider = crate::test_utils::create_mock_provider_with_network_error();
    let app_state = crate::test_utils::create_test_app_state_with_provider(mock_provider).await;
    let state = State::from(&app_state);
    let token = ApiToken("test_token".to_string());

    let request = Json(UnregisterBeaconRequest {
        beacon_address: "0x1111111111111111111111111111111111111111".to_string(),
        registry_address: Some("0x2222222222222222222222222222222222222222".to_string()),
    });

    let result = unregister_beacon(request, token, state).await;
    assert_eq!(result.unwrap_err(), Status::InternalServerError);
}

// registry_address = None must be accepted (defaulted to the configured registry, not rejected
// as a BadRequest). With the provider unavailable the strict check then surfaces as HTTP 500,
// confirming the request reached the service layer rather than being turned away at validation.
#[tokio::test]
async fn test_unregister_beacon_defaults_registry_when_absent() {
    let mock_provider = crate::test_utils::create_mock_provider_with_network_error();
    let app_state = crate::test_utils::create_test_app_state_with_provider(mock_provider).await;
    let state = State::from(&app_state);
    let token = ApiToken("test_token".to_string());

    let request = Json(UnregisterBeaconRequest {
        beacon_address: "0x1111111111111111111111111111111111111111".to_string(),
        registry_address: None,
    });

    let result = unregister_beacon(request, token, state).await;
    // Not BadRequest: None was accepted and defaulted; the failure is the unreachable provider.
    assert_eq!(result.unwrap_err(), Status::InternalServerError);
}

#[tokio::test]
async fn test_unregister_beacon_request_serialization_with_registry() {
    let request = UnregisterBeaconRequest {
        beacon_address: "0x1234567890123456789012345678901234567890".to_string(),
        registry_address: Some("0x9876543210987654321098765432109876543210".to_string()),
    };

    let serialized = serde_json::to_string(&request).unwrap();
    assert!(serialized.contains("beacon_address"));
    assert!(serialized.contains("registry_address"));
    assert!(serialized.contains("0x1234567890123456789012345678901234567890"));
    assert!(serialized.contains("0x9876543210987654321098765432109876543210"));

    let deserialized: UnregisterBeaconRequest = serde_json::from_str(&serialized).unwrap();
    assert_eq!(deserialized.beacon_address, request.beacon_address);
    assert_eq!(deserialized.registry_address, request.registry_address);
}

#[tokio::test]
async fn test_unregister_beacon_request_deserialization_without_registry() {
    // An omitted registry_address deserializes to None (defaults to the configured registry).
    let json = r#"{"beacon_address":"0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}"#;

    let request: UnregisterBeaconRequest = serde_json::from_str(json).unwrap();
    assert_eq!(
        request.beacon_address,
        "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    );
    assert_eq!(request.registry_address, None);
}

#[test]
fn test_unregister_beacon_request_field_access() {
    let request = UnregisterBeaconRequest {
        beacon_address: "0x1111111111111111111111111111111111111111".to_string(),
        registry_address: Some("0x2222222222222222222222222222222222222222".to_string()),
    };

    assert_eq!(
        request.beacon_address,
        "0x1111111111111111111111111111111111111111"
    );
    assert_eq!(
        request.registry_address,
        Some("0x2222222222222222222222222222222222222222".to_string())
    );

    // Address round-trips through Address parsing (route-level parse contract).
    assert!(Address::from_str(&request.beacon_address).is_ok());
}
