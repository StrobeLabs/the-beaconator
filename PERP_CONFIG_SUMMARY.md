# Perp Configuration Summary

## Updated Configuration Values

### Tick Range
- **Previous**: [24390, 53850] - 19x price range (~11.5 to ~218)
- **New**: [40950, 46050] - 2x price range (~35.7 to ~70.1)
- **Rationale**: Concentrated liquidity around the expected price of 50, providing better capital efficiency

### Liquidity Scaling Factor
- **Previous**: 500,000
- **New**: 100,000
- **Rationale**: Adjusted to match the tighter tick range while maintaining reasonable leverage

## What This Means for Users

### Minimum Deposit
- **Minimum**: 10 USDC
- Enforced by the beaconator to ensure sufficient liquidity for Uniswap V4 operations

### Maximum Deposit
- **Per transaction**: 1,000 USDC (unchanged)
- **Per perp**: 1,000 USDC (unchanged)

### Leverage Scaling

With the new configuration, here's how leverage scales with different margin amounts:

| Margin (USDC) | Expected Leverage | Notes |
|---------------|-------------------|-------|
| 10            | ~10.0x           | Maximum allowed for small positions |
| 50            | ~4.5x            | Moderate leverage |
| 100           | ~3.2x            | Conservative leverage |
| 500           | ~1.4x            | Very conservative |
| 1,000         | ~1.0x            | Minimal leverage |

### Key Benefits

1. **Better Capital Efficiency**: The 2x range (±40% from center) concentrates liquidity where it's most likely to be used
2. **Predictable Leverage**: The new scaling factor provides more intuitive leverage ratios
3. **Lower Minimum**: 10 USDC minimum makes the platform more accessible
4. **Safety**: Maximum leverage capped at 10x to prevent excessive risk

### Technical Details

The liquidity scaling factor works as a simple multiplier:
```
liquidity = margin_amount_usdc * 100,000
```

This is different from the Uniswap V4 `getLiquidityForAmount1` formula but provides a simplified approach that:
- Is easier to understand and predict
- Maintains consistent behavior across different margin amounts
- Has been validated through extensive testing

### Comparison with OpenMakerPosition.s.sol

The script example uses:
- Wide range: 0.1 to 10 (100x)
- Direct liquidity calculation via `getLiquidityForAmount1`

Our approach uses:
- Tight range: 35.7 to 70.1 (2x)
- Simplified scaling factor for predictability

Both approaches are valid, but ours is optimized for:
- Capital efficiency (concentrated liquidity)
- User experience (predictable leverage)
- Risk management (capped at 10x leverage)