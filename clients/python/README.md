# Beaconator Python Client

Auto-generated Python client for The Beaconator API.

## Installation

From the clients/python directory:

```bash
pip install -e .
```

## Usage

```python
from clients.python.client import Client
from clients.python.api.beacon import create_perpcity_beacon
from clients.python.models import ApiResponse

# Initialize the client
client = Client(base_url="https://your-beaconator-instance.com")

# Set the Bearer token for authentication
client.headers = {"Authorization": f"Bearer {your_token}"}

# Create a Perpcity beacon
response = create_perpcity_beacon.sync(client=client)
print(f"Beacon created: {response.data}")
```

## Available Endpoints

### Information
- `index()` - Get API summary
- `all_beacons()` - List all registered beacons (not implemented)

### Beacon Operations
- `create_beacon()` - Create a new beacon
- `register_beacon()` - Register an existing beacon
- `create_perpcity_beacon()` - Create and register a Perpcity beacon
- `batch_create_perpcity_beacon()` - Batch create multiple beacons
- `update_beacon()` - Update beacon with ZK proof
- `batch_update_beacon()` - Batch update multiple beacons
- `create_verifiable_beacon()` - Create verifiable beacon with Halo2 proof

### Perpetual Operations
- `deploy_perp_for_beacon_endpoint()` - Deploy perpetual for beacon
- `batch_deploy_perps_for_beacons()` - Batch deploy perpetuals
- `deposit_liquidity_for_perp_endpoint()` - Deposit liquidity for perp
- `batch_deposit_liquidity_for_perps()` - Batch deposit liquidity

### Wallet Operations
- `fund_guest_wallet()` - Fund a guest wallet with USDC and ETH

## Error Handling

```python
from clients.python.errors import UnexpectedStatus

try:
    response = create_perpcity_beacon.sync(client=client)
except UnexpectedStatus as e:
    print(f"API error: {e.status_code} - {e.content}")
```

## Async Support

All endpoints support both sync and async usage:

```python
import asyncio
from clients.python.api.beacon import create_perpcity_beacon

async def main():
    response = await create_perpcity_beacon.asyncio(client=client)
    print(response.data)

asyncio.run(main())
```

## Regenerating the Client

To regenerate this client after API changes:

```bash
openapi-python-client generate --path ../../openapi.json --output-path . --meta none
```

## Documentation

For complete API documentation, visit `/rapidoc/` on your Beaconator instance.
