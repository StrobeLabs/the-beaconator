#!/bin/bash

# Generate OpenAPI specification file
# Uses the generate_openapi example to create the spec without starting the server

set -e

echo "Generating OpenAPI specification..."

# Generate spec using the example program
cargo run --example generate_openapi > openapi.json 2>&1

echo "OpenAPI spec saved to openapi.json"
echo "File size: $(wc -c < openapi.json | awk '{print $1}') bytes"
