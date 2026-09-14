// Module swap encoding tests.
//
// Golden vectors come from real Arbitrum Sepolia transactions, not from the
// implementation under test:
// - The 2026-07-03 priceImpact exec batch
//   0xfb01a3da9125988a685b36e1e9e5adf2383eb14299054b2c72b41d024dd123cd
//   (Safe execTransaction -> MultiSendCallOnly, 54 setter calls)
// - The 2026-07-23 submit + execute batches
//   0x0e9a9ac85a6c65707b1a347e38fbffe1d8fbbbd60606c539a6359a5ef444b1b7 /
//   0x7d2c2e0139073d31a28aeb1a7256a8918731d66085b7273f2fcbb4a30ad9c587
//   (52 perps swapped to 0xdA88...bD889)
// - EIP-712 Safe tx hashes cross-checked against the deployed testnet Safe's
//   own getTransactionHash view (Safe v1.4.1 at 0x6D82...13e7).

use alloy::primitives::{Address, address};
use the_beaconator::services::perp::module_swap::{
    MULTI_SEND_CALL_ONLY, ModuleType, SwapPhase, build_batch, multisend_calldata, multisend_item,
    submit_calldata,
};
use the_beaconator::services::safe::SafeTransactionService;

const NEW_MODULE: Address = address!("da88b0e8204c671106cfa8db98c69fa9019bd889");
const PERP: Address = address!("a77de9df6e08beb8f523153dd0110465190526e3");
const TESTNET_SAFE: Address = address!("6D826eD4097b19373b11aca5B00F378Af61C13e7");

/// setPriceImpactModule(0xdA88...bD889) calldata as it appears in the real
/// 2026-07-03 / 2026-07-23 batches.
const SETTER_HEX: &str = "a79cae0c000000000000000000000000da88b0e8204c671106cfa8db98c69fa9019bd889";

#[test]
fn test_setter_calldata_matches_onchain_payload() {
    let calldata = ModuleType::PriceImpact.setter_calldata(NEW_MODULE);
    assert_eq!(hex::encode(&calldata), SETTER_HEX);
}

#[test]
fn test_setter_selectors() {
    // Selectors of the six deployed timelocked setters (perpcity-contracts@v0.1.0).
    let cases = [
        (ModuleType::Beacon, "d42afb56"),
        (ModuleType::Pricing, "05d719f1"),
        (ModuleType::Funding, "348519fe"),
        (ModuleType::Fees, "f7e39255"),
        (ModuleType::MarginRatios, "717f1709"),
        (ModuleType::PriceImpact, "a79cae0c"),
    ];
    for (module_type, expected) in cases {
        let calldata = module_type.setter_calldata(NEW_MODULE);
        assert_eq!(
            hex::encode(&calldata[..4]),
            expected,
            "selector mismatch for {}",
            module_type.as_str()
        );
        assert_eq!(calldata.len(), 36);
    }
}

#[test]
fn test_submit_calldata_matches_onchain_payload() {
    // submit(bytes) wrapping the setter calldata, as sent in the 2026-07-23
    // submit batch: selector ef7fa71b, offset 0x20, length 0x24, payload padded
    // to a 32-byte boundary.
    let setter = ModuleType::PriceImpact.setter_calldata(NEW_MODULE);
    let calldata = submit_calldata(&setter);
    let expected = format!(
        "ef7fa71b\
         0000000000000000000000000000000000000000000000000000000000000020\
         0000000000000000000000000000000000000000000000000000000000000024\
         {SETTER_HEX}\
         00000000000000000000000000000000000000000000000000000000"
    );
    assert_eq!(hex::encode(&calldata), expected);
}

#[test]
fn test_multisend_item_matches_onchain_batch() {
    // First item of the real 2026-07-03 exec batch: operation 00, the perp
    // address, zero value, length 0x24, then the setter calldata.
    let setter = ModuleType::PriceImpact.setter_calldata(NEW_MODULE);
    let item = multisend_item(PERP, &setter);
    let expected = format!(
        "00\
         a77de9df6e08beb8f523153dd0110465190526e3\
         0000000000000000000000000000000000000000000000000000000000000000\
         0000000000000000000000000000000000000000000000000000000000000024\
         {SETTER_HEX}"
    );
    assert_eq!(hex::encode(&item), expected);
}

#[test]
fn test_build_batch_concatenates_items() {
    let setter = ModuleType::PriceImpact.setter_calldata(NEW_MODULE);
    let other: Address = address!("dd0ce5a9f8e60fa95b2a82c9b45847c1532acbf4");
    let batch = build_batch(&[PERP, other], &setter);
    let expected = [
        multisend_item(PERP, &setter),
        multisend_item(other, &setter),
    ]
    .concat();
    assert_eq!(batch, expected);
    assert_eq!(batch.len(), 2 * (85 + 36));
}

#[test]
fn test_multisend_calldata_wraps_batch() {
    // multiSend(bytes) selector is 8d80ff0a (as in the real execTransaction data).
    let setter = ModuleType::PriceImpact.setter_calldata(NEW_MODULE);
    let batch = build_batch(&[PERP], &setter);
    let calldata = multisend_calldata(&batch);
    assert_eq!(hex::encode(&calldata[..4]), "8d80ff0a");
    // ABI: offset word, length word, then the padded batch bytes.
    let batch_len = u64::from_be_bytes(calldata[60..68].try_into().unwrap());
    assert_eq!(batch_len as usize, batch.len());
    assert_eq!(&calldata[68..68 + batch.len()], batch.as_slice());
}

#[test]
fn test_multi_send_call_only_address() {
    // Canonical MultiSendCallOnly v1.3.0 (same on Arbitrum One and Sepolia);
    // the `to` of the real 2026-07 execTransaction calls.
    assert_eq!(
        MULTI_SEND_CALL_ONLY,
        address!("9641d764fc13c8B624c04430C7356C1C7C8102e2")
    );
}

#[test]
fn test_encode_safe_tx_hash_against_deployed_safe() {
    // Expected values returned by the deployed testnet Safe's getTransactionHash
    // for (to=MultiSendCallOnly, value=0, data=0xdeadbeef, operation, all gas
    // fields 0, nonce=5) on chain 421614 — fetched via cast on 2026-08-10.
    let data = hex::decode("deadbeef").unwrap();
    let call_hash = SafeTransactionService::encode_safe_tx_hash(
        TESTNET_SAFE,
        421614,
        MULTI_SEND_CALL_ONLY,
        &data,
        0,
        5,
    );
    assert_eq!(
        hex::encode(call_hash),
        "0765ad761be1d2048d9946da82668ead065f424946a54dab87f1a383ef9411f3"
    );
    let delegatecall_hash = SafeTransactionService::encode_safe_tx_hash(
        TESTNET_SAFE,
        421614,
        MULTI_SEND_CALL_ONLY,
        &data,
        1,
        5,
    );
    assert_eq!(
        hex::encode(delegatecall_hash),
        "45ce1517a35892bbe74697e5244f6c93c89957d79ccc0f7d6a1d523cedaf24b6"
    );
}

#[test]
fn test_module_type_parse_roundtrip() {
    for name in [
        "beacon",
        "pricing",
        "funding",
        "fees",
        "margin_ratios",
        "price_impact",
    ] {
        let parsed = ModuleType::parse(name).expect("known module type");
        assert_eq!(parsed.as_str(), name);
    }
    assert!(ModuleType::parse("priceImpact").is_none());
    assert!(ModuleType::parse("").is_none());
}

#[test]
fn test_swap_phase_parse() {
    assert_eq!(SwapPhase::parse(None), Some(SwapPhase::Both));
    assert_eq!(SwapPhase::parse(Some("both")), Some(SwapPhase::Both));
    assert_eq!(SwapPhase::parse(Some("submit")), Some(SwapPhase::Submit));
    assert_eq!(SwapPhase::parse(Some("execute")), Some(SwapPhase::Execute));
    assert_eq!(SwapPhase::parse(Some("Both")), None);
    assert_eq!(SwapPhase::parse(Some("all")), None);
}
