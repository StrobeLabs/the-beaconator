use alloy::primitives::Address;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Identifies beacon creation strategy.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub enum FactoryType {
    /// Deploy IdentityBeacon with ECDSA verifier via ECDSAVerifierFactory
    Identity,
}

/// Configuration for a registered beacon type stored in Redis.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct BeaconTypeConfig {
    /// Unique slug identifier (e.g., "perpcity", "verifiable-twap")
    pub slug: String,
    /// Human-readable name
    pub name: String,
    /// Optional description
    pub description: Option<String>,
    /// Factory contract address on-chain
    #[schemars(with = "String")]
    pub factory_address: Address,
    /// Which factory interface to use for createBeacon()
    pub factory_type: FactoryType,
    /// Optional registry address to auto-register beacons after creation
    #[schemars(with = "Option<String>")]
    pub registry_address: Option<Address>,
    /// Whether this beacon type is enabled
    pub enabled: bool,
    /// Unix timestamp of when this config was created
    pub created_at: u64,
    /// Unix timestamp of last modification
    pub updated_at: u64,
}

/// Result from seeding default beacon types
#[derive(Debug)]
pub struct SeedResult {
    pub seeded: usize,
    pub skipped: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::primitives::address;

    #[test]
    fn test_beacon_type_config_serde_roundtrip() {
        let config = BeaconTypeConfig {
            slug: "identity".to_string(),
            name: "Identity Beacon".to_string(),
            description: Some("ECDSA-verified identity beacon".to_string()),
            factory_address: address!("0x1234567890abcdef1234567890abcdef12345678"),
            factory_type: FactoryType::Identity,
            registry_address: Some(address!("0xabcdefabcdefabcdefabcdefabcdefabcdefabcd")),
            enabled: true,
            created_at: 1700000000,
            updated_at: 1700000000,
        };

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: BeaconTypeConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.slug, "identity");
        assert_eq!(deserialized.factory_type, FactoryType::Identity);
        assert!(deserialized.registry_address.is_some());
        assert!(deserialized.enabled);
    }

    #[test]
    fn test_factory_type_serde() {
        let identity = FactoryType::Identity;
        let json = serde_json::to_string(&identity).unwrap();
        assert_eq!(json, "\"Identity\"");

        let deserialized: FactoryType = serde_json::from_str("\"Identity\"").unwrap();
        assert_eq!(deserialized, FactoryType::Identity);
    }
}
