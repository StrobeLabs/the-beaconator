FROM rustlang/rust:nightly AS builder
WORKDIR /app

# Copy all source files
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim AS runtime
# Install ca-certificates for HTTPS requests
RUN apt-get update && apt-get install -y ca-certificates libssl3 && rm -rf /var/lib/apt/lists/*
WORKDIR /app
# Copy the binary from builder stage
COPY --from=builder /app/target/release/the-beaconator /app/the-beaconator

# Accept build arguments for environment variables
ARG RPC_URL
ARG PRIVATE_KEY
ARG SENTRY_DSN
ARG ENV
ARG BEACONATOR_ACCESS_TOKEN

# Set environment variables for Rocket and application
ENV ROCKET_ADDRESS=::
ENV ROCKET_PORT=${PORT:-8000}
ENV RPC_URL=${RPC_URL}
ENV PRIVATE_KEY=${PRIVATE_KEY}
ENV SENTRY_DSN=${SENTRY_DSN}
ENV ENV=${ENV}
ENV BEACONATOR_ACCESS_TOKEN=${BEACONATOR_ACCESS_TOKEN}

# Expose the port (Railway will use the PORT environment variable)
EXPOSE ${PORT:-8000}
# Run the binary
CMD ["./the-beaconator"]