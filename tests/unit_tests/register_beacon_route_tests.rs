use alloy::primitives::Address;
use rocket::State;
use rocket::http::Status;
use rocket::serde::json::Json;
use std::str::FromStr;

use the_beaconator::guards::ApiToken;
use the_beaconator::models::RegisterBeaconRequest;
use the_beaconator::routes::beacon::register_beacon;

#[tokio::test]
async fn test_register_beacon_invalid_beacon_address() {
    let app_state = crate::test_utils::create_simple_test_app_state();
    let state = State::from(&app_state);
    let token = ApiToken("test_token".to_string());

    let request = Json(RegisterBeaconRequest {
        beacon_address: "invalid_address".to_string(),
        registry_address: "0x1234567890123456789012345678901234567890".to_string(),
    });

    let result = register_beacon(request, token, state).await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), Status::BadRequest);
}

#[tokio::test]
async fn test_register_beacon_invalid_registry_address() {
    let app_state = crate::test_utils::create_simple_test_app_state();
    let state = State::from(&app_state);
    let token = ApiToken("test_token".to_string());

    let request = Json(RegisterBeaconRequest {
        beacon_address: "0x1234567890123456789012345678901234567890".to_string(),
        registry_address: "not_an_address".to_string(),
    });

    let result = register_beacon(request, token, state).await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), Status::BadRequest);
}

#[tokio::test]
async fn test_register_beacon_both_addresses_invalid() {
    let app_state = crate::test_utils::create_simple_test_app_state();
    let state = State::from(&app_state);
    let token = ApiToken("test_token".to_string());

    let request = Json(RegisterBeaconRequest {
        beacon_address: "invalid".to_string(),
        registry_address: "also_invalid".to_string(),
    });

    let result = register_beacon(request, token, state).await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), Status::BadRequest);
}

#[tokio::test]
async fn test_register_beacon_zero_beacon_address() {
    let app_state = crate::test_utils::create_simple_test_app_state();
    let state = State::from(&app_state);
    let token = ApiToken("test_token".to_string());

    let request = Json(RegisterBeaconRequest {
        beacon_address: "0x0000000000000000000000000000000000000000".to_string(),
        registry_address: "0x1234567890123456789012345678901234567890".to_string(),
    });

    // Zero address is valid format, should attempt registration (will fail at network level)
    let result = register_beacon(request, token, state).await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), Status::InternalServerError);
}

#[tokio::test]
async fn test_register_beacon_zero_registry_address() {
    let app_state = crate::test_utils::create_simple_test_app_state();
    let state = State::from(&app_state);
    let token = ApiToken("test_token".to_string());

    let request = Json(RegisterBeaconRequest {
        beacon_address: "0x1234567890123456789012345678901234567890".to_string(),
        registry_address: "0x0000000000000000000000000000000000000000".to_string(),
    });

    // Zero address is valid format, should attempt registration (will fail at network level)
    let result = register_beacon(request, token, state).await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), Status::InternalServerError);
}

#[tokio::test]
async fn test_register_beacon_network_failure() {
    let app_state = crate::test_utils::create_simple_test_app_state();
    let state = State::from(&app_state);
    let token = ApiToken("test_token".to_string());

    let request = Json(RegisterBeaconRequest {
        beacon_address: "0x1111111111111111111111111111111111111111".to_string(),
        registry_address: "0x2222222222222222222222222222222222222222".to_string(),
    });

    // Valid addresses but will fail at network call
    let result = register_beacon(request, token, state).await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), Status::InternalServerError);
}

#[tokio::test]
async fn test_register_beacon_address_case_sensitivity() {
    let app_state = crate::test_utils::create_simple_test_app_state();
    let state = State::from(&app_state);
    let token = ApiToken("test_token".to_string());

    // Mixed case addresses (EIP-55 checksummed)
    let request = Json(RegisterBeaconRequest {
        beacon_address: "0xAbCdEf1234567890123456789012345678901234".to_string(),
        registry_address: "0xFeDcBa9876543210987654321098765432109876".to_string(),
    });

    // Should parse successfully (case insensitive), fail at network level
    let result = register_beacon(request, token, state).await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), Status::InternalServerError);
}

#[tokio::test]
async fn test_register_beacon_address_without_0x_prefix() {
    let app_state = crate::test_utils::create_simple_test_app_state();
    let state = State::from(&app_state);
    let token = ApiToken("test_token".to_string());

    let request = Json(RegisterBeaconRequest {
        beacon_address: "1234567890123456789012345678901234567890".to_string(),
        registry_address: "0x1234567890123456789012345678901234567890".to_string(),
    });

    // Hex strings without 0x prefix might parse but will fail at network level
    let result = register_beacon(request, token, state).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_register_beacon_too_short_address() {
    let app_state = crate::test_utils::create_simple_test_app_state();
    let state = State::from(&app_state);
    let token = ApiToken("test_token".to_string());

    let request = Json(RegisterBeaconRequest {
        beacon_address: "0x1234".to_string(),
        registry_address: "0x1234567890123456789012345678901234567890".to_string(),
    });

    let result = register_beacon(request, token, state).await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), Status::BadRequest);
}

#[tokio::test]
async fn test_register_beacon_too_long_address() {
    let app_state = crate::test_utils::create_simple_test_app_state();
    let state = State::from(&app_state);
    let token = ApiToken("test_token".to_string());

    let request = Json(RegisterBeaconRequest {
        beacon_address: "0x12345678901234567890123456789012345678901".to_string(), // 41 chars
        registry_address: "0x1234567890123456789012345678901234567890".to_string(),
    });

    let result = register_beacon(request, token, state).await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), Status::BadRequest);
}

#[tokio::test]
async fn test_register_beacon_request_serialization() {
    let request = RegisterBeaconRequest {
        beacon_address: "0x1234567890123456789012345678901234567890".to_string(),
        registry_address: "0x9876543210987654321098765432109876543210".to_string(),
    };

    let serialized = serde_json::to_string(&request).unwrap();
    assert!(serialized.contains("beacon_address"));
    assert!(serialized.contains("registry_address"));
    assert!(serialized.contains("0x1234567890123456789012345678901234567890"));
    assert!(serialized.contains("0x9876543210987654321098765432109876543210"));

    let deserialized: RegisterBeaconRequest = serde_json::from_str(&serialized).unwrap();
    assert_eq!(deserialized.beacon_address, request.beacon_address);
    assert_eq!(deserialized.registry_address, request.registry_address);
}

#[tokio::test]
async fn test_register_beacon_request_deserialization() {
    let json = r#"{"beacon_address":"0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","registry_address":"0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"}"#;

    let request: RegisterBeaconRequest = serde_json::from_str(json).unwrap();
    assert_eq!(
        request.beacon_address,
        "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    );
    assert_eq!(
        request.registry_address,
        "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
    );
}

#[tokio::test]
async fn test_register_beacon_same_beacon_and_registry() {
    let app_state = crate::test_utils::create_simple_test_app_state();
    let state = State::from(&app_state);
    let token = ApiToken("test_token".to_string());

    // Same address for both (edge case)
    let same_address = "0x1234567890123456789012345678901234567890".to_string();
    let request = Json(RegisterBeaconRequest {
        beacon_address: same_address.clone(),
        registry_address: same_address,
    });

    // Should parse successfully, fail at logic level
    let result = register_beacon(request, token, state).await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), Status::InternalServerError);
}

#[test]
fn test_register_beacon_request_field_access() {
    let request = RegisterBeaconRequest {
        beacon_address: "0x1111111111111111111111111111111111111111".to_string(),
        registry_address: "0x2222222222222222222222222222222222222222".to_string(),
    };

    assert_eq!(
        request.beacon_address,
        "0x1111111111111111111111111111111111111111"
    );
    assert_eq!(
        request.registry_address,
        "0x2222222222222222222222222222222222222222"
    );
}

#[test]
fn test_address_parsing_for_register_beacon() {
    // Test various address formats that register_beacon will encounter
    let valid_addresses = vec![
        "0x0000000000000000000000000000000000000000",
        "0x1234567890123456789012345678901234567890",
        "0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF",
        "0xAbCdEf1234567890AbCdEf1234567890AbCdEf12",
    ];

    for addr_str in valid_addresses {
        let result = Address::from_str(addr_str);
        assert!(result.is_ok(), "Failed to parse valid address: {addr_str}");
    }

    let invalid_addresses = vec![
        "invalid_address",
        "0x123",
        "",
        "0xZZZZ567890123456789012345678901234567890",
        "12345678901234567890123456789012345678901", // No 0x prefix, 41 chars
    ];

    for addr_str in invalid_addresses {
        let result = Address::from_str(addr_str);
        assert!(result.is_err(), "Should have failed to parse: {addr_str}");
    }
}
