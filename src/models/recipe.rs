use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::models::component_factory::ComponentFactoryType;

/// A beacon creation recipe stored in Redis.
/// Describes which component factories to call and in what order.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct BeaconRecipe {
    /// Unique slug identifier (e.g., "lbcgbm", "dgbm", "dominance")
    pub slug: String,
    /// Human-readable name
    pub name: String,
    /// Optional description
    pub description: Option<String>,
    /// What kind of beacon this recipe produces and which components it uses
    pub beacon_kind: BeaconKind,
    /// Whether this recipe is enabled
    pub enabled: bool,
    /// Unix timestamp of creation
    pub created_at: u64,
    /// Unix timestamp of last modification
    pub updated_at: u64,
}

/// The type of beacon a recipe creates, along with its component specifications.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum BeaconKind {
    /// Simple identity beacon (verifier + passthrough)
    Identity,
    /// Standalone beacon: verifier + preprocessor + baseFn + transform
    Standalone {
        preprocessor: PreprocessorSpec,
        base_fn: BaseFnSpec,
        transform: TransformSpec,
    },
    /// Composite beacon: references + composer
    Composite { composer: ComposerSpec },
    /// Group manager: verifier + groupFn + groupTransform
    Group {
        group_fn: GroupFnSpec,
        group_transform: GroupTransformSpec,
    },
}

/// Which preprocessor factory to use.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub enum PreprocessorSpec {
    /// IdentityPreprocessorFactory - scales measurement by WAD
    Identity,
    /// ThresholdFactory - binary threshold classification
    Threshold,
    /// TernaryToBinaryFactory - 3-class to binary conversion
    TernaryToBinary,
    /// ArgmaxFactory - returns index of max value
    Argmax,
}

impl PreprocessorSpec {
    pub fn factory_type(&self) -> ComponentFactoryType {
        match self {
            Self::Identity => ComponentFactoryType::IdentityPreprocessorFactory,
            Self::Threshold => ComponentFactoryType::ThresholdFactory,
            Self::TernaryToBinary => ComponentFactoryType::TernaryToBinaryFactory,
            Self::Argmax => ComponentFactoryType::ArgmaxFactory,
        }
    }
}

/// Which base function factory to use.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub enum BaseFnSpec {
    /// CGBMFactory - Continuous GBM with power law exponent
    CGBM,
    /// DGBMFactory - Discrete GBM for binary predictions
    DGBM,
}

impl BaseFnSpec {
    pub fn factory_type(&self) -> ComponentFactoryType {
        match self {
            Self::CGBM => ComponentFactoryType::CGBMFactory,
            Self::DGBM => ComponentFactoryType::DGBMFactory,
        }
    }
}

/// Which transform factory to use.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub enum TransformSpec {
    /// BoundedFactory - sigmoid mapping to [min, max]
    Bounded,
    /// UnboundedFactory - exponential transformation
    Unbounded,
}

impl TransformSpec {
    pub fn factory_type(&self) -> ComponentFactoryType {
        match self {
            Self::Bounded => ComponentFactoryType::BoundedFactory,
            Self::Unbounded => ComponentFactoryType::UnboundedFactory,
        }
    }
}

/// Which composer factory to use.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub enum ComposerSpec {
    /// WeightedSumComponentFactory - weighted sum of reference beacon indices
    WeightedSum,
}

impl ComposerSpec {
    pub fn factory_type(&self) -> ComponentFactoryType {
        match self {
            Self::WeightedSum => ComponentFactoryType::WeightedSumComponentFactory,
        }
    }
}

/// Which group function factory to use.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub enum GroupFnSpec {
    /// DominanceFactory - raw dominance factors with EMA
    Dominance,
    /// RelativeDominanceFactory - relative dominance with dual-EWMA
    RelativeDominance,
    /// ContinuousAllocationFactory - continuous allocation
    ContinuousAllocation,
    /// DiscreteAllocationFactory - discrete allocation (winner-take-all)
    DiscreteAllocation,
}

impl GroupFnSpec {
    pub fn factory_type(&self) -> ComponentFactoryType {
        match self {
            Self::Dominance => ComponentFactoryType::DominanceFactory,
            Self::RelativeDominance => ComponentFactoryType::RelativeDominanceFactory,
            Self::ContinuousAllocation => ComponentFactoryType::ContinuousAllocationFactory,
            Self::DiscreteAllocation => ComponentFactoryType::DiscreteAllocationFactory,
        }
    }
}

/// Which group transform factory to use.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub enum GroupTransformSpec {
    /// SoftmaxFactory - softmax normalization
    Softmax,
    /// GMNormalizeFactory - geometric mean normalization (GM = 1)
    GMNormalize,
}

impl GroupTransformSpec {
    pub fn factory_type(&self) -> ComponentFactoryType {
        match self {
            Self::Softmax => ComponentFactoryType::SoftmaxFactory,
            Self::GMNormalize => ComponentFactoryType::GMNormalizeFactory,
        }
    }
}

impl BeaconKind {
    /// Returns all component factory types required by this beacon kind.
    /// Always includes ECDSAVerifierFactory since all beacons need a verifier.
    pub fn required_factory_types(&self) -> Vec<ComponentFactoryType> {
        let mut types = vec![ComponentFactoryType::ECDSAVerifierFactory];
        match self {
            BeaconKind::Identity => {
                types.push(ComponentFactoryType::IdentityBeaconFactory);
            }
            BeaconKind::Standalone {
                preprocessor,
                base_fn,
                transform,
            } => {
                types.push(ComponentFactoryType::StandaloneBeaconFactory);
                types.push(preprocessor.factory_type());
                types.push(base_fn.factory_type());
                types.push(transform.factory_type());
            }
            BeaconKind::Composite { composer } => {
                types.push(ComponentFactoryType::CompositeBeaconFactory);
                types.push(composer.factory_type());
            }
            BeaconKind::Group {
                group_fn,
                group_transform,
            } => {
                types.push(ComponentFactoryType::GroupManagerFactory);
                types.push(group_fn.factory_type());
                types.push(group_transform.factory_type());
            }
        }
        types
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_beacon_recipe_serde_standalone() {
        let recipe = BeaconRecipe {
            slug: "lbcgbm".to_string(),
            name: "LBCGBM".to_string(),
            description: Some("Identity >> CGBM >> Bounded".to_string()),
            beacon_kind: BeaconKind::Standalone {
                preprocessor: PreprocessorSpec::Identity,
                base_fn: BaseFnSpec::CGBM,
                transform: TransformSpec::Bounded,
            },
            enabled: true,
            created_at: 1700000000,
            updated_at: 1700000000,
        };

        let json = serde_json::to_string(&recipe).unwrap();
        let deser: BeaconRecipe = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.slug, "lbcgbm");
        match deser.beacon_kind {
            BeaconKind::Standalone {
                preprocessor,
                base_fn,
                transform,
            } => {
                assert_eq!(preprocessor, PreprocessorSpec::Identity);
                assert_eq!(base_fn, BaseFnSpec::CGBM);
                assert_eq!(transform, TransformSpec::Bounded);
            }
            _ => panic!("Expected Standalone"),
        }
    }

    #[test]
    fn test_beacon_recipe_serde_group() {
        let recipe = BeaconRecipe {
            slug: "dominance".to_string(),
            name: "Dominance".to_string(),
            description: Some("Dominance >> GMNormalize".to_string()),
            beacon_kind: BeaconKind::Group {
                group_fn: GroupFnSpec::Dominance,
                group_transform: GroupTransformSpec::GMNormalize,
            },
            enabled: true,
            created_at: 1700000000,
            updated_at: 1700000000,
        };

        let json = serde_json::to_string(&recipe).unwrap();
        let deser: BeaconRecipe = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.slug, "dominance");
        match deser.beacon_kind {
            BeaconKind::Group {
                group_fn,
                group_transform,
            } => {
                assert_eq!(group_fn, GroupFnSpec::Dominance);
                assert_eq!(group_transform, GroupTransformSpec::GMNormalize);
            }
            _ => panic!("Expected Group"),
        }
    }

    #[test]
    fn test_spec_factory_type_mapping() {
        assert_eq!(
            PreprocessorSpec::TernaryToBinary.factory_type(),
            ComponentFactoryType::TernaryToBinaryFactory
        );
        assert_eq!(
            BaseFnSpec::DGBM.factory_type(),
            ComponentFactoryType::DGBMFactory
        );
        assert_eq!(
            TransformSpec::Unbounded.factory_type(),
            ComponentFactoryType::UnboundedFactory
        );
        assert_eq!(
            GroupFnSpec::DiscreteAllocation.factory_type(),
            ComponentFactoryType::DiscreteAllocationFactory
        );
        assert_eq!(
            GroupTransformSpec::Softmax.factory_type(),
            ComponentFactoryType::SoftmaxFactory
        );
    }
}
