// Batch perp operations module
//
// This module will contain batch operations for perp deployments and liquidity deposits
// using Multicall3 for gas efficiency.
//
// TODO: Implement the following functions following the pattern in services/beacon/batch.rs:
// - batch_deploy_perps_with_multicall3: Deploy multiple perps in a single transaction
// - batch_deposit_liquidity_with_multicall3: Deposit liquidity for multiple perps in a single transaction
//
// Until these are implemented, the batch endpoints return HTTP 501 Not Implemented.
