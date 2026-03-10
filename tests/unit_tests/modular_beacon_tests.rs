// Modular beacon tests - recipes, component factories, and modular creation request/response types

use the_beaconator::models::component_factory::ComponentFactoryType;
use the_beaconator::models::recipe::*;
use the_beaconator::models::requests::{CreateModularBeaconRequest, ModularBeaconParams};
use the_beaconator::models::responses::{BeaconComponentAddresses, CreateModularBeaconResponse};

// ============================================================================
// CREATE MODULAR BEACON REQUEST SERDE TESTS
// ============================================================================

#[test]
fn test_create_modular_beacon_request_serde_roundtrip() {
    let request = CreateModularBeaconRequest {
        recipe: "lbcgbm".to_string(),
        params: ModularBeaconParams {
            measurement_scale: Some(1_000_000_000_000_000_000),
            sigma_base: Some(50_000_000_000_000_000),
            scaling_factor: Some(1_000_000_000_000_000_000),
            alpha: Some(500_000_000_000_000_000),
            decay: Some(950_000_000_000_000_000),
            initial_sigma_ratio: Some(1_000_000_000_000_000_000),
            variance_scaling: Some(true),
            min_index: Some(100_000_000_000_000_000_000),
            max_index: Some(10_000_000_000_000_000_000_000),
            steepness: Some(1_000_000_000_000_000_000),
            initial_index: Some(1_000_000_000_000_000_000_000),
            threshold: Some(500_000_000_000_000_000),
            ..Default::default()
        },
    };

    let json = serde_json::to_string(&request).unwrap();
    let deserialized: CreateModularBeaconRequest = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.recipe, "lbcgbm");
    assert_eq!(
        deserialized.params.measurement_scale,
        Some(1_000_000_000_000_000_000)
    );
    assert_eq!(deserialized.params.sigma_base, Some(50_000_000_000_000_000));
    assert_eq!(
        deserialized.params.scaling_factor,
        Some(1_000_000_000_000_000_000)
    );
    assert_eq!(deserialized.params.alpha, Some(500_000_000_000_000_000));
    assert_eq!(deserialized.params.decay, Some(950_000_000_000_000_000));
    assert_eq!(
        deserialized.params.initial_sigma_ratio,
        Some(1_000_000_000_000_000_000)
    );
    assert_eq!(deserialized.params.variance_scaling, Some(true));
    assert_eq!(
        deserialized.params.min_index,
        Some(100_000_000_000_000_000_000)
    );
    assert_eq!(
        deserialized.params.max_index,
        Some(10_000_000_000_000_000_000_000)
    );
    assert_eq!(
        deserialized.params.steepness,
        Some(1_000_000_000_000_000_000)
    );
    assert_eq!(
        deserialized.params.initial_index,
        Some(1_000_000_000_000_000_000_000)
    );
    assert_eq!(deserialized.params.threshold, Some(500_000_000_000_000_000));
    // Fields not set should remain None
    assert!(deserialized.params.reference_beacons.is_none());
    assert!(deserialized.params.weights.is_none());
    assert!(deserialized.params.num_classes.is_none());
}

// ============================================================================
// MODULAR BEACON PARAMS SERIALIZATION TESTS
// ============================================================================

#[test]
fn test_modular_beacon_params_all_none_serialization() {
    let params = ModularBeaconParams::default();

    let json = serde_json::to_string(&params).unwrap();
    let deserialized: ModularBeaconParams = serde_json::from_str(&json).unwrap();

    assert!(deserialized.measurement_scale.is_none());
    assert!(deserialized.threshold.is_none());
    assert!(deserialized.sigma_base.is_none());
    assert!(deserialized.scaling_factor.is_none());
    assert!(deserialized.alpha.is_none());
    assert!(deserialized.decay.is_none());
    assert!(deserialized.initial_sigma_ratio.is_none());
    assert!(deserialized.variance_scaling.is_none());
    assert!(deserialized.initial_positive_rate.is_none());
    assert!(deserialized.min_index.is_none());
    assert!(deserialized.max_index.is_none());
    assert!(deserialized.steepness.is_none());
    assert!(deserialized.initial_index.is_none());
    assert!(deserialized.reference_beacons.is_none());
    assert!(deserialized.weights.is_none());
    assert!(deserialized.num_classes.is_none());
    assert!(deserialized.class_probs.is_none());
    assert!(deserialized.initial_indices.is_none());
    assert!(deserialized.initial_z_space_indices.is_none());
    assert!(deserialized.initial_ema.is_none());
    assert!(deserialized.decay_fast.is_none());
    assert!(deserialized.decay_slow.is_none());
    assert!(deserialized.initial_m_fast.is_none());
    assert!(deserialized.initial_m_slow.is_none());
    assert!(deserialized.index_scale.is_none());

    // All null values in JSON
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    for (_key, val) in value.as_object().unwrap() {
        assert!(val.is_null(), "expected null for all default params");
    }
}

