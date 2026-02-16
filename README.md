# ScreenControl

## Overview

**ScreenControl** is a modern, high-performance remote desktop and fleet management solution designed for speed, security, and simplicity. Built with Rust and Next.js, it offers a lag-free experience for managing thousands of devices from a single pane of glass.

Unlike SaaS solutions (like TeamViewer or ScreenConnect) that charge per-technician or per-endpoint, ScreenControl gives you complete ownership of your data and infrastructure. Whether you are an **MSP** managing client workstations, a **DevOps engineer** monitoring servers, or an **IT department** supporting a remote workforce, ScreenControl provides the critical tools you need—Remote Desktop, Terminal Access, File Transfer, and Auditing—without the recurring costs or privacy concerns.


## Features

![Dashboard](docs/images/home.png)

### 🖥️ Remote Support & Control
Initiate remote control sessions instantly. ScreenControl supports full PTY-backed terminals, desktop streaming, and file management.
![Support Sessions](docs/images/support.png)

### 🛡️ Fleet Access
Manage your entire fleet from a single pane of glass. secure unattended access to servers and workstations.
![Access](docs/images/access.png)

### 🧰 Toolbox & Scripts
Execute pre-defined scripts and tools across your managed devices.
![Toolbox](docs/images/toolbox.png)

### 📊 Reports & Auditing
Track session history, system health, and administrative actions with detailed reporting and audit logs.
| Reports | Audit Log |
|---------|-----------|
| ![Reports](docs/images/reports.png) | ![Audit](docs/images/admin_audit.png) |

---

## Advanced Capabilities

### 🧩 Extensions & Automation
Extend functionality with plugins and automate routine maintenance tasks using the built-in scheduler.
| Extensions | Scheduled Tasks |
|------------|-----------------|
| ![Extensions](docs/images/extensions.png) | ![Scheduled Tasks](docs/images/scheduled_tasks.png) |

### 🤝 Collaboration
Collaborate with your team using built-in notifications and meeting tools.
| Notifications | Meetings |
|---------------|----------|
| ![Notifications](docs/images/notifications.png) | ![Meetings](docs/images/meeting.png) |

---

## Administration

Comprehensive system management allows you to configure security policies, general settings, and monitor system performance.

| System Health | Security Settings | General Config |
|---------------|-------------------|----------------|
| ![System](docs/images/admin_system.png) | ![Security](docs/images/admin_security.png) | ![General](docs/images/admin_general.png) |

---

## Architecture

ScreenControl follows a microservices-inspired architecture for scalability and fault tolerance.

```mermaid
graph TD
    User["User / Browser"] <-->|"HTTPS/WSS"| Web["Web Console (Next.js)"]
    Web <-->|"API"| Server["API Server (Axum)"]
    Server <-->|"SQL"| DB[("PostgreSQL")]
    Server <-->|"PubSub"| Redis[("Redis")]
    Server <-->|"S3 API"| MinIO[("MinIO")]
    
    Agent["Remote Agent (Rust)"] <-->|"WSS"| Server
    Agent <-->|"TCP"| Relay["Relay Server"]
    User -.-|"Direct/Relay"| Relay
```

| Component | Tech Stack | Description |
|-----------|------------|-------------|
| **Server** | Rust (Axum) | High-performance API and WebSocket gateway. Handles session management, auth, and agent coordination. |
| **Agent** | Rust | Lightweight binary running on target machines. Handles PTY spawning, screen capture, input injection, and file operations. |
| **Web** | Next.js 16 | Modern React-based dashboard. Uses Server Components for data fetching and client components for interactive sessions. |
| **Relay** | Rust | Traversal server for NAT busting. Allows connections when direct P2P or server-mediated connections fail. |

### 🧠 Deep Dive: How it Works

#### 1. Agent Connection & Auth
The `sc-agent` binary initiates a WebSocket connection to the server's `/ws/agent` endpoint. It authenticates using a pre-generated **Enrollment Token**.
1.  Admin generates a token in the dashboard.
2.  Agent starts with `--token <TOKEN>`.
3.  Server validates token, registers the agent in Redis, and marks it "Online".

#### 2. PTY Streaming (Terminal)
When a user opens a terminal session:
1.  **Browser** connects to Server via WebSocket (`/ws/client/session`).
2.  **Server** signals **Agent** to spawn a PTY (Pseudo-Terminal) process (e.g., `/bin/bash` or `powershell.exe`).
3.  **Agent** captures `stdout`/`stderr` from the PTY and streams it via binary WS frames to the Server.
4.  **Server** relays frames to the Browser.
5.  **Browser** (xterm.js) renders the text. Keystrokes flow in reverse.

#### 3. The Relay Server
Direct connections between users and agents aren't always possible due to NATs/Firewalls. The **Relay Server** acts as a middleman.
-   If a direct connection fails, both the Browser and the Agent connect to `sc-relay`.
-   The relay blindly pipes TCP traffic between the two peers using a `SessionID` to match them.
-   This ensures connectivity even behind strict corporate firewalls.

---

## 🚀 Quick Start (Docker)

The easiest way to run ScreenControl is using Docker Compose.

### Prerequisites
- Docker Engine & Docker Compose

### 1. Configure Environment
```bash
cp .env.example .env
```
> **⚠️ CRITICAL**: Edit `.env` and set `SC__AUTH__JWT_SECRET` to a strong random string.

### 2. Start Services
```bash
docker compose up -d --build
```

### 3. Access the Dashboard
Navigate to `http://localhost:3000`. You will be greeted by the login screen.

![Login](docs/images/first_login.png)

- **Default User**: `admin@screencontrol.local`
- **Default Pass**: `admin`

---

## 🛠️ Build from Source

### Prerequisites
- **Rust**: v1.93+
- **Node.js**: v22+
- **Protobuf Compiler**: `protoc`
- **PostgreSQL**: v14+
- **Redis**: v6+

### 1. Backend Build
```bash
cd server
cargo build --release
```

### 2. Frontend Build
```bash
cd web
npm ci
npm run build
npm start
```

### 3. Database Setup
```bash
cargo install sqlx-cli
cd server
sqlx migrate run
```

---

## 🚢 Production Deployment

### Nginx Configuration Example

```nginx
server {
    listen 80;
    server_name screencontrol.example.com;
    return 301 https://$host$request_uri;
}

server {
    listen 443 ssl http2;
    server_name screencontrol.example.com;

    ssl_certificate /etc/letsencrypt/live/screencontrol.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/screencontrol.example.com/privkey.pem;

    location / {
        proxy_pass http://localhost:3000;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
    }

    location /api {
        proxy_pass http://localhost:8080;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
    }

    location /ws {
        proxy_pass http://localhost:8080;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header Host $host;
    }
}
```

---

## 💻 Development

### Running Tests
```bash
cd server && cargo test
cd web && npm run test
```

### Linting
```bash
cd server && cargo clippy
cd web && npm run lint
```

## Configuration Reference

| Variable | System | Description | Default |
|----------|--------|-------------|---------|
| `SC__DATABASE__URL` | Backend | Postgres connection string | `postgres://...` |
| `SC__REDIS__URL` | Backend | Redis connection string | `redis://...` |
| `SC__AUTH__JWT_SECRET` | Backend | **REQUIRED**. Signing key | *None* |
| `SC__S3__BUCKET` | Backend | Bucket name for files | `screencontrol` |
| `NEXT_PUBLIC_API_URL` | Frontend | API URL for browser | `http://.../api` |
| `NEXT_PUBLIC_WS_URL` | Frontend | WS URL for browser | `ws://.../ws` |

## License
Distributed under the MIT License. See `LICENSE` for more information.
