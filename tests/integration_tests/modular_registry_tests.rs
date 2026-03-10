// Integration tests for ComponentFactoryRegistry and RecipeRegistry (Redis-backed)

use alloy::primitives::address;
use serial_test::serial;
use the_beaconator::models::component_factory::{ComponentFactoryConfig, ComponentFactoryType};
use the_beaconator::models::recipe::*;
use the_beaconator::services::beacon::ComponentFactoryRegistry;
use the_beaconator::services::beacon::RecipeRegistry;

const REDIS_URL: &str = "redis://127.0.0.1:6379";

// ============================================================================
// COMPONENT FACTORY REGISTRY TESTS
// ============================================================================

#[tokio::test]
#[serial]
#[ignore = "requires Redis"]
async fn test_component_factory_registry_seed_and_list() {
    let registry = ComponentFactoryRegistry::with_prefix(REDIS_URL, "test-cfr-seed-and-list:")
        .await
        .expect("Failed to create ComponentFactoryRegistry");

    let configs = vec![
        ComponentFactoryConfig {
            factory_type: ComponentFactoryType::CGBMFactory,
            address: address!("0x1111111111111111111111111111111111111111"),
            enabled: true,
        },
        ComponentFactoryConfig {
            factory_type: ComponentFactoryType::DGBMFactory,
            address: address!("0x2222222222222222222222222222222222222222"),
            enabled: true,
        },
        ComponentFactoryConfig {
            factory_type: ComponentFactoryType::BoundedFactory,
            address: address!("0x3333333333333333333333333333333333333333"),
            enabled: true,
        },
    ];

    let result = registry.seed_defaults(&configs).await.unwrap();
    assert_eq!(result.seeded, 3);
    assert_eq!(result.skipped, 0);

    let factories = registry.list_factories().await.unwrap();
    assert_eq!(factories.len(), 3);

    registry.cleanup().await.unwrap();
}

#[tokio::test]
#[serial]
#[ignore = "requires Redis"]
async fn test_component_factory_registry_get_address() {
    let registry = ComponentFactoryRegistry::with_prefix(REDIS_URL, "test-cfr-get-address:")
        .await
        .expect("Failed to create ComponentFactoryRegistry");

    let expected_address = address!("0x4444444444444444444444444444444444444444");
    let configs = vec![ComponentFactoryConfig {
        factory_type: ComponentFactoryType::CGBMFactory,
        address: expected_address,
        enabled: true,
    }];

    registry.seed_defaults(&configs).await.unwrap();

    let addr = registry
        .get_factory_address(&ComponentFactoryType::CGBMFactory)
        .await
        .unwrap();
    assert_eq!(addr, expected_address);

    registry.cleanup().await.unwrap();
}

#[tokio::test]
#[serial]
#[ignore = "requires Redis"]
async fn test_component_factory_registry_disabled_factory_errors() {
    let registry = ComponentFactoryRegistry::with_prefix(REDIS_URL, "test-cfr-disabled-factory:")
        .await
        .expect("Failed to create ComponentFactoryRegistry");

    let configs = vec![ComponentFactoryConfig {
        factory_type: ComponentFactoryType::SoftmaxFactory,
        address: address!("0x5555555555555555555555555555555555555555"),
        enabled: false,
    }];

    registry.seed_defaults(&configs).await.unwrap();

    let result = registry
        .get_factory_address(&ComponentFactoryType::SoftmaxFactory)
        .await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.contains("disabled"),
        "Expected 'disabled' in error message, got: {err}"
    );

    registry.cleanup().await.unwrap();
}

#[tokio::test]
#[serial]
#[ignore = "requires Redis"]
async fn test_component_factory_registry_missing_factory_errors() {
    let registry = ComponentFactoryRegistry::with_prefix(REDIS_URL, "test-cfr-missing-factory:")
        .await
        .expect("Failed to create ComponentFactoryRegistry");

    let result = registry
        .get_factory_address(&ComponentFactoryType::ECDSAVerifierFactory)
        .await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.contains("not found"),
        "Expected 'not found' in error message, got: {err}"
    );

    registry.cleanup().await.unwrap();
}

#[tokio::test]
#[serial]
#[ignore = "requires Redis"]
async fn test_component_factory_registry_seed_idempotent() {
    let registry = ComponentFactoryRegistry::with_prefix(REDIS_URL, "test-cfr-seed-idempotent:")
        .await
        .expect("Failed to create ComponentFactoryRegistry");

    let configs = vec![
        ComponentFactoryConfig {
            factory_type: ComponentFactoryType::IdentityBeaconFactory,
            address: address!("0x6666666666666666666666666666666666666666"),
            enabled: true,
        },
        ComponentFactoryConfig {
            factory_type: ComponentFactoryType::StandaloneBeaconFactory,
            address: address!("0x7777777777777777777777777777777777777777"),
            enabled: true,
        },
        ComponentFactoryConfig {
            factory_type: ComponentFactoryType::CompositeBeaconFactory,
            address: address!("0x8888888888888888888888888888888888888888"),
            enabled: true,
        },
    ];

    let first = registry.seed_defaults(&configs).await.unwrap();
    assert_eq!(first.seeded, 3);
    assert_eq!(first.skipped, 0);

    let second = registry.seed_defaults(&configs).await.unwrap();
    assert_eq!(second.seeded, 0);
    assert_eq!(second.skipped, 3);

    registry.cleanup().await.unwrap();
}

#[tokio::test]
#[serial]
#[ignore = "requires Redis"]
async fn test_component_factory_registry_factory_exists() {
    let registry = ComponentFactoryRegistry::with_prefix(REDIS_URL, "test-cfr-factory-exists:")
        .await
        .expect("Failed to create ComponentFactoryRegistry");

    let configs = vec![ComponentFactoryConfig {
        factory_type: ComponentFactoryType::UnboundedFactory,
        address: address!("0x9999999999999999999999999999999999999999"),
        enabled: true,
    }];

    registry.seed_defaults(&configs).await.unwrap();

    let exists = registry
        .factory_exists(&ComponentFactoryType::UnboundedFactory)
        .await
        .unwrap();
    assert!(exists, "Seeded factory should exist");

    let missing = registry
        .factory_exists(&ComponentFactoryType::ThresholdFactory)
        .await
        .unwrap();
    assert!(!missing, "Non-seeded factory should not exist");

    registry.cleanup().await.unwrap();
}

// ============================================================================
// RECIPE REGISTRY TESTS
// ============================================================================

#[tokio::test]
#[serial]
#[ignore = "requires Redis"]
async fn test_recipe_registry_seed_standard_recipes() {
    let registry = RecipeRegistry::with_prefix(REDIS_URL, "test-rr-seed-standard:")
        .await
        .expect("Failed to create RecipeRegistry");

    let result = registry.seed_standard_recipes().await.unwrap();
    assert_eq!(
        result.seeded, 12,
        "Expected 12 standard recipes seeded, got: {}",
        result.seeded
    );
    assert_eq!(result.skipped, 0);

    registry.cleanup().await.unwrap();
}

#[tokio::test]
#[serial]
#[ignore = "requires Redis"]
async fn test_recipe_registry_get_recipe() {
    let registry = RecipeRegistry::with_prefix(REDIS_URL, "test-rr-get-recipe:")
        .await
        .expect("Failed to create RecipeRegistry");

    registry.seed_standard_recipes().await.unwrap();

    let recipe = registry
        .get_recipe("lbcgbm")
        .await
        .unwrap()
        .expect("lbcgbm recipe should exist after seeding");

    assert_eq!(recipe.slug, "lbcgbm");
    assert_eq!(recipe.name, "LBCGBM");
    assert!(recipe.enabled);

    match recipe.beacon_kind {
        BeaconKind::Standalone {
            preprocessor,
            base_fn,
            transform,
        } => {
            assert_eq!(preprocessor, PreprocessorSpec::Identity);
            assert_eq!(base_fn, BaseFnSpec::CGBM);
            assert_eq!(transform, TransformSpec::Bounded);
        }
        other => panic!(
            "Expected BeaconKind::Standalone for lbcgbm, got: {:?}",
            other
        ),
    }

    registry.cleanup().await.unwrap();
}

