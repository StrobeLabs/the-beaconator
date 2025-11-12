# Beaconator TypeScript Client

Auto-generated TypeScript types and client for The Beaconator API.

## Installation

```bash
npm install openapi-fetch
```

## Usage

```typescript
import createClient from "openapi-fetch";
import type { paths } from "./api";

// Create typed client
const client = createClient<paths>({
  baseUrl: "https://your-beaconator-instance.com"
});

// Set Bearer token for authentication
client.use({
  onRequest(req) {
    req.headers.set("Authorization", `Bearer ${process.env.BEACONATOR_ACCESS_TOKEN}`);
    return req;
  },
});

// Make typed requests
const { data, error } = await client.POST("/create_perpcity_beacon");

if (error) {
  console.error("Error creating beacon:", error);
} else {
  console.log("Beacon created:", data?.data);
}
```

## Available Endpoints

### Information
- `GET /` - Get API summary
- `GET /all_beacons` - List all registered beacons (not implemented)

### Beacon Operations
- `POST /create_beacon` - Create a new beacon
- `POST /register_beacon` - Register an existing beacon
- `POST /create_perpcity_beacon` - Create and register a Perpcity beacon
- `POST /batch_create_perpcity_beacon` - Batch create multiple beacons
- `POST /update_beacon` - Update beacon with ZK proof
- `POST /batch_update_beacon` - Batch update multiple beacons
- `POST /create_verifiable_beacon` - Create verifiable beacon with Halo2 proof

### Perpetual Operations
- `POST /deploy_perp_for_beacon` - Deploy perpetual for beacon
- `POST /batch_deploy_perps_for_beacons` - Batch deploy perpetuals
- `POST /deposit_liquidity_for_perp` - Deposit liquidity for perp
- `POST /batch_deposit_liquidity_for_perps` - Batch deposit liquidity

### Wallet Operations
- `POST /fund_guest_wallet` - Fund a guest wallet with USDC and ETH

## Full Example

```typescript
import createClient from "openapi-fetch";
import type { paths, components } from "./api";

const client = createClient<paths>({
  baseUrl: process.env.BEACONATOR_URL || "http://localhost:8000"
});

// Add authentication middleware
client.use({
  onRequest(req) {
    const token = process.env.BEACONATOR_ACCESS_TOKEN;
    if (token) {
      req.headers.set("Authorization", `Bearer ${token}`);
    }
    return req;
  },
});

async function createBeacon() {
  const { data, error, response } = await client.POST("/create_perpcity_beacon");

  if (error) {
    console.error(`Error ${response.status}:`, error);
    return;
  }

  console.log("Success:", data?.message);
  console.log("Beacon address:", data?.data);
}

async function deployPerpForBeacon(beaconAddress: string) {
  const { data, error } = await client.POST("/deploy_perp_for_beacon", {
    body: {
      beacon_address: beaconAddress,
    },
  });

  if (error) {
    console.error("Error deploying perp:", error);
    return;
  }

  console.log("Perp deployed:", data?.data);
}

// Run example
createBeacon().then(() => process.exit(0));
```

## Type Safety

The generated types provide full type safety:

```typescript
import type { components } from "./api";

type BatchCreateRequest = components["schemas"]["BatchCreatePerpcityBeaconRequest"];
type DeployPerpResponse = components["schemas"]["DeployPerpForBeaconResponse"];

// TypeScript will validate request/response shapes
const request: BatchCreateRequest = {
  count: 5, // Type-checked
};
```

## Error Handling

```typescript
const { data, error, response } = await client.POST("/create_perpcity_beacon");

if (error) {
  // error is typed based on the endpoint's error responses
  switch (response.status) {
    case 401:
      console.error("Unauthorized - check your API token");
      break;
    case 500:
      console.error("Server error:", error);
      break;
    default:
      console.error("Unknown error:", error);
  }
  return;
}

// data is typed as ApiResponse<string> here
console.log(data.message);
```

## Regenerating Types

To regenerate types after API changes:

```bash
npm run generate:ts
```

Or manually:

```bash
npx openapi-typescript ../../openapi.json -o api.ts
```

## Documentation

For complete API documentation, visit `/rapidoc/` on your Beaconator instance.
