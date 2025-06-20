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
cargo build
```

4. Run tests:
```bash
cargo test
```

5. Run the server:
```bash
cargo run
```

The server will start on `http://localhost:8000`

## Environment Variables

Create a `.env` file in the project root with the following variables:

```env
# Base Chain RPC URL
RPC_URL=https://mainnet.base.org

# Private key for the wallet that will pay for gas (without 0x prefix)
PRIVATE_KEY=your_private_key_here_without_0x_prefix
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

This project is open source and available under the [GPL-3.0 License](LICENSE).
