# Beaconator API Clients

Auto-generated API clients for The Beaconator, built from the OpenAPI 3.0 specification.

## Available Clients

### Python Client (`python/`)
- Full async support
- Type hints for all endpoints
- 55 generated files
- See [python/README.md](python/README.md) for usage

### TypeScript Client (`typescript/`)
- Full type safety with TypeScript
- Compatible with openapi-fetch
- 1000+ lines of generated types
- See [typescript/README.md](typescript/README.md) for usage

## Quick Start

### Python
```python
from clients.python.client import Client
from clients.python.api.beacon import create_perpcity_beacon

client = Client(base_url="https://your-instance.com")
client.headers = {"Authorization": f"Bearer {token}"}
response = create_perpcity_beacon.sync(client=client)
```

### TypeScript
```typescript
import createClient from "openapi-fetch";
import type { paths } from "./typescript/api";

const client = createClient<paths>({ baseUrl: "https://your-instance.com" });
const { data } = await client.POST("/create_perpcity_beacon");
```

## Regenerating Clients

After making changes to the API:

1. **Regenerate OpenAPI spec:**
   ```bash
   cargo run --example generate_openapi > openapi.json
   ```
   Or use the convenience script:
   ```bash
   ./scripts/generate-openapi.sh
   ```

2. **Generate TypeScript client:**
   ```bash
   npm run generate:ts
   ```

3. **Generate Python client:**
   ```bash
   openapi-python-client generate --path openapi.json --output-path clients/python --meta none
   ```

## API Documentation

Interactive API documentation is available at `/rapidoc/` when running the Beaconator server.

## Client Features

Both clients provide:
- ✅ Type-safe request/response handling
- ✅ Bearer token authentication support
- ✅ All 14 API endpoints
- ✅ Request/response models for all operations
- ✅ Error handling utilities
- ✅ Batch operation support

## Project Structure

```
clients/
├── README.md           # This file
├── python/             # Python client
│   ├── README.md
│   ├── client.py       # Main client class
│   ├── api/            # API endpoint modules
│   └── models/         # Request/response models
└── typescript/         # TypeScript client
    ├── README.md
    └── api.ts          # Type definitions
```

## Notes

- Clients are regenerated from `openapi.json` in the project root
- The OpenAPI spec is generated from Rust code using rocket_okapi
- All clients support both authenticated and unauthenticated endpoints
- See individual client READMEs for detailed usage examples
