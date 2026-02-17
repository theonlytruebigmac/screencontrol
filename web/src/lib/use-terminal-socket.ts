/**
 * WebSocket hook for terminal sessions.
 *
 * Connects to the server's console WebSocket endpoint, sends/receives
 * protobuf-encoded Envelope messages for terminal data streaming.
 */

import { useRef, useCallback, useEffect } from 'react';
import {
    encodeTerminalData,
    encodeTerminalResize,
    decodeEnvelope,
} from '@/lib/proto';
import { getWsBase } from '@/lib/urls';

export interface UseTerminalSocketOptions {
    sessionId: string;
    onData: (data: Uint8Array) => void;
    onConnected?: () => void;
    onDisconnected?: () => void;
    onError?: (error: Event) => void;
}

export function useTerminalSocket({
    sessionId,
    onData,
    onConnected,
    onDisconnected,
    onError,
}: UseTerminalSocketOptions) {
    const wsRef = useRef<WebSocket | null>(null);

    const connect = useCallback(() => {
        if (wsRef.current?.readyState === WebSocket.OPEN) return;

        const ws = new WebSocket(`${getWsBase()}/console/${sessionId}`);
        ws.binaryType = 'arraybuffer';

        ws.onopen = () => {
            console.log(`[WS] Connected to session ${sessionId}`);
            onConnected?.();
        };

        ws.onmessage = (event) => {
            if (event.data instanceof ArrayBuffer) {
                const bytes = new Uint8Array(event.data);
                const envelope = decodeEnvelope(bytes);
                if (envelope && envelope.payload.type === 'terminal_data') {
                    onData(envelope.payload.data);
                }
            }
        };

        ws.onclose = () => {
            console.log(`[WS] Disconnected from session ${sessionId}`);
            onDisconnected?.();
        };

        ws.onerror = (event) => {
            console.error(`[WS] Error on session ${sessionId}`, event);
            onError?.(event);
        };

        wsRef.current = ws;
    }, [sessionId, onData, onConnected, onDisconnected, onError]);

    const disconnect = useCallback(() => {
        wsRef.current?.close();
        wsRef.current = null;
    }, []);

    const sendData = useCallback((data: string | Uint8Array) => {
        if (wsRef.current?.readyState !== WebSocket.OPEN) return;

        const payload = typeof data === 'string'
            ? new TextEncoder().encode(data)
            : data;

        const encoded = encodeTerminalData(sessionId, payload);
        wsRef.current.send(encoded);
    }, [sessionId]);

    const sendResize = useCallback((cols: number, rows: number) => {
        if (wsRef.current?.readyState !== WebSocket.OPEN) return;

        const encoded = encodeTerminalResize(sessionId, cols, rows);
        wsRef.current.send(encoded);
    }, [sessionId]);

    // Cleanup on unmount
    useEffect(() => {
        return () => {
            wsRef.current?.close();
        };
    }, []);

    return { connect, disconnect, sendData, sendResize };
}
