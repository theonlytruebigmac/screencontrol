# Custom cross-compilation image for aarch64-unknown-linux-gnu
# Extends the default cross image (which includes Rust toolchain, etc.)
# and adds Ubuntu 22.04 (jammy) arm64 repos because:
#   - glib-sys v0.18 requires glib >= 2.70
#   - The default cross image is Ubuntu 20.04 (focal) which has glib 2.64
#   - Ubuntu 22.04 (jammy) has glib 2.72
FROM ghcr.io/cross-rs/aarch64-unknown-linux-gnu:main

# Replace the focal arm64 package sources with jammy ones
# The base system stays focal (x86_64) but the arm64 cross-libs come from jammy
RUN dpkg --add-architecture arm64 && \
    # Remove existing focal sources for arm64 and add jammy ones
    (rm -f /etc/apt/sources.list.d/cross-*.list 2>/dev/null || true) && \
    echo "deb [arch=arm64] http://ports.ubuntu.com/ubuntu-ports/ jammy main restricted universe" > /etc/apt/sources.list.d/jammy-arm64.list && \
    echo "deb [arch=arm64] http://ports.ubuntu.com/ubuntu-ports/ jammy-updates main restricted universe" >> /etc/apt/sources.list.d/jammy-arm64.list && \
    echo "deb [arch=arm64] http://ports.ubuntu.com/ubuntu-ports/ jammy-security main restricted universe" >> /etc/apt/sources.list.d/jammy-arm64.list && \
    # Import jammy signing keys
    apt-key adv --keyserver keyserver.ubuntu.com --recv-keys 871920D1991BC93C 3B4FE6ACC0B21F32 || true && \
    apt-get update && \
    # Install arm64 dev libraries from jammy (glib 2.72+)
    apt-get install -y --no-install-recommends \
    protobuf-compiler \
    libglib2.0-dev:arm64 \
    libssl-dev:arm64 \
    libgtk-3-dev:arm64 \
    libwebkit2gtk-4.0-dev:arm64 \
    libsoup2.4-dev:arm64 \
    && ( apt-get install -y --no-install-recommends libayatana-appindicator3-dev:arm64 \
    || apt-get install -y --no-install-recommends libappindicator3-dev:arm64 \
    || true ) \
    && rm -rf /var/lib/apt/lists/*

ENV PKG_CONFIG_PATH=/usr/lib/aarch64-linux-gnu/pkgconfig
ENV PKG_CONFIG_ALLOW_CROSS=1
