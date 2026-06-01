//! Regression tests for `abis/IdentityBeacon.bytecode`.
//!
//! The on-disk bytecode is what `services::beacon::verifiable::deploy_identity_beacon`
//! ships in `eth_sendTransaction` when creating a fresh IdentityBeacon (the bytes are
//! loaded into `AppState.contracts.identity_beacon_bytecode` at startup, see `lib.rs`).
//!
//! On 2026-05-29 we discovered that the file had been stale for months: it was
//! compiled before `beacons` added `BindingLib`, so the constructor no longer
//! called `BindingLib.bindComponent(_verifier)`. Every beacon ever created via
//! `POST /create_beacon_with_ecdsa` on the mainnet deploy ended up with
//! `verifier.authorizedCaller == address(0)`, and every subsequent
//! `IdentityBeacon.update(...)` call reverted with `UnauthorizedCaller`
//! (`0x5c427cd9`).
//!
//! These tests pin the bytecode to a shape that includes the `BindingLib`
//! constructor wiring, so a future `make refresh-abis` against a beacons
//! release that drops the bind step fails CI loudly instead of silently
//! shipping broken beacons.

use alloy::primitives::keccak256;

const BYTECODE_PATH: &str = "abis/IdentityBeacon.bytecode";

/// Read `abis/IdentityBeacon.bytecode` and decode it to raw EVM bytes.
fn load_bytecode() -> Vec<u8> {
    let raw = std::fs::read_to_string(BYTECODE_PATH).unwrap_or_else(|e| {
        panic!(
            "{BYTECODE_PATH} missing or unreadable ({e}). \
             Run `make refresh-abis` to regenerate."
        )
    });
    let trimmed = raw.trim();
    let hex_payload = trimmed.strip_prefix("0x").unwrap_or(trimmed);
    hex::decode(hex_payload).unwrap_or_else(|e| panic!("{BYTECODE_PATH} is not valid hex: {e}"))
}

/// Test that the on-disk bytecode is non-trivial and looks like contract
/// creation bytecode. Catches an empty / truncated artefact, which is the
/// failure mode the atomic-mv refactor of `scripts/refresh-abis.sh` is
/// designed to prevent.
#[test]
fn identity_beacon_bytecode_is_non_trivial_creation_code() {
    let bytes = load_bytecode();
    assert!(
        bytes.len() >= 2_000,
"{BYTECODE_PATH} is suspiciously short ({} bytes); expected creation \
 code on the order of 8-9 KiB. Either `forge inspect` produced a \
 partial write or someone hand-edited the file.",
        bytes.len()
    );
    // Solady-compiled `solc 0.8.30` creation code starts with 0x60... (PUSH1).
    assert_eq!(
        bytes[0], 0x60,
        "{BYTECODE_PATH} does not start with PUSH1; this isn't EVM creation code"
    );
}

/// Test that the constructor includes the `BindingLib.bindComponent`
/// staticcall path. `BindingLib` (`beacons/src/libraries/BindingLib.sol`)
/// is implemented as a pair of selector-driven calls on the verifier:
///
/// 1. `staticcall(authorizedCaller())` — to detect whether the component is an
///    `ICallerBound` and whether it's already bound.
/// 2. `call(bind(this))` — to write `address(this)` into `authorizedCaller`.
///
/// Both selectors land in the constructor bytecode as PUSH4 immediates. If
/// either is missing, the constructor was compiled against a pre-BindingLib
/// `IdentityBeacon.sol` and the resulting beacon will not bind its verifier
/// at deploy time.
#[test]
fn identity_beacon_constructor_calls_bindcomponent() {
    let bytes = load_bytecode();

    // Compute the function selectors. Doing this from the source string at
    // test time (rather than hard-coding the hex bytes) means the assertion
    // is self-documenting and survives a future canonical-form rename.
    let authorized_caller_sig = b"authorizedCaller()".as_slice();
    let bind_sig = b"bind(address)".as_slice();
    let auth_sel = &keccak256(authorized_caller_sig)[..4];
    let bind_sel = &keccak256(bind_sig)[..4];

    assert!(
        bytes.windows(4).any(|w| w == auth_sel),
        "IdentityBeacon constructor is missing the staticcall to authorizedCaller() \
         (selector 0x{}). The bytecode at {BYTECODE_PATH} was compiled before \
         BindingLib was added to beacons; regenerate it via `make refresh-abis` \
         against a beacons release that includes `add caller binding`.",
        hex::encode(auth_sel)
    );
    assert!(
        bytes.windows(4).any(|w| w == bind_sel),
        "IdentityBeacon constructor is missing the call to bind(address) \
         (selector 0x{}). The bytecode at {BYTECODE_PATH} was compiled before \
         BindingLib was added to beacons; regenerate it via `make refresh-abis` \
         against a beacons release that includes `add caller binding`.",
        hex::encode(bind_sel)
    );
}
