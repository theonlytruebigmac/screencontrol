'use client';

/**
 * Desktop viewer component — ported from web app for Tauri.
 *
 * Renders remote desktop frames on <canvas> and captures
 * mouse/keyboard events for input injection via WebSocket.
 *
 * Uses protobuf Envelope messages for all communication.
 * Exposes controls via ref (useImperativeHandle) for the parent.
 */

import {
    useRef, useEffect, useState, useCallback, forwardRef, useImperativeHandle,
} from 'react';
import { WasmH264Decoder } from '../lib/wasm-h264-decoder';
import {
    decodeEnvelope,
    encodeMouseMove,
    encodeMouseButton,
    encodeMouseScroll,
    encodeKeyEvent,
    encodeSessionEnd,
    encodeClipboardData,
    encodePing,
    encodeQualitySettings,
    type MonitorInfo,
} from '../lib/proto';
import { Monitor } from 'lucide-react';

const RECONNECT_DELAYS = [1000, 2000, 4000, 8000, 15000];

export type ViewerStatus = 'connecting' | 'connected' | 'disconnected' | 'reconnecting';

export interface DesktopViewerHandle {
    sendInput: (data: Uint8Array) => void;
    getMonitors: () => MonitorInfo[];
    getStatus: () => ViewerStatus;
    getResolution: () => { width: number; height: number };
    getFps: () => number;
    getLatency: () => number;
    setAutoQuality: (enabled: boolean) => void;
    getAutoQualityTier: () => string;
}

interface DesktopViewerProps {
    sessionId: string;
    wsUrl: string;            // Direct WS URL (from Tauri command)
    className?: string;
    onStatusChange?: (status: ViewerStatus) => void;
    onMonitorsChange?: (monitors: MonitorInfo[]) => void;
    onResolutionChange?: (res: { width: number; height: number }) => void;
    onFpsChange?: (fps: number) => void;
    onLatencyChange?: (latencyMs: number) => void;
    onAutoQualityTierChange?: (tier: string) => void;
    onClipboardReceived?: (text: string) => void;
}

// Map browser e.code to keyCode for backward compat with the agent
function codeToKeyCode(code: string): number | null {
    const map: Record<string, number> = {
        Backspace: 8, Tab: 9, Enter: 13, ShiftLeft: 16, ShiftRight: 16,
        ControlLeft: 17, ControlRight: 17, AltLeft: 18, AltRight: 18,
        Pause: 19, CapsLock: 20, Escape: 27, Space: 32,
        PageUp: 33, PageDown: 34, End: 35, Home: 36,
        ArrowLeft: 37, ArrowUp: 38, ArrowRight: 39, ArrowDown: 40,
        PrintScreen: 44, Insert: 45, Delete: 46,
        Digit0: 48, Digit1: 49, Digit2: 50, Digit3: 51, Digit4: 52,
        Digit5: 53, Digit6: 54, Digit7: 55, Digit8: 56, Digit9: 57,
        KeyA: 65, KeyB: 66, KeyC: 67, KeyD: 68, KeyE: 69, KeyF: 70,
        KeyG: 71, KeyH: 72, KeyI: 73, KeyJ: 74, KeyK: 75, KeyL: 76,
        KeyM: 77, KeyN: 78, KeyO: 79, KeyP: 80, KeyQ: 81, KeyR: 82,
        KeyS: 83, KeyT: 84, KeyU: 85, KeyV: 86, KeyW: 87, KeyX: 88,
        KeyY: 89, KeyZ: 90,
        MetaLeft: 91, MetaRight: 92, ContextMenu: 93,
        Numpad0: 96, Numpad1: 97, Numpad2: 98, Numpad3: 99, Numpad4: 100,
        Numpad5: 101, Numpad6: 102, Numpad7: 103, Numpad8: 104, Numpad9: 105,
        NumpadMultiply: 106, NumpadAdd: 107, NumpadSubtract: 109,
        NumpadDecimal: 110, NumpadDivide: 111,
        F1: 112, F2: 113, F3: 114, F4: 115, F5: 116, F6: 117,
        F7: 118, F8: 119, F9: 120, F10: 121, F11: 122, F12: 123,
        NumLock: 144, ScrollLock: 145,
        Semicolon: 186, Equal: 187, Comma: 188, Minus: 189, Period: 190,
        Slash: 191, Backquote: 192, BracketLeft: 219, Backslash: 220,
        BracketRight: 221, Quote: 222,
    };
    return map[code] ?? null;
}

