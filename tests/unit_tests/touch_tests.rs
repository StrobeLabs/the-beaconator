//! Unit tests for the touch-on-update pure helpers (no network / no chain).

use std::str::FromStr;
use std::time::{Duration, Instant};

use alloy::primitives::Address;
use the_beaconator::services::touch::{
    dedup_preserving_order, entry_is_fresh, markets_url, parse_perp_addresses_from_json,
    touch_batch_gas_limit, touch_calldata, touch_calls,
};

#[test]
fn markets_url_has_no_trailing_slash_and_expected_query() {
    let beacon = Address::from_str("0x00000000000000000000000000000000000000ab").unwrap();
    let url = markets_url("https://bot.example.com", beacon, 500, 0);
    assert_eq!(
        url,
        "https://bot.example.com/markets?beacon=0x00000000000000000000000000000000000000ab&limit=500&offset=0"
    );
    // FastAPI 307-redirects /markets/ -> /markets; we must request without it.
    assert!(url.contains("/markets?"));
    assert!(!url.contains("/markets/"));
}

#[test]
fn markets_url_paginates_via_offset() {
    let beacon = Address::repeat_byte(0x11);
    let url = markets_url("http://botapi.testnet.perpcity.sst:8001", beacon, 500, 1000);
    assert!(url.ends_with("&limit=500&offset=1000"));
}

#[test]
fn parse_markets_json_extracts_perp_addresses() {
    let body = r#"{
        "items": [
            {"perp_address": "0x1111111111111111111111111111111111111111", "beacon_address": "0xabc"},
            {"perp_address": "0x2222222222222222222222222222222222222222"}
        ],
        "total": 2, "limit": 500, "offset": 0, "has_more": false
    }"#;
    let perps = parse_perp_addresses_from_json(body).unwrap();
    assert_eq!(
        perps,
        vec![
            Address::from_str("0x1111111111111111111111111111111111111111").unwrap(),
            Address::from_str("0x2222222222222222222222222222222222222222").unwrap(),
        ]
    );
}

#[test]
fn parse_markets_json_empty_items_is_empty() {
    let body = r#"{"items": [], "has_more": false}"#;
    assert!(parse_perp_addresses_from_json(body).unwrap().is_empty());
}

#[test]
fn parse_markets_json_skips_unparseable_addresses() {
    let body = r#"{"items": [
        {"perp_address": "not-an-address"},
        {"perp_address": "0x3333333333333333333333333333333333333333"}
    ]}"#;
    let perps = parse_perp_addresses_from_json(body).unwrap();
    assert_eq!(
        perps,
        vec![Address::from_str("0x3333333333333333333333333333333333333333").unwrap()]
    );
}

#[test]
fn parse_markets_json_malformed_is_error() {
    assert!(parse_perp_addresses_from_json("not json at all").is_err());
}

#[test]
fn entry_freshness_uses_ttl_for_nonempty() {
    let now = Instant::now();
    let ttl = Duration::from_secs(300);
    let empty_ttl = Duration::from_secs(60);
    assert!(entry_is_fresh(
        now,
        false,
        ttl,
        empty_ttl,
        now + Duration::from_secs(100)
    ));
    assert!(!entry_is_fresh(
        now,
        false,
        ttl,
        empty_ttl,
        now + Duration::from_secs(301)
    ));
}

#[test]
fn entry_freshness_uses_shorter_empty_ttl_for_empty() {
    let now = Instant::now();
    let ttl = Duration::from_secs(300);
    let empty_ttl = Duration::from_secs(60);
    // Within the empty TTL -> fresh.
    assert!(entry_is_fresh(
        now,
        true,
        ttl,
        empty_ttl,
        now + Duration::from_secs(30)
    ));
    // Past the empty TTL -> stale, even though it is still within the normal TTL.
    assert!(!entry_is_fresh(
        now,
        true,
        ttl,
        empty_ttl,
        now + Duration::from_secs(61)
    ));
}

#[test]
fn dedup_preserves_first_seen_order() {
    let a = Address::repeat_byte(1);
    let b = Address::repeat_byte(2);
    let c = Address::repeat_byte(3);
    let mut v = vec![a, b, a, c, b, a];
    dedup_preserving_order(&mut v);
    assert_eq!(v, vec![a, b, c]);
}

#[test]
fn touch_calldata_is_the_touch_selector() {
    // keccak256("touch()")[..4] == 0xa55526db
    let data = touch_calldata();
    assert_eq!(data.len(), 4);
    assert_eq!(data.as_ref(), [0xa5, 0x55, 0x26, 0xdb]);
}

#[test]
fn touch_calls_are_allow_failure_per_perp() {
    let perps = vec![Address::repeat_byte(0xaa), Address::repeat_byte(0xbb)];
    let calls = touch_calls(&perps);
    assert_eq!(calls.len(), 2);
    for (i, call) in calls.iter().enumerate() {
        assert!(
            call.allowFailure,
            "touch sub-calls must not revert the batch"
        );
        assert_eq!(call.target, perps[i]);
        assert_eq!(call.callData.as_ref(), [0xa5, 0x55, 0x26, 0xdb]);
    }
}

#[test]
fn touch_calls_empty_input_yields_no_calls() {
    assert!(touch_calls(&[]).is_empty());
}

#[test]
fn touch_batch_gas_limit_scales_per_perp() {
    // A single touch() runs ~130k gas on prod perps; the estimator cannot be
    // trusted with allowFailure=true (it starves the sub-calls), so the limit
    // must comfortably cover every sub-call plus batch overhead.
    let one = touch_batch_gas_limit(1);
    assert!(one >= 200_000, "one perp needs sub-call gas + overhead");
    let per_perp = touch_batch_gas_limit(2) - one;
    assert!(
        per_perp >= 150_000,
        "each extra perp needs its own allowance"
    );
    // Full default batch (50) must stay under Arbitrum's 32M block gas limit.
    assert!(touch_batch_gas_limit(50) < 32_000_000);
}
