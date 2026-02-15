'use client';

/**
 * Desktop viewer component.
 *
 * Renders remote desktop JPEG frames on a <canvas> element and
 * captures mouse/keyboard events for input injection.
 *
 * Uses protobuf Envelope messages for all communication.
 * Exposes controls via ref (useImperativeHandle) for the parent page.
 */

import {
    useRef, useEffect, useState, useCallback, forwardRef, useImperativeHandle,
} from 'react';
import {
    decodeEnvelope,
    encodeMouseMove,
    encodeMouseButton,
    encodeMouseScroll,
    encodeKeyEvent,
    encodeSessionEnd,
    type MonitorInfo,
} from '@/lib/proto';
import { Monitor } from 'lucide-react';

const WS_BASE = process.env.NEXT_PUBLIC_WS_URL || 'ws://localhost:8080/ws';
const RECONNECT_DELAYS = [1000, 2000, 4000, 8000, 15000]; // exponential backoff

export type ViewerStatus = 'connecting' | 'connected' | 'disconnected' | 'reconnecting';

export interface DesktopViewerHandle {
    sendInput: (data: Uint8Array) => void;
    getMonitors: () => MonitorInfo[];
    getStatus: () => ViewerStatus;
    getResolution: () => { width: number; height: number };
    getFps: () => number;
}

interface DesktopViewerProps {
    sessionId: string;
    className?: string;
    showStatusBar?: boolean;
    onStatusChange?: (status: ViewerStatus) => void;
    onMonitorsChange?: (monitors: MonitorInfo[]) => void;
    onResolutionChange?: (res: { width: number; height: number }) => void;
    onFpsChange?: (fps: number) => void;
}

// Map browser e.code to keyCode for backward compat with the agent
function codeToKeyCode(code: string): number | null {
    // Letters
    if (code.startsWith('Key')) return code.charCodeAt(3);
    // Digits
    if (code.startsWith('Digit')) return code.charCodeAt(5);
    const map: Record<string, number> = {
        Backspace: 8, Tab: 9, Enter: 13, ShiftLeft: 16, ShiftRight: 16,
        ControlLeft: 17, ControlRight: 17, AltLeft: 18, AltRight: 18,
        Pause: 19, CapsLock: 20, Escape: 27, Space: 32,
        PageUp: 33, PageDown: 34, End: 35, Home: 36,
        ArrowLeft: 37, ArrowUp: 38, ArrowRight: 39, ArrowDown: 40,
        Insert: 45, Delete: 46, MetaLeft: 91, MetaRight: 91,
        F1: 112, F2: 113, F3: 114, F4: 115, F5: 116, F6: 117,
        F7: 118, F8: 119, F9: 120, F10: 121, F11: 122, F12: 123,
        Semicolon: 186, Equal: 187, Comma: 188, Minus: 189,
        Period: 190, Slash: 191, Backquote: 192,
        BracketLeft: 219, Backslash: 220, BracketRight: 221, Quote: 222,
        PrintScreen: 44, ScrollLock: 145, NumLock: 144,
        Numpad0: 96, Numpad1: 97, Numpad2: 98, Numpad3: 99,
        Numpad4: 100, Numpad5: 101, Numpad6: 102, Numpad7: 103,
        Numpad8: 104, Numpad9: 105,
        NumpadMultiply: 106, NumpadAdd: 107, NumpadSubtract: 109,
        NumpadDecimal: 110, NumpadDivide: 111, NumpadEnter: 13,
    };
    return map[code] ?? null;
}

