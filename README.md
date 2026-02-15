# ScreenControl

**Remote console management platform** — Terminal, Desktop, File Transfer, and Chat sessions for remote machines.

## Architecture

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│  Web Console │────▶│   Server    │◀────│    Agent    │
│  (Next.js)   │ WS  │   (Axum)    │ WS  │   (Rust)    │
└─────────────┘     └──────┬──────┘     └─────────────┘
                           │
                    ┌──────┴──────┐
                    │  PostgreSQL │  Redis  │  MinIO
                    └─────────────┘
```

| Crate | Description |
|-------|-------------|
| `sc-server` | HTTP API + WebSocket hub |
| `sc-agent` | Runs on target machines — PTY, screen capture, input injection, file browser |
| `sc-relay` | TCP relay for NAT traversal |
| `sc-protocol` | Protobuf message definitions |
| `sc-common` | Shared config, errors, utility types |

## Quick Start

### Prerequisites

- Rust 1.84+, Node.js 22+, Docker

### 1. Start infrastructure

```bash
cp .env.example .env
docker compose up -d postgres redis minio
```

### 2. Run the server

```bash
cd server
cargo run --bin sc-server
```

### 3. Run the web console

```bash
cd web
npm install
npm run dev
```

### 4. Run an agent

```bash
cd server
cargo run --bin sc-agent
```

### 5. Login

Open [http://localhost:3000/login](http://localhost:3000/login)

Default credentials:
- **Email:** `admin@screencontrol.local`
- **Password:** `admin`

> ⚠️ Change the default password and JWT secret before deploying to production.

## Docker Deployment

Build and run everything:

```bash
cp .env.example .env
# Edit .env — set SC__AUTH__JWT_SECRET to a strong random value
docker compose up -d --build
```

| Service | Port | Description |
|---------|------|-------------|
| `sc-server` | 8080 | API + WebSocket |
| `sc-relay` | 8041 | NAT traversal relay |
| `web` | 3000 | Web console |
| `postgres` | 5432 | Database |
| `redis` | 6379 | Pub/sub + cache |
| `minio` | 9000/9001 | Object storage (file transfers) |

## Features

- **Remote Terminal** — Full PTY-backed terminal via xterm.js
- **Remote Desktop** — JPEG screen streaming + mouse/keyboard injection
- **File Browser** — Navigate remote file systems, upload/download files
- **Chat** — Real-time messaging between operator and agent
- **JWT Auth** — Role-based access control with token refresh
- **Multi-tenant** — Tenant isolation with enrollment tokens
- **Audit Log** — All actions tracked for compliance

## Environment Variables

See [.env.example](.env.example) for all configuration options. Key variables:

| Variable | Default | Description |
|----------|---------|-------------|
| `SC__DATABASE__URL` | — | PostgreSQL connection string |
| `SC__REDIS__URL` | — | Redis connection string |
| `SC__AUTH__JWT_SECRET` | — | **Required.** JWT signing secret |
| `SC__SERVER__API_PORT` | `8080` | HTTP API port |
| `NEXT_PUBLIC_API_URL` | `http://localhost:8080/api` | Server API URL for web console |

## Project Structure

```
screencontrol/
├── proto/                    # Protobuf definitions
│   └── messages.proto
├── server/
│   ├── crates/
│   │   ├── sc-server/        # HTTP API + WebSocket hub
│   │   ├── sc-agent/         # Remote agent binary
│   │   ├── sc-relay/         # NAT traversal relay
│   │   ├── sc-protocol/      # Generated protobuf types
│   │   └── sc-common/        # Shared utilities
│   └── migrations/           # PostgreSQL migrations
├── web/                      # Next.js 16 web console
│   └── src/
│       ├── app/              # Pages (dashboard, agents, sessions, files, login)
│       ├── components/       # UI (sidebar, terminal, desktop-viewer, file-manager, chat-panel)
│       └── lib/              # API client, auth store
├── docker/                   # Dockerfiles (server, relay, web)
├── docker-compose.yml
└── .env.example
```

## License

Proprietary — All rights reserved.
