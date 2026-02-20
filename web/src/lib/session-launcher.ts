/**
 * Session launcher — opens desktop/terminal sessions.
 *
 * When the native Tauri viewer is installed, clicking "Join" opens
 * a `screencontrol://` deep link. The Tauri app receives the session ID,
 * server URL, and auth token, then connects directly — no login required.
 */

import { getAccessToken } from './auth-store';

const TERMINAL_WINDOW = { width: 960, height: 640 };

function openPopout(url: string, name: string, width: number, height: number): boolean {
    const left = Math.round((screen.width - width) / 2);
    const top = Math.round((screen.height - height) / 2);
    const features = [
        `width=${width}`,
        `height=${height}`,
        `left=${left}`,
        `top=${top}`,
        'menubar=no',
        'toolbar=no',
        'location=no',
        'status=no',
        'resizable=yes',
        'scrollbars=no',
    ].join(',');

    const win = window.open(url, name, features);
    return !!win;
}

/**
 * Launch a desktop (agents) session via the native Tauri viewer.
 *
 * Opens a screencontrol:// deep link that the Tauri app handles.
 * No browser fallback — the native viewer is the only path.
 *
 * URL format: screencontrol://session/{sessionId}?server={apiBase}&token={jwt}
 */
export function launchDesktopSession(sessionId: string): boolean {
    const token = getAccessToken();
    if (!token) return false;

    // Derive the actual backend server URL (not the web frontend).
    // When on Next.js dev port (3000), the backend is on a separate port.
    // Behind a reverse proxy, same origin handles everything.
    let server: string;
    if (window.location.port === '3000') {
        const backendPort = process.env.NEXT_PUBLIC_BACKEND_PORT || '8080';
        server = `${window.location.protocol}//${window.location.hostname}:${backendPort}`;
    } else {
        server = window.location.origin;
    }
    const deepLink = `screencontrol://session/${sessionId}?server=${encodeURIComponent(server)}&token=${encodeURIComponent(token)}`;

    // Use a hidden iframe to trigger the protocol handler
    // This avoids navigation errors if the handler isn't registered
    const iframe = document.createElement('iframe');
    iframe.style.display = 'none';
    iframe.src = deepLink;
    document.body.appendChild(iframe);

    // Clean up after a short delay
    setTimeout(() => {
        document.body.removeChild(iframe);
    }, 500);

    return true;
}

/**
 * Launch a terminal session in a pop-out window.
 * Returns true if the window opened, false if blocked.
 */
export function launchTerminalSession(sessionId: string): boolean {
    const url = `/terminal/${sessionId}?popout=1`;
    return openPopout(url, `sc-terminal-${sessionId.slice(0, 8)}`, TERMINAL_WINDOW.width, TERMINAL_WINDOW.height);
}

/**
 * Check if we're running inside a pop-out session window.
 */
export function isPopout(): boolean {
    if (typeof window === 'undefined') return false;
    const params = new URLSearchParams(window.location.search);
    return params.get('popout') === '1';
}
