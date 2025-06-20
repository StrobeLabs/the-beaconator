# Build stage
FROM rustlang/rust:nightly as builder

WORKDIR /app
COPY . .

RUN cargo build --release

# Runtime stage
FROM debian:bullseye-slim

WORKDIR /app
COPY --from=builder /app/target/release/the-beaconator /app/the-beaconator

# Set environment variables for Rocket
ENV ROCKET_ADDRESS=0.0.0.0
ENV ROCKET_PORT=8000
ENV RPC_URL=""
ENV PRIVATE_KEY=""
ENV SENTRY_DSN=""
ENV ENV="production"

EXPOSE 8000

CMD ["./the-beaconator"]