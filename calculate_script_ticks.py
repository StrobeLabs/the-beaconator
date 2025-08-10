#!/usr/bin/env python3
"""Calculate the exact tick values from OpenMakerPosition.sol"""

import math

# Constants from the script
Q96 = 2**96
TICK_SPACING = 30

# From OpenMakerPosition.sol:
# SQRT_PRICE_LOWER_X96 = uint160(1 * FixedPoint96.Q96 / 10);
# SQRT_PRICE_UPPER_X96 = uint160(10 * FixedPoint96.Q96);

sqrt_price_lower_x96 = Q96 // 10  # Q96 / 10
sqrt_price_upper_x96 = 10 * Q96   # 10 * Q96

print("=== OpenMakerPosition.sol Constants ===")
print(f"SQRT_PRICE_LOWER_X96 = {sqrt_price_lower_x96:,}")
print(f"SQRT_PRICE_UPPER_X96 = {sqrt_price_upper_x96:,}")

# Convert sqrt prices to prices
price_lower = (sqrt_price_lower_x96 / Q96) ** 2
price_upper = (sqrt_price_upper_x96 / Q96) ** 2

print(f"\nImplied prices:")
print(f"Price lower: {price_lower:.4f}")
print(f"Price upper: {price_upper:.4f}")

# Calculate ticks using the inverse of TickMath.getSqrtPriceAtTick
# Formula: tick = log(price) / log(1.0001)
# Since we have sqrt prices: tick = 2 * log(sqrt_price) / log(1.0001)
tick_lower_exact = 2 * math.log(sqrt_price_lower_x96 / Q96) / math.log(1.0001)
tick_upper_exact = 2 * math.log(sqrt_price_upper_x96 / Q96) / math.log(1.0001)

print(f"\nExact ticks:")
print(f"Tick lower: {tick_lower_exact:.0f}")
print(f"Tick upper: {tick_upper_exact:.0f}")

# Round to tick spacing
tick_lower = int(tick_lower_exact // TICK_SPACING) * TICK_SPACING
tick_upper = int(tick_upper_exact // TICK_SPACING) * TICK_SPACING

print(f"\nRounded to tick spacing {TICK_SPACING}:")
print(f"Tick lower: {tick_lower}")
print(f"Tick upper: {tick_upper}")

# Verify by calculating back to prices
verify_price_lower = 1.0001 ** tick_lower
verify_price_upper = 1.0001 ** tick_upper

print(f"\nVerification (tick -> price):")
print(f"Price at tick {tick_lower}: {verify_price_lower:.4f}")
print(f"Price at tick {tick_upper}: {verify_price_upper:.4f}")

print(f"\nThese are the tick values we should use in the beaconator!")
print(f"default_tick_lower: {tick_lower}")
print(f"default_tick_upper: {tick_upper}")