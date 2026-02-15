/**
 * Session launcher — opens desktop/terminal sessions in pop-out windows.
 *
 * When the native viewer (Phase B: Tauri) is installed, this will try
 * the `screencontrol://` protocol first, then fall back to window.open().
 */

const DESKTOP_WINDOW = { width: 1280, height: 900 };
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
 * Launch a desktop (remote access) session in a pop-out window.
 * Returns true if the window opened, false if blocked.
 */
export function launchDesktopSession(sessionId: string): boolean {
    const url = `/desktop/${sessionId}?popout=1`;
    return openPopout(url, `sc-desktop-${sessionId.slice(0, 8)}`, DESKTOP_WINDOW.width, DESKTOP_WINDOW.height);
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