const DesktopViewer = forwardRef<DesktopViewerHandle, DesktopViewerProps>(function DesktopViewer(
    { sessionId, className, showStatusBar = false, onStatusChange, onMonitorsChange, onResolutionChange, onFpsChange },
    ref
) {
    const canvasRef = useRef<HTMLCanvasElement>(null);
    const containerRef = useRef<HTMLDivElement>(null);
    const wsRef = useRef<WebSocket | null>(null);
    const [status, setStatus] = useState<ViewerStatus>('connecting');
    const [resolution, setResolution] = useState({ width: 0, height: 0 });
    const [fps, setFps] = useState(0);
    const [monitors, setMonitors] = useState<MonitorInfo[]>([]);
    const [hasFocus, setHasFocus] = useState(false);
    const frameCountRef = useRef(0);
    const lastFpsTimeRef = useRef(Date.now());
    const mouseMoveThrottleRef = useRef(0);
    const intentionalCloseRef = useRef(false);
    const reconnectAttemptRef = useRef(0);
    const reconnectTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
    const cancelledRef = useRef(false);

    // Propagate state to parent
    useEffect(() => { onStatusChange?.(status); }, [status, onStatusChange]);
    useEffect(() => { onMonitorsChange?.(monitors); }, [monitors, onMonitorsChange]);
    useEffect(() => { onResolutionChange?.(resolution); }, [resolution, onResolutionChange]);
    useEffect(() => { onFpsChange?.(fps); }, [fps, onFpsChange]);

    // Expose controls to parent via ref
    useImperativeHandle(ref, () => ({
        sendInput: (data: Uint8Array) => {
            const ws = wsRef.current;
            if (ws && ws.readyState === WebSocket.OPEN) ws.send(data);
        },
        getMonitors: () => monitors,
        getStatus: () => status,
        getResolution: () => resolution,
        getFps: () => fps,
    }), [monitors, status, resolution, fps]);

    // FPS counter
    useEffect(() => {
        const interval = setInterval(() => {
            const now = Date.now();
            const elapsed = (now - lastFpsTimeRef.current) / 1000;
            const currentFps = Math.round(frameCountRef.current / elapsed);
            setFps(currentFps);
            frameCountRef.current = 0;
            lastFpsTimeRef.current = now;
        }, 1000);
        return () => clearInterval(interval);
    }, []);

    // WebSocket connection with auto-reconnect
    const connect = useCallback(() => {
        if (cancelledRef.current) return;

        const ws = new WebSocket(`${WS_BASE}/console/${sessionId}`);
        ws.binaryType = 'arraybuffer';
        wsRef.current = ws;

        ws.onopen = () => {
            if (cancelledRef.current) { ws.close(); return; }
            console.log(`[Desktop WS] Connected to session ${sessionId}`);
            setStatus('connected');
            reconnectAttemptRef.current = 0;
        };

        ws.onmessage = (event) => {
            if (cancelledRef.current) return;
            if (!(event.data instanceof ArrayBuffer)) return;
            const bytes = new Uint8Array(event.data);
            const envelope = decodeEnvelope(bytes);
            if (!envelope) return;

            switch (envelope.payload.type) {
                case 'desktop_frame': {
                    const frame = envelope.payload;
                    const blob = new Blob([new Uint8Array(frame.data)], { type: 'image/jpeg' });
                    const url = URL.createObjectURL(blob);
                    const img = new Image();
                    img.onload = () => {
                        if (cancelledRef.current) { URL.revokeObjectURL(url); return; }
                        const canvas = canvasRef.current;
                        if (!canvas) { URL.revokeObjectURL(url); return; }

                        if (canvas.width !== img.width || canvas.height !== img.height) {
                            canvas.width = img.width;
                            canvas.height = img.height;
                            setResolution({ width: img.width, height: img.height });
                        }

                        const ctx = canvas.getContext('2d');
                        if (ctx) ctx.drawImage(img, 0, 0);
                        URL.revokeObjectURL(url);
                        frameCountRef.current++;
                    };
                    img.src = url;
                    break;
                }
                case 'screen_info': {
                    const info = envelope.payload;
                    setMonitors(info.monitors);
                    if (info.monitors.length > 0) {
                        const primary = info.monitors.find(m => m.primary) || info.monitors[0];
                        setResolution({ width: primary.width, height: primary.height });
                    }
                    break;
                }
                case 'session_end': {
                    console.log(`[Desktop WS] Session ended: ${envelope.payload.reason}`);
                    setStatus('disconnected');
                    break;
                }
                default:
                    break;
            }
        };

        ws.onclose = () => {
            if (cancelledRef.current) return;
            console.log(`[Desktop WS] Disconnected from session ${sessionId}`);

            // Auto-reconnect unless intentionally closed
            if (!intentionalCloseRef.current) {
                const attempt = reconnectAttemptRef.current;
                const delay = RECONNECT_DELAYS[Math.min(attempt, RECONNECT_DELAYS.length - 1)];
                console.log(`[Desktop WS] Reconnecting in ${delay}ms (attempt ${attempt + 1})`);
                setStatus('reconnecting');
                reconnectAttemptRef.current = attempt + 1;
                reconnectTimerRef.current = setTimeout(connect, delay);
            } else {
                setStatus('disconnected');
            }
        };

        ws.onerror = () => {
            if (cancelledRef.current) return;
            // onclose will fire after onerror, handling reconnect
        };
    }, [sessionId]);

    useEffect(() => {
        cancelledRef.current = false;
        intentionalCloseRef.current = false;
        connect();

        return () => {
            cancelledRef.current = true;
            if (reconnectTimerRef.current) {
                clearTimeout(reconnectTimerRef.current);
                reconnectTimerRef.current = null;
            }
            const ws = wsRef.current;
            if (ws) {
                if (intentionalCloseRef.current && ws.readyState === WebSocket.OPEN) {
                    const endMsg = encodeSessionEnd(sessionId, 'user_disconnected');
                    ws.send(endMsg);
                }
                ws.close();
                wsRef.current = null;
            }
        };
    }, [connect, sessionId]);

    // Set intentional close when the component is about to unmount for real (page navigation)
    useEffect(() => {
        return () => { intentionalCloseRef.current = true; };
    }, []);

    const sendBinary = useCallback((data: Uint8Array) => {
        const ws = wsRef.current;
        if (ws && ws.readyState === WebSocket.OPEN) ws.send(data);
    }, []);

    // Get normalised coordinates from mouse event
    const getNormCoords = useCallback((e: React.MouseEvent<HTMLCanvasElement>) => {
        const canvas = canvasRef.current;
        if (!canvas) return { x: 0, y: 0 };
        const rect = canvas.getBoundingClientRect();
        return {
            x: (e.clientX - rect.left) / rect.width,
            y: (e.clientY - rect.top) / rect.height,
        };
    }, []);

    // Auto-focus canvas on mount
    useEffect(() => {
        const timer = setTimeout(() => canvasRef.current?.focus(), 100);
        return () => clearTimeout(timer);
    }, []);

    return (
        <div ref={containerRef} className={`desktop-viewer-wrapper ${className || ''}`}>
            {/* Optional minimal status bar */}
            {showStatusBar && (
                <div className="desktop-viewer-status-bar">
                    <div className={`dv-status-dot ${status}`} />
                    <span className="dv-status-text">
                        {status === 'connecting' && 'Connecting...'}
                        {status === 'connected' && 'Connected'}
                        {status === 'disconnected' && 'Disconnected'}
                        {status === 'reconnecting' && 'Reconnecting...'}
                    </span>
                    {resolution.width > 0 && (
                        <span className="dv-resolution">{resolution.width}×{resolution.height}</span>
                    )}
                    <span className="dv-fps">{fps} FPS</span>
                    <span className="dv-session-id">{sessionId.slice(0, 8)}</span>
                </div>
            )}

            {/* Canvas */}
            <div className="desktop-viewer-canvas-container">
                {(status === 'connecting' || status === 'reconnecting') && (
                    <div className="dv-connecting-overlay">
                        <div className="dv-spinner" />
                        <p>{status === 'reconnecting' ? 'Reconnecting...' : 'Waiting for desktop stream...'}</p>
                    </div>
                )}
                {status === 'disconnected' && (
                    <div className="dv-connecting-overlay">
                        <p style={{ color: '#ff6b6b' }}>Connection lost</p>
                    </div>
                )}

                {/* Click-to-focus overlay */}
                {status === 'connected' && !hasFocus && (
                    <div
                        className="dv-focus-overlay"
                        onClick={() => { canvasRef.current?.focus(); }}
                    >
                        <div className="dv-focus-hint">
                            <Monitor className="w-4 h-4" />
                            <span>Click to interact</span>
                        </div>
                    </div>
                )}

                <canvas
                    ref={canvasRef}
                    className={`desktop-viewer-canvas ${hasFocus ? 'focused' : ''}`}
                    tabIndex={0}
                    onFocus={() => setHasFocus(true)}
                    onBlur={() => setHasFocus(false)}
                    onMouseMove={(e) => {
                        const now = Date.now();
                        if (now - mouseMoveThrottleRef.current < 16) return;
                        mouseMoveThrottleRef.current = now;
                        const { x, y } = getNormCoords(e);
                        sendBinary(encodeMouseMove(sessionId, x, y));
                    }}
                    onMouseDown={(e) => {
                        // Focus on click
                        canvasRef.current?.focus();
                        const { x, y } = getNormCoords(e);
                        sendBinary(encodeMouseButton(sessionId, e.button, true, x, y));
                    }}
                    onMouseUp={(e) => {
                        const { x, y } = getNormCoords(e);
                        sendBinary(encodeMouseButton(sessionId, e.button, false, x, y));
                    }}
                    onWheel={(e) => {
                        e.preventDefault();
                        const { x, y } = getNormCoords(e);
                        sendBinary(encodeMouseScroll(sessionId, e.deltaX / 120, e.deltaY / 120, x, y));
                    }}
                    onKeyDown={(e) => {
                        e.preventDefault();
                        e.stopPropagation();
                        const keyCode = codeToKeyCode(e.code);
                        if (keyCode !== null) {
                            sendBinary(encodeKeyEvent(sessionId, keyCode, true, {
                                ctrl: e.ctrlKey, alt: e.altKey,
                                shift: e.shiftKey, meta: e.metaKey,
                            }));
                        }
                    }}
                    onKeyUp={(e) => {
                        e.preventDefault();
                        e.stopPropagation();
                        const keyCode = codeToKeyCode(e.code);
                        if (keyCode !== null) {
                            sendBinary(encodeKeyEvent(sessionId, keyCode, false, {
                                ctrl: e.ctrlKey, alt: e.altKey,
                                shift: e.shiftKey, meta: e.metaKey,
                            }));
                        }
                    }}
                    onContextMenu={(e) => e.preventDefault()}
                />
            </div>

            <style jsx>{`
                .desktop-viewer-wrapper {
                    display: flex;
                    flex-direction: column;
                    background: #06060e;
                    overflow: hidden;
                    height: 100%;
                }

                .desktop-viewer-status-bar {
                    display: flex;
                    align-items: center;
                    gap: 12px;
                    padding: 6px 16px;
                    background: rgba(255, 255, 255, 0.03);
                    border-bottom: 1px solid rgba(255, 255, 255, 0.06);
                    font-size: 11px;
                    font-family: 'Inter', sans-serif;
                    flex-shrink: 0;
                }

                .dv-status-dot {
                    width: 7px;
                    height: 7px;
                    border-radius: 50%;
                    flex-shrink: 0;
                    transition: background 0.3s ease;
                }
                .dv-status-dot.connecting, .dv-status-dot.reconnecting {
                    background: #ffd43b;
                    animation: dv-pulse 1.5s infinite;
                }
                .dv-status-dot.connected {
                    background: #69db7c;
                    box-shadow: 0 0 6px rgba(105, 219, 124, 0.4);
                }
                .dv-status-dot.disconnected {
                    background: #ff6b6b;
                }

                .dv-status-text { color: rgba(255,255,255,0.6); white-space: nowrap; }
                .dv-resolution, .dv-monitors {
                    color: rgba(255,255,255,0.3);
                    font-family: 'JetBrains Mono', monospace;
                    font-size: 10px;
                    white-space: nowrap;
                }
                .dv-fps {
                    color: rgba(167,139,250,0.6);
                    font-family: 'JetBrains Mono', monospace;
                    font-size: 10px;
                    white-space: nowrap;
                }
                .dv-session-id {
                    margin-left: auto;
                    color: rgba(255,255,255,0.2);
                    font-family: 'JetBrains Mono', monospace;
                    font-size: 10px;
                    white-space: nowrap;
                }

                .desktop-viewer-canvas-container {
                    flex: 1;
                    display: flex;
                    align-items: center;
                    justify-content: center;
                    background: #000;
                    overflow: hidden;
                    position: relative;
                    min-height: 0;
                }

                .dv-connecting-overlay {
                    position: absolute;
                    inset: 0;
                    display: flex;
                    flex-direction: column;
                    align-items: center;
                    justify-content: center;
                    gap: 16px;
                    color: rgba(255,255,255,0.5);
                    font-size: 13px;
                    z-index: 2;
                }

                .dv-focus-overlay {
                    position: absolute;
                    inset: 0;
                    z-index: 1;
                    cursor: pointer;
                }
                .dv-focus-hint {
                    position: absolute;
                    bottom: 16px;
                    left: 50%;
                    transform: translateX(-50%);
                    display: flex;
                    align-items: center;
                    gap: 6px;
                    padding: 6px 14px;
                    background: rgba(0,0,0,0.7);
                    backdrop-filter: blur(8px);
                    border: 1px solid rgba(255,255,255,0.1);
                    border-radius: 8px;
                    color: rgba(255,255,255,0.6);
                    font-size: 12px;
                    pointer-events: none;
                    animation: dv-fade-in 0.3s ease;
                }

                .dv-spinner {
                    width: 28px;
                    height: 28px;
                    border: 2px solid rgba(255,255,255,0.1);
                    border-top-color: #e05246;
                    border-radius: 50%;
                    animation: dv-spin 0.7s linear infinite;
                }

                .desktop-viewer-canvas {
                    max-width: 100%;
                    max-height: 100%;
                    outline: none;
                    image-rendering: auto;
                    cursor: default;
                }
                .desktop-viewer-canvas.focused {
                    cursor: none;
                }

                @keyframes dv-pulse {
                    0%, 100% { opacity: 1; }
                    50% { opacity: 0.4; }
                }
                @keyframes dv-spin {
                    to { transform: rotate(360deg); }
                }
                @keyframes dv-fade-in {
                    from { opacity: 0; transform: translateX(-50%) translateY(8px); }
                    to { opacity: 1; transform: translateX(-50%) translateY(0); }
                }
            `}</style>
        </div>
    );
});

export default DesktopViewer;
