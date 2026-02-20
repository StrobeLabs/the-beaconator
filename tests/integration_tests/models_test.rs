#[cfg(test)]
mod tests {
    // ============================================================================
    // BEACON TYPE MODEL TESTS
    // ============================================================================

    #[test]
    fn test_create_beacon_with_ecdsa_request_serialization() {
        use the_beaconator::models::CreateBeaconWithEcdsaRequest;

        let request = CreateBeaconWithEcdsaRequest {
            beacon_type: "verifiable-twap".to_string(),
            initial_data: 50_u128 << 96, // 50 scaled by 2^96
            initial_cardinality: 100,
        };

        // Test JSON serialization
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("beacon_type"));
        assert!(json.contains("initial_data"));
        assert!(json.contains("initial_cardinality"));

        // Test JSON deserialization
        let deserialized: CreateBeaconWithEcdsaRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.beacon_type, request.beacon_type);
        assert_eq!(deserialized.initial_data, request.initial_data);
        assert_eq!(
            deserialized.initial_cardinality,
            request.initial_cardinality
        );
    }

    #[test]
    fn test_create_beacon_with_ecdsa_request_validation() {
        use the_beaconator::models::CreateBeaconWithEcdsaRequest;

        // Test valid request
        let valid_request = CreateBeaconWithEcdsaRequest {
            beacon_type: "verifiable-twap".to_string(),
            initial_data: 0,        // Minimum value
            initial_cardinality: 1, // Minimum value
        };

        let json = serde_json::to_string(&valid_request).unwrap();
        let _: CreateBeaconWithEcdsaRequest = serde_json::from_str(&json).unwrap();

        // Test with maximum initial_data value
        let max_request = CreateBeaconWithEcdsaRequest {
            beacon_type: "verifiable-twap".to_string(),
            initial_data: u128::MAX,
            initial_cardinality: u32::MAX,
        };

        let json = serde_json::to_string(&max_request).unwrap();
        let _: CreateBeaconWithEcdsaRequest = serde_json::from_str(&json).unwrap();
    }

    #[test]
    fn test_create_beacon_by_type_request_field_requirements() {
        use the_beaconator::models::CreateBeaconByTypeRequest;

        // Test CreateBeaconByTypeRequest required fields
        let create_json = r#"{
            "beacon_type": "perpcity"
        }"#;

        let create_request: CreateBeaconByTypeRequest = serde_json::from_str(create_json).unwrap();
        assert_eq!(create_request.beacon_type, "perpcity");
        assert!(create_request.params.is_none());
    }

    #[test]
    fn test_create_beacon_by_type_request_with_params() {
        use the_beaconator::models::CreateBeaconByTypeRequest;

        let create_json = r#"{
            "beacon_type": "verifiable-twap",
            "params": {
                "verifier_address": "0x1234567890123456789012345678901234567890",
                "initial_data": 7922816251426433759354395033600,
                "initial_cardinality": 100
            }
        }"#;

        let request: CreateBeaconByTypeRequest = serde_json::from_str(create_json).unwrap();
        assert_eq!(request.beacon_type, "verifiable-twap");
        assert!(request.params.is_some());
        let params = request.params.unwrap();
        assert_eq!(
            params.verifier_address.unwrap(),
            "0x1234567890123456789012345678901234567890"
        );
        assert_eq!(
            params.initial_data.unwrap(),
            7922816251426433759354395033600
        );
        assert_eq!(params.initial_cardinality.unwrap(), 100);
    }

    #[test]
    fn test_ecdsa_request_data_scaling() {
        use the_beaconator::models::CreateBeaconWithEcdsaRequest;

        // Test various initial_data values and their scaling
        let test_values = vec![
            (0, 0_u128),           // Zero
            (1, 1_u128 << 96),     // 1 scaled by 2^96
            (50, 50_u128 << 96),   // 50 scaled by 2^96
            (100, 100_u128 << 96), // 100 scaled by 2^96
        ];

        for (raw_value, expected_scaled) in test_values {
            let request = CreateBeaconWithEcdsaRequest {
                beacon_type: "verifiable-twap".to_string(),
                initial_data: expected_scaled,
                initial_cardinality: 100,
            };

            // Verify the scaled value is correctly stored
            assert_eq!(request.initial_data, expected_scaled);

            // Verify we can unscale it back to the original value
            let unscaled = request.initial_data >> 96;
            assert_eq!(unscaled, raw_value);
        }
    }

    #[test]
    fn test_ecdsa_request_cardinality_bounds() {
        use the_beaconator::models::CreateBeaconWithEcdsaRequest;

        // Test boundary values for initial_cardinality
        let boundary_values = vec![
            1,        // Minimum practical value
            100,      // Typical value
            1000,     // High value
            u32::MAX, // Maximum possible value
        ];

        for cardinality in boundary_values {
            let request = CreateBeaconWithEcdsaRequest {
                beacon_type: "verifiable-twap".to_string(),
                initial_data: 50_u128 << 96,
                initial_cardinality: cardinality,
            };

            // Should serialize/deserialize without issues
            let json = serde_json::to_string(&request).unwrap();
            let deserialized: CreateBeaconWithEcdsaRequest = serde_json::from_str(&json).unwrap();
            assert_eq!(deserialized.initial_cardinality, cardinality);
        }
    }

    #[test]
    fn test_beacon_type_config_serialization() {
        use alloy::primitives::Address;
        use std::str::FromStr;
        use the_beaconator::models::beacon_type::{BeaconTypeConfig, FactoryType};

        let config = BeaconTypeConfig {
            slug: "perpcity".to_string(),
            name: "PerpCity".to_string(),
            description: Some("PerpCity beacon factory".to_string()),
            factory_address: Address::from_str("0x1234567890123456789012345678901234567890")
                .unwrap(),
            factory_type: FactoryType::Simple,
            registry_address: Some(
                Address::from_str("0x9876543210987654321098765432109876543210").unwrap(),
            ),
            enabled: true,
            created_at: 1000,
            updated_at: 2000,
        };

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: BeaconTypeConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.slug, "perpcity");
        assert_eq!(deserialized.factory_type, FactoryType::Simple);
        assert!(deserialized.enabled);
        assert!(deserialized.registry_address.is_some());
    }
}
