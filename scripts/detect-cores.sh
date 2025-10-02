#!/bin/bash

# Cross-platform script to detect optimal number of test threads
# Uses cores/2 to avoid overwhelming the system

detect_cores() {
    local cores=1

    # Try different methods for core detection
    if command -v nproc >/dev/null 2>&1; then
        # Linux
        cores=$(nproc)
    elif command -v sysctl >/dev/null 2>&1; then
        # macOS
        cores=$(sysctl -n hw.ncpu 2>/dev/null || echo 1)
    elif [ -r /proc/cpuinfo ]; then
        # Fallback for Linux
        cores=$(grep -c ^processor /proc/cpuinfo 2>/dev/null || echo 1)
    fi

    # Use cores/2, minimum 1, maximum 8 for reasonable CI performance
    local optimal=$((cores / 2))
    if [ "$optimal" -lt 1 ]; then
        optimal=1
    elif [ "$optimal" -gt 8 ]; then
        optimal=8
    fi

    echo "$optimal"
}

detect_cores