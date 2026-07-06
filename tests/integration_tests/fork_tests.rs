//! Fork-tier integration tests: the full beacon lifecycle against the REAL
//! deployed testnet contracts (Anvil fork of Arbitrum Sepolia), instead of
//! the hand-written mocks in tests/contracts.
//!
//! This is the tier that catches encoding drift the mock tests cannot: real
//! `createPerp` module-struct encoding, real event decoding, real revert
//! reasons — against the exact bytecode pinned by `.contracts-versions`
//! (addresses in tests/fork_config/addresses.testnet.json).
//!
//! Requirements (hence #[ignore] by default):
//!   FORK_RPC_URL  — an Arbitrum Sepolia RPC endpoint
//!   FORK_BLOCK    — optional pinned block (set it in CI: deterministic +
//!                   provider-cache-friendly)
//!   REDIS_URL     — wallet pool + component factory registry
//!
//! Run via `make test-fork`.

#[cfg(test)]
mod tests {
    use alloy::primitives::{Address, B256, U256};
    use serial_test::serial;
    use std::str::FromStr;
    use the_beaconator::models::recipe::{BeaconKind, BeaconRecipe};
    use the_beaconator::models::requests::ModularBeaconParams;
    use the_beaconator::models::{AppState, UpdateBeaconWithEcdsaRequest};
    use the_beaconator::routes::{IBeacon, IBeaconRegistry, IPerpFactory};
    use the_beaconator::services::beacon::core::{
        RegistrationOutcome, register_beacon_with_registry,
    };
    use the_beaconator::services::beacon::ecdsa::update_beacon_with_ecdsa;
    use the_beaconator::services::beacon::modular::create_modular_beacon;
    use the_beaconator::services::perp::core::deploy_perp_for_beacon;

    use crate::test_utils::{ForkFixture, adopt_ownership, create_fork_fixture};

    fn identity_recipe() -> BeaconRecipe {
        BeaconRecipe {
            slug: "fork-test-identity".to_string(),
            name: "Fork test identity beacon".to_string(),
            description: None,
            beacon_kind: BeaconKind::Identity,
            enabled: true,
            created_at: 0,
            updated_at: 0,
        }
    }

    async fn has_code(state: &AppState, address: Address) -> bool {
        use alloy::providers::Provider;
        state
            .provider
            .read_provider
            .get_code_at(address)
            .await
            .map(|code| !code.is_empty())
            .unwrap_or(false)
    }

    /// The whole production beacon-shipping flow, against real bytecode:
    /// create (real component factories) -> failed register (real revert
    /// from the un-adopted registry) -> register (after impersonated
    /// ownership handover) -> ECDSA update (real verifier) -> perp deploy
    /// (real createPerp module-struct encoding + PerpCreated decode).
    #[tokio::test]
    #[serial]
    #[ignore = "requires FORK_RPC_URL + REDIS_URL (run via make test-fork)"]
    async fn test_fork_full_beacon_lifecycle() {
        let ForkFixture {
            app_state,
            addresses,
            pool_wallet,
            anvil: _anvil,
        } = create_fork_fixture().await;

        // --- Create: real ECDSAVerifierFactory + IdentityBeaconFactory ---
        let params = ModularBeaconParams {
            initial_index: Some(1_000_000_000_000_000_000), // 1.0 WAD
            ..Default::default()
        };
        let created = create_modular_beacon(&app_state, &identity_recipe(), &params)
            .await
            .expect("create identity beacon against real factories");
        let beacon = created.beacon_address;
        let verifier = created.verifier_address.expect("verifier address");
        assert!(has_code(&app_state, beacon).await, "beacon has code");
        assert!(has_code(&app_state, verifier).await, "verifier has code");

        // The real beacon wires back to the real verifier.
        let beacon_contract = IBeacon::new(beacon, &*app_state.provider.read_provider);
        let wired_verifier = beacon_contract.verifier().call().await.expect("verifier()");
        assert_eq!(wired_verifier, verifier);

        // --- Deliberate revert: register against the registry while its  ---
        // --- owner is still the real testnet beaconator wallet           ---
        let err = register_beacon_with_registry(&app_state, beacon, addresses.perpcity_registry)
            .await
            .expect_err("register must revert while we are not the registry owner");
        assert!(
            err.to_lowercase().contains("unauthorized")
                || err.contains("0x82b42900") // solady Ownable.Unauthorized selector
                || err.to_lowercase().contains("revert"),
            "expected a decoded ownership revert, got: {err}"
        );

        // --- Adopt the registry (impersonated handover), then register ---
        adopt_ownership(
            &app_state.provider.read_provider,
            addresses.perpcity_registry,
            pool_wallet,
        )
        .await;
        let outcome =
            register_beacon_with_registry(&app_state, beacon, addresses.perpcity_registry)
                .await
                .expect("register after ownership handover");
        assert!(
            matches!(outcome, RegistrationOutcome::OnChainConfirmed(_)),
            "expected on-chain confirmation, got {outcome:?}"
        );
        let registry = IBeaconRegistry::new(
            addresses.perpcity_registry,
            &*app_state.provider.read_provider,
        );
        assert!(
            registry
                .isBeaconRegistered(beacon)
                .call()
                .await
                .expect("isBeaconRegistered"),
            "beacon must be registered on the real registry"
        );

        // --- ECDSA update: real verifier bytecode verifies our signature ---
        let new_index_q96: U256 = U256::from(2u128) << 96; // 2.0 in Q96
        let outcome = update_beacon_with_ecdsa(
            &app_state,
            UpdateBeaconWithEcdsaRequest {
                beacon_address: beacon.to_string(),
                measurement: vec![new_index_q96.to_string()],
            },
        )
        .await
        .expect("ECDSA update against real verifier");
        assert_ne!(outcome.tx_hash, B256::ZERO);
        let index = beacon_contract.index().call().await.expect("index()");
        assert_eq!(index, new_index_q96, "IndexUpdated must land the new value");

        // --- Perp deploy: real createPerp encoding + PerpCreated decode ---
        let response = deploy_perp_for_beacon(
            &app_state,
            beacon,
            pool_wallet,
            "Fork Test Market".to_string(),
            "FORK".to_string(),
            "ipfs://fork-test".to_string(),
            3600,
            B256::from(U256::from(0xf02c_u64)),
        )
        .await
        .expect("deploy perp against real factory");

        let perp = Address::from_str(&response.perp_address).expect("perp address");
        assert!(has_code(&app_state, perp).await, "perp has code");
        let factory = IPerpFactory::new(addresses.perp_factory, &*app_state.provider.read_provider);
        assert!(
            factory.perps(perp).call().await.expect("perps(perp)"),
            "factory must acknowledge the deployed perp"
        );
        assert!(
            !response.pool_id.is_empty() && response.pool_id != format!("0x{}", "00".repeat(32)),
            "pool id must decode from PerpCreated"
        );
    }
}
