# Docker-native cross-compilation image for building sc-agent targeting aarch64.
#
# This replaces `cross` because cross-rs cannot handle the glibc version
# gap between Ubuntu 20.04 (focal, glibc 2.31) and Ubuntu 22.04 (jammy,
# glibc 2.35). The project depends on glib >= 2.70 (via glib-sys v0.18)
# and webkit2gtk-4.1, both of which require jammy or newer.
#
# We install Rust and dependencies directly inside the container so there is
# no host/container glibc mismatch.
FROM ubuntu:22.04

ENV DEBIAN_FRONTEND=noninteractive

# ── System packages and cross-compilation toolchain ────────────────────
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates curl \
    build-essential \
    gcc-aarch64-linux-gnu g++-aarch64-linux-gnu \
    libc6-dev-arm64-cross \
    pkg-config \
    protobuf-compiler libprotobuf-dev \
    && rm -rf /var/lib/apt/lists/*

# ── arm64 development libraries ────────────────────────────────────────
RUN dpkg --add-architecture arm64 \
    && sed -i 's/^deb /deb [arch=amd64] /' /etc/apt/sources.list \
    && echo "deb [arch=arm64] http://ports.ubuntu.com/ubuntu-ports/ jammy main restricted universe" >> /etc/apt/sources.list \
    && echo "deb [arch=arm64] http://ports.ubuntu.com/ubuntu-ports/ jammy-updates main restricted universe" >> /etc/apt/sources.list \
    && echo "deb [arch=arm64] http://ports.ubuntu.com/ubuntu-ports/ jammy-security main restricted universe" >> /etc/apt/sources.list \
    && apt-get update \
    && apt-get install -y --no-install-recommends \
    libglib2.0-dev:arm64 \
    libssl-dev:arm64 \
    libgtk-3-dev:arm64 \
    libwebkit2gtk-4.1-dev:arm64 \
    libjavascriptcoregtk-4.1-dev:arm64 \
    libsoup-3.0-dev:arm64 \
    && ( apt-get install -y --no-install-recommends libayatana-appindicator3-dev:arm64 \
    || apt-get install -y --no-install-recommends libappindicator3-dev:arm64 \
    || true ) \
    && rm -rf /var/lib/apt/lists/*

# ── Rust toolchain (inside container, matching container glibc) ────────
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | \
    sh -s -- -y --default-toolchain stable --target aarch64-unknown-linux-gnu
ENV PATH="/root/.cargo/bin:${PATH}"

# ── Cross-compilation environment ─────────────────────────────────────
ENV PKG_CONFIG_PATH=/usr/lib/aarch64-linux-gnu/pkgconfig
ENV PKG_CONFIG_ALLOW_CROSS=1
ENV CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc
ENV CC_aarch64_unknown_linux_gnu=aarch64-linux-gnu-gcc
ENV CXX_aarch64_unknown_linux_gnu=aarch64-linux-gnu-g++

WORKDIR /build
