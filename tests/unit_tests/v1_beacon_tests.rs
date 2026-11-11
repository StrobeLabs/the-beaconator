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
    CREATE_FRESHNESS_MARGIN_SECS, DeployV1PerpParams, MIN_V1_LATENCY_SECS,
    check_fresh_for_create, validate_params,
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