#[tokio::test]
#[serial]
#[ignore = "requires Redis"]
async fn test_recipe_registry_missing_recipe() {
    let registry = RecipeRegistry::with_prefix(REDIS_URL, "test-rr-missing-recipe:")
        .await
        .expect("Failed to create RecipeRegistry");

    let result = registry.get_recipe("nonexistent").await.unwrap();
    assert!(
        result.is_none(),
        "Expected None for nonexistent recipe, got: {:?}",
        result
    );

    registry.cleanup().await.unwrap();
}

#[tokio::test]
#[serial]
#[ignore = "requires Redis"]
async fn test_recipe_registry_seed_idempotent() {
    let registry = RecipeRegistry::with_prefix(REDIS_URL, "test-rr-seed-idempotent:")
        .await
        .expect("Failed to create RecipeRegistry");

    let first = registry.seed_standard_recipes().await.unwrap();
    assert_eq!(first.seeded, 12);
    assert_eq!(first.skipped, 0);

    let second = registry.seed_standard_recipes().await.unwrap();
    assert_eq!(second.seeded, 0);
    assert_eq!(second.skipped, 12);

    registry.cleanup().await.unwrap();
}

#[tokio::test]
#[serial]
#[ignore = "requires Redis"]
async fn test_recipe_registry_list_all() {
    let registry = RecipeRegistry::with_prefix(REDIS_URL, "test-rr-list-all:")
        .await
        .expect("Failed to create RecipeRegistry");

    registry.seed_standard_recipes().await.unwrap();

    let recipes = registry.list_recipes().await.unwrap();
    assert_eq!(
        recipes.len(),
        12,
        "Expected 12 standard recipes, got: {}",
        recipes.len()
    );

    registry.cleanup().await.unwrap();
}

#[tokio::test]
#[serial]
#[ignore = "requires Redis"]
async fn test_recipe_registry_register_custom() {
    let registry = RecipeRegistry::with_prefix(REDIS_URL, "test-rr-register-custom:")
        .await
        .expect("Failed to create RecipeRegistry");

    let custom_recipe = BeaconRecipe {
        slug: "custom-test-recipe".to_string(),
        name: "Custom Test Recipe".to_string(),
        description: Some("A custom recipe for testing".to_string()),
        beacon_kind: BeaconKind::Group {
            group_fn: GroupFnSpec::Dominance,
            group_transform: GroupTransformSpec::Softmax,
        },
        enabled: true,
        created_at: 1700000000,
        updated_at: 1700000000,
    };

    registry.register_recipe(&custom_recipe).await.unwrap();

    let fetched = registry
        .get_recipe("custom-test-recipe")
        .await
        .unwrap()
        .expect("Custom recipe should exist after registration");

    assert_eq!(fetched.slug, "custom-test-recipe");
    assert_eq!(fetched.name, "Custom Test Recipe");
    assert_eq!(
        fetched.description,
        Some("A custom recipe for testing".to_string())
    );
    assert!(fetched.enabled);

    match fetched.beacon_kind {
        BeaconKind::Group {
            group_fn,
            group_transform,
        } => {
            assert_eq!(group_fn, GroupFnSpec::Dominance);
            assert_eq!(group_transform, GroupTransformSpec::Softmax);
        }
        other => panic!("Expected BeaconKind::Group, got: {:?}", other),
    }

    registry.cleanup().await.unwrap();
}

#[tokio::test]
#[serial]
#[ignore = "requires Redis"]
async fn test_recipe_registry_register_duplicate_errors() {
    let registry = RecipeRegistry::with_prefix(REDIS_URL, "test-rr-register-duplicate:")
        .await
        .expect("Failed to create RecipeRegistry");

    let recipe = BeaconRecipe {
        slug: "duplicate-slug".to_string(),
        name: "First Recipe".to_string(),
        description: None,
        beacon_kind: BeaconKind::Identity,
        enabled: true,
        created_at: 1700000000,
        updated_at: 1700000000,
    };

    registry.register_recipe(&recipe).await.unwrap();

    let duplicate = BeaconRecipe {
        slug: "duplicate-slug".to_string(),
        name: "Second Recipe".to_string(),
        description: None,
        beacon_kind: BeaconKind::Identity,
        enabled: true,
        created_at: 1700000001,
        updated_at: 1700000001,
    };

    let result = registry.register_recipe(&duplicate).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.contains("already exists"),
        "Expected 'already exists' in error message, got: {err}"
    );

    registry.cleanup().await.unwrap();
}
