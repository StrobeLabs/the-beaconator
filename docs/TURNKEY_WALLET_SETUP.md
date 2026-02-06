# Turnkey Wallet Initialization Plan

This document outlines the steps to initialize Turnkey wallets for the multi-wallet beaconator.

## Prerequisites

1. **Turnkey Account**: You need a Turnkey organization account at https://app.turnkey.com
2. **API Credentials**: Generate API credentials (public/private key pair) from Turnkey dashboard
3. **Redis**: Running Redis instance for wallet pool management

## Environment Variables Required

```bash
# Turnkey Configuration
TURNKEY_API_URL=https://api.turnkey.com
TURNKEY_ORGANIZATION_ID=<your-org-id>
TURNKEY_API_PUBLIC_KEY=<your-api-public-key>
TURNKEY_API_PRIVATE_KEY=<your-api-private-key>

# Redis Configuration
REDIS_URL=redis://127.0.0.1:6379

# Optional
BEACONATOR_INSTANCE_ID=beaconator-prod-1
CHAIN_ID=8453  # Base mainnet
```

## Step 1: Create Turnkey Wallets

### Option A: Via Turnkey Dashboard
1. Log into https://app.turnkey.com
2. Navigate to "Wallets" section
3. Click "Create Wallet"
4. Choose "Ethereum" as the wallet type
5. Name the wallet (e.g., `beaconator-wallet-1`)
6. Note down the wallet address and private key ID
7. Repeat for each wallet you want in the pool (recommended: 3-5 wallets)

### Option B: Via Turnkey SDK (programmatic)
```typescript
import { Turnkey } from "@turnkey/sdk-server";

const turnkey = new Turnkey({
  apiBaseUrl: "https://api.turnkey.com",
  organizationId: TURNKEY_ORGANIZATION_ID,
  apiPublicKey: TURNKEY_API_PUBLIC_KEY,
  apiPrivateKey: TURNKEY_API_PRIVATE_KEY,
});

// Create a new wallet
const wallet = await turnkey.createWallet({
  walletName: "beaconator-wallet-1",
  accounts: [
    {
      curve: "CURVE_SECP256K1",
      pathFormat: "PATH_FORMAT_BIP32",
      path: "m/44'/60'/0'/0/0",
      addressFormat: "ADDRESS_FORMAT_ETHEREUM",
    },
  ],
});

console.log("Wallet ID:", wallet.walletId);
console.log("Address:", wallet.addresses[0]);
```

## Step 2: Fund the Wallets

Each wallet needs ETH for gas fees on Base network:

1. Get the wallet addresses from Step 1
2. Send ETH to each wallet address:
   - Recommended: 0.1 ETH per wallet for initial funding
   - Monitor and top up as needed

```bash
# Example: Check wallet balance
cast balance <WALLET_ADDRESS> --rpc-url https://mainnet.base.org
```

## Step 3: Register Wallets in Redis Pool

Use the beaconator CLI or API to add wallets to the Redis pool:

### Via Redis CLI
```bash
redis-cli

# Add wallet to pool set
SADD beaconator:wallet_pool "0xWALLET_ADDRESS_1"
SADD beaconator:wallet_pool "0xWALLET_ADDRESS_2"

# Store wallet info as JSON
SET beaconator:wallet:0xWALLET_ADDRESS_1 '{"address":"0xWALLET_ADDRESS_1","turnkey_key_id":"KEY_ID_1","status":"Available","designated_beacons":[]}'
SET beaconator:wallet:0xWALLET_ADDRESS_2 '{"address":"0xWALLET_ADDRESS_2","turnkey_key_id":"KEY_ID_2","status":"Available","designated_beacons":[]}'
```

### Via Rust Code (if you add an admin endpoint)
```rust
use the_beaconator::services::wallet::{WalletPool, WalletInfo, WalletStatus};

let pool = WalletPool::new(&redis_url, "admin".to_string()).await?;

let info = WalletInfo {
    address: "0x...".parse()?,
    turnkey_key_id: "turnkey-key-id".to_string(),
    status: WalletStatus::Available,
    designated_beacons: vec![],
};

pool.add_wallet(info).await?;
```

## Step 4: Configure Beacon-to-Wallet Mappings (Optional)

If specific beacons require specific wallets (ECDSA signers):

```bash
redis-cli

# Map beacon to wallet
SET beaconator:beacon_wallet:0xBEACON_ADDRESS "0xWALLET_ADDRESS"

# Add beacon to wallet's designated list
SADD beaconator:wallet_beacons:0xWALLET_ADDRESS "0xBEACON_ADDRESS"
```

## Step 5: Verify Setup

### Check Redis Pool
```bash
redis-cli

# List all wallets in pool
SMEMBERS beaconator:wallet_pool

# Check wallet info
GET beaconator:wallet:0xWALLET_ADDRESS

# Check wallet count
SCARD beaconator:wallet_pool
```

### Test Wallet Acquisition
Run the wallet tests with Redis:
```bash
make test-wallet
```

## Step 6: Deploy and Monitor

1. **Deploy the beaconator** with the new environment variables
2. **Monitor logs** for wallet acquisition messages:
   ```
   Acquired wallet 0x... via WalletManager for beacon 0x...
   ```
3. **Monitor Redis** for lock keys:
   ```bash
   redis-cli KEYS "beaconator:wallet_lock:*"
   ```

## Troubleshooting

### Wallet not found in pool
```bash
# Check if wallet exists
redis-cli SISMEMBER beaconator:wallet_pool "0xWALLET_ADDRESS"

# Add if missing
redis-cli SADD beaconator:wallet_pool "0xWALLET_ADDRESS"
```

### Lock stuck (instance crashed)
Locks automatically expire after TTL (default 60s). To manually clear:
```bash
redis-cli DEL beaconator:wallet_lock:0xWALLET_ADDRESS
```

### Wallet balance low
```bash
# Check balance
cast balance 0xWALLET_ADDRESS --rpc-url https://mainnet.base.org

# Fund wallet
cast send 0xWALLET_ADDRESS --value 0.1ether --rpc-url https://mainnet.base.org
```

## Wallet Pool Schema

```
Redis Keys:
- beaconator:wallet_pool                    SET of wallet addresses
- beaconator:wallet:<address>               JSON WalletInfo
- beaconator:wallet_lock:<address>          Lock holder instance ID (with TTL)
- beaconator:wallet_beacons:<address>       SET of beacon addresses for this wallet
- beaconator:beacon_wallet:<beacon>         Wallet address for this beacon
```

## Security Notes

1. **Never commit** Turnkey API private keys to version control
2. **Use environment variables** or a secrets manager (e.g., Railway secrets)
3. **Rotate API keys** periodically
4. **Monitor wallet balances** and set up alerts for low balances
5. **Use separate wallets** for production and testing
