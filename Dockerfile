# Build stage
FROM rust:1.75 as builder

WORKDIR /app

# Copy manifests
COPY Cargo.toml ./

# Copy source code
COPY src ./src

# Build the application
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

# Install required dependencies
RUN apt-get update && \
    apt-get install -y ca-certificates libssl3 && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the binary from builder
COPY --from=builder /app/target/release/dstack-backend /app/dstack-backend

# Set environment variables with defaults
ENV LISTEN_ADDR="0.0.0.0:8080"
ENV DSTACK_URL="http://localhost:19060"

# Expose the port
EXPOSE 8080

# Run the binary
CMD ["/app/dstack-backend"]
