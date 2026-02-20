use alloy::primitives::Address;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Identifies which factory contract interface to use for beacon creation.
/// Each variant corresponds to a compile-time sol! interface.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub enum FactoryType {
    /// IBeaconFactory: createBeacon(address owner) -> address
    Simple,
    /// IDichotomousBeaconFactory: createBeacon(address verifier, uint256 initialData, uint32 initialCardinalityNext) -> address
    Dichotomous,
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
            slug: "perpcity".to_string(),
            name: "PerpCity Beacon".to_string(),
            description: Some("Simple beacon for PerpCity perpetuals".to_string()),
            factory_address: address!("0x1234567890abcdef1234567890abcdef12345678"),
            factory_type: FactoryType::Simple,
            registry_address: Some(address!("0xabcdefabcdefabcdefabcdefabcdefabcdefabcd")),
            enabled: true,
            created_at: 1700000000,
            updated_at: 1700000000,
        };

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: BeaconTypeConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.slug, "perpcity");
        assert_eq!(deserialized.factory_type, FactoryType::Simple);
        assert!(deserialized.registry_address.is_some());
        assert!(deserialized.enabled);
    }

    #[test]
    fn test_factory_type_serde() {
        let simple = FactoryType::Simple;
        let json = serde_json::to_string(&simple).unwrap();
        assert_eq!(json, "\"Simple\"");

        let dichotomous = FactoryType::Dichotomous;
        let json = serde_json::to_string(&dichotomous).unwrap();
        assert_eq!(json, "\"Dichotomous\"");

        let deserialized: FactoryType = serde_json::from_str("\"Dichotomous\"").unwrap();
        assert_eq!(deserialized, FactoryType::Dichotomous);
    }
}
