//! Level 1 tests for the v1 beacon kind (plan row 2c): nonce sequence, heartbeat gate,
//! unconfirmed-receipt nonce re-read, kind detection and the v1 createPerp checks. The
//! on-chain `bind` and nonce tests are in integration_tests/v1_beacon_integration_tests.rs.

use alloy::primitives::{Address, B256, U256};
use the_beaconator::services::beacon::SkipReason;
use the_beaconator::services::beacon::v1::{
    self, HEARTBEAT_MIN_AGE_SECS, V1Gate, VerifierKind, decode_index_return,
};
use the_beaconator::services::perp::ContractErrorDecoder;
use the_beaconator::services::perp::v1::{
    CREATE_FRESHNESS_MARGIN_SECS, DeployV1PerpParams, MIN_V1_LATENCY_SECS, check_fresh_for_create,
    validate_params,
};

fn word(v: u64) -> [u8; 32] {
    U256::from(v).to_be_bytes::<32>()
}

#[test]
fn unit_tests_index_reply_length_selects_the_kind() {
    let legacy = decode_index_return(&word(7)).unwrap();
    assert_eq!(legacy.kind, VerifierKind::Legacy);
    assert_eq!(legacy.index, U256::from(7));
    assert_eq!(legacy.updated_at, None);

    let mut both = word(7).to_vec();
    both.extend_from_slice(&word(1_700_000_000));
    let modern = decode_index_return(&both).unwrap();
    assert_eq!(modern.kind, VerifierKind::V1);
    assert_eq!(modern.index, U256::from(7));
    assert_eq!(modern.updated_at, Some(U256::from(1_700_000_000u64)));

    assert!(decode_index_return(&[]).is_err());
    assert!(decode_index_return(&[0u8; 96]).is_err());
}

#[test]
fn unit_tests_nonce_sequence_is_last_nonce_plus_one() {
    assert_eq!(v1::next_nonce(U256::ZERO).unwrap(), U256::from(1));
    assert_eq!(v1::next_nonce(U256::from(1)).unwrap(), U256::from(2));
    assert_eq!(v1::next_nonce(U256::from(41)).unwrap(), U256::from(42));
    assert!(v1::next_nonce(U256::MAX).is_err());
}

#[test]
fn unit_tests_nonce_uses_the_confirmed_floor_against_a_lagging_replica() {
    let verifier = Address::repeat_byte(0xa1);
    // Nothing confirmed yet: the chain read decides.
    assert_eq!(
        v1::nonce_for_update(verifier, U256::from(3)).unwrap(),
        U256::from(4)
    );
    // Nonce 5 confirmed; a lagging replica still says 3. Sign 6, not 4.
    v1::record_confirmed_nonce(verifier, U256::from(5));
    assert_eq!(
        v1::nonce_for_update(verifier, U256::from(3)).unwrap(),
        U256::from(6)
    );
    // The chain moved past the floor (another instance): the chain read wins.
    assert_eq!(
        v1::nonce_for_update(verifier, U256::from(9)).unwrap(),
        U256::from(10)
    );
    // The floor never moves down.
    v1::record_confirmed_nonce(verifier, U256::from(2));
    assert_eq!(
        v1::nonce_for_update(verifier, U256::ZERO).unwrap(),
        U256::from(6)
    );
}

#[test]
fn unit_tests_heartbeat_skips_when_age_is_below_900s() {
    assert_eq!(HEARTBEAT_MIN_AGE_SECS, 900);
    assert_eq!(v1::gate(true, 0), V1Gate::SkipHeartbeat { age_secs: 0 });
    assert_eq!(v1::gate(true, 899), V1Gate::SkipHeartbeat { age_secs: 899 });
    assert_eq!(v1::gate(true, 900), V1Gate::PublishNow { age_secs: 900 });
    assert_eq!(v1::gate(true, 5000), V1Gate::PublishNow { age_secs: 5000 });
}

#[test]
fn unit_tests_stale_v1_beacon_publishes_past_the_rate_limit() {
    // A normal update on a fresh beacon keeps the normal gates.
    assert_eq!(v1::gate(false, 10), V1Gate::Normal { age_secs: 10 });
    assert_eq!(v1::gate(false, 899), V1Gate::Normal { age_secs: 899 });
    // At 900 s a normal update publishes now, even if the value is unchanged.
    assert_eq!(v1::gate(false, 900), V1Gate::PublishNow { age_secs: 900 });
}

