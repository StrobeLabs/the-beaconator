FROM rust:1.95-bookworm AS builder
WORKDIR /app

# Copy all source files
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim AS runtime
# ca-certificates/libssl3 for HTTPS requests; curl so ECS container health
# checks can run `curl -f http://localhost:8000/health`.
RUN apt-get update && apt-get install -y ca-certificates libssl3 curl && rm -rf /var/lib/apt/lists/*
# Create non-root user to reduce blast radius if the process is compromised
RUN groupadd --system beaconator && useradd --system --gid beaconator --create-home beaconator
WORKDIR /app
# Copy the binary from builder stage
COPY --from=builder --chown=beaconator:beaconator /app/target/release/the-beaconator /app/the-beaconator
# Copy the abis directory
COPY --from=builder --chown=beaconator:beaconator /app/abis /app/abis
USER beaconator

# All app config (RPC_URL, PRIVATE_KEY, ENV, contract addresses, tokens) is
# injected at RUNTIME as task-definition env by SST's link contract — NOT baked
# in as build args. ECS task env overrides any image ENV, so nothing config-
# related belongs here. Only the server bind is fixed at image level.
#
# Bind 0.0.0.0 (IPv4), not `::`: on ECS Fargate awsvpc the task ENI is reached
# over IPv4 (Cloud Map service discovery + ALB health checks resolve to the
# task's private IPv4), so an IPv6-only bind is unreachable service-to-service.
ENV ROCKET_ADDRESS=0.0.0.0
ENV ROCKET_PORT=8000
# Graceful shutdown budget (Rocket 0.5 figment env syntax: a TOML-ish dict).
# ECS sends SIGTERM on task stop; without this Rocket severs in-flight signing
# after its 5s default grace. 60s grace + 30s mercy fits under a 120s ECS
# stopTimeout and lets pending receipt waits finish or fail cleanly.
ENV ROCKET_SHUTDOWN='{grace=60,mercy=30}'

EXPOSE 8000
# Run the binary
CMD ["./the-beaconator"]