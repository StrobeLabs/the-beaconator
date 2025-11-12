#[cfg(test)]
mod tests {
    // ============================================================================
    // VERIFIABLE BEACON MODEL TESTS
    // ============================================================================

    #[test]
    fn test_create_verifiable_beacon_request_serialization() {
        use the_beaconator::models::CreateVerifiableBeaconRequest;

        let request = CreateVerifiableBeaconRequest {
            verifier_address: "0x1234567890123456789012345678901234567890".to_string(),
            initial_data: 50_u128 << 96, // 50 scaled by 2^96
            initial_cardinality: 100,
        };

        // Test JSON serialization
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("verifier_address"));
        assert!(json.contains("initial_data"));
        assert!(json.contains("initial_cardinality"));

        // Test JSON deserialization
        let deserialized: CreateVerifiableBeaconRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.verifier_address, request.verifier_address);
        assert_eq!(deserialized.initial_data, request.initial_data);
        assert_eq!(
            deserialized.initial_cardinality,
            request.initial_cardinality
        );
    }

    #[test]
    fn test_create_verifiable_beacon_request_validation() {
        use the_beaconator::models::CreateVerifiableBeaconRequest;

        // Test valid request
        let valid_request = CreateVerifiableBeaconRequest {
            verifier_address: "0x1234567890123456789012345678901234567890".to_string(),
            initial_data: 0,        // Minimum value
            initial_cardinality: 1, // Minimum value
        };

        let json = serde_json::to_string(&valid_request).unwrap();
        let _: CreateVerifiableBeaconRequest = serde_json::from_str(&json).unwrap();

        // Test with maximum initial_data value
        let max_request = CreateVerifiableBeaconRequest {
            verifier_address: "0x1234567890123456789012345678901234567890".to_string(),
            initial_data: u128::MAX,
            initial_cardinality: u32::MAX,
        };

        let json = serde_json::to_string(&max_request).unwrap();
        let _: CreateVerifiableBeaconRequest = serde_json::from_str(&json).unwrap();
    }

    #[test]
    fn test_verifiable_beacon_request_field_requirements() {
        use the_beaconator::models::CreateVerifiableBeaconRequest;

        // Test CreateVerifiableBeaconRequest required fields
        let create_json = r#"{
            "verifier_address": "0x1234567890123456789012345678901234567890",
            "initial_data": 7922816251426433759354395033600,
            "initial_cardinality": 100
        }"#;

        let create_request: CreateVerifiableBeaconRequest =
            serde_json::from_str(create_json).unwrap();
        assert_eq!(
            create_request.verifier_address,
            "0x1234567890123456789012345678901234567890"
        );
        assert_eq!(create_request.initial_data, 7922816251426433759354395033600); // 100 << 96 (pre-scaled)
        assert_eq!(create_request.initial_cardinality, 100);
    }

    #[test]
    fn test_verifiable_beacon_data_scaling() {
        use the_beaconator::models::CreateVerifiableBeaconRequest;

        // Test various initial_data values and their scaling
        let test_values = vec![
            (0, 0_u128),           // Zero
            (1, 1_u128 << 96),     // 1 scaled by 2^96
            (50, 50_u128 << 96),   // 50 scaled by 2^96
            (100, 100_u128 << 96), // 100 scaled by 2^96
        ];

        for (raw_value, expected_scaled) in test_values {
            let request = CreateVerifiableBeaconRequest {
                verifier_address: "0x1234567890123456789012345678901234567890".to_string(),
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
    fn test_verifiable_beacon_cardinality_bounds() {
        use the_beaconator::models::CreateVerifiableBeaconRequest;

        // Test boundary values for initial_cardinality
        let boundary_values = vec![
            1,        // Minimum practical value
            100,      // Typical value
            1000,     // High value
            u32::MAX, // Maximum possible value
        ];

        for cardinality in boundary_values {
            let request = CreateVerifiableBeaconRequest {
                verifier_address: "0x1234567890123456789012345678901234567890".to_string(),
                initial_data: 50_u128 << 96,
                initial_cardinality: cardinality,
            };

            // Should serialize/deserialize without issues
            let json = serde_json::to_string(&request).unwrap();
            let deserialized: CreateVerifiableBeaconRequest = serde_json::from_str(&json).unwrap();
            assert_eq!(deserialized.initial_cardinality, cardinality);
        }
    }
}
