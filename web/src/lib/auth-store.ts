/**
 * Auth store — manages JWT tokens and user state.
 *
 * Uses localStorage to persist tokens across page reloads.
 */

interface AuthState {
    accessToken: string | null;
    refreshToken: string | null;
    user: AuthUser | null;
    isAuthenticated: boolean;
}

export interface AuthUser {
    sub: string;
    email: string;
    role: string;
    tenantId: string;
}

const AUTH_KEY = 'sc_auth';

function loadState(): AuthState {
    if (typeof window === 'undefined') {
        return { accessToken: null, refreshToken: null, user: null, isAuthenticated: false };
    }

    try {
        const stored = localStorage.getItem(AUTH_KEY);
        if (stored) {
            const parsed = JSON.parse(stored);
            return {
                accessToken: parsed.accessToken || null,
                refreshToken: parsed.refreshToken || null,
                user: parsed.user || null,
                isAuthenticated: !!parsed.accessToken,
            };
        }
    } catch {
        // ignore parse errors
    }

    return { accessToken: null, refreshToken: null, user: null, isAuthenticated: false };
}

function saveState(state: AuthState) {
    if (typeof window === 'undefined') return;
    localStorage.setItem(AUTH_KEY, JSON.stringify({
        accessToken: state.accessToken,
        refreshToken: state.refreshToken,
        user: state.user,
    }));
}

/** Parse JWT payload (no validation — server validates). */
function parseJwt(token: string): AuthUser | null {
    try {
        const payload = token.split('.')[1];
        const decoded = JSON.parse(atob(payload));
        return {
            sub: decoded.sub,
            email: decoded.email,
            role: decoded.role,
            tenantId: decoded.tenant_id,
        };
    } catch {
        return null;
    }
}

// ─── Simple reactive store ───────────────────────────────────

type Listener = () => void;
const listeners: Set<Listener> = new Set();
let state = loadState();

export function getAuthState(): AuthState {
    return state;
}

export function subscribe(fn: Listener): () => void {
    listeners.add(fn);
    return () => listeners.delete(fn);
}

function notify() {
    listeners.forEach((fn) => fn());
}

/** Store tokens from login response. */
export function setTokens(accessToken: string, refreshToken: string) {
    const user = parseJwt(accessToken);
    state = {
        accessToken,
        refreshToken,
        user,
        isAuthenticated: true,
    };
    saveState(state);
    notify();
}

/** Clear all auth state (logout). */
export function clearAuth() {
    state = { accessToken: null, refreshToken: null, user: null, isAuthenticated: false };
    if (typeof window !== 'undefined') {
        localStorage.removeItem(AUTH_KEY);
        localStorage.removeItem('sc_access_token');  // legacy key
        // Clear all cookies
        document.cookie.split(';').forEach((c) => {
            document.cookie = c.replace(/^ +/, '').replace(/=.*/, '=;expires=' + new Date(0).toUTCString() + ';path=/');
        });
    }
    notify();
}

/** Get the current access token (for API calls). */
export function getAccessToken(): string | null {
    return state.accessToken;
}
