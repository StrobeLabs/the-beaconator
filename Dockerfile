FROM lukemathwalker/cargo-chef:latest-rust-1 AS chef
# Create and change to the app directory.
WORKDIR /app

FROM chef AS planner
COPY . ./
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
# Build dependencies - this is the caching Docker layer!
RUN --mount=type=cache,id=s/5bf4ee55-33a7-4a11-beb2-483beb90f05e-/root/cargo/git,target=/root/.cargo/git \
    --mount=type=cache,id=s/5bf4ee55-33a7-4a11-beb2-483beb90f05e-/root/cargo/registry,target=/root/.cargo/registry \
    --mount=type=cache,id=s/5bf4ee55-33a7-4a11-beb2-483beb90f05e-target,target=/app/target \
    cargo chef cook --release --recipe-path recipe.json
# Build application
COPY . ./
RUN --mount=type=cache,id=s/5bf4ee55-33a7-4a11-beb2-483beb90f05e-/root/cargo/git,target=/root/.cargo/git \
    --mount=type=cache,id=s/5bf4ee55-33a7-4a11-beb2-483beb90f05e-/root/cargo/registry,target=/root/.cargo/registry \
    --mount=type=cache,id=s/5bf4ee55-33a7-4a11-beb2-483beb90f05e-target,target=/app/target \
    cargo build --release --target x86_64-unknown-linux-musl

FROM alpine:latest AS runtime
# Install ca-certificates for HTTPS requests
RUN apk --no-cache add ca-certificates
WORKDIR /app
# Copy the binary from builder stage
COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/the-beaconator /app/the-beaconator
# Set environment variable for Rocket to listen on all interfaces
ENV ROCKET_ADDRESS=0.0.0.0
ENV ROCKET_PORT=8000
# Expose the port
EXPOSE 8000
# Run the binary
CMD ["./the-beaconator"] 