# Custom cross-compilation image for aarch64-unknown-linux-gnu
# Based on Ubuntu 22.04 (jammy) because glib-sys v0.18 requires glib >= 2.70,
# which is not available in Ubuntu 20.04 (focal ships glib 2.64).
#
# cross-rs mounts the host Rust toolchain into the container, so we only
# need the C cross-compiler and target-arch development libraries here.
FROM ubuntu:22.04

ENV DEBIAN_FRONTEND=noninteractive

# Install the amd64 cross-compilation toolchain + basic build tools
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates curl \
    build-essential \
    gcc-aarch64-linux-gnu g++-aarch64-linux-gnu \
    libc6-dev-arm64-cross \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*

# Add arm64 architecture and install target-arch development libraries.
# Since both host and target are on the same Ubuntu 22.04 base, there are
# no version conflicts between amd64 and arm64 packages.
RUN dpkg --add-architecture arm64 && \
    # Restrict existing sources to amd64 only
    sed -i 's/^deb /deb [arch=amd64] /' /etc/apt/sources.list && \
    # Add arm64 sources from ports
    echo "deb [arch=arm64] http://ports.ubuntu.com/ubuntu-ports/ jammy main restricted universe" >> /etc/apt/sources.list && \
    echo "deb [arch=arm64] http://ports.ubuntu.com/ubuntu-ports/ jammy-updates main restricted universe" >> /etc/apt/sources.list && \
    echo "deb [arch=arm64] http://ports.ubuntu.com/ubuntu-ports/ jammy-security main restricted universe" >> /etc/apt/sources.list && \
    apt-get update && \
    apt-get install -y --no-install-recommends \
    protobuf-compiler \
    libglib2.0-dev:arm64 \
    libssl-dev:arm64 \
    libgtk-3-dev:arm64 \
    libwebkit2gtk-4.0-dev:arm64 \
    libwebkit2gtk-4.1-dev:arm64 \
    libjavascriptcoregtk-4.1-dev:arm64 \
    libsoup2.4-dev:arm64 \
    libsoup-3.0-dev:arm64 \
    && ( apt-get install -y --no-install-recommends libayatana-appindicator3-dev:arm64 \
    || apt-get install -y --no-install-recommends libappindicator3-dev:arm64 \
    || true ) \
    && rm -rf /var/lib/apt/lists/*

# Set up pkg-config for cross-compilation
ENV PKG_CONFIG_PATH=/usr/lib/aarch64-linux-gnu/pkgconfig
ENV PKG_CONFIG_ALLOW_CROSS=1
ENV CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc
ENV CC_aarch64_unknown_linux_gnu=aarch64-linux-gnu-gcc
ENV CXX_aarch64_unknown_linux_gnu=aarch64-linux-gnu-g++