#[test]
fn test_modular_beacon_params_standalone_cgbm_fields() {
    let params = ModularBeaconParams {
        sigma_base: Some(50_000_000_000_000_000),
        scaling_factor: Some(1_000_000_000_000_000_000),
        alpha: Some(500_000_000_000_000_000),
        decay: Some(950_000_000_000_000_000),
        initial_sigma_ratio: Some(1_000_000_000_000_000_000),
        variance_scaling: Some(true),
        measurement_scale: Some(1_000_000_000_000_000_000),
        min_index: Some(100_000_000_000_000_000_000),
        max_index: Some(10_000_000_000_000_000_000_000),
        steepness: Some(1_000_000_000_000_000_000),
        initial_index: Some(1_000_000_000_000_000_000_000),
        ..Default::default()
    };

    let json = serde_json::to_string(&params).unwrap();
    let deserialized: ModularBeaconParams = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.sigma_base, Some(50_000_000_000_000_000));
    assert_eq!(deserialized.scaling_factor, Some(1_000_000_000_000_000_000));
    assert_eq!(deserialized.alpha, Some(500_000_000_000_000_000));
    assert_eq!(deserialized.decay, Some(950_000_000_000_000_000));
    assert_eq!(
        deserialized.initial_sigma_ratio,
        Some(1_000_000_000_000_000_000)
    );
    assert_eq!(deserialized.variance_scaling, Some(true));
    assert_eq!(
        deserialized.measurement_scale,
        Some(1_000_000_000_000_000_000)
    );
    assert_eq!(deserialized.min_index, Some(100_000_000_000_000_000_000));
    assert_eq!(deserialized.max_index, Some(10_000_000_000_000_000_000_000));
    assert_eq!(deserialized.steepness, Some(1_000_000_000_000_000_000));
    assert_eq!(
        deserialized.initial_index,
        Some(1_000_000_000_000_000_000_000)
    );
    // Group fields remain None
    assert!(deserialized.num_classes.is_none());
    assert!(deserialized.initial_indices.is_none());
    assert!(deserialized.index_scale.is_none());
}

#[test]
fn test_modular_beacon_params_group_fields() {
    let params = ModularBeaconParams {
        num_classes: Some(3),
        initial_indices: Some(vec![
            1_000_000_000_000_000_000,
            2_000_000_000_000_000_000,
            3_000_000_000_000_000_000,
        ]),
        initial_z_space_indices: Some(vec![100, -200, 300]),
        alpha: Some(500_000_000_000_000_000),
        decay: Some(950_000_000_000_000_000),
        initial_ema: Some(vec![
            1_000_000_000_000_000_000,
            1_000_000_000_000_000_000,
            1_000_000_000_000_000_000,
        ]),
        index_scale: Some(1_000_000_000_000_000_000),
        ..Default::default()
    };

    let json = serde_json::to_string(&params).unwrap();
    let deserialized: ModularBeaconParams = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.num_classes, Some(3));
    assert_eq!(deserialized.initial_indices.as_ref().unwrap().len(), 3);
    assert_eq!(
        deserialized.initial_z_space_indices.as_ref().unwrap(),
        &[100, -200, 300]
    );
    assert_eq!(deserialized.alpha, Some(500_000_000_000_000_000));
    assert_eq!(deserialized.decay, Some(950_000_000_000_000_000));
    assert_eq!(deserialized.initial_ema.as_ref().unwrap().len(), 3);
    assert_eq!(deserialized.index_scale, Some(1_000_000_000_000_000_000));
    // Standalone fields remain None
    assert!(deserialized.sigma_base.is_none());
    assert!(deserialized.scaling_factor.is_none());
    assert!(deserialized.min_index.is_none());
    assert!(deserialized.max_index.is_none());
}

#[test]
fn test_modular_beacon_params_composite_fields() {
    let params = ModularBeaconParams {
        reference_beacons: Some(vec![
            "0x1111111111111111111111111111111111111111".to_string(),
            "0x2222222222222222222222222222222222222222".to_string(),
            "0x3333333333333333333333333333333333333333".to_string(),
        ]),
        weights: Some(vec![
            333_333_333_333_333_333,
            333_333_333_333_333_333,
            333_333_333_333_333_334,
        ]),
        ..Default::default()
    };

    let json = serde_json::to_string(&params).unwrap();
    let deserialized: ModularBeaconParams = serde_json::from_str(&json).unwrap();

    let beacons = deserialized.reference_beacons.unwrap();
    assert_eq!(beacons.len(), 3);
    assert_eq!(beacons[0], "0x1111111111111111111111111111111111111111");
    assert_eq!(beacons[2], "0x3333333333333333333333333333333333333333");

    let weights = deserialized.weights.unwrap();
    assert_eq!(weights.len(), 3);
    assert_eq!(weights[0], 333_333_333_333_333_333);
    assert_eq!(weights[2], 333_333_333_333_333_334);

    // Non-composite fields remain None
    assert!(deserialized.sigma_base.is_none());
    assert!(deserialized.num_classes.is_none());
}

// ============================================================================
// CREATE MODULAR BEACON RESPONSE SERDE TESTS
// ============================================================================

#[test]
fn test_create_modular_beacon_response_serde_roundtrip() {
    let response = CreateModularBeaconResponse {
        beacon_address: "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
        verifier_address: Some("0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string()),
        recipe: "lbcgbm".to_string(),
        components: BeaconComponentAddresses {
            preprocessor: Some("0x1111111111111111111111111111111111111111".to_string()),
            base_fn: Some("0x2222222222222222222222222222222222222222".to_string()),
            transform: Some("0x3333333333333333333333333333333333333333".to_string()),
            composer: None,
            group_fn: None,
            group_transform: None,
        },
        registered: true,
        safe_proposal_hash: Some(
            "0xabcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890".to_string(),
        ),
    };

    let json = serde_json::to_string(&response).unwrap();
    let deserialized: CreateModularBeaconResponse = serde_json::from_str(&json).unwrap();

    assert_eq!(
        deserialized.beacon_address,
        "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    );
    assert_eq!(
        deserialized.verifier_address.as_ref().unwrap(),
        "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
    );
    assert_eq!(deserialized.recipe, "lbcgbm");
    assert!(deserialized.registered);
    assert!(deserialized.safe_proposal_hash.is_some());

    // Components
    assert_eq!(
        deserialized.components.preprocessor.as_ref().unwrap(),
        "0x1111111111111111111111111111111111111111"
    );
    assert_eq!(
        deserialized.components.base_fn.as_ref().unwrap(),
        "0x2222222222222222222222222222222222222222"
    );
    assert_eq!(
        deserialized.components.transform.as_ref().unwrap(),
        "0x3333333333333333333333333333333333333333"
    );
    assert!(deserialized.components.composer.is_none());
    assert!(deserialized.components.group_fn.is_none());
    assert!(deserialized.components.group_transform.is_none());
}

// ============================================================================
// BEACON COMPONENT ADDRESSES TESTS
// ============================================================================

#[test]
fn test_beacon_component_addresses_default_skip_serializing() {
    let components = BeaconComponentAddresses::default();

    assert!(components.preprocessor.is_none());
    assert!(components.base_fn.is_none());
    assert!(components.transform.is_none());
    assert!(components.composer.is_none());
    assert!(components.group_fn.is_none());
    assert!(components.group_transform.is_none());

    // skip_serializing_if means None fields should not appear in JSON
    let json = serde_json::to_string(&components).unwrap();
    assert!(!json.contains("preprocessor"));
    assert!(!json.contains("base_fn"));
    assert!(!json.contains("transform"));
    assert!(!json.contains("composer"));
    assert!(!json.contains("group_fn"));
    assert!(!json.contains("group_transform"));
    assert_eq!(json, "{}");
}

#[test]
fn test_beacon_component_addresses_with_standalone_fields() {
    let components = BeaconComponentAddresses {
        preprocessor: Some("0x1111111111111111111111111111111111111111".to_string()),
        base_fn: Some("0x2222222222222222222222222222222222222222".to_string()),
        transform: Some("0x3333333333333333333333333333333333333333".to_string()),
        composer: None,
        group_fn: None,
        group_transform: None,
    };

    let json = serde_json::to_string(&components).unwrap();
    assert!(json.contains("preprocessor"));
    assert!(json.contains("base_fn"));
    assert!(json.contains("transform"));
    assert!(!json.contains("composer"));
    assert!(!json.contains("group_fn"));
    assert!(!json.contains("group_transform"));

    let deserialized: BeaconComponentAddresses = serde_json::from_str(&json).unwrap();
    assert_eq!(
        deserialized.preprocessor.as_ref().unwrap(),
        "0x1111111111111111111111111111111111111111"
    );
    assert_eq!(
        deserialized.base_fn.as_ref().unwrap(),
        "0x2222222222222222222222222222222222222222"
    );
    assert_eq!(
        deserialized.transform.as_ref().unwrap(),
        "0x3333333333333333333333333333333333333333"
    );
}

// ============================================================================
// COMPONENT FACTORY TYPE TESTS
// ============================================================================

#[test]
fn test_component_factory_type_all_20_variants_display_roundtrip() {
    let variants: Vec<(ComponentFactoryType, &str)> = vec![
        (
            ComponentFactoryType::IdentityBeaconFactory,
            "IdentityBeaconFactory",
        ),
        (
            ComponentFactoryType::StandaloneBeaconFactory,
            "StandaloneBeaconFactory",
        ),
        (
            ComponentFactoryType::CompositeBeaconFactory,
            "CompositeBeaconFactory",
        ),
        (
            ComponentFactoryType::GroupManagerFactory,
            "GroupManagerFactory",
        ),
        (
            ComponentFactoryType::IdentityPreprocessorFactory,
            "IdentityPreprocessorFactory",
        ),
        (ComponentFactoryType::ThresholdFactory, "ThresholdFactory"),
        (
            ComponentFactoryType::TernaryToBinaryFactory,
            "TernaryToBinaryFactory",
        ),
        (ComponentFactoryType::ArgmaxFactory, "ArgmaxFactory"),
        (ComponentFactoryType::CGBMFactory, "CGBMFactory"),
        (ComponentFactoryType::DGBMFactory, "DGBMFactory"),
        (ComponentFactoryType::BoundedFactory, "BoundedFactory"),
        (ComponentFactoryType::UnboundedFactory, "UnboundedFactory"),
        (
            ComponentFactoryType::WeightedSumComponentFactory,
            "WeightedSumComponentFactory",
        ),
        (ComponentFactoryType::DominanceFactory, "DominanceFactory"),
        (
            ComponentFactoryType::RelativeDominanceFactory,
            "RelativeDominanceFactory",
        ),
        (
            ComponentFactoryType::ContinuousAllocationFactory,
            "ContinuousAllocationFactory",
        ),
        (
            ComponentFactoryType::DiscreteAllocationFactory,
            "DiscreteAllocationFactory",
        ),
        (ComponentFactoryType::SoftmaxFactory, "SoftmaxFactory"),
        (
            ComponentFactoryType::GMNormalizeFactory,
            "GMNormalizeFactory",
        ),
        (
            ComponentFactoryType::ECDSAVerifierFactory,
            "ECDSAVerifierFactory",
        ),
    ];

    assert_eq!(
        variants.len(),
        20,
        "must cover all 20 ComponentFactoryType variants"
    );

    // Verify each variant has a unique display string
    let display_strings: Vec<String> = variants.iter().map(|(v, _)| v.to_string()).collect();
    let mut unique = display_strings.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(
        unique.len(),
        20,
        "all 20 variants must have unique display strings"
    );

    for (variant, expected_display) in &variants {
        // Display matches expected
        let display = variant.to_string();
        assert_eq!(&display, expected_display);

        // Roundtrip through Display + from_str_name
        let roundtripped = ComponentFactoryType::from_str_name(&display)
            .unwrap_or_else(|| panic!("from_str_name failed for {}", display));
        assert_eq!(&roundtripped, variant);

        // Serde roundtrip
        let json = serde_json::to_string(variant).unwrap();
        let deser: ComponentFactoryType = serde_json::from_str(&json).unwrap();
        assert_eq!(&deser, variant);
    }
}

// ============================================================================
// BEACON RECIPE SERDE TESTS (from external test file)
// ============================================================================

#[test]
fn test_beacon_recipe_serde_standalone_external() {
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
    assert_eq!(deser.name, "LBCGBM");
    assert_eq!(
        deser.description.as_deref(),
        Some("Identity >> CGBM >> Bounded")
    );
    assert!(deser.enabled);
    assert_eq!(deser.created_at, 1700000000);

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
        _ => panic!("Expected Standalone beacon kind"),
    }
}

#[test]
fn test_beacon_recipe_serde_group_external() {
    let recipe = BeaconRecipe {
        slug: "dominance-softmax".to_string(),
        name: "DominanceSoftmax".to_string(),
        description: Some("Dominance >> Softmax".to_string()),
        beacon_kind: BeaconKind::Group {
            group_fn: GroupFnSpec::Dominance,
            group_transform: GroupTransformSpec::Softmax,
        },
        enabled: true,
        created_at: 1700000000,
        updated_at: 1700000000,
    };

    let json = serde_json::to_string(&recipe).unwrap();
    let deser: BeaconRecipe = serde_json::from_str(&json).unwrap();
    assert_eq!(deser.slug, "dominance-softmax");
    assert_eq!(deser.name, "DominanceSoftmax");

    match deser.beacon_kind {
        BeaconKind::Group {
            group_fn,
            group_transform,
        } => {
            assert_eq!(group_fn, GroupFnSpec::Dominance);
            assert_eq!(group_transform, GroupTransformSpec::Softmax);
        }
        _ => panic!("Expected Group beacon kind"),
    }
}

// ============================================================================
// SPEC FACTORY_TYPE MAPPING TESTS
// ============================================================================

#[test]
fn test_spec_factory_type_mappings_all() {
    // PreprocessorSpec mappings
    assert_eq!(
        PreprocessorSpec::Identity.factory_type(),
        ComponentFactoryType::IdentityPreprocessorFactory
    );
    assert_eq!(
        PreprocessorSpec::Threshold.factory_type(),
        ComponentFactoryType::ThresholdFactory
    );
    assert_eq!(
        PreprocessorSpec::TernaryToBinary.factory_type(),
        ComponentFactoryType::TernaryToBinaryFactory
    );
    assert_eq!(
        PreprocessorSpec::Argmax.factory_type(),
        ComponentFactoryType::ArgmaxFactory
    );

    // BaseFnSpec mappings
    assert_eq!(
        BaseFnSpec::CGBM.factory_type(),
        ComponentFactoryType::CGBMFactory
    );
    assert_eq!(
        BaseFnSpec::DGBM.factory_type(),
        ComponentFactoryType::DGBMFactory
    );

    // TransformSpec mappings
    assert_eq!(
        TransformSpec::Bounded.factory_type(),
        ComponentFactoryType::BoundedFactory
    );
    assert_eq!(
        TransformSpec::Unbounded.factory_type(),
        ComponentFactoryType::UnboundedFactory
    );

    // ComposerSpec mappings
    assert_eq!(
        ComposerSpec::WeightedSum.factory_type(),
        ComponentFactoryType::WeightedSumComponentFactory
    );

    // GroupFnSpec mappings
    assert_eq!(
        GroupFnSpec::Dominance.factory_type(),
        ComponentFactoryType::DominanceFactory
    );
    assert_eq!(
        GroupFnSpec::RelativeDominance.factory_type(),
        ComponentFactoryType::RelativeDominanceFactory
    );
    assert_eq!(
        GroupFnSpec::ContinuousAllocation.factory_type(),
        ComponentFactoryType::ContinuousAllocationFactory
    );
    assert_eq!(
        GroupFnSpec::DiscreteAllocation.factory_type(),
        ComponentFactoryType::DiscreteAllocationFactory
    );

    // GroupTransformSpec mappings
    assert_eq!(
        GroupTransformSpec::Softmax.factory_type(),
        ComponentFactoryType::SoftmaxFactory
    );
    assert_eq!(
        GroupTransformSpec::GMNormalize.factory_type(),
        ComponentFactoryType::GMNormalizeFactory
    );
}

// ============================================================================
// BEACON KIND ALL STANDALONE COMBOS (4 preprocessor x 2 basefn x 2 transform = 16)
// ============================================================================

#[test]
fn test_beacon_kind_all_standalone_combos_serde() {
    let preprocessors = [
        PreprocessorSpec::Identity,
        PreprocessorSpec::Threshold,
        PreprocessorSpec::TernaryToBinary,
        PreprocessorSpec::Argmax,
    ];
    let base_fns = [BaseFnSpec::CGBM, BaseFnSpec::DGBM];
    let transforms = [TransformSpec::Bounded, TransformSpec::Unbounded];

    let mut count = 0;
    for pp in &preprocessors {
        for bf in &base_fns {
            for tf in &transforms {
                let kind = BeaconKind::Standalone {
                    preprocessor: pp.clone(),
                    base_fn: bf.clone(),
                    transform: tf.clone(),
                };

                let json = serde_json::to_string(&kind).unwrap();
                let deserialized: BeaconKind = serde_json::from_str(&json).unwrap();

                match deserialized {
                    BeaconKind::Standalone {
                        preprocessor,
                        base_fn,
                        transform,
                    } => {
                        assert_eq!(preprocessor, *pp);
                        assert_eq!(base_fn, *bf);
                        assert_eq!(transform, *tf);
                    }
                    _ => panic!("Expected Standalone, got different variant"),
                }

                count += 1;
            }
        }
    }

    assert_eq!(count, 16, "must cover all 16 standalone combinations");
}

// ============================================================================
// BEACON KIND ALL GROUP COMBOS (4 groupfn x 2 grouptransform = 8)
// ============================================================================

#[test]
fn test_beacon_kind_all_group_combos_serde() {
    let group_fns = [
        GroupFnSpec::Dominance,
        GroupFnSpec::RelativeDominance,
        GroupFnSpec::ContinuousAllocation,
        GroupFnSpec::DiscreteAllocation,
    ];
    let group_transforms = [GroupTransformSpec::Softmax, GroupTransformSpec::GMNormalize];

    let mut count = 0;
    for gf in &group_fns {
        for gt in &group_transforms {
            let kind = BeaconKind::Group {
                group_fn: gf.clone(),
                group_transform: gt.clone(),
            };

            let json = serde_json::to_string(&kind).unwrap();
            let deserialized: BeaconKind = serde_json::from_str(&json).unwrap();

            match deserialized {
                BeaconKind::Group {
                    group_fn,
                    group_transform,
                } => {
                    assert_eq!(group_fn, *gf);
                    assert_eq!(group_transform, *gt);
                }
                _ => panic!("Expected Group, got different variant"),
            }

            count += 1;
        }
    }

    assert_eq!(count, 8, "must cover all 8 group combinations");
}

// ============================================================================
// BEACON KIND IDENTITY & COMPOSITE SERDE TESTS
// ============================================================================

#[test]
fn test_beacon_kind_identity_serde() {
    let kind = BeaconKind::Identity;
    let json = serde_json::to_string(&kind).unwrap();
    let deserialized: BeaconKind = serde_json::from_str(&json).unwrap();
    assert!(matches!(deserialized, BeaconKind::Identity));
}

#[test]
fn test_beacon_recipe_serde_identity() {
    let recipe = BeaconRecipe {
        slug: "identity".to_string(),
        name: "Identity".to_string(),
        description: Some("Simple identity beacon".to_string()),
        beacon_kind: BeaconKind::Identity,
        enabled: true,
        created_at: 1700000000,
        updated_at: 1700000000,
    };

    let json = serde_json::to_string(&recipe).unwrap();
    let deser: BeaconRecipe = serde_json::from_str(&json).unwrap();
    assert_eq!(deser.slug, "identity");
    assert!(matches!(deser.beacon_kind, BeaconKind::Identity));
}

#[test]
fn test_beacon_recipe_serde_composite() {
    let recipe = BeaconRecipe {
        slug: "weighted-sum".to_string(),
        name: "WeightedSum".to_string(),
        description: Some("WeightedSum composite beacon".to_string()),
        beacon_kind: BeaconKind::Composite {
            composer: ComposerSpec::WeightedSum,
        },
        enabled: true,
        created_at: 1700000000,
        updated_at: 1700000000,
    };

    let json = serde_json::to_string(&recipe).unwrap();
    let deser: BeaconRecipe = serde_json::from_str(&json).unwrap();
    assert_eq!(deser.slug, "weighted-sum");
    match deser.beacon_kind {
        BeaconKind::Composite { composer } => {
            assert_eq!(composer, ComposerSpec::WeightedSum);
        }
        _ => panic!("Expected Composite beacon kind"),
    }
}

// ============================================================================
// BEACON KIND REQUIRED FACTORY TYPES TESTS
// ============================================================================

#[test]
fn test_beacon_kind_required_factory_types_identity() {
    let kind = BeaconKind::Identity;
    let types = kind.required_factory_types();
    assert!(types.contains(&ComponentFactoryType::ECDSAVerifierFactory));
    assert!(types.contains(&ComponentFactoryType::IdentityBeaconFactory));
    assert_eq!(types.len(), 2);
}

#[test]
fn test_beacon_kind_required_factory_types_standalone() {
    let kind = BeaconKind::Standalone {
        preprocessor: PreprocessorSpec::Identity,
        base_fn: BaseFnSpec::CGBM,
        transform: TransformSpec::Bounded,
    };
    let types = kind.required_factory_types();
    assert!(types.contains(&ComponentFactoryType::ECDSAVerifierFactory));
    assert!(types.contains(&ComponentFactoryType::StandaloneBeaconFactory));
    assert!(types.contains(&ComponentFactoryType::IdentityPreprocessorFactory));
    assert!(types.contains(&ComponentFactoryType::CGBMFactory));
    assert!(types.contains(&ComponentFactoryType::BoundedFactory));
    assert_eq!(types.len(), 5);
}

#[test]
fn test_beacon_kind_required_factory_types_composite() {
    let kind = BeaconKind::Composite {
        composer: ComposerSpec::WeightedSum,
    };
    let types = kind.required_factory_types();
    assert!(types.contains(&ComponentFactoryType::ECDSAVerifierFactory));
    assert!(types.contains(&ComponentFactoryType::CompositeBeaconFactory));
    assert!(types.contains(&ComponentFactoryType::WeightedSumComponentFactory));
    assert_eq!(types.len(), 3);
}

#[test]
fn test_beacon_kind_required_factory_types_group() {
    let kind = BeaconKind::Group {
        group_fn: GroupFnSpec::Dominance,
        group_transform: GroupTransformSpec::Softmax,
    };
    let types = kind.required_factory_types();
    assert!(types.contains(&ComponentFactoryType::ECDSAVerifierFactory));
    assert!(types.contains(&ComponentFactoryType::GroupManagerFactory));
    assert!(types.contains(&ComponentFactoryType::DominanceFactory));
    assert!(types.contains(&ComponentFactoryType::SoftmaxFactory));
    assert_eq!(types.len(), 4);
}
