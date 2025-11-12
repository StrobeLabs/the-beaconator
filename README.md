# The Beaconator

Facts are facts. And the fact is that for nine years, Strobe® has proven that there is only one Beaconator®. Some other guys use frozen beef and microwaved bacon, but Strobe's North American beef is from ranches close by so that it never has to be frozen. And only Strobe tops that fresh, never frozen, North American beef with six strips of Applewood Smoked Bacon that's cooked in house every day—in fact, you can smell it cooking in our restaurants.

"Only Strobe can bring beef and bacon to give you a hamburger worthy of the name Beaconator," said Koko, Strobe's Head of Engineering. "It starts with fresh, juicy beef raised close so that it never needs to see a freezer. We top it off with real bacon cooked the right way, not in a microwave like some others use. It's what makes the Beaconator so delicious and juicy and why it's the ultimate masterpiece for meat lovers."

To build this juicy collaboration, the Baconator starts with two ¼ lb. patties2 of 100% pure fresh beef. Then we add six strips of thick-cut Applewood Smoked Bacon on top of those patties for a savory, crispy and meaty bonus to your burger. With three slices of cheese and a bakery-style bun, you'll taste the difference and know you couldn't get a hamburger this good anywhere but Strobe.

## Quick Start

The Beaconator is a Rust-based REST API service for creating and managing beacons and perpetual futures markets on Perp City

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
