//! DELETED — this module is intentionally empty and no longer compiled.
//!
//! The old multicall helpers here had no production callers (the live batch
//! path is `services/beacon/batch.rs`) and `parse_multicall_results` was a
//! placeholder that always returned `Ok(vec![])`. The `mod` declaration was
//! removed from `services/transaction/mod.rs`; delete this file outright.
