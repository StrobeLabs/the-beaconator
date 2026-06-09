use alloy::primitives::Address;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Identifies one of the 20 deployed component factory contracts.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Hash)]
pub enum ComponentFactoryType {
    // Beacon factories
    IdentityBeaconFactory,
    StandaloneBeaconFactory,
    CompositeBeaconFactory,
    GroupManagerFactory,
    // Preprocessors
    IdentityPreprocessorFactory,
    ThresholdFactory,
    TernaryToBinaryFactory,
    ArgmaxFactory,
    // BaseFns
    CGBMFactory,
    DGBMFactory,
    // Transforms
    BoundedFactory,
    UnboundedFactory,
    // Composers
    WeightedSumComponentFactory,
    // GroupFns
    DominanceFactory,
    RelativeDominanceFactory,
    ContinuousAllocationFactory,
    DiscreteAllocationFactory,
    // GroupTransforms
    SoftmaxFactory,
    GMNormalizeFactory,
    // Verifier
    ECDSAVerifierFactory,
}

impl fmt::Display for ComponentFactoryType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Serialize variant name as display string (matches Redis key convention)
        let json = serde_json::to_string(self).unwrap_or_default();
        // Strip surrounding quotes from JSON string
        write!(f, "{}", json.trim_matches('"'))
    }
}

impl ComponentFactoryType {
    /// Parse from string (inverse of Display)
    pub fn from_str_name(s: &str) -> Option<Self> {
        let quoted = format!("\"{s}\"");
        serde_json::from_str(&quoted).ok()
    }
}

/// A component factory address stored in Redis.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ComponentFactoryConfig {
    /// Which factory this is
    pub factory_type: ComponentFactoryType,
    /// On-chain address
    #[schemars(with = "String")]
    pub address: Address,
    /// Whether this factory is enabled
    pub enabled: bool,
}

/// Parse a `{"FactoryType": "0xaddress", ...}` JSON map (the COMPONENT_FACTORIES_JSON
/// env var set by the AWS deployment) into enabled factory configs for seeding.
///
/// Errors on malformed JSON, unknown factory type names, and unparseable addresses —
/// a deployment with a bad map should fail loudly at startup, not half-seed.
pub fn parse_component_factories_json(json: &str) -> Result<Vec<ComponentFactoryConfig>, String> {
    use std::collections::BTreeMap;
    use std::str::FromStr;

    let map: BTreeMap<String, String> = serde_json::from_str(json)
        .map_err(|e| format!("COMPONENT_FACTORIES_JSON is not a JSON string map: {e}"))?;

    map.into_iter()
        .map(|(type_name, addr)| {
            let factory_type = ComponentFactoryType::from_str_name(&type_name)
                .ok_or_else(|| format!("Unknown component factory type '{type_name}'"))?;
            let address = Address::from_str(addr.trim())
                .map_err(|e| format!("Invalid address for component factory '{type_name}': {e}"))?;
            Ok(ComponentFactoryConfig {
                factory_type,
                address,
                enabled: true,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::primitives::address;

    #[test]
    fn test_component_factory_type_display_roundtrip() {
        let ft = ComponentFactoryType::CGBMFactory;
        let s = ft.to_string();
        assert_eq!(s, "CGBMFactory");
        assert_eq!(
            ComponentFactoryType::from_str_name(&s),
            Some(ComponentFactoryType::CGBMFactory)
        );
    }

    #[test]
    fn test_component_factory_config_serde() {
        let config = ComponentFactoryConfig {
            factory_type: ComponentFactoryType::StandaloneBeaconFactory,
            address: address!("0x3AC62e8909987956C3C4Aa68dc11C662076Ddf79"),
            enabled: true,
        };
        let json = serde_json::to_string(&config).unwrap();
        let deser: ComponentFactoryConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(
            deser.factory_type,
            ComponentFactoryType::StandaloneBeaconFactory
        );
        assert!(deser.enabled);
    }

    #[test]
    fn test_parse_component_factories_json() {
        let json = r#"{
            "CGBMFactory": "0x569AF5De8f8815a662aD4dffC6391b70ADFA9C2A",
            "ECDSAVerifierFactory": "0x47978f1AB8911064B2979aB0e9E90152c1d916c0"
        }"#;
        let configs = parse_component_factories_json(json).unwrap();
        assert_eq!(configs.len(), 2);
        assert!(configs.iter().all(|c| c.enabled));
        let cgbm = configs
            .iter()
            .find(|c| c.factory_type == ComponentFactoryType::CGBMFactory)
            .unwrap();
        assert_eq!(
            cgbm.address,
            address!("0x569AF5De8f8815a662aD4dffC6391b70ADFA9C2A")
        );
    }

    #[test]
    fn test_parse_component_factories_json_rejects_unknown_type() {
        let err = parse_component_factories_json(
            r#"{"NotAFactory": "0x0000000000000000000000000000000000000001"}"#,
        )
        .unwrap_err();
        assert!(err.contains("Unknown component factory type 'NotAFactory'"));
    }

    #[test]
    fn test_parse_component_factories_json_rejects_bad_address() {
        let err = parse_component_factories_json(r#"{"CGBMFactory": "0x123"}"#).unwrap_err();
        assert!(err.contains("Invalid address for component factory 'CGBMFactory'"));
    }

    #[test]
    fn test_parse_component_factories_json_rejects_malformed_json() {
        let err = parse_component_factories_json("not json").unwrap_err();
        assert!(err.contains("not a JSON string map"));
    }
}
