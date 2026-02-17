/**
 * Runtime URL derivation for self-hosted deployments.
 *
 * Derives API and WebSocket base URLs from `window.location` so the
 * Docker image works on any domain without rebuild.
 */

/** HTTP API base — e.g. "https://sc.example.com/api" */
export function getApiBase(): string {
    if (typeof window === 'undefined') {
        // SSR fallback — API calls won't happen server-side anyway
        return 'http://localhost:8080/api';
    }
    return `${window.location.origin}/api`;
}

/** WebSocket base — e.g. "wss://sc.example.com/ws" */
export function getWsBase(): string {
    if (typeof window === 'undefined') {
        return 'ws://localhost:8080/ws';
    }
    const proto = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
    return `${proto}//${window.location.host}/ws`;
}
