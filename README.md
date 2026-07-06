# The Beaconator

Facts are facts. And the fact is that for nine years, Strobe® has proven that there is only one Beaconator®. Some other guys use frozen beef and microwaved bacon, but Strobe's North American beef is from ranches close by so that it never has to be frozen. And only Strobe tops that fresh, never frozen, North American beef with six strips of Applewood Smoked Bacon that's cooked in house every day—in fact, you can smell it cooking in our restaurants.

"Only Strobe can bring beef and bacon to give you a hamburger worthy of the name Beaconator," said Koko, Strobe's Head of Engineering. "It starts with fresh, juicy beef raised close so that it never needs to see a freezer. We top it off with real bacon cooked the right way, not in a microwave like some others use. It's what makes the Beaconator so delicious and juicy and why it's the ultimate masterpiece for meat lovers."

To build this juicy collaboration, the Baconator starts with two ¼ lb. patties2 of 100% pure fresh beef. Then we add six strips of thick-cut Applewood Smoked Bacon on top of those patties for a savory, crispy and meaty bonus to your burger. With three slices of cheese and a bakery-style bun, you'll taste the difference and know you couldn't get a hamburger this good anywhere but Strobe.

## Quick Start

The Beaconator is a Rust-based REST API service for creating and managing beacons and perpetual futures markets on Perp City.

### Pinned contract versions

The-beaconator's REST surface is pinned to specific release tags of the contract repos (not main). See `.contracts-versions` for the source of truth, and run `make refresh-abis` after bumping it.

- `perpcity-contracts` @ **v0.1.0** (`PerpFactory` + per-market `Perp` contracts)
- `beacons` @ **v0.0.1**

### Prerequisites

- Rust (nightly version required)
- Cargo
- Arbitrum RPC access (Arbitrum One for mainnet, Arbitrum Sepolia for testnet)
- Ethereum wallet with private key
- **Anvil** (for running tests) - Install via [Foundry](https://book.getfoundry.sh/getting-started/installation):
  ```bash
  curl -L https://foundry.paradigm.xyz | bash
  foundryup
  ```
- **Docker** (for containerized deployment) - [Install Docker](https://docs.docker.com/get-docker/)

### Installation

1. Clone the repository:
```bash
git clone <your-repo-url>
cd the-beaconator
```

2. Set up environment variables:
```bash
cp env.example .env
# Edit .env with your actual values
```

3. Build the project:
```bash
make build
# or for release build: make build-release
```

4. Run tests:
```bash
make test
```
**Note**: Tests require Anvil to be installed and available in your PATH. The integration tests will automatically start local blockchain instances.

5. Run the server:
```bash
make dev
```

### Development Commands

The project includes a Makefile with useful development commands:

```bash
make help               # Show all available commands
make test               # Run all tests (unit parallel, integration single-threaded)
make test-unit          # Run unit tests only (fast)
make test-integration   # Run integration tests only (single-threaded)
make quality            # Run all quality checks (format, lint, test)
make lint               # Run clippy linter with strict warnings
```

### Docker Deployment

The project uses a single `Dockerfile` optimized for Railway deployment that builds everything from scratch for reliability.

#### Local Docker Build
```bash
docker build -t the-beaconator .
docker run -p 8000:8000 -e RPC_URL=your_rpc_url -e PRIVATE_KEY=your_private_key the-beaconator
```

## Environment Variables

Create a `.env` file in the project root with the following variables:

```env
# Arbitrum RPC URL (Arbitrum One mainnet shown; replace with your private endpoint)
RPC_URL=https://arb1.arbitrum.io/rpc

# Private key for the EIP-712 measurement signer (without 0x prefix). This
# wallet signs beacon-update digests only — it never holds or sends funds.
# Gas and guest funding transfers come from the WALLET_PRIVATE_KEYS pool.
PRIVATE_KEY=your_private_key_here_without_0x_prefix

# Contract addresses (replace with actual deployed contract addresses)
BEACON_FACTORY_ADDRESS=0x1234567890123456789012345678901234567890
PERPCITY_REGISTRY_ADDRESS=0x3456789012345678901234567890123456789012

# API access token for authentication
BEACONATOR_ACCESS_TOKEN=your_api_token_here

# Environment type (mainnet, testnet, or localnet)
ENV=testnet

# ETH (wei) a pool wallet must retain AFTER a guest-funding transfer.
# Keep ABOVE the 0.01 ETH BeaconatorWalletGasLow paging threshold so the
# faucet throttles before beacon gas is at risk. Default 0.02 ETH.
FAUCET_RESERVE_ETH_WEI=20000000000000000
```

## Wallet pool top-up (testnet)

The pool's USDC replenishes itself: the deployed testnet USDC
(0xBEF280BefeE2Cb28c20D1E4Cc1da999B4DA0f1fD on Arbitrum Sepolia) has a
permissionless mint (verified on-chain 2026-07-06; the deployed code differs
from the owner-gated repo mock), and the admin route mints every pool wallet
up to a per-wallet target:

```bash
curl -X POST "$BEACONATOR/top_up_pool" \
  -H "Authorization: Bearer $BEACONATOR_ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"usdc_target": "10000000000"}'   # 10,000 USDC per wallet (default)
```

The route is hard-disabled off Arbitrum Sepolia / local Anvil (same
fail-closed chain guard as fund_guest_wallet).

ETH cannot be minted. Gas top-ups stay manual: bridge or faucet Sepolia ETH
to the pool wallet addresses (listed by the balance sweep logs and the
WalletEthBalance CloudWatch metric). Guest funding refuses to take a wallet
below FAUCET_RESERVE_ETH_WEI, so beacon updates keep working while the pool
waits for gas.

## API Documentation

**OpenAPI Spec:** Available at `/openapi.json` when the server is running.

**Generate API Clients:**

The Beaconator provides an OpenAPI 3.0 specification that can be used to generate type-safe API clients in any language.

Generate the spec by running the server and accessing `/openapi.json`:
```bash
# Start the server
make dev

# In another terminal, download the spec
curl http://localhost:8000/openapi.json > openapi.json
```

Generate TypeScript client:
```bash
npm install -D openapi-typescript
npx openapi-typescript openapi.json -o api.ts
```

Generate Python client:
```bash
pipx install openapi-python-client
openapi-python-client generate --path openapi.json --output-path client/python
```

**Interactive Documentation:**

You can view interactive API documentation using any OpenAPI UI viewer:
- [RapiDoc](https://rapidocweb.com/): `npx serve` then open with RapiDoc
- [Swagger UI](https://swagger.io/tools/swagger-ui/): Upload `openapi.json`
- [Redoc](https://redocly.com/): `npx @redocly/cli preview-docs openapi.json`

**Examples:** See the [Beaconator section](docs.strobe.org/docs/developer/beaconator) for some basic usage and documentation.


This project is open source and available under the [MIT License](LICENSE).