#[test]
fn unit_tests_age_is_clock_minus_updated_at() {
    assert_eq!(v1::age_secs(U256::from(1_000u64), 1_900), 900);
    // A future-dated observation counts as fresh, never as a huge age.
    assert_eq!(v1::age_secs(U256::from(2_000u64), 1_900), 0);
    // An updatedAt that does not fit in u64 counts as age 0.
    assert_eq!(v1::age_secs(U256::MAX, 1_900), 0);
}

#[test]
fn unit_tests_unconfirmed_receipt_rereads_last_nonce() {
    let sent = U256::from(5);
    // lastNonce still 4: not on chain; the beaconator keeps the lock and records nothing.
    assert!(!v1::landed(U256::from(4), sent));
    // lastNonce reached our nonce: the update is on chain; report it as confirmed.
    assert!(v1::landed(U256::from(5), sent));
    assert!(v1::landed(U256::from(6), sent));
}

#[test]
fn unit_tests_heartbeat_skip_reason_names_the_age() {
    let reason = SkipReason::HeartbeatNotDue { age_secs: 120 };
    let text = reason.to_string();
    assert!(text.contains("120s"), "{text}");
    assert!(text.contains("900s"), "{text}");
}

#[test]
fn unit_tests_kind_cache_remembers_each_beacon() {
    let beacon = Address::repeat_byte(0xb2);
    assert_eq!(v1::known_kind(beacon), None);
    v1::remember_kind(beacon, VerifierKind::Legacy);
    assert_eq!(v1::known_kind(beacon), Some(VerifierKind::Legacy));
}

fn params() -> DeployV1PerpParams {
    DeployV1PerpParams {
        beacon: Address::repeat_byte(1),
        owner: Address::repeat_byte(2),
        governor: Address::repeat_byte(3),
        ema_window: 3600,
        latency: 3600,
        salt: B256::ZERO,
        withdrawal_capacity: 0,
        withdrawal_refill_per_second: 0,
        withdrawal_challenge_window: 86_400,
    }
}

#[test]
fn unit_tests_v1_create_perp_params_are_checked() {
    assert!(validate_params(&params()).is_ok());

    let mut p = params();
    p.ema_window = 0;
    assert!(validate_params(&p).is_err());

    let mut p = params();
    p.latency = MIN_V1_LATENCY_SECS - 1;
    assert!(validate_params(&p).is_err());

    let mut p = params();
    p.withdrawal_challenge_window = 0;
    assert!(validate_params(&p).is_err());

    let mut p = params();
    p.governor = Address::ZERO;
    assert!(validate_params(&p).is_err());
}

#[test]
fn unit_tests_v1_create_perp_needs_a_fresh_beacon() {
    let latency = 3600u32;
    let limit = u64::from(latency) - CREATE_FRESHNESS_MARGIN_SECS;
    assert!(check_fresh_for_create(0, latency).is_ok());
    assert!(check_fresh_for_create(limit, latency).is_ok());
    assert!(check_fresh_for_create(limit + 1, latency).is_err());
}

#[test]
fn unit_tests_v1_error_selectors_decode() {
    for (selector, name) in [
        ("0x61200552", "StaleBeaconIndex"),
        ("0x6c776dda", "BeaconIndexUnreadable"),
        ("0x756688fe", "InvalidNonce"),
        ("0x682a9065", "AlreadyBound"),
        ("0xfef019bb", "UnauthorizedBinder"),
    ] {
        let decoded = ContractErrorDecoder::decode_error_data(selector).unwrap();
        assert!(decoded.starts_with(name), "{selector}: {decoded}");
    }
}

// ---------------------------------------------------------------------------
// v1 measurement signer (V1_SIGNER_KMS_KEY_ID). A local key stands in for the
// KMS key: `PoolSigner` gives both the same address and `sign_hash` surface.
// ---------------------------------------------------------------------------

mod v1_signer {
    use alloy::primitives::{Address, B256};
    use alloy::signers::local::PrivateKeySigner;
    use the_beaconator::check_expected_v1_signer;
    use the_beaconator::models::app_state::{MeasurementSigner, WalletConfig};
    use the_beaconator::services::wallet::PoolSigner;

    use crate::test_utils::create_simple_test_app_state;

