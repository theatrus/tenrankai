# Build stage
FROM rust:1.94-bookworm AS builder

# Install build dependencies for image processing libraries (AVIF and HEIF)
RUN apt-get update && apt-get install -y \
    cmake \
    nasm \
    ninja-build \
    meson \
    pkg-config \
    libssl-dev \
    git \
    python3 \
    curl \
    # libheif build dependencies
    libde265-dev \
    libx265-dev \
    libaom-dev \
    && rm -rf /var/lib/apt/lists/*

ARG AOM_VERSION=v3.12.1
ARG AOM_COMMIT=10aece4157eb79315da205f39e19bf6ab3ee30d0

# Preload the AOM source tree used by libavif-sys so CMake FetchContent does
# not fetch the flaky googlesource archive during the Rust build.
RUN git clone --depth 1 --branch "${AOM_VERSION}" https://aomedia.googlesource.com/aom /opt/aom \
    && test "$(git -C /opt/aom rev-parse HEAD)" = "${AOM_COMMIT}"

# Build and install libheif from source (need >= 1.21 for libheif-rs)
# Use limited parallelism to avoid OOM during cross-compilation
RUN git clone --depth 1 --branch v1.21.1 https://github.com/strukturag/libheif.git /tmp/libheif \
    && cd /tmp/libheif \
    && mkdir build && cd build \
    && cmake .. -DCMAKE_BUILD_TYPE=Release -DWITH_EXAMPLES=OFF \
    && make -j2 \
    && make install \
    && ldconfig \
    && rm -rf /tmp/libheif

# Install Node.js for frontend build
RUN curl -fsSL https://deb.nodesource.com/setup_20.x | bash - \
    && apt-get install -y nodejs

# Set working directory
WORKDIR /app

COPY .github/cmake/aom-fetchcontent-cache.cmake /opt/aom-fetchcontent-cache.cmake
ENV AOM_SOURCE_DIR=/opt/aom
ENV CMAKE_TOOLCHAIN_FILE=/opt/aom-fetchcontent-cache.cmake

# Copy workspace manifests and all crate Cargo.toml files first (for caching)
COPY Cargo.toml Cargo.lock ./
COPY tenrankai/Cargo.toml ./tenrankai/
COPY tenrankai-config-storage/Cargo.toml ./tenrankai-config-storage/
COPY tenrankai-email/Cargo.toml ./tenrankai-email/
COPY tenrankai-image/Cargo.toml ./tenrankai-image/
COPY tenrankai-metadata/Cargo.toml ./tenrankai-metadata/
COPY tenrankai-storage/Cargo.toml ./tenrankai-storage/
COPY tenrankai-users/Cargo.toml ./tenrankai-users/

# Copy all Rust source files
COPY tenrankai/src ./tenrankai/src
COPY tenrankai/build.rs ./tenrankai/
COPY tenrankai-config-storage/src ./tenrankai-config-storage/src
COPY tenrankai-email/src ./tenrankai-email/src
COPY tenrankai-image/src ./tenrankai-image/src
COPY tenrankai-metadata/src ./tenrankai-metadata/src
COPY tenrankai-storage/src ./tenrankai-storage/src
COPY tenrankai-users/src ./tenrankai-users/src

# Copy frontend source and config
COPY frontend ./frontend
COPY admin ./admin
COPY package.json package-lock.json tsconfig.json tsconfig.legacy.json vite.config.js .stylelintrc.json ./

# Copy templates, static assets, and build scripts
COPY templates ./templates
COPY static ./static
COPY scripts ./scripts

# Build frontend assets (served from disk, not embedded in binary)
RUN npm ci && cd admin && npm ci && cd .. && npm run build:prod

# Build the Rust binary (frontend is already built above)
RUN cargo build --release -p tenrankai

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    # libheif runtime dependencies
    libde265-0 \
    libx265-199 \
    libaom3 \
    && rm -rf /var/lib/apt/lists/*

# Copy libheif from builder
COPY --from=builder /usr/local/lib/libheif* /usr/local/lib/
RUN ldconfig

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
