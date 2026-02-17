/**
 * WebSocket hook for agent command/chat sessions.
 *
 * Creates an ad-hoc "command" session for the agent, connects via
 * `/ws/console/{sessionId}`, and provides `sendCommand` / `sendChat`
 * helpers that encode protobuf Envelope messages.
 *
 * Incoming `CommandResponse` and `ChatMessage` envelopes are
 * decoded and surfaced through callback props.
 */

import { useRef, useCallback, useEffect, useState } from 'react';
import { api } from '@/lib/api';
import {
    encodeCommandRequest,
    encodeChatMessage,
    decodeEnvelope,
} from '@/lib/proto';
import { getWsBase } from '@/lib/urls';

export interface CommandResult {
    exitCode: number;
    stdout: string;
    stderr: string;
    timedOut: boolean;
}

export interface ChatMsg {
    senderId: string;
    senderName: string;
    content: string;
    timestamp: Date;
}

export interface UseAgentSocketOptions {
    agentId: string;
    onCommandResponse?: (result: CommandResult) => void;
    onChatMessage?: (msg: ChatMsg) => void;
}

export type AgentSocketStatus = 'idle' | 'connecting' | 'connected' | 'disconnected' | 'error';

export function useAgentSocket({ agentId, onCommandResponse, onChatMessage }: UseAgentSocketOptions) {
    const wsRef = useRef<WebSocket | null>(null);
    const sessionIdRef = useRef<string | null>(null);
    const [status, setStatus] = useState<AgentSocketStatus>('idle');

    const connect = useCallback(async () => {
        if (wsRef.current?.readyState === WebSocket.OPEN) return;
        setStatus('connecting');

        try {
            // Create a "command" session via REST API
            const session = await api.createSession(agentId, 'terminal');
            sessionIdRef.current = session.id;

            const ws = new WebSocket(`${getWsBase()}/console/${session.id}`);
            ws.binaryType = 'arraybuffer';

            ws.onopen = () => {
                console.log(`[AgentSocket] Connected to session ${session.id}`);
                setStatus('connected');
            };

            ws.onmessage = (event) => {
                if (!(event.data instanceof ArrayBuffer)) return;

                const bytes = new Uint8Array(event.data);
                const envelope = decodeEnvelope(bytes);
                if (!envelope) return;

                if (envelope.payload.type === 'command_response') {
                    onCommandResponse?.({
                        exitCode: envelope.payload.exitCode,
                        stdout: envelope.payload.stdout,
                        stderr: envelope.payload.stderr,
                        timedOut: envelope.payload.timedOut,
                    });
                } else if (envelope.payload.type === 'chat_message') {
                    onChatMessage?.({
                        senderId: envelope.payload.senderId,
                        senderName: envelope.payload.senderName,
                        content: envelope.payload.content,
                        timestamp: new Date(),
                    });
                }
            };

            ws.onclose = () => {
                console.log(`[AgentSocket] Disconnected from session ${session.id}`);
                setStatus('disconnected');
            };

            ws.onerror = () => {
                setStatus('error');
            };

            wsRef.current = ws;
        } catch (err) {
            console.error('[AgentSocket] Failed to create session:', err);
            setStatus('error');
        }
    }, [agentId, onCommandResponse, onChatMessage]);

    const disconnect = useCallback(() => {
        const sid = sessionIdRef.current;
        if (sid) {
            api.endSession(sid).catch(() => { });
        }
        wsRef.current?.close();
        wsRef.current = null;
        sessionIdRef.current = null;
        setStatus('idle');
    }, []);

    const sendCommand = useCallback((command: string, args: string[] = [], timeoutSecs = 30) => {
        const sid = sessionIdRef.current;
        if (!sid || wsRef.current?.readyState !== WebSocket.OPEN) return;

        const encoded = encodeCommandRequest(sid, command, args, '', timeoutSecs);
        wsRef.current.send(encoded);
    }, []);

    const sendChat = useCallback((content: string, senderName = 'Admin') => {
        const sid = sessionIdRef.current;
        if (!sid || wsRef.current?.readyState !== WebSocket.OPEN) return;

        const encoded = encodeChatMessage(sid, 'admin', senderName, content);
        wsRef.current.send(encoded);
    }, []);

    // Cleanup on unmount
    useEffect(() => {
        return () => {
            const sid = sessionIdRef.current;
            if (sid) {
                api.endSession(sid).catch(() => { });
            }
            wsRef.current?.close();
        };
    }, []);

    return { connect, disconnect, sendCommand, sendChat, status, sessionId: sessionIdRef.current };
}
