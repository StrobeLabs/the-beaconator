//! Redis-backed beacon type registry
//!
//! Manages beacon type configurations in Redis, allowing dynamic
//! registration of factory addresses, beacon types, and metadata
//! without requiring redeployment.

use redis::AsyncCommands;

use crate::models::beacon_type::{BeaconTypeConfig, SeedResult};
use crate::models::wallet::PrefixedRedisKeys;

/// Redis-backed registry of beacon type configurations
pub struct BeaconTypeRegistry {
    redis: redis::Client,
    keys: PrefixedRedisKeys,
}

impl BeaconTypeRegistry {
    /// Create a new beacon type registry with the default "beaconator:" prefix
    pub async fn new(redis_url: &str) -> Result<Self, String> {
        Self::with_prefix(redis_url, "beaconator:").await
    }

    /// Create a test stub that will fail on actual Redis operations.
    /// Use this in tests that don't exercise beacon type registry functionality.
    pub fn test_stub() -> Self {
        let redis = redis::Client::open("redis://127.0.0.1:6379")
            .expect("Failed to create Redis client for test stub");
        Self {
            redis,
            keys: PrefixedRedisKeys::new("test-stub:"),
        }
    }

    /// Create a new beacon type registry with a custom prefix (for test isolation)
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

        tracing::info!(
            "BeaconTypeRegistry connected to Redis with prefix '{}'",
            prefix
        );

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

    /// List all registered beacon types
    pub async fn list_types(&self) -> Result<Vec<BeaconTypeConfig>, String> {
        let mut conn = self.get_conn().await?;

        let slugs: Vec<String> = conn
            .smembers(self.keys.beacon_types_set())
            .await
            .map_err(|e| format!("Failed to list beacon types: {e}"))?;

        let mut configs = Vec::new();
        for slug in &slugs {
            match self.get_type(slug).await {
                Ok(Some(config)) => configs.push(config),
                Ok(None) => {
                    tracing::warn!("Beacon type slug '{}' in set but config key missing", slug);
                }
                Err(e) => {
                    tracing::warn!("Failed to load beacon type '{}': {}", slug, e);
                }
            }
        }

        Ok(configs)
    }

    /// Get a specific beacon type by slug
    pub async fn get_type(&self, slug: &str) -> Result<Option<BeaconTypeConfig>, String> {
        let mut conn = self.get_conn().await?;

        let config_json: Option<String> = conn
            .get(self.keys.beacon_type_config(slug))
            .await
            .map_err(|e| format!("Failed to get beacon type: {e}"))?;

        match config_json {
            Some(json) => {
                let config: BeaconTypeConfig = serde_json::from_str(&json)
                    .map_err(|e| format!("Failed to deserialize beacon type config: {e}"))?;
                Ok(Some(config))
            }
            None => Ok(None),
        }
    }

    /// Register a new beacon type (errors if slug already exists)
    pub async fn register_type(&self, config: &BeaconTypeConfig) -> Result<(), String> {
        let mut conn = self.get_conn().await?;

        // Check if slug already exists
        let exists: bool = conn
            .sismember(self.keys.beacon_types_set(), &config.slug)
            .await
            .map_err(|e| format!("Failed to check beacon type existence: {e}"))?;

        if exists {
            return Err(format!("Beacon type '{}' already exists", config.slug));
        }

        let config_json = serde_json::to_string(config)
            .map_err(|e| format!("Failed to serialize beacon type config: {e}"))?;

        // Atomic pipeline: add slug to set + store config
        let _: () = redis::pipe()
            .atomic()
            .sadd(self.keys.beacon_types_set(), &config.slug)
            .set(self.keys.beacon_type_config(&config.slug), config_json)
            .query_async(&mut conn)
            .await
            .map_err(|e| format!("Failed to register beacon type: {e}"))?;

        tracing::info!("Registered beacon type '{}'", config.slug);
        Ok(())
    }

    /// Update an existing beacon type config
    pub async fn update_type(&self, slug: &str, updated: &BeaconTypeConfig) -> Result<(), String> {
        let mut conn = self.get_conn().await?;

        // Verify it exists
        let exists: bool = conn
            .sismember(self.keys.beacon_types_set(), slug)
            .await
            .map_err(|e| format!("Failed to check beacon type existence: {e}"))?;

        if !exists {
            return Err(format!("Beacon type '{slug}' not found"));
        }

        let config_json = serde_json::to_string(updated)
            .map_err(|e| format!("Failed to serialize beacon type config: {e}"))?;

        let _: () = conn
            .set(self.keys.beacon_type_config(slug), config_json)
            .await
            .map_err(|e| format!("Failed to update beacon type: {e}"))?;

        tracing::info!("Updated beacon type '{}'", slug);
        Ok(())
    }

    /// Delete a beacon type
    pub async fn delete_type(&self, slug: &str) -> Result<(), String> {
        let mut conn = self.get_conn().await?;

        // Verify it exists
        let exists: bool = conn
            .sismember(self.keys.beacon_types_set(), slug)
            .await
            .map_err(|e| format!("Failed to check beacon type existence: {e}"))?;

        if !exists {
            return Err(format!("Beacon type '{slug}' not found"));
        }

        // Atomic pipeline: remove from set + delete config
        let _: () = redis::pipe()
            .atomic()
            .srem(self.keys.beacon_types_set(), slug)
            .del(self.keys.beacon_type_config(slug))
            .query_async(&mut conn)
            .await
            .map_err(|e| format!("Failed to delete beacon type: {e}"))?;

        tracing::info!("Deleted beacon type '{}'", slug);
        Ok(())
    }

    /// Check if a beacon type exists
    pub async fn type_exists(&self, slug: &str) -> Result<bool, String> {
        let mut conn = self.get_conn().await?;

        conn.sismember(self.keys.beacon_types_set(), slug)
            .await
            .map_err(|e| format!("Failed to check beacon type existence: {e}"))
    }

    /// Seed default beacon types from a list of configs.
    /// Only writes entries whose slugs do NOT already exist in Redis.
    pub async fn seed_defaults(&self, configs: &[BeaconTypeConfig]) -> Result<SeedResult, String> {
        let mut seeded = 0;
        let mut skipped = 0;

        for config in configs {
            match self.type_exists(&config.slug).await? {
                true => {
                    tracing::debug!(
                        "Beacon type '{}' already exists, skipping seed",
                        config.slug
                    );
                    skipped += 1;
                }
                false => {
                    self.register_type(config).await?;
                    tracing::info!("Seeded beacon type '{}'", config.slug);
                    seeded += 1;
                }
            }
        }

        Ok(SeedResult { seeded, skipped })
    }

    /// Clean up all beacon type keys with our prefix (for testing)
    pub async fn cleanup(&self) -> Result<(), String> {
        let mut conn = self.get_conn().await?;

        // Get all slugs first
        let slugs: Vec<String> = conn
            .smembers(self.keys.beacon_types_set())
            .await
            .map_err(|e| format!("Failed to list beacon types for cleanup: {e}"))?;

        if slugs.is_empty() {
            return Ok(());
        }

        // Build atomic pipeline to delete everything
        let mut pipe = redis::pipe();
        pipe.atomic();

        for slug in &slugs {
            pipe.del(self.keys.beacon_type_config(slug));
        }
        pipe.del(self.keys.beacon_types_set());

        let _: () = pipe
            .query_async(&mut conn)
            .await
            .map_err(|e| format!("Failed to cleanup beacon types: {e}"))?;

        tracing::info!("Cleaned up {} beacon type(s)", slugs.len());
        Ok(())
    }
}
