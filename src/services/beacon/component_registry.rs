//! Redis-backed component factory registry
//!
//! Stores the addresses of the 20 deployed component factory contracts.
//! Factory addresses are pre-seeded into Redis before deployment.

use redis::AsyncCommands;
use redis::aio::ConnectionManager;

use crate::models::beacon_type::SeedResult;
use crate::models::component_factory::{ComponentFactoryConfig, ComponentFactoryType};
use crate::models::wallet::PrefixedRedisKeys;
use alloy::primitives::Address;

/// Redis-backed registry of component factory addresses
pub struct ComponentFactoryRegistry {
    /// Shared auto-reconnecting connection; None only for test stubs
    conn: Option<ConnectionManager>,
    keys: PrefixedRedisKeys,
}

impl ComponentFactoryRegistry {
    /// Create a new component factory registry with the default "beaconator:" prefix
    pub async fn new(redis_url: &str) -> Result<Self, String> {
        Self::with_prefix(redis_url, "beaconator:").await
    }

    /// Create a test stub that will fail on actual Redis operations.
    /// Use this in tests that don't exercise component factory registry functionality.
    pub fn test_stub() -> Self {
        Self {
            conn: None,
            keys: PrefixedRedisKeys::new("test-stub:"),
        }
    }

    /// Create a new component factory registry with a custom prefix (for test isolation)
    pub async fn with_prefix(redis_url: &str, prefix: &str) -> Result<Self, String> {
        let redis = redis::Client::open(redis_url)
            .map_err(|e| format!("Failed to connect to Redis: {e}"))?;

        // One auto-reconnecting connection, cloned per operation (avoids a fresh
        // TLS handshake per Redis command).
        let mut conn = ConnectionManager::new(redis)
            .await
            .map_err(|e| format!("Failed to get Redis connection: {e}"))?;

        let _: String = redis::cmd("PING")
            .query_async(&mut conn)
            .await
            .map_err(|e| format!("Redis ping failed: {e}"))?;

        tracing::info!(
            "ComponentFactoryRegistry connected to Redis with prefix '{}'",
            prefix
        );

        Ok(Self {
            conn: Some(conn),
            keys: PrefixedRedisKeys::new(prefix),
        })
    }

    /// Get a Redis connection (cheap clone of the shared auto-reconnecting manager)
    fn get_conn(&self) -> Result<ConnectionManager, String> {
        self.conn
            .clone()
            .ok_or_else(|| "Redis connection not available (test stub)".to_string())
    }

    /// Get the key generator (useful for tests)
    pub fn keys(&self) -> &PrefixedRedisKeys {
        &self.keys
    }

    /// Get the address for a specific factory type
    pub async fn get_factory_address(
        &self,
        factory_type: &ComponentFactoryType,
    ) -> Result<Address, String> {
        let type_name = factory_type.to_string();
        let mut conn = self.get_conn()?;

        let config_json: Option<String> =
            conn.get(self.keys.component_factory(&type_name))
                .await
                .map_err(|e| format!("Failed to get component factory: {e}"))?;

        match config_json {
            Some(json) => {
                let config: ComponentFactoryConfig = serde_json::from_str(&json)
                    .map_err(|e| format!("Failed to deserialize component factory config: {e}"))?;
                if !config.enabled {
                    return Err(format!("Component factory '{type_name}' is disabled"));
                }
                Ok(config.address)
            }
            None => Err(format!(
                "Component factory '{type_name}' not found in Redis",
            )),
        }
    }

    /// List all registered factories
    pub async fn list_factories(&self) -> Result<Vec<ComponentFactoryConfig>, String> {
        let mut conn = self.get_conn()?;

        let type_names: Vec<String> = conn
            .smembers(self.keys.component_factories_set())
            .await
            .map_err(|e| format!("Failed to list component factories: {e}"))?;

        let mut configs = Vec::new();
        for type_name in &type_names {
            let config_json: Option<String> = conn
                .get(self.keys.component_factory(type_name))
                .await
                .map_err(|e| format!("Failed to get component factory: {e}"))?;

            match config_json {
                Some(json) => match serde_json::from_str::<ComponentFactoryConfig>(&json) {
                    Ok(config) => configs.push(config),
                    Err(e) => {
                        tracing::warn!(
                            "Failed to deserialize component factory '{}': {}",
                            type_name,
                            e
                        );
                    }
                },
                None => {
                    tracing::warn!(
                        "Component factory type '{}' in set but config key missing",
                        type_name
                    );
                }
            }
        }

        Ok(configs)
    }

    /// Seed defaults (only writes entries whose types don't exist)
    pub async fn seed_defaults(
        &self,
        configs: &[ComponentFactoryConfig],
    ) -> Result<SeedResult, String> {
        let mut seeded = 0;
        let mut skipped = 0;

        for config in configs {
            match self.factory_exists(&config.factory_type).await? {
                true => {
                    tracing::debug!(
                        "Component factory '{}' already exists, skipping seed",
                        config.factory_type
                    );
                    skipped += 1;
                }
                false => {
                    self.register_factory(config).await?;
                    tracing::info!("Seeded component factory '{}'", config.factory_type);
                    seeded += 1;
                }
            }
        }

        Ok(SeedResult { seeded, skipped })
    }

    /// Register a new component factory (errors if type already exists)
    async fn register_factory(&self, config: &ComponentFactoryConfig) -> Result<(), String> {
        let type_name = config.factory_type.to_string();
        let mut conn = self.get_conn()?;

        // Check if type already exists
        let exists: bool = conn
            .sismember(self.keys.component_factories_set(), &type_name)
            .await
            .map_err(|e| format!("Failed to check component factory existence: {e}"))?;

        if exists {
            return Err(format!("Component factory '{type_name}' already exists"));
        }

        let config_json = serde_json::to_string(config)
            .map_err(|e| format!("Failed to serialize component factory config: {e}"))?;

        // Atomic pipeline: add type name to set + store config
        let _: () = redis::pipe()
            .atomic()
            .sadd(self.keys.component_factories_set(), &type_name)
            .set(self.keys.component_factory(&type_name), config_json)
            .query_async(&mut conn)
            .await
            .map_err(|e| format!("Failed to register component factory: {e}"))?;

        tracing::info!("Registered component factory '{}'", type_name);
        Ok(())
    }

    /// Check if a factory type exists
    pub async fn factory_exists(
        &self,
        factory_type: &ComponentFactoryType,
    ) -> Result<bool, String> {
        let type_name = factory_type.to_string();
        let mut conn = self.get_conn()?;

        conn.sismember(self.keys.component_factories_set(), &type_name)
            .await
            .map_err(|e| format!("Failed to check component factory existence: {e}"))
    }

    /// Clean up all component factory keys (for tests)
    pub async fn cleanup(&self) -> Result<(), String> {
        let mut conn = self.get_conn()?;

        // Get all type names first
        let type_names: Vec<String> = conn
            .smembers(self.keys.component_factories_set())
            .await
            .map_err(|e| format!("Failed to list component factories for cleanup: {e}"))?;

        if type_names.is_empty() {
            return Ok(());
        }

        // Build atomic pipeline to delete everything
        let mut pipe = redis::pipe();
        pipe.atomic();

        for type_name in &type_names {
            pipe.del(self.keys.component_factory(type_name));
        }
        pipe.del(self.keys.component_factories_set());

        let _: () = pipe
            .query_async(&mut conn)
            .await
            .map_err(|e| format!("Failed to cleanup component factories: {e}"))?;

        tracing::info!("Cleaned up {} component factory(ies)", type_names.len());
        Ok(())
    }
}
