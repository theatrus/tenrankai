# Build stage
FROM rust:1.89-bookworm AS builder

# Install build dependencies for image processing libraries and AVIF
RUN apt-get update && apt-get install -y \
    cmake \
    nasm \
    ninja-build \
    meson \
    pkg-config \
    libssl-dev \
    git \
    python3 \
    && rm -rf /var/lib/apt/lists/*

# Set working directory
WORKDIR /app

# Copy all source files and assets
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY templates ./templates
COPY static ./static

# Build the application with all features in release mode
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Create app user for security
RUN useradd -m -u 1001 -s /bin/bash appuser

# Set working directory
WORKDIR /app

# Copy release binary from builder
COPY --from=builder /app/target/release/tenrankai /usr/local/bin/tenrankai

# Copy static assets and templates
COPY --from=builder /app/static ./static
COPY --from=builder /app/templates ./templates

# Create default directories for photos and cache
RUN mkdir -p /app/photos /app/cache /app/config && \
    chown -R appuser:appuser /app

# Switch to non-root user
USER appuser

# Default environment variables
ENV RUST_LOG=info

# Expose default port
EXPOSE 8080

# Volume mounts for configuration and data
# Users can mount their own paths:
# - /app/config for config.toml and users.toml
# - /app/photos for photo galleries
# - /app/cache for image cache
VOLUME ["/app/config", "/app/photos", "/app/cache"]

# Default command
# Users can override with their own parameters
ENTRYPOINT ["tenrankai"]
CMD ["--host", "0.0.0.0", "--port", "8080", "--config-file", "/app/config/config.toml"]