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
}
