'use client';

/**
 * useEvents — React hook for real-time events via WebSocket.
 *
 * Connects to `ws://<server>/ws/events` and listens for all event types.
 * Returns both an agent status map AND a notifications array.
 */

import { useState, useEffect, useRef, useCallback } from 'react';

const WS_BASE = process.env.NEXT_PUBLIC_WS_URL || 'ws://localhost:8080/ws';

export interface ServerEvent {
    type: string;
    agent_id?: string;
    machine_name?: string;
    status?: string;
    session_id?: string;
    session_type?: string;
    user_id?: string;
    email?: string;
    action?: string;
    timestamp?: string;
}

export interface LiveNotification {
    id: string;
    title: string;
    message: string;
    type: 'info' | 'warning' | 'success' | 'error';
    source: string;
    time: string;
    read: boolean;
}

function eventToNotification(event: ServerEvent): LiveNotification | null {
    const id = `evt-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`;
    const time = event.timestamp || new Date().toISOString();

    switch (event.type) {
        case 'agent.status':
            if (event.status === 'online') {
                return { id, title: 'Agent Online', message: `${event.machine_name || event.agent_id} connected`, type: 'success', source: 'Agent Monitor', time, read: false };
            }
            return { id, title: 'Agent Offline', message: `${event.machine_name || event.agent_id} disconnected`, type: 'warning', source: 'Agent Monitor', time, read: false };

        case 'session.create':
            return { id, title: 'Session Started', message: `${event.session_type || 'Session'} session created`, type: 'info', source: 'Sessions', time, read: false };

        case 'session.end':
            return { id, title: 'Session Ended', message: `Session ended`, type: 'info', source: 'Sessions', time, read: false };

        case 'user.login':
            return { id, title: 'User Login', message: `${event.email || 'A user'} signed in`, type: 'info', source: 'Security', time, read: false };

        default:
            return null;
    }
}

const MAX_NOTIFICATIONS = 100;

export function useEvents() {
    const [statusMap, setStatusMap] = useState<Map<string, string>>(new Map());
    const [notifications, setNotifications] = useState<LiveNotification[]>([]);
    const wsRef = useRef<WebSocket | null>(null);
    const retryRef = useRef(1);

    const connect = useCallback(() => {
        if (typeof window === 'undefined') return;

        const ws = new WebSocket(`${WS_BASE}/events`);
        wsRef.current = ws;

        ws.onopen = () => {
            retryRef.current = 1;
        };

        ws.onmessage = (event) => {
            try {
                const data = JSON.parse(event.data) as ServerEvent;

                // Update agent status map
                if (data.type === 'agent.status' && data.agent_id && data.status) {
                    setStatusMap((prev) => {
                        const next = new Map(prev);
                        next.set(data.agent_id!, data.status!);
                        return next;
                    });
                }

                // Convert to notification
                const notif = eventToNotification(data);
                if (notif) {
                    setNotifications((prev) => [notif, ...prev].slice(0, MAX_NOTIFICATIONS));
                }
            } catch {
                // ignore non-JSON
            }
        };

        ws.onclose = () => {
            const delay = Math.min(retryRef.current * 1000, 30000);
            retryRef.current = Math.min(retryRef.current * 2, 30);
            setTimeout(connect, delay);
        };

        ws.onerror = () => {
            ws.close();
        };
    }, []);

    useEffect(() => {
        connect();
        return () => {
            wsRef.current?.close();
        };
    }, [connect]);

    const markRead = useCallback((id: string) => {
        setNotifications((prev) => prev.map(n => n.id === id ? { ...n, read: true } : n));
    }, []);

    const markAllRead = useCallback(() => {
        setNotifications((prev) => prev.map(n => ({ ...n, read: true })));
    }, []);

    const dismiss = useCallback((id: string) => {
        setNotifications((prev) => prev.filter(n => n.id !== id));
    }, []);

    const clearAll = useCallback(() => {
        setNotifications([]);
    }, []);

    return {
        statusMap,
        notifications,
        markRead,
        markAllRead,
        dismiss,
        clearAll,
        unreadCount: notifications.filter(n => !n.read).length,
    };
}
