#[cfg(test)]
mod tests {
    // ============================================================================
    // BEACON TYPE MODEL TESTS
    // ============================================================================

    #[test]
    fn test_create_beacon_with_ecdsa_request_serialization() {
        use the_beaconator::models::CreateBeaconWithEcdsaRequest;

        let request = CreateBeaconWithEcdsaRequest {
            initial_index: 50_u128 << 96, // 50 scaled by 2^96
        };

        // Test JSON serialization
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("initial_index"));

        // Test JSON deserialization
        let deserialized: CreateBeaconWithEcdsaRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.initial_index, request.initial_index);
    }

    #[test]
    fn test_create_beacon_with_ecdsa_request_validation() {
        use the_beaconator::models::CreateBeaconWithEcdsaRequest;

        // Test valid request
        let valid_request = CreateBeaconWithEcdsaRequest {
            initial_index: 0, // Minimum value
        };

        let json = serde_json::to_string(&valid_request).unwrap();
        let _: CreateBeaconWithEcdsaRequest = serde_json::from_str(&json).unwrap();

        // Test with maximum initial_index value
        let max_request = CreateBeaconWithEcdsaRequest {
            initial_index: u128::MAX,
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
            "beacon_type": "identity",
            "params": {
                "initial_index": 7922816251426433759354395033600
            }
        }"#;

        let request: CreateBeaconByTypeRequest = serde_json::from_str(create_json).unwrap();
        assert_eq!(request.beacon_type, "identity");
        assert!(request.params.is_some());
        let params = request.params.unwrap();
        assert_eq!(
            params.initial_index.unwrap(),
            7922816251426433759354395033600
        );
    }

    #[test]
    fn test_ecdsa_request_data_scaling() {
        use the_beaconator::models::CreateBeaconWithEcdsaRequest;

        // Test various initial_index values and their scaling
        let test_values = vec![
            (0, 0_u128),           // Zero
            (1, 1_u128 << 96),     // 1 scaled by 2^96
            (50, 50_u128 << 96),   // 50 scaled by 2^96
            (100, 100_u128 << 96), // 100 scaled by 2^96
        ];

        for (raw_value, expected_scaled) in test_values {
            let request = CreateBeaconWithEcdsaRequest {
                initial_index: expected_scaled,
            };

            // Verify the scaled value is correctly stored
            assert_eq!(request.initial_index, expected_scaled);

            // Verify we can unscale it back to the original value
            let unscaled = request.initial_index >> 96;
            assert_eq!(unscaled, raw_value);
        }
    }

    #[test]
    fn test_ecdsa_request_index_bounds() {
        use the_beaconator::models::CreateBeaconWithEcdsaRequest;

        // Test boundary values for initial_index
        let boundary_values = vec![
            0_u128,    // Minimum value
            1,         // Smallest positive
            100,       // Typical value
            u128::MAX, // Maximum possible value
        ];

        for index_value in boundary_values {
            let request = CreateBeaconWithEcdsaRequest {
                initial_index: index_value,
            };

            // Should serialize/deserialize without issues
            let json = serde_json::to_string(&request).unwrap();
            let deserialized: CreateBeaconWithEcdsaRequest = serde_json::from_str(&json).unwrap();
            assert_eq!(deserialized.initial_index, index_value);
        }
    }

    #[test]
    fn test_beacon_type_config_serialization() {
        use alloy::primitives::Address;
        use std::str::FromStr;
        use the_beaconator::models::beacon_type::{BeaconTypeConfig, FactoryType};

        let config = BeaconTypeConfig {
            slug: "identity".to_string(),
            name: "Identity".to_string(),
            description: Some("Identity beacon with ECDSA verifier".to_string()),
            factory_address: Address::from_str("0x1234567890123456789012345678901234567890")
                .unwrap(),
            factory_type: FactoryType::Identity,
            registry_address: Some(
                Address::from_str("0x9876543210987654321098765432109876543210").unwrap(),
            ),
            enabled: true,
            created_at: 1000,
            updated_at: 2000,
        };

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: BeaconTypeConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.slug, "identity");
        assert_eq!(deserialized.factory_type, FactoryType::Identity);
        assert!(deserialized.enabled);
        assert!(deserialized.registry_address.is_some());
    }
}
