import type { NextConfig } from "next";

// Backend URL — inside Docker the service name resolves; locally fall back to localhost
const BACKEND = process.env.BACKEND_URL || "http://sc-server:8080";

const nextConfig: NextConfig = {
  output: 'standalone',
  reactCompiler: true,
  async rewrites() {
    return [
      // Proxy /api/* → backend API
      { source: '/api/:path*', destination: `${BACKEND}/api/:path*` },
      // Proxy /ws/* → backend WebSocket
      { source: '/ws/:path*', destination: `${BACKEND}/ws/:path*` },
    ];
  },
};

export default nextConfig;