    async fn wallets_with_v1_signer() -> (WalletConfig, Address, Address) {
        let mut wallets = create_simple_test_app_state().await.wallets;
        let kms_stand_in = PrivateKeySigner::random();
        let v1_address = kms_stand_in.address();
        assert_ne!(v1_address, wallets.signer.address());
        wallets.v1_signer = Some(PoolSigner::Local(kms_stand_in));
        let legacy_address = wallets.signer.address();
        (wallets, legacy_address, v1_address)
    }

    #[tokio::test]
    async fn unit_tests_new_v1_verifiers_name_the_kms_signer_when_configured() {
        let (wallets, legacy_address, v1_address) = wallets_with_v1_signer().await;
        assert_eq!(wallets.v1_signer_address(), v1_address);
        // The legacy create path keeps naming PRIVATE_KEY.
        assert_eq!(wallets.signer.address(), legacy_address);
    }

    #[tokio::test]
    async fn unit_tests_new_v1_verifiers_name_private_key_without_a_kms_signer() {
        let wallets = create_simple_test_app_state().await.wallets;
        assert!(wallets.v1_signer.is_none());
        assert_eq!(wallets.v1_signer_address(), wallets.signer.address());
    }

    #[tokio::test]
    async fn unit_tests_each_beacon_is_signed_by_the_key_its_verifier_names() {
        let (wallets, legacy_address, v1_address) = wallets_with_v1_signer().await;

        let legacy = wallets.measurement_signer_for(legacy_address).unwrap();
        assert!(matches!(legacy, MeasurementSigner::PrivateKey(_)));
        assert_eq!(legacy.address(), legacy_address);
        assert_eq!(legacy.label(), "PRIVATE_KEY");

        let v1 = wallets.measurement_signer_for(v1_address).unwrap();
        assert!(matches!(v1, MeasurementSigner::V1Kms(_)));
        assert_eq!(v1.address(), v1_address);
        assert_eq!(v1.label(), "V1_SIGNER_KMS_KEY_ID");

        // A verifier that names a key this service does not hold gets no signer,
        // rather than a signature the verifier would reject on chain.
        assert!(
            wallets
                .measurement_signer_for(Address::repeat_byte(0x42))
                .is_none()
        );
    }

    #[tokio::test]
    async fn unit_tests_a_v1_beacon_bound_to_private_key_stays_on_private_key() {
        // A v1 beacon created before the KMS signer was configured names
        // PRIVATE_KEY. Adding the KMS signer must not move it.
        let (wallets, legacy_address, _) = wallets_with_v1_signer().await;
        let signer = wallets.measurement_signer_for(legacy_address).unwrap();
        assert!(matches!(signer, MeasurementSigner::PrivateKey(_)));
    }

    #[tokio::test]
    async fn unit_tests_without_a_kms_signer_only_private_key_matches() {
        let wallets = create_simple_test_app_state().await.wallets;
        assert!(
            wallets
                .measurement_signer_for(wallets.signer.address())
                .is_some()
        );
        assert!(
            wallets
                .measurement_signer_for(Address::repeat_byte(0x42))
                .is_none()
        );
    }

    #[tokio::test]
    async fn unit_tests_the_selected_signer_produces_a_signature_from_its_own_address() {
        let (wallets, legacy_address, v1_address) = wallets_with_v1_signer().await;
        let digest = B256::repeat_byte(0x5a);
        for designated in [legacy_address, v1_address] {
            let signer = wallets.measurement_signer_for(designated).unwrap();
            let signature = signer.sign_hash(&digest).await.unwrap();
            // What ECDSAVerifier checks: the recovered address is SIGNER.
            assert_eq!(
                signature.recover_address_from_prehash(&digest).unwrap(),
                designated
            );
        }
    }

    #[test]
    fn unit_tests_expected_v1_signer_guard() {
        let a = Address::repeat_byte(0xbe);
        let b = Address::repeat_byte(0x93);
        // No expectation: any signer is accepted.
        assert!(check_expected_v1_signer(None, a, "PRIVATE_KEY").is_ok());
        // Matching expectation.
        assert!(check_expected_v1_signer(Some(a), a, "V1_SIGNER_KMS_KEY_ID").is_ok());
        // A different key must stop startup, and the message names both.
        let err = check_expected_v1_signer(Some(a), b, "PRIVATE_KEY").unwrap_err();
        assert!(err.contains(&a.to_string()));
        assert!(err.contains(&b.to_string()));
        assert!(err.contains("PRIVATE_KEY"));
    }
}