// Draw a small cursor arrow at (x, y) for visual feedback
function drawCursorArrow(ctx: CanvasRenderingContext2D, x: number, y: number, canvasWidth: number) {
    const scale = Math.max(1, canvasWidth / 1920);
    const size = 12 * scale;
    ctx.save();
    ctx.translate(x, y);
    ctx.beginPath();
    ctx.moveTo(0, 0);
    ctx.lineTo(0, size);
    ctx.lineTo(size * 0.35, size * 0.75);
    ctx.lineTo(size * 0.55, size * 1.1);
    ctx.lineTo(size * 0.7, size * 1.0);
    ctx.lineTo(size * 0.5, size * 0.65);
    ctx.lineTo(size * 0.85, size * 0.65);
    ctx.closePath();
    ctx.fillStyle = 'rgba(255,255,255,0.9)';
    ctx.fill();
    ctx.strokeStyle = 'rgba(0,0,0,0.6)';
    ctx.lineWidth = scale;
    ctx.stroke();
    ctx.restore();
}

const DesktopViewer = forwardRef<DesktopViewerHandle, DesktopViewerProps>((props, ref) => {
    const {
        sessionId, wsUrl, className,
        onStatusChange, onMonitorsChange, onResolutionChange,
        onFpsChange, onLatencyChange, onAutoQualityTierChange,
        onClipboardReceived,
    } = props;

    const canvasRef = useRef<HTMLCanvasElement>(null);
    const containerRef = useRef<HTMLDivElement>(null);
    const wsRef = useRef<WebSocket | null>(null);
    const ctxRef = useRef<CanvasRenderingContext2D | null>(null);
    const cancelledRef = useRef(false);
    const intentionalCloseRef = useRef(false);
    const reconnectAttemptRef = useRef(0);
    const reconnectTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
    const pingIntervalRef = useRef<ReturnType<typeof setInterval> | null>(null);
    const pendingFrameRef = useRef(0);
    const frameCountRef = useRef(0);
    const lastFpsTimeRef = useRef(Date.now());
    const mouseMoveThrottleRef = useRef(0);
    const cursorNormRef = useRef<{ x: number; y: number } | null>(null);

    const [status, setStatus] = useState<ViewerStatus>('connecting');
    const [monitors, setMonitors] = useState<MonitorInfo[]>([]);
    const [resolution, setResolution] = useState({ width: 0, height: 0 });
    const [fps, setFps] = useState(0);
    const [latency, setLatency] = useState(0);
    const [hasFocus, setHasFocus] = useState(false);
    const [h264Error, setH264Error] = useState('');

    // Auto-adaptive quality
    const autoQualityRef = useRef(false);
    const currentTierRef = useRef('Auto');
    const lastTierSentRef = useRef('');

    // WebCodecs H264 decoder
    const h264DecoderRef = useRef<VideoDecoder | null>(null);
    const h264ConfiguredRef = useRef(false);
    const h264TimestampRef = useRef(0);

    // WASM-based H264 fallback (for WebKitGTK / environments without working VideoDecoder)
    const wasmDecoderRef = useRef<WasmH264Decoder | null>(null);
    const useWasmFallbackRef = useRef(typeof VideoDecoder === 'undefined');

    useEffect(() => { onStatusChange?.(status); }, [status, onStatusChange]);
    useEffect(() => { onMonitorsChange?.(monitors); }, [monitors, onMonitorsChange]);
    useEffect(() => { onResolutionChange?.(resolution); }, [resolution, onResolutionChange]);
    useEffect(() => { onFpsChange?.(fps); }, [fps, onFpsChange]);
    useEffect(() => { onLatencyChange?.(latency); }, [latency, onLatencyChange]);

    useImperativeHandle(ref, () => ({
        sendInput: (data: Uint8Array) => {
            const ws = wsRef.current;
            if (ws && ws.readyState === WebSocket.OPEN) ws.send(data);
        },
        getMonitors: () => monitors,
        getStatus: () => status,
        getResolution: () => resolution,
        getFps: () => fps,
        getLatency: () => latency,
        setAutoQuality: (enabled: boolean) => {
            autoQualityRef.current = enabled;
            if (!enabled) {
                currentTierRef.current = 'Manual';
                lastTierSentRef.current = '';
            } else {
                currentTierRef.current = 'Auto';
            }
        },
        getAutoQualityTier: () => currentTierRef.current,
    }), [monitors, status, resolution, fps, latency]);

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

        const ws = new WebSocket(wsUrl);
        ws.binaryType = 'arraybuffer';
        wsRef.current = ws;

        ws.onopen = () => {
            if (cancelledRef.current) { ws.close(); return; }
            console.log(`[Desktop WS] Connected to session ${sessionId}`);
            setStatus('connected');
            reconnectAttemptRef.current = 0;

            if (pingIntervalRef.current) clearInterval(pingIntervalRef.current);
            pingIntervalRef.current = setInterval(() => {
                if (ws.readyState === WebSocket.OPEN) {
                    ws.send(encodePing(sessionId, Date.now()));
                }
            }, 2000);
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
                    const CODEC_H264 = 1;

                    if (frame.codec === CODEC_H264) {
                        // ── WASM fallback path (WebKitGTK / no working VideoDecoder) ──
                        if (useWasmFallbackRef.current) {
                            const canvas = canvasRef.current;
                            if (!canvas) break;

                            if (!wasmDecoderRef.current) {
                                console.log('[H264-WASM] Using WASM H.264 fallback decoder');
                                wasmDecoderRef.current = new WasmH264Decoder({
                                    canvas,
                                    onFrame: () => {
                                        frameCountRef.current++;
                                        // Draw cursor overlay
                                        const cp = cursorNormRef.current;
                                        if (cp && canvasRef.current) {
                                            const ctx = canvasRef.current.getContext('2d');
                                            if (ctx) drawCursorArrow(ctx, cp.x * canvasRef.current.width, cp.y * canvasRef.current.height, canvasRef.current.width);
                                        }
                                    },
                                    onResize: (w, h) => {
                                        ctxRef.current = null;
                                        setResolution({ width: w, height: h });
                                    },
                                    onError: (err) => {
                                        console.error('[H264-WASM] Fatal error:', err);
                                        setH264Error(`WASM H.264 decode failed: ${err}`);
                                    },
                                });
                            }

                            wasmDecoderRef.current.pushFrame(
                                new Uint8Array(frame.data),
                                !!frame.isKeyframe
                            );
                            break;
                        }

                        // ── WebCodecs VideoDecoder path (Chrome, Edge, Safari) ──

                        // ── NAL extraction helpers ──
                        const extractNALs = (annexB: Uint8Array): Uint8Array[] => {
                            const nals: Uint8Array[] = [];
                            let i = 0;
                            while (i < annexB.length - 3) {
                                let scLen = 0;
                                if (annexB[i] === 0 && annexB[i + 1] === 0 && annexB[i + 2] === 1) scLen = 3;
                                else if (annexB[i] === 0 && annexB[i + 1] === 0 && i + 3 < annexB.length && annexB[i + 2] === 0 && annexB[i + 3] === 1) scLen = 4;
                                if (scLen === 0) { i++; continue; }
                                const nalStart = i + scLen;
                                let nalEnd = annexB.length;
                                for (let j = nalStart + 1; j < annexB.length - 2; j++) {
                                    if (annexB[j] === 0 && annexB[j + 1] === 0 &&
                                        (annexB[j + 2] === 1 || (j + 3 < annexB.length && annexB[j + 2] === 0 && annexB[j + 3] === 1))) {
                                        nalEnd = j;
                                        break;
                                    }
                                }
                                nals.push(annexB.slice(nalStart, nalEnd));
                                i = nalEnd;
                            }
                            return nals;
                        };

                        const buildAvcC = (sps: Uint8Array, pps: Uint8Array): Uint8Array => {
                            const len = 11 + sps.length + pps.length;
                            const buf = new Uint8Array(len);
                            buf[0] = 1; buf[1] = sps[1]; buf[2] = sps[2]; buf[3] = sps[3];
                            buf[4] = 0xFF; buf[5] = 0xE1;
                            buf[6] = (sps.length >> 8) & 0xFF; buf[7] = sps.length & 0xFF;
                            buf.set(sps, 8);
                            const ppsOff = 8 + sps.length;
                            buf[ppsOff] = 1;
                            buf[ppsOff + 1] = (pps.length >> 8) & 0xFF;
                            buf[ppsOff + 2] = pps.length & 0xFF;
                            buf.set(pps, ppsOff + 3);
                            return buf;
                        };

                        const annexBtoAVCC = (annexB: Uint8Array): Uint8Array => {
                            const allNals = extractNALs(annexB);
                            const nals = allNals.filter(nal => {
                                const t = nal[0] & 0x1F;
                                return t >= 1 && t <= 6;
                            });
                            let totalLen = 0;
                            for (const nal of nals) totalLen += 4 + nal.length;
                            const avcc = new Uint8Array(totalLen);
                            let offset = 0;
                            for (const nal of nals) {
                                avcc[offset] = (nal.length >> 24) & 0xFF;
                                avcc[offset + 1] = (nal.length >> 16) & 0xFF;
                                avcc[offset + 2] = (nal.length >> 8) & 0xFF;
                                avcc[offset + 3] = nal.length & 0xFF;
                                avcc.set(nal, offset + 4);
                                offset += 4 + nal.length;
                            }
                            return avcc;
                        };

                        // Initialize decoder on first keyframe
                        if (!h264DecoderRef.current || h264DecoderRef.current.state === 'closed') {
                            if (!frame.isKeyframe) break;

                            const h264ErrorCountKey = '__h264ErrorCount';
                            const errorCount = (window as any)[h264ErrorCountKey] || 0;
                            if (errorCount >= 3) {
                                // WebCodecs failed repeatedly — switch to WASM fallback
                                console.warn('[H264] WebCodecs failed 3 times, switching to WASM fallback');
                                useWasmFallbackRef.current = true;
                                // Re-process this frame through the MSE path on next iteration
                                break;
                            }

                            const drawDecodedFrame = (videoFrame: VideoFrame) => {
                                if (cancelledRef.current) { videoFrame.close(); return; }
                                const canvas = canvasRef.current;
                                if (!canvas) { videoFrame.close(); return; }

                                const vw = videoFrame.displayWidth;
                                const vh = videoFrame.displayHeight;
                                if (canvas.width !== vw || canvas.height !== vh) {
                                    canvas.width = vw;
                                    canvas.height = vh;
                                    ctxRef.current = null;
                                    setResolution({ width: vw, height: vh });
                                }
                                if (!ctxRef.current) ctxRef.current = canvas.getContext('2d');
                                const ctx = ctxRef.current;
                                if (ctx) {
                                    ctx.drawImage(videoFrame, 0, 0);
                                    const cp = cursorNormRef.current;
                                    if (cp) drawCursorArrow(ctx, cp.x * canvas.width, cp.y * canvas.height, canvas.width);
                                }
                                videoFrame.close();
                                frameCountRef.current++;
                                (window as any)[h264ErrorCountKey] = 0;
                            };

                            h264DecoderRef.current = new VideoDecoder({
                                output: drawDecodedFrame,
                                error: (e) => {
                                    console.error('[H264] Decode error:', e);
                                    const count = ((window as any)[h264ErrorCountKey] || 0) + 1;
                                    (window as any)[h264ErrorCountKey] = count;
                                    try { h264DecoderRef.current?.close(); } catch { }
                                    h264DecoderRef.current = null;
                                    h264ConfiguredRef.current = false;
                                },
                            });

                            const annexBData = new Uint8Array(frame.data);
                            const nals = extractNALs(annexBData);
                            let spsNAL: Uint8Array | null = null;
                            let ppsNAL: Uint8Array | null = null;
                            let codecString = 'avc1.42C033';

                            for (const nal of nals) {
                                const nalType = nal[0] & 0x1F;
                                if (nalType === 7 && !spsNAL) {
                                    spsNAL = nal;
                                    const profile = nal[1];
                                    const compat = nal[2];
                                    const level = nal[3];
                                    codecString = `avc1.${profile.toString(16).padStart(2, '0')}${compat.toString(16).padStart(2, '0')}${level.toString(16).padStart(2, '0')}`;
                                } else if (nalType === 8 && !ppsNAL) {
                                    ppsNAL = nal;
                                }
                            }

                            const decoderConfig: VideoDecoderConfig = {
                                codec: codecString,
                                optimizeForLatency: true,
                            };

                            if (spsNAL && ppsNAL) {
                                const avcC = buildAvcC(spsNAL, ppsNAL);
                                decoderConfig.description = avcC.buffer;
                            }

                            try {
                                h264DecoderRef.current.configure(decoderConfig);
                                h264ConfiguredRef.current = true;
                                h264TimestampRef.current = 0;
                            } catch (e) {
                                console.error('[H264] configure() threw:', e);
                                const count = ((window as any)[h264ErrorCountKey] || 0) + 1;
                                (window as any)[h264ErrorCountKey] = count;
                                try { h264DecoderRef.current?.close(); } catch { }
                                h264DecoderRef.current = null;
                                h264ConfiguredRef.current = false;
                                break;
                            }
                        }

                        const decoder = h264DecoderRef.current;
                        if (decoder && decoder.state === 'configured') {
                            if (decoder.decodeQueueSize > 3) break;
                            try {
                                const avccData = annexBtoAVCC(new Uint8Array(frame.data));
                                const chunk = new EncodedVideoChunk({
                                    type: frame.isKeyframe ? 'key' : 'delta',
                                    timestamp: h264TimestampRef.current,
                                    data: avccData,
                                });
                                h264TimestampRef.current += 33333;
                                decoder.decode(chunk);
                            } catch (e) {
                                console.error('[H264] decode() threw:', e);
                            }
                        }
                    } else {
                        // JPEG path
                        if (pendingFrameRef.current > 1) break;
                        pendingFrameRef.current++;

                        const blob = new Blob([new Uint8Array(frame.data)], { type: 'image/jpeg' });
                        createImageBitmap(blob).then((bitmap) => {
                            pendingFrameRef.current--;
                            if (cancelledRef.current) { bitmap.close(); return; }
                            const canvas = canvasRef.current;
                            if (!canvas) { bitmap.close(); return; }

                            if (canvas.width !== bitmap.width || canvas.height !== bitmap.height) {
                                canvas.width = bitmap.width;
                                canvas.height = bitmap.height;
                                ctxRef.current = null;
                                setResolution({ width: bitmap.width, height: bitmap.height });
                            }
                            if (!ctxRef.current) ctxRef.current = canvas.getContext('2d');
                            const ctx = ctxRef.current;
                            if (ctx) {
                                ctx.drawImage(bitmap, 0, 0);
                                const cp = cursorNormRef.current;
                                if (cp) drawCursorArrow(ctx, cp.x * canvas.width, cp.y * canvas.height, canvas.width);
                            }
                            bitmap.close();
                            frameCountRef.current++;
                        }).catch(() => { pendingFrameRef.current--; });
                    }
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
                    setStatus('disconnected');
                    break;
                }
                case 'clipboard_data': {
                    const clip = envelope.payload;
                    if (clip.text && navigator.clipboard) {
                        navigator.clipboard.writeText(clip.text).catch(() => { });
                        onClipboardReceived?.(clip.text);
                    }
                    break;
                }
                case 'pong': {
                    const rtt = Date.now() - envelope.payload.timestamp;
                    setLatency(rtt);

                    if (autoQualityRef.current && ws.readyState === WebSocket.OPEN) {
                        let tier: string;
                        let quality: number;
                        let maxFps: number;
                        let bitrate: number;
                        if (rtt < 50) {
                            tier = 'Ultra'; quality = 95; maxFps = 30; bitrate = 8000;
                        } else if (rtt < 100) {
                            tier = 'High'; quality = 75; maxFps = 30; bitrate = 5000;
                        } else if (rtt < 200) {
                            tier = 'Medium'; quality = 50; maxFps = 24; bitrate = 3000;
                        } else {
                            tier = 'Low'; quality = 25; maxFps = 15; bitrate = 1500;
                        }
                        if (tier !== lastTierSentRef.current) {
                            lastTierSentRef.current = tier;
                            currentTierRef.current = tier;
                            onAutoQualityTierChange?.(tier);
                            ws.send(encodeQualitySettings(sessionId, quality, maxFps, bitrate));
                        }
                    }
                    break;
                }
                default:
                    break;
            }
        };

        ws.onclose = () => {
            if (cancelledRef.current) return;
            if (pingIntervalRef.current) {
                clearInterval(pingIntervalRef.current);
                pingIntervalRef.current = null;
            }
            if (h264DecoderRef.current) {
                try { h264DecoderRef.current.close(); } catch { }
                h264DecoderRef.current = null;
                h264ConfiguredRef.current = false;
                h264TimestampRef.current = 0;
            }
            if (!intentionalCloseRef.current) {
                const attempt = reconnectAttemptRef.current;
                const delay = RECONNECT_DELAYS[Math.min(attempt, RECONNECT_DELAYS.length - 1)];
                setStatus('reconnecting');
                reconnectAttemptRef.current = attempt + 1;
                reconnectTimerRef.current = setTimeout(connect, delay);
            } else {
                setStatus('disconnected');
            }
        };

        ws.onerror = () => { };
    }, [sessionId, wsUrl]);

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
            // Clean up WASM fallback decoder
            if (wasmDecoderRef.current) {
                wasmDecoderRef.current.close();
                wasmDecoderRef.current = null;
            }
            const ws = wsRef.current;
            if (ws) {
                if (intentionalCloseRef.current && ws.readyState === WebSocket.OPEN) {
                    ws.send(encodeSessionEnd(sessionId, 'user_disconnected'));
                }
                ws.close();
                wsRef.current = null;
            }
        };
    }, [connect, sessionId]);

    useEffect(() => {
        return () => { intentionalCloseRef.current = true; };
    }, []);

    const sendBinary = useCallback((data: Uint8Array) => {
        const ws = wsRef.current;
        if (ws && ws.readyState === WebSocket.OPEN) ws.send(data);
    }, []);

    const getNormCoords = useCallback((e: React.MouseEvent<HTMLCanvasElement>) => {
        const canvas = canvasRef.current;
        if (!canvas) return { x: 0, y: 0 };
        const rect = canvas.getBoundingClientRect();
        return {
            x: (e.clientX - rect.left) / rect.width,
            y: (e.clientY - rect.top) / rect.height,
        };
    }, []);

    useEffect(() => {
        const timer = setTimeout(() => canvasRef.current?.focus(), 100);
        return () => clearTimeout(timer);
    }, []);

    return (
        <div ref={containerRef} className={`desktop-viewer-wrapper ${className || ''}`}>
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
                {h264Error && (
                    <div className="h264-error-banner">
                        <strong>⚠ H.264 Decode Error</strong><br />
                        {h264Error}
                    </div>
                )}

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
                        const { x, y } = getNormCoords(e);
                        cursorNormRef.current = { x, y };
                        const now = Date.now();
                        if (now - mouseMoveThrottleRef.current < 8) return;
                        mouseMoveThrottleRef.current = now;
                        sendBinary(encodeMouseMove(sessionId, x, y));
                    }}
                    onMouseLeave={() => { cursorNormRef.current = null; }}
                    onMouseDown={(e) => {
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
                    onPaste={(e) => {
                        e.preventDefault();
                        const text = e.clipboardData?.getData('text/plain');
                        if (text) {
                            sendBinary(encodeClipboardData(sessionId, text));
                        }
                    }}
                />
            </div>
        </div>
    );
});

DesktopViewer.displayName = 'DesktopViewer';
export default DesktopViewer;
