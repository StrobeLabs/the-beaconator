# The Beaconator

Facts are facts. And the fact is that for nine years, Wendy's® has proven that there is only one Baconator®. Some other guys use frozen beef and microwaved bacon, but Wendy's North American beef is from ranches close by so that it never has to be frozen. And only Wendy's tops that fresh, never frozen, North American beef1 with six strips of Applewood Smoked Bacon that's cooked in house every day—in fact, you can smell it cooking in our restaurants.

"Only Wendy's can bring beef and bacon to give you a hamburger worthy of the name Baconator," said Kurt Kane, Wendy's Chief Concept and Marketing Officer. "It starts with fresh, juicy beef raised close so that it never needs to see a freezer. We top it off with real bacon cooked the right way, not in a microwave like some others use. It's what makes the Baconator so delicious and juicy and why it's the ultimate masterpiece for meat lovers."

To build this juicy collaboration, the Baconator starts with two ¼ lb. patties2 of 100% pure fresh beef. Then we add six strips of thick-cut Applewood Smoked Bacon on top of those patties for a savory, crispy and meaty bonus to your burger. With three slices of cheese and a bakery-style bun, you'll taste the difference and know you couldn't get a hamburger this good anywhere but Wendy's.

## Quick Start

### Prerequisites

- Rust (nightly version required)
- Cargo
- Base chain RPC access
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

**Testing Notes:**
- Unit tests run in parallel for speed (12 tests)
- Integration tests run single-threaded to prevent race conditions (93 tests)
- Use `./scripts/anvil-cleanup.sh` if tests behave unexpectedly

The server will start on `http://localhost:8000`

### Docker Deployment

The project uses a single `Dockerfile` optimized for Railway deployment that builds everything from scratch for reliability.

#### CI/CD Optimizations

The GitHub Actions workflow is heavily optimized for speed through comprehensive caching:

**🚀 Key Optimizations:**
- **Rust toolchain caching** - Avoids downloading Rust/Cargo on every run
- **Cargo dependency caching** - Caches downloaded crates and compiled dependencies
- **Build artifact caching** - Reuses compiled code between test and Docker jobs
- **Docker layer caching** - Speeds up Docker builds by caching intermediate layers
- **Parallel job execution** - Tests and Docker build run independently for speed

**📊 Expected Performance:**
- First run: ~8-12 minutes (cold cache) 
- Subsequent runs: ~3-5 minutes (warm cache)
- Code-only changes: ~2-3 minutes
- Dependency changes: ~4-6 minutes

#### Local Docker Build
```bash
docker build -t the-beaconator .
docker run -p 8000:8000 -e RPC_URL=your_rpc_url -e PRIVATE_KEY=your_private_key the-beaconator
```

#### Railway Deployment
1. Connect your GitHub repository to Railway
2. Railway will automatically detect the Dockerfile and use it for deployment
3. Set the environment variables in Railway dashboard:
   - `RPC_URL`: Your Base chain RPC URL (e.g., `https://mainnet.base.org`)
   - `PRIVATE_KEY`: Your wallet private key (without 0x prefix)
   - `SENTRY_DSN`: Your Sentry DSN for error tracking (optional)
   - `ENV`: Environment name (e.g., `production`, `staging`)
   - `BEACONATOR_ACCESS_TOKEN`: Your API access token for authentication
   - `ROCKET_ADDRESS`: Set to `0.0.0.0` (already configured in Dockerfile)
   - `ROCKET_PORT`: Set to `8000` (already configured in Dockerfile)
4. Deploy and Railway will generate a public URL

**Note:** The Dockerfile is configured to consume these environment variables from Railway's environment configuration. The application will automatically use the values you set in the Railway dashboard.

## Environment Variables

Create a `.env` file in the project root with the following variables:

