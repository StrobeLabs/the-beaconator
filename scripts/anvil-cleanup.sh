#!/bin/bash

# Anvil cleanup script for The Beaconator
# This script helps manage Anvil resources and prevent resource accumulation

echo "Cleaning up Anvil resources..."

# Clean Anvil cache directory
if [ -d "$HOME/.foundry/anvil/tmp" ]; then
    echo "Removing Anvil cache directory..."
    rm -rf "$HOME/.foundry/anvil/tmp"
    echo "Anvil cache cleared"
else
    echo "No Anvil cache directory found"
fi

# Kill any running Anvil processes
echo "Checking for running Anvil processes..."
ANVIL_PIDS=$(pgrep -f "anvil" || true)

if [ -n "$ANVIL_PIDS" ]; then
    echo "Found running Anvil processes: $ANVIL_PIDS"
    kill $ANVIL_PIDS
    sleep 2
    
    # Force kill if still running
    REMAINING_PIDS=$(pgrep -f "anvil" || true)
    if [ -n "$REMAINING_PIDS" ]; then
        echo "Force killing remaining processes: $REMAINING_PIDS"
        kill -9 $REMAINING_PIDS
    fi
    echo "Anvil processes terminated"
else
    echo "No running Anvil processes found"
fi

# Clean temporary test files
if [ -d "./target/tmp" ]; then
    echo "Cleaning temporary test files..."
    rm -rf "./target/tmp"
    echo "Temporary test files cleaned"
fi

echo "Anvil cleanup complete!"