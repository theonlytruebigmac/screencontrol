/**
 * Runtime URL derivation for self-hosted deployments.
 *
 * Derives API and WebSocket base URLs from `window.location` so the
 * Docker image works on any domain without rebuild.
 *
 * When running behind a reverse proxy (Traefik, Nginx, etc.) the API
 * and WS paths are served on the same origin. When accessing Next.js
 * directly (port 3000, no proxy), API calls are handled by Next.js
 * rewrites but WebSocket upgrades are routed directly to the backend.
 */

/** The default backend port when no reverse proxy is present. */
const BACKEND_PORT = process.env.NEXT_PUBLIC_BACKEND_PORT || '8080';

/** HTTP API base — e.g. "https://sc.example.com/api" */
export function getApiBase(): string {
    if (typeof window === 'undefined') {
        // SSR fallback — API calls won't happen server-side anyway
        return 'http://localhost:8080/api';
    }
    // Next.js rewrites proxy /api → backend, so origin works in all cases
    return `${window.location.origin}/api`;
}

/** WebSocket base — e.g. "wss://sc.example.com/ws" */
export function getWsBase(): string {
    if (typeof window === 'undefined') {
        return 'ws://localhost:8080/ws';
    }
    const proto = window.location.protocol === 'https:' ? 'wss:' : 'ws:';

    // If we're on the Next.js dev port (3000), there's no reverse proxy
    // to handle WS upgrades — connect directly to the backend instead.
    if (window.location.port === '3000') {
        return `${proto}//${window.location.hostname}:${BACKEND_PORT}/ws`;
    }

    // Behind a reverse proxy — same origin handles everything
    return `${proto}//${window.location.host}/ws`;
}

