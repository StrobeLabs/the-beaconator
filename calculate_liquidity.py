#!/usr/bin/env python3
"""Calculate liquidity using Uniswap V4 formula"""

# Constants
Q96 = 2**96

# Script's price range: 0.1 to 100 (1000x range)
sqrt_price_lower_x96 = Q96 / 10  # sqrt(0.1) * Q96
sqrt_price_upper_x96 = 10 * Q96  # sqrt(100) * Q96

print("=== OpenMakerPosition.sol Analysis ===")
print(f"Price range: 0.1 to 100 (1000x range)")
print(f"sqrt_price_lower_x96: {sqrt_price_lower_x96:,}")
print(f"sqrt_price_upper_x96: {sqrt_price_upper_x96:,}")
print(f"Denominator: {sqrt_price_upper_x96 - sqrt_price_lower_x96:,}")

# Calculate liquidity for 200 USDC (as in script)
amount_200_usdc_18_decimals = 200 * 10**18
liquidity_200 = (amount_200_usdc_18_decimals * Q96) // (sqrt_price_upper_x96 - sqrt_price_lower_x96)
print(f"\nFor 200 USDC (200e18):")
print(f"Liquidity = {liquidity_200:,}")

# Calculate liquidity for 10 USDC
amount_10_usdc_18_decimals = 10 * 10**18
liquidity_10 = (amount_10_usdc_18_decimals * Q96) // (sqrt_price_upper_x96 - sqrt_price_lower_x96)
print(f"\nFor 10 USDC (10e18):")
print(f"Liquidity = {liquidity_10:,}")

# Calculate implied scaling factor
scaling_factor_10_usdc = liquidity_10 // (10 * 10**6)  # 10 USDC in 6 decimals
print(f"\nImplied scaling factor for 10 USDC:")
print(f"Scaling factor = {scaling_factor_10_usdc:,}")

# Now calculate for our tick range [40950, 46050]
# Tick to price: price = 1.0001^tick
# Tick to sqrt price: sqrt_price = sqrt(1.0001^tick) = 1.0001^(tick/2)
import math

# Let's find the correct ticks for a 2x range around 50
# For price ~35: tick = ln(35) / ln(1.0001) 
# For price ~70: tick = ln(70) / ln(1.0001)
import math

target_price_lower = 35.0
target_price_upper = 70.0

tick_lower_exact = math.log(target_price_lower) / math.log(1.0001)
tick_upper_exact = math.log(target_price_upper) / math.log(1.0001)

# Round to nearest tick spacing (30)
tick_spacing = 30
tick_lower = int(tick_lower_exact // tick_spacing) * tick_spacing
tick_upper = int((tick_upper_exact + tick_spacing - 1) // tick_spacing) * tick_spacing

print(f"\nCalculating correct ticks for 35-70 range:")
print(f"Exact tick for price 35: {tick_lower_exact:.0f}")
print(f"Exact tick for price 70: {tick_upper_exact:.0f}")
print(f"Rounded to tick spacing 30: [{tick_lower}, {tick_upper}]")
price_lower = 1.0001**tick_lower  # Should be ~35.7
price_upper = 1.0001**tick_upper  # Should be ~70.1

# Let me recalculate more carefully
# The tick math seems off. Let's verify
print(f"\nPrice calculation check:")
print(f"Price at tick {tick_lower}: {price_lower:.2f}")
print(f"Price at tick {tick_upper}: {price_upper:.2f}")

sqrt_price_lower = math.sqrt(price_lower)
sqrt_price_upper = math.sqrt(price_upper)

sqrt_price_lower_x96_our = int(sqrt_price_lower * Q96)
sqrt_price_upper_x96_our = int(sqrt_price_upper * Q96)

print(f"\n=== Our Tick Range Analysis ===")
print(f"Tick range: [{tick_lower}, {tick_upper}]")
print(f"Price range: {price_lower:.1f} to {price_upper:.1f} ({price_upper/price_lower:.1f}x range)")
print(f"sqrt_price_lower_x96: {sqrt_price_lower_x96_our:,}")
print(f"sqrt_price_upper_x96: {sqrt_price_upper_x96_our:,}")
print(f"Denominator: {sqrt_price_upper_x96_our - sqrt_price_lower_x96_our:,}")

# Calculate liquidity for 10 USDC with our range
liquidity_10_our = (amount_10_usdc_18_decimals * Q96) // (sqrt_price_upper_x96_our - sqrt_price_lower_x96_our)
print(f"\nFor 10 USDC with our range:")
print(f"Liquidity = {liquidity_10_our:,}")

# Calculate implied scaling factor for our range
scaling_factor_our = liquidity_10_our // (10 * 10**6)
print(f"\nImplied scaling factor for our range:")
print(f"Scaling factor = {scaling_factor_our:,}")

print(f"\n=== Comparison ===")
print(f"Script's wide range scaling factor: {scaling_factor_10_usdc:,}")
print(f"Our tight range scaling factor: {scaling_factor_our:,}")
print(f"Our current scaling factor: 100,000")
print(f"Ratio (proper/current): {scaling_factor_our / 100_000:.2f}x")