# Perp Configuration Summary

## Current Default Configuration Values

### Core Trading Parameters
- **Trading Fee**: 5000 bps (0.5%)
- **Tick Spacing**: 30
- **Starting Price**: 50 (sqrt price in X96: 560227709747861419891227623424)
- **Funding Interval**: 86400 seconds (1 day)

### Margin Limits
- **Minimum Margin**: 0 USDC (no enforced minimum in contract)
- **Maximum Margin**: 1,000 USDC (1,000,000,000 in 6 decimals)
- **Maximum Margin per Perp**: 1,000 USDC

### Leverage Configuration
- **Minimum Opening Leverage**: 0x (no minimum)
- **Maximum Opening Leverage**: 10x (790273926286361721684336819027 in X96)
- **Liquidation Leverage**: 10x (790273926286361721684336819027 in X96)

### Liquidation Parameters
- **Liquidation Fee**: 1% (790273926286361721684336819 in X96)
- **Liquidation Fee Split**: 50% (39513699123034658136834084095 in X96)

### Tick Range (V4 Optimized)
- **Default Tick Lower**: -46080 (price ~0.01)
- **Default Tick Upper**: 46050 (price ~100)
- **Price Range**: 10,000x range (0.01 to 100)

### Liquidity Calculation
- **Method**: Uniswap V4 `getLiquidityForAmount1` implementation
- **Scaling Factor**: 200e18 (200,000,000,000,000,000,000)
- **Formula**: Converts USDC margin to 18-decimal amount1, then calculates liquidity

## Implementation Details

### V4 Compliance
The implementation now uses the exact Uniswap V4 `getLiquidityForAmount1` formula:
```rust
liquidity = (amount1 * sqrt_price_upper * sqrt_price_lower) / (sqrt_price_upper - sqrt_price_lower)
```

### Key Changes from Previous Version
1. **Function Update**: Replaced V3's `getSqrtRatioAtTick` with V4's `getSqrtPriceAtTick`
2. **Return Types**: Changed from u128 to U256 to prevent overflow
3. **Constants**: Using exact V4 TickMath constants
4. **Calculation**: Direct implementation of V4 periphery library

### Practical Implications

#### Minimum Deposit Recommendation
- **Beaconator Enforced**: 10 USDC minimum (to ensure meaningful liquidity)
- **Contract Allows**: 0 USDC minimum (but not practical)

#### Leverage Scaling

With the current V4 implementation and wide tick range (0.01 to 100), here's how leverage scales with different margin amounts:

| Margin (USDC) | Expected Leverage | Notes |
|---------------|-------------------|-------|
| 10            | 9.97x            | High leverage (near maximum) |
| 25            | 6.32x            | High leverage |
| 50            | 4.47x            | Moderate leverage |
| 100           | 3.16x            | Moderate leverage |
| 250           | 2.00x            | Moderate leverage |
| 500           | 1.41x            | Conservative |
| 750           | 1.15x            | Conservative |
| 1,000         | 1.00x            | Conservative (minimum leverage) |

**Key Observations:**
- Leverage decreases as margin increases (inverse relationship)
- Small margins (≤10 USDC) approach the 10x maximum leverage limit
- The wide tick range (10,000x from 0.01 to 100) creates a gradual leverage curve
- All margin amounts pass leverage validation (no rejections)

#### Safety Features
1. **Overflow Protection**: U256 calculations prevent arithmetic overflow
2. **Validation**: Pre-flight checks ensure parameters are within bounds
3. **Error Handling**: Clear error messages for invalid configurations

### Environment Variables
All configuration values can be overridden via environment variables:
- `PERP_TRADING_FEE_BPS`
- `PERP_MIN_MARGIN_USDC`
- `PERP_MAX_MARGIN_USDC`
- `PERP_MIN_OPENING_LEVERAGE_X96`
- `PERP_MAX_OPENING_LEVERAGE_X96`
- `PERP_LIQUIDATION_LEVERAGE_X96`
- `PERP_LIQUIDATION_FEE_X96`
- `PERP_LIQUIDATION_FEE_SPLIT_X96`
- `PERP_FUNDING_INTERVAL_SECONDS`
- `PERP_TICK_SPACING`
- `PERP_STARTING_SQRT_PRICE_X96`
- `PERP_DEFAULT_TICK_LOWER`
- `PERP_DEFAULT_TICK_UPPER`
- `PERP_LIQUIDITY_SCALING_FACTOR`
- `PERP_MAX_MARGIN_PER_PERP_USDC`

### Technical Notes
1. **X96 Format**: Fixed-point arithmetic with 96 bits of precision (2^96)
2. **Tick Alignment**: All ticks must be divisible by tick spacing (30)
3. **Price Calculation**: price = (sqrtPriceX96 / 2^96)^2
4. **Liquidity Precision**: Uses U256 internally, converts to u128 for contracts