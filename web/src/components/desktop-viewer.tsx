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
    encodeClipboardData,
    encodePing,
    encodeQualitySettings,
    type MonitorInfo,
} from '@/lib/proto';
import { getWsBase } from '@/lib/urls';
import { Monitor } from 'lucide-react';

const RECONNECT_DELAYS = [1000, 2000, 4000, 8000, 15000]; // exponential backoff

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
    className?: string;
    showStatusBar?: boolean;
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

/**
 * Draw a standard arrow cursor on the canvas.
 * The cursor is drawn at (cx, cy) with size proportional to canvas width
 * so it looks correct regardless of stream resolution.
 */
function drawCursorArrow(ctx: CanvasRenderingContext2D, cx: number, cy: number, canvasWidth: number) {
    // Scale cursor size proportionally: ~24px at 1920w, smaller/larger at other resolutions
    const scale = Math.max(canvasWidth / 1920, 0.5);
    const s = scale;

    ctx.save();
    ctx.translate(cx, cy);

    // Standard arrow cursor shape (tip at origin, pointing down-right)
    ctx.beginPath();
    ctx.moveTo(0, 0);             // tip
    ctx.lineTo(0, 21 * s);        // down
    ctx.lineTo(4.2 * s, 17 * s);  // notch right
    ctx.lineTo(7.8 * s, 24 * s);  // arrow tail right
    ctx.lineTo(11 * s, 22.5 * s); // arrow tail right top
    ctx.lineTo(7.2 * s, 15.8 * s);// notch inner
    ctx.lineTo(12.5 * s, 15.8 * s);// wing right
    ctx.closePath();

    // Black outline
    ctx.lineWidth = 2 * s;
    ctx.lineJoin = 'round';
    ctx.strokeStyle = '#000000';
    ctx.stroke();

    // White fill
    ctx.fillStyle = '#ffffff';
    ctx.fill();

    ctx.restore();
}

const DesktopViewer = forwardRef<DesktopViewerHandle, DesktopViewerProps>(function DesktopViewer(
    { sessionId, className, showStatusBar = false, onStatusChange, onMonitorsChange, onResolutionChange, onFpsChange, onLatencyChange, onAutoQualityTierChange, onClipboardReceived },
    ref
) {
    const canvasRef = useRef<HTMLCanvasElement>(null);
    const containerRef = useRef<HTMLDivElement>(null);
    const wsRef = useRef<WebSocket | null>(null);
    const [status, setStatus] = useState<ViewerStatus>('connecting');
    const [resolution, setResolution] = useState({ width: 0, height: 0 });
    const [fps, setFps] = useState(0);
    const [latency, setLatency] = useState(0);
    const [monitors, setMonitors] = useState<MonitorInfo[]>([]);
    const [h264Error, setH264Error] = useState<string | null>(null);
    const pingIntervalRef = useRef<ReturnType<typeof setInterval> | null>(null);
    const [hasFocus, setHasFocus] = useState(false);
    const cursorNormRef = useRef<{ x: number; y: number } | null>(null);
    const frameCountRef = useRef(0);
    const lastFpsTimeRef = useRef(Date.now());
    const mouseMoveThrottleRef = useRef(0);
    const intentionalCloseRef = useRef(false);
    const reconnectAttemptRef = useRef(0);
    const reconnectTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
    const cancelledRef = useRef(false);
    const ctxRef = useRef<CanvasRenderingContext2D | null>(null);
    const pendingFrameRef = useRef(0); // tracks in-flight frame decodes

    // ── Auto-adaptive quality ──
    const autoQualityRef = useRef(false);
    const currentTierRef = useRef('Auto');
    const lastTierSentRef = useRef('');

    // ── WebCodecs H264 decoder ──
    const h264DecoderRef = useRef<VideoDecoder | null>(null);
    const h264ConfiguredRef = useRef(false);
    const h264TimestampRef = useRef(0);

    // Propagate state to parent
    useEffect(() => { onStatusChange?.(status); }, [status, onStatusChange]);
    useEffect(() => { onMonitorsChange?.(monitors); }, [monitors, onMonitorsChange]);
    useEffect(() => { onResolutionChange?.(resolution); }, [resolution, onResolutionChange]);
    useEffect(() => { onFpsChange?.(fps); }, [fps, onFpsChange]);
    useEffect(() => { onLatencyChange?.(latency); }, [latency, onLatencyChange]);

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

        const ws = new WebSocket(`${getWsBase()}/console/${sessionId}`);
        ws.binaryType = 'arraybuffer';
        wsRef.current = ws;

        ws.onopen = () => {
            if (cancelledRef.current) { ws.close(); return; }
            console.log(`[Desktop WS] Connected to session ${sessionId}`);
            setStatus('connected');
            reconnectAttemptRef.current = 0;

            // Start ping interval for latency measurement
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
                        // ── H264 path via WebCodecs VideoDecoder ──
                        if (typeof VideoDecoder === 'undefined') {
                            // Browser doesn't support WebCodecs
                            if (!h264Error) {
                                setH264Error('VideoDecoder API not available in this browser. H.264 streaming requires Chrome, Edge, or Firefox 130+ with dom.media.webcodecs.enabled=true');
                            }
                            break;
                        }

                        // ── Helper: extract NAL units from Annex-B byte stream ──
                        const extractNALs = (annexB: Uint8Array): Uint8Array[] => {
                            const nals: Uint8Array[] = [];
                            let i = 0;
                            while (i < annexB.length - 3) {
                                // Find start code (00 00 00 01 or 00 00 01)
                                let scLen = 0;
                                if (annexB[i] === 0 && annexB[i + 1] === 0 && annexB[i + 2] === 1) scLen = 3;
                                else if (annexB[i] === 0 && annexB[i + 1] === 0 && i + 3 < annexB.length && annexB[i + 2] === 0 && annexB[i + 3] === 1) scLen = 4;
                                if (scLen === 0) { i++; continue; }
                                const nalStart = i + scLen;
                                // Find next start code
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

                        // ── Helper: build avcC decoder configuration record ──
                        const buildAvcC = (sps: Uint8Array, pps: Uint8Array): Uint8Array => {
                            // ISO 14496-15 AVCDecoderConfigurationRecord
                            const len = 11 + sps.length + pps.length;
                            const buf = new Uint8Array(len);
                            buf[0] = 1;           // configurationVersion
                            buf[1] = sps[1];      // AVCProfileIndication
                            buf[2] = sps[2];      // profile_compatibility
                            buf[3] = sps[3];      // AVCLevelIndication
                            buf[4] = 0xFF;        // lengthSizeMinusOne=3 (4-byte lengths) | reserved 6 bits
                            buf[5] = 0xE1;        // numSPS=1 | reserved 3 bits
                            buf[6] = (sps.length >> 8) & 0xFF;
                            buf[7] = sps.length & 0xFF;
                            buf.set(sps, 8);
                            const ppsOff = 8 + sps.length;
                            buf[ppsOff] = 1;      // numPPS
                            buf[ppsOff + 1] = (pps.length >> 8) & 0xFF;
                            buf[ppsOff + 2] = pps.length & 0xFF;
                            buf.set(pps, ppsOff + 3);
                            return buf;
                        };

                        // ── Helper: convert Annex-B to AVCC (4-byte length prefix) ──
                        // When description (avcC) is provided, SPS/PPS/AUD must NOT be in
                        // the frame data — they are out-of-band. Only keep VCL (1-5) and SEI (6).
                        const annexBtoAVCC = (annexB: Uint8Array): Uint8Array => {
                            const allNals = extractNALs(annexB);
                            // Filter: keep only VCL NALs (types 1-5) and SEI (type 6)
                            // Strip: SPS (7), PPS (8), AUD (9), and other non-VCL types
                            const nals = allNals.filter(nal => {
                                const t = nal[0] & 0x1F;
                                return t >= 1 && t <= 6;
                            });
                            // Calculate total size: 4 bytes length prefix per NAL + NAL data
                            let totalLen = 0;
                            for (const nal of nals) totalLen += 4 + nal.length;
                            const avcc = new Uint8Array(totalLen);
                            let offset = 0;
                            for (const nal of nals) {
                                // 4-byte big-endian length prefix
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
                            if (!frame.isKeyframe) break; // can't start without a keyframe

                            // Check for too many consecutive failures (prevent infinite retry)
                            const h264ErrorCountKey = '__h264ErrorCount';
                            const errorCount = (window as any)[h264ErrorCountKey] || 0;
                            if (errorCount >= 3) {
                                // After 3 failures, stop trying — H.264 is not working in this browser
                                if (errorCount === 3 || errorCount === 4) {
                                    console.error('[H264] ❌ H.264 decoding failed 3 times. WebCodecs H.264 is NOT supported in this browser.');
                                    console.error('[H264] Please use Chrome/Edge for H.264 streaming, or the server needs JPEG fallback.');
                                    (window as any)[h264ErrorCountKey] = 4; // prevent spamming
                                    setH264Error('H.264 WebCodecs decoding failed. This browser may not support H.264 hardware decoding. Try Chrome or Edge.');
                                }
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
                                // Reset error count on successful decode
                                (window as any)[h264ErrorCountKey] = 0;
                            };

                            h264DecoderRef.current = new VideoDecoder({
                                output: drawDecodedFrame,
                                error: (e) => {
                                    console.error('[H264] ❌ Decode error:', e);
                                    const count = ((window as any)[h264ErrorCountKey] || 0) + 1;
                                    (window as any)[h264ErrorCountKey] = count;
                                    console.error(`[H264] Error count: ${count}/3`);
                                    try {
                                        h264DecoderRef.current?.close();
                                    } catch { /* already closed */ }
                                    h264DecoderRef.current = null;
                                    h264ConfiguredRef.current = false;
                                },
                            });

                            // Extract SPS and PPS NAL units from keyframe for avcC description
                            const annexBData = new Uint8Array(frame.data);
                            const nals = extractNALs(annexBData);
                            let spsNAL: Uint8Array | null = null;
                            let ppsNAL: Uint8Array | null = null;
                            let codecString = 'avc1.42C033'; // Default: Constrained Baseline L5.1

                            for (const nal of nals) {
                                const nalType = nal[0] & 0x1F;
                                if (nalType === 7 && !spsNAL) {
                                    spsNAL = nal;
                                    const profile = nal[1];
                                    const compat = nal[2];
                                    const level = nal[3];
                                    codecString = `avc1.${profile.toString(16).padStart(2, '0')}${compat.toString(16).padStart(2, '0')}${level.toString(16).padStart(2, '0')}`;
                                    console.log(`[H264] SPS detected: profile=0x${profile.toString(16)}, compat=0x${compat.toString(16)}, level=0x${level.toString(16)} → codec=${codecString}`);
                                } else if (nalType === 8 && !ppsNAL) {
                                    ppsNAL = nal;
                                }
                            }

                            // Build decoder config with avcC description for cross-browser compat
                            const decoderConfig: VideoDecoderConfig = {
                                codec: codecString,
                                optimizeForLatency: true,
                            };

                            if (spsNAL && ppsNAL) {
                                const avcC = buildAvcC(spsNAL, ppsNAL);
                                decoderConfig.description = avcC.buffer;
                                console.log(`[H264] Built avcC description (${avcC.length} bytes) for cross-browser compatibility`);
                            } else {
                                console.warn('[H264] No SPS/PPS found in keyframe');
                            }

                            // Check if this config is actually supported before configuring
                            VideoDecoder.isConfigSupported(decoderConfig).then(support => {
                                console.log(`[H264] isConfigSupported result:`, JSON.stringify({
                                    supported: support.supported,
                                    codec: decoderConfig.codec,
                                    hasDescription: !!decoderConfig.description,
                                }));
                                if (!support.supported) {
                                    console.error('[H264] ❌ VideoDecoder does NOT support this H.264 config!');
                                    (window as any)[h264ErrorCountKey] = 3; // trigger fallback
                                    setH264Error(`H.264 codec "${decoderConfig.codec}" is NOT supported by WebCodecs in this browser. Try Chrome or Edge.`);
                                }
                            }).catch(e => {
                                console.error('[H264] ❌ isConfigSupported threw:', e);
                            });

                            try {
                                console.log(`[H264] Configuring VideoDecoder with codec=${codecString}`);
                                h264DecoderRef.current.configure(decoderConfig);
                                h264ConfiguredRef.current = true;
                                h264TimestampRef.current = 0;
                                console.log(`[H264] ✅ VideoDecoder configured successfully`);
                            } catch (e) {
                                console.error('[H264] ❌ configure() threw:', e);
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
                            // Throttle: skip if decoder queue is backing up
                            if (decoder.decodeQueueSize > 3) break;

                            try {
                                // Convert Annex-B to AVCC format for cross-browser compatibility
                                const avccData = annexBtoAVCC(new Uint8Array(frame.data));
                                const chunk = new EncodedVideoChunk({
                                    type: frame.isKeyframe ? 'key' : 'delta',
                                    timestamp: h264TimestampRef.current,
                                    data: avccData,
                                });
                                h264TimestampRef.current += 33333; // ~30fps in microseconds
                                decoder.decode(chunk);
                            } catch (e) {
                                console.error('[H264] ❌ decode() threw:', e);
                            }
                        }
                    } else {
                        // ── JPEG path (codec === 0 or unset) ──
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
                    console.log(`[Desktop WS] Session ended: ${envelope.payload.reason}`);
                    setStatus('disconnected');
                    break;
                }
                case 'clipboard_data': {
                    const clip = envelope.payload;
                    if (clip.text && navigator.clipboard) {
                        navigator.clipboard.writeText(clip.text).catch(() => {
                            console.debug('[Clipboard] writeText not allowed');
                        });
                        onClipboardReceived?.(clip.text);
                    }
                    break;
                }
                case 'pong': {
                    const rtt = Date.now() - envelope.payload.timestamp;
                    setLatency(rtt);

                    // Auto-adaptive quality: adjust tier based on RTT
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
                        // Only send when tier changes to avoid flooding
                        if (tier !== lastTierSentRef.current) {
                            lastTierSentRef.current = tier;
                            currentTierRef.current = tier;
                            onAutoQualityTierChange?.(tier);
                            ws.send(encodeQualitySettings(sessionId, quality, maxFps, bitrate));
                            console.log(`[AutoQuality] RTT=${rtt}ms → ${tier} (q=${quality}, fps=${maxFps}, bitrate=${bitrate}kbps)`);
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
            console.log(`[Desktop WS] Disconnected from session ${sessionId}`);

            // Stop ping interval
            if (pingIntervalRef.current) {
                clearInterval(pingIntervalRef.current);
                pingIntervalRef.current = null;
            }

            // Reset H264 decoder so it re-initializes with new stream's SPS/PPS on reconnect
            if (h264DecoderRef.current) {
                try { h264DecoderRef.current.close(); } catch { /* already closed */ }
                h264DecoderRef.current = null;
                h264ConfiguredRef.current = false;
                h264TimestampRef.current = 0;
                console.log('[H264] Decoder reset on disconnect');
            }

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
                {h264Error && (
                    <div style={{
                        position: 'absolute', top: 0, left: 0, right: 0,
                        padding: '16px', zIndex: 100,
                        background: 'rgba(180, 30, 30, 0.95)',
                        color: '#fff', fontFamily: 'monospace', fontSize: '13px',
                        borderBottom: '2px solid #ff4444',
                    }}>
                        <strong>⚠ H.264 Decode Error</strong><br />
                        {h264Error}<br />
                        <em style={{ opacity: 0.7 }}>UserAgent: {typeof navigator !== 'undefined' ? navigator.userAgent.slice(0, 120) : 'N/A'}</em>
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
                        const { x, y } = getNormCoords(e);
                        // Store normalized position for cursor drawing (on every move for smoothness)
                        cursorNormRef.current = { x, y };

                        const now = Date.now();
                        if (now - mouseMoveThrottleRef.current < 8) return;
                        mouseMoveThrottleRef.current = now;
                        sendBinary(encodeMouseMove(sessionId, x, y));
                    }}
                    onMouseLeave={() => { cursorNormRef.current = null; }}
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
                    onPaste={(e) => {
                        e.preventDefault();
                        const text = e.clipboardData?.getData('text/plain');
                        if (text) {
                            sendBinary(encodeClipboardData(sessionId, text));
                        }
                    }}
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
