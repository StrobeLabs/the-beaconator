//! Redis-backed beacon recipe registry
//!
//! Manages beacon creation recipes. Standard recipes are seeded at startup.

use std::time::{SystemTime, UNIX_EPOCH};

use redis::AsyncCommands;

use crate::models::beacon_type::SeedResult;
use crate::models::recipe::*;
use crate::models::wallet::PrefixedRedisKeys;

/// Redis-backed registry of beacon creation recipes
pub struct RecipeRegistry {
    redis: redis::Client,
    keys: PrefixedRedisKeys,
}

impl RecipeRegistry {
    /// Create a new recipe registry with the default "beaconator:" prefix
    pub async fn new(redis_url: &str) -> Result<Self, String> {
        Self::with_prefix(redis_url, "beaconator:").await
    }

    /// Create a test stub that will fail on actual Redis operations.
    /// Use this in tests that don't exercise recipe registry functionality.
    pub fn test_stub() -> Self {
        let redis = redis::Client::open("redis://127.0.0.1:6379")
            .expect("Failed to create Redis client for test stub");
        Self {
            redis,
            keys: PrefixedRedisKeys::new("test-stub:"),
        }
    }

    /// Create a new recipe registry with a custom prefix (for test isolation)
    pub async fn with_prefix(redis_url: &str, prefix: &str) -> Result<Self, String> {
        let redis = redis::Client::open(redis_url)
            .map_err(|e| format!("Failed to connect to Redis: {e}"))?;

        // Test connection
        let mut conn = redis
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| format!("Failed to get Redis connection: {e}"))?;

        let _: String = redis::cmd("PING")
            .query_async(&mut conn)
            .await
            .map_err(|e| format!("Redis ping failed: {e}"))?;

        tracing::info!("RecipeRegistry connected to Redis with prefix '{}'", prefix);

        Ok(Self {
            redis,
            keys: PrefixedRedisKeys::new(prefix),
        })
    }

    /// Get a Redis connection
    async fn get_conn(&self) -> Result<redis::aio::MultiplexedConnection, String> {
        self.redis
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| format!("Redis connection failed: {e}"))
    }

    /// Get the key generator (useful for tests)
    pub fn keys(&self) -> &PrefixedRedisKeys {
        &self.keys
    }

    /// Get a specific recipe by slug
    pub async fn get_recipe(&self, slug: &str) -> Result<Option<BeaconRecipe>, String> {
        let mut conn = self.get_conn().await?;

        let config_json: Option<String> = conn
            .get(self.keys.beacon_recipe_config(slug))
            .await
            .map_err(|e| format!("Failed to get beacon recipe: {e}"))?;

        match config_json {
            Some(json) => {
                let recipe: BeaconRecipe = serde_json::from_str(&json)
                    .map_err(|e| format!("Failed to deserialize beacon recipe: {e}"))?;
                Ok(Some(recipe))
            }
            None => Ok(None),
        }
    }

    /// List all registered recipes
    pub async fn list_recipes(&self) -> Result<Vec<BeaconRecipe>, String> {
        let mut conn = self.get_conn().await?;

        let slugs: Vec<String> = conn
            .smembers(self.keys.beacon_recipes_set())
            .await
            .map_err(|e| format!("Failed to list beacon recipes: {e}"))?;

        let mut recipes = Vec::new();
        for slug in &slugs {
            match self.get_recipe(slug).await {
                Ok(Some(recipe)) => recipes.push(recipe),
                Ok(None) => {
                    tracing::warn!(
                        "Beacon recipe slug '{}' in set but config key missing",
                        slug
                    );
                }
                Err(e) => {
                    tracing::warn!("Failed to load beacon recipe '{}': {}", slug, e);
                }
            }
        }

        Ok(recipes)
    }

    /// Register a new recipe (errors if slug already exists)
    pub async fn register_recipe(&self, recipe: &BeaconRecipe) -> Result<(), String> {
        let mut conn = self.get_conn().await?;

        // Check if slug already exists
        let exists: bool = conn
            .sismember(self.keys.beacon_recipes_set(), &recipe.slug)
            .await
            .map_err(|e| format!("Failed to check beacon recipe existence: {e}"))?;

        if exists {
            return Err(format!("Beacon recipe '{}' already exists", recipe.slug));
        }

        let recipe_json = serde_json::to_string(recipe)
            .map_err(|e| format!("Failed to serialize beacon recipe: {e}"))?;

        // Atomic pipeline: add slug to set + store config
        let _: () = redis::pipe()
            .atomic()
            .sadd(self.keys.beacon_recipes_set(), &recipe.slug)
            .set(self.keys.beacon_recipe_config(&recipe.slug), recipe_json)
            .query_async(&mut conn)
            .await
            .map_err(|e| format!("Failed to register beacon recipe: {e}"))?;

        tracing::info!("Registered beacon recipe '{}'", recipe.slug);
        Ok(())
    }

    /// Check if a recipe exists
    pub async fn recipe_exists(&self, slug: &str) -> Result<bool, String> {
        let mut conn = self.get_conn().await?;

        conn.sismember(self.keys.beacon_recipes_set(), slug)
            .await
            .map_err(|e| format!("Failed to check beacon recipe existence: {e}"))
    }

    /// Clean up all recipe keys (for tests)
    pub async fn cleanup(&self) -> Result<(), String> {
        let mut conn = self.get_conn().await?;

        // Get all slugs first
        let slugs: Vec<String> = conn
            .smembers(self.keys.beacon_recipes_set())
            .await
            .map_err(|e| format!("Failed to list beacon recipes for cleanup: {e}"))?;

        if slugs.is_empty() {
            return Ok(());
        }

        // Build atomic pipeline to delete everything
        let mut pipe = redis::pipe();
        pipe.atomic();

        for slug in &slugs {
            pipe.del(self.keys.beacon_recipe_config(slug));
        }
        pipe.del(self.keys.beacon_recipes_set());

        let _: () = pipe
            .query_async(&mut conn)
            .await
            .map_err(|e| format!("Failed to cleanup beacon recipes: {e}"))?;

        tracing::info!("Cleaned up {} beacon recipe(s)", slugs.len());
        Ok(())
    }

    /// Seed the 12 standard beacon recipes.
    /// Only writes entries whose slugs do NOT already exist in Redis.
    pub async fn seed_standard_recipes(&self) -> Result<SeedResult, String> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| format!("Failed to get current time: {e}"))?
            .as_secs();

        let standard_recipes = vec![
            // Standalone recipes: preprocessor + baseFn + transform
            BeaconRecipe {
                slug: "lbcgbm".to_string(),
                name: "LBCGBM".to_string(),
                description: Some("Identity >> CGBM >> Bounded".to_string()),
                beacon_kind: BeaconKind::Standalone {
                    preprocessor: PreprocessorSpec::Identity,
                    base_fn: BaseFnSpec::CGBM,
                    transform: TransformSpec::Bounded,
                },
                enabled: true,
                created_at: now,
                updated_at: now,
            },
            BeaconRecipe {
                slug: "cgbm".to_string(),
                name: "CGBM".to_string(),
                description: Some("Identity >> CGBM >> Unbounded".to_string()),
                beacon_kind: BeaconKind::Standalone {
                    preprocessor: PreprocessorSpec::Identity,
                    base_fn: BaseFnSpec::CGBM,
                    transform: TransformSpec::Unbounded,
                },
                enabled: true,
                created_at: now,
                updated_at: now,
            },
            BeaconRecipe {
                slug: "lbdgbm".to_string(),
                name: "LBDGBM".to_string(),
                description: Some("Threshold >> DGBM >> Bounded".to_string()),
                beacon_kind: BeaconKind::Standalone {
                    preprocessor: PreprocessorSpec::Threshold,
                    base_fn: BaseFnSpec::DGBM,
                    transform: TransformSpec::Bounded,
                },
                enabled: true,
                created_at: now,
                updated_at: now,
            },
            BeaconRecipe {
                slug: "dgbm".to_string(),
                name: "DGBM".to_string(),
                description: Some("Threshold >> DGBM >> Unbounded".to_string()),
                beacon_kind: BeaconKind::Standalone {
                    preprocessor: PreprocessorSpec::Threshold,
                    base_fn: BaseFnSpec::DGBM,
                    transform: TransformSpec::Unbounded,
                },
                enabled: true,
                created_at: now,
                updated_at: now,
            },
            BeaconRecipe {
                slug: "ternary_lbcgbm".to_string(),
                name: "Ternary LBCGBM".to_string(),
                description: Some("TernaryToBinary >> CGBM >> Bounded".to_string()),
                beacon_kind: BeaconKind::Standalone {
                    preprocessor: PreprocessorSpec::TernaryToBinary,
                    base_fn: BaseFnSpec::CGBM,
                    transform: TransformSpec::Bounded,
                },
                enabled: true,
                created_at: now,
                updated_at: now,
            },
            BeaconRecipe {
                slug: "ternary_cgbm".to_string(),
                name: "Ternary CGBM".to_string(),
                description: Some("TernaryToBinary >> CGBM >> Unbounded".to_string()),
                beacon_kind: BeaconKind::Standalone {
                    preprocessor: PreprocessorSpec::TernaryToBinary,
                    base_fn: BaseFnSpec::CGBM,
                    transform: TransformSpec::Unbounded,
                },
                enabled: true,
                created_at: now,
                updated_at: now,
            },
            BeaconRecipe {
                slug: "ternary_lbdgbm".to_string(),
                name: "Ternary LBDGBM".to_string(),
                description: Some("TernaryToBinary >> DGBM >> Bounded".to_string()),
                beacon_kind: BeaconKind::Standalone {
                    preprocessor: PreprocessorSpec::TernaryToBinary,
                    base_fn: BaseFnSpec::DGBM,
                    transform: TransformSpec::Bounded,
                },
                enabled: true,
                created_at: now,
                updated_at: now,
            },
            BeaconRecipe {
                slug: "ternary_dgbm".to_string(),
                name: "Ternary DGBM".to_string(),
                description: Some("TernaryToBinary >> DGBM >> Unbounded".to_string()),
                beacon_kind: BeaconKind::Standalone {
                    preprocessor: PreprocessorSpec::TernaryToBinary,
                    base_fn: BaseFnSpec::DGBM,
                    transform: TransformSpec::Unbounded,
                },
                enabled: true,
                created_at: now,
                updated_at: now,
            },
            // Group recipes: groupFn + groupTransform
            BeaconRecipe {
                slug: "discrete_allocation".to_string(),
                name: "Discrete Allocation".to_string(),
                description: Some("DiscreteAllocation >> Softmax".to_string()),
                beacon_kind: BeaconKind::Group {
                    group_fn: GroupFnSpec::DiscreteAllocation,
                    group_transform: GroupTransformSpec::Softmax,
                },
                enabled: true,
                created_at: now,
                updated_at: now,
            },
            BeaconRecipe {
                slug: "continuous_allocation".to_string(),
                name: "Continuous Allocation".to_string(),
                description: Some("ContinuousAllocation >> Softmax".to_string()),
                beacon_kind: BeaconKind::Group {
                    group_fn: GroupFnSpec::ContinuousAllocation,
                    group_transform: GroupTransformSpec::Softmax,
                },
                enabled: true,
                created_at: now,
                updated_at: now,
            },
            BeaconRecipe {
                slug: "dominance".to_string(),
                name: "Dominance".to_string(),
                description: Some("Dominance >> GMNormalize".to_string()),
                beacon_kind: BeaconKind::Group {
                    group_fn: GroupFnSpec::Dominance,
                    group_transform: GroupTransformSpec::GMNormalize,
                },
                enabled: true,
                created_at: now,
                updated_at: now,
            },
            BeaconRecipe {
                slug: "relative_dominance".to_string(),
                name: "Relative Dominance".to_string(),
                description: Some("RelativeDominance >> GMNormalize".to_string()),
                beacon_kind: BeaconKind::Group {
                    group_fn: GroupFnSpec::RelativeDominance,
                    group_transform: GroupTransformSpec::GMNormalize,
                },
                enabled: true,
                created_at: now,
                updated_at: now,
            },
        ];

        let mut seeded = 0;
        let mut skipped = 0;

        for recipe in &standard_recipes {
            match self.recipe_exists(&recipe.slug).await? {
                true => {
                    tracing::debug!(
                        "Beacon recipe '{}' already exists, skipping seed",
                        recipe.slug
                    );
                    skipped += 1;
                }
                false => {
                    self.register_recipe(recipe).await?;
                    tracing::info!("Seeded beacon recipe '{}'", recipe.slug);
                    seeded += 1;
                }
            }
        }

        Ok(SeedResult { seeded, skipped })
    }
}
