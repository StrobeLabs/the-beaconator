# ---- Build ----------------------------------------------------------------
# cargo-chef splits the ~600-crate dependency compile into its own layer, so a
# source-only change rebuilds just the application crate instead of every
# dependency. Before this, `COPY . . && cargo build` put all deps in a single
# RUN layer that any source edit invalidated -- which defeated the buildx layer
# cache (`cache-from/to: type=gha` in release-image.yml, the local buildx cache
# in ci.yml's docker job). cargo-chef is installed into the pinned rust image
# rather than pulled as a separate chef base image, so the build resolves
# identically on amd64 (CI) and arm64 (Fargate/prod) and stays on the
# rust:1.95 toolchain we already vet.
FROM rust:1.95-bookworm AS chef
WORKDIR /app
# mold: a much faster linker for the final release binary, which links the whole
# alloy + aws-sdk object graph. Installed once in this cached layer.
RUN apt-get update && apt-get install -y --no-install-recommends mold && rm -rf /var/lib/apt/lists/*
# Pin the cargo-chef version for reproducible builds: --locked only freezes its
# transitive deps, so an unpinned install could pull a future release that
# changes the recipe format or bumps the required Rust toolchain and break the
# build. 0.1.77 is the version verified against this Dockerfile.
RUN cargo install cargo-chef --locked --version 0.1.77
# Copy .cargo/config.toml before the cook step so the RUSTFLAGS it sets
# (-D warnings) match between the dependency cook and the final build; a
# mismatch changes the rustc fingerprint and makes the cooked-dependency cache
# miss.
COPY .cargo .cargo
# Route the linker to mold for the dependency cook AND the final build (the same
# RUSTFLAGS must apply to both, or the cook cache misses). This ENV overrides
# .cargo/config.toml's build.rustflags, so keep -D warnings here too.
ENV RUSTFLAGS="-D warnings -C link-arg=-fuse-ld=mold"

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
# Compile dependencies only. This layer is cached until Cargo.toml / Cargo.lock
# change, so ordinary source edits skip it entirely.
RUN cargo chef cook --release --recipe-path recipe.json
# Copy the full source and build only the application crate on top of the
# already-compiled dependencies.
COPY . .
RUN cargo build --release

# ---- Runtime --------------------------------------------------------------
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