```env
# Base Chain RPC URL
RPC_URL=https://mainnet.base.org

# Private key for the wallet that will pay for gas (without 0x prefix)
PRIVATE_KEY=your_private_key_here_without_0x_prefix

# Contract addresses (replace with actual deployed contract addresses)
BEACON_FACTORY_ADDRESS=0x1234567890123456789012345678901234567890
PERPCITY_REGISTRY_ADDRESS=0x3456789012345678901234567890123456789012

# API access token for authentication
BEACONATOR_ACCESS_TOKEN=your_api_token_here

# Environment type (mainnet, testnet, or localnet)
ENV=testnet
```

## API Endpoints

### Base URL
```
http://localhost:8000
```

### Endpoints

#### `GET /`
Welcome page with available endpoints information.

#### `POST /update_beacon`
Update beacon data with a zero-knowledge proof.

**Headers:**
```
Authorization: Bearer <your_api_token>
```

**Request Body:**
```json
{
  "beacon_address": "0x1234567890123456789012345678901234567890",
  "value": 42,
  "proof": [1, 2, 3, 4, 5, ...]
}
```

**Response:**
```json
{
  "success": true,
  "data": "Transaction hash: 0x...",
  "message": "Beacon updated successfully"
}
```

#### `GET /all_beacons`
List all registered beacons (not yet implemented).

**Headers:**
```
Authorization: Bearer <your_api_token>
```

**Response:**
```json
{
  "success": false,
  "data": null,
  "message": "all_beacons endpoint not yet implemented"
}
```

#### `POST /create_beacon`
Create a new beacon (not yet implemented).

**Headers:**
```
Authorization: Bearer <your_api_token>
```

**Request Body:**
```json
{
  // TODO: Define beacon creation parameters
}
```

#### `POST /register_beacon`
Register an existing beacon (not yet implemented).

**Headers:**
```
Authorization: Bearer <your_api_token>
```

**Request Body:**
```json
{
  // TODO: Define beacon registration parameters
}
```

#### `POST /create_perpcity_beacon`
Create a new beacon and register it with the Perpcity registry. The beacon is created with the authenticated wallet as the owner.

**Headers:**
```
Authorization: Bearer <your_api_token>
```

**Request Body:**
No request body required.

**Response:**
```json
{
  "success": true,
  "data": "Beacon address: 0x...",
  "message": "Perpcity beacon created and registered successfully"
}
```

#### `POST /batch_create_perpcity_beacon`
Create multiple beacons in a batch and register them with the Perpcity registry. This is more efficient than calling the individual endpoint multiple times. Each beacon is created with the authenticated wallet as the owner.

**Headers:**
```
Authorization: Bearer <your_api_token>
```

**Request Body:**
```json
{
  "count": 5
}
```

**Response:**
```json
{
  "success": true,
  "data": [
    "0x1111111111111111111111111111111111111111",
    "0x2222222222222222222222222222222222222222",
    "0x3333333333333333333333333333333333333333",
    "0x4444444444444444444444444444444444444444",
    "0x5555555555555555555555555555555555555555"
  ],
  "message": "Successfully created and registered 5 Perpcity beacons"
}
```

**Validation:**
- `count` must be between 1 and 100 (inclusive)
- Invalid counts will return `400 Bad Request`

**Note:** This endpoint creates beacons sequentially for transaction reliability. Each beacon creation and registration is performed as separate transactions to ensure proper error handling and event parsing.

#### `POST /deploy_perp_for_beacon`
Deploy a perpetual for a beacon (not yet implemented).

**Headers:**
```
Authorization: Bearer <your_api_token>
```

## Authentication

All API endpoints (except the index page) require authentication using a Bearer token. Set the `BEACONATOR_ACCESS_TOKEN` environment variable in Railway to enable API access.

**Example:**
```bash
curl -H "Authorization: Bearer your_api_token_here" \
     -H "Content-Type: application/json" \
     -d '{"beacon_address":"0x...","value":42,"proof":[...]}' \
     http://localhost:8000/update_beacon
```

This project is open source and available under the [GPL-3.0 License](LICENSE).
