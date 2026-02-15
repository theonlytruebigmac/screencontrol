"use client";

import { useState, useEffect, useRef, useCallback } from "react";
import {
    Bell,
    X,
    Monitor,
    Terminal,
    Shield,
    Wifi,
    Clock,
    CheckCheck,
} from "lucide-react";
import { api, type AuditEntry } from "@/lib/api";
import { getAccessToken } from "@/lib/auth-store";

export interface Notification {
    id: string;
    type: "agent" | "session" | "system" | "security";
    title: string;
    message: string;
    timestamp: Date;
    read: boolean;
}

const iconMap = {
    agent: Monitor,
    session: Terminal,
    system: Wifi,
    security: Shield,
};

const colorMap = {
    agent: "text-emerald-400 bg-emerald-500/15",
    session: "text-cyan-400 bg-cyan-500/15",
    system: "text-amber-400 bg-amber-500/15",
    security: "text-red-400 bg-red-500/15",
};

// Map audit log actions → notification types
function auditToNotification(entry: AuditEntry): Notification {
    const meta = entry.metadata as Record<string, string>;
    let type: Notification["type"] = "system";
    let title = entry.action.replace(/\./g, " ").replace(/\b\w/g, c => c.toUpperCase());
    let message = `Action: ${entry.action}`;

    switch (entry.action) {
        case "agent.register":
            type = "agent";
            title = "Agent Registered";
            message = meta?.machine_name ? `${meta.machine_name} joined the fleet` : "A new agent registered";
            break;
        case "agent.heartbeat":
            type = "agent";
            title = "Agent Heartbeat";
            message = meta?.machine_name ? `${meta.machine_name} checked in` : "Agent heartbeat received";
            break;
        case "session.create":
            type = "session";
            title = "Session Started";
            message = meta?.session_type
                ? `${meta.session_type} session created${meta.agent_name ? ` on ${meta.agent_name}` : ""}`
                : "A new session was created";
            break;
        case "session.end":
            type = "session";
            title = "Session Ended";
            message = meta?.session_type
                ? `${meta.session_type} session ended${meta.agent_name ? ` on ${meta.agent_name}` : ""}`
                : "A session ended";
            break;
        case "user.login":
            type = "security";
            title = "User Login";
            message = entry.ip_address
                ? `User logged in from ${entry.ip_address}`
                : "User logged in";
            break;
        case "user.create":
            type = "security";
            title = "User Created";
            message = meta?.email ? `New user ${meta.email} created` : "A new user was created";
            break;
        case "user.delete":
            type = "security";
            title = "User Deleted";
            message = meta?.email ? `User ${meta.email} removed` : "A user was deleted";
            break;
        default:
            if (entry.action.startsWith("agent.")) type = "agent";
            else if (entry.action.startsWith("session.")) type = "session";
            else if (entry.action.startsWith("user.")) type = "security";
            break;
    }

    return {
        id: entry.id,
        type,
        title,
        message,
        timestamp: new Date(entry.created_at),
        read: false,
    };
}

export function NotificationCenter() {
    const [isOpen, setIsOpen] = useState(false);
    const [notifications, setNotifications] = useState<Notification[]>([]);
    const [readIds, setReadIds] = useState<Set<string>>(new Set());
    const panelRef = useRef<HTMLDivElement>(null);
    const hasFetched = useRef(false);

    const fetchNotifications = useCallback(async () => {
        try {
            const token = getAccessToken();
            if (token) api.setToken(token);
            const entries = await api.getAuditLog({ limit: 20 });
            const mapped = entries.map(auditToNotification).map(n => ({
                ...n,
                read: readIds.has(n.id),
            }));
            setNotifications(mapped);
        } catch (e) {
            console.error("Failed to fetch notifications:", e);
        }
    }, [readIds]);

    // Fetch on mount
    useEffect(() => {
        if (!hasFetched.current) {
            hasFetched.current = true;
            fetchNotifications();
        }
    }, [fetchNotifications]);

    // Refresh when panel opens
    useEffect(() => {
        if (isOpen) fetchNotifications();
    }, [isOpen, fetchNotifications]);

    const unreadCount = notifications.filter((n) => !n.read).length;

    // Close on click outside
    useEffect(() => {
        if (!isOpen) return;
        const handler = (e: MouseEvent) => {
            if (panelRef.current && !panelRef.current.contains(e.target as Node)) {
                setIsOpen(false);
            }
        };
        document.addEventListener("mousedown", handler);
        return () => document.removeEventListener("mousedown", handler);
    }, [isOpen]);

    // Close on ESC
    useEffect(() => {
        const handler = (e: KeyboardEvent) => { if (e.key === "Escape") setIsOpen(false); };
        document.addEventListener("keydown", handler);
        return () => document.removeEventListener("keydown", handler);
    }, []);

    const markAllRead = useCallback(() => {
        setReadIds(prev => {
            const next = new Set(prev);
            notifications.forEach(n => next.add(n.id));
            return next;
        });
        setNotifications((prev) =>
            prev.map((n) => ({ ...n, read: true }))
        );
    }, [notifications]);

    const clearAll = useCallback(() => {
        setNotifications([]);
        setIsOpen(false);
    }, []);

    const formatTime = (d: Date) => {
        const diff = Date.now() - d.getTime();
        const mins = Math.floor(diff / 60_000);
        if (mins < 1) return "Just now";
        if (mins < 60) return `${mins}m ago`;
        const hrs = Math.floor(mins / 60);
        if (hrs < 24) return `${hrs}h ago`;
        return `${Math.floor(hrs / 24)}d ago`;
    };

    return (
        <div className="relative" ref={panelRef}>
            {/* Bell trigger */}
            <button
                onClick={() => setIsOpen(!isOpen)}
                className="relative flex items-center justify-center w-full py-2 text-white/60 hover:text-white hover:bg-white/10 transition-colors"
                title="Notifications"
            >
                <Bell className="w-4.5 h-4.5" />
                {unreadCount > 0 && (
                    <span className="absolute top-1 right-2 w-4 h-4 rounded-full bg-[#e05246] text-[9px] font-bold text-white flex items-center justify-center">
                        {unreadCount > 9 ? "9+" : unreadCount}
                    </span>
                )}
            </button>

            {/* Panel */}
            {isOpen && (
                <div className="absolute left-full bottom-0 ml-2 w-80 bg-[#1e1e1e] border border-[#333] rounded-xl shadow-2xl z-[100] slide-up overflow-hidden">
                    {/* Header */}
                    <div className="flex items-center justify-between px-4 py-3 border-b border-[#333]">
                        <div className="flex items-center gap-2">
                            <Bell className="w-4 h-4 text-[#e05246]" />
                            <span className="text-sm font-semibold text-white">Notifications</span>
                            {unreadCount > 0 && (
                                <span className="bg-[#e05246] text-white text-[10px] px-1.5 py-0.5 rounded-full font-medium">
                                    {unreadCount}
                                </span>
                            )}
                        </div>
                        <div className="flex items-center gap-1">
                            {unreadCount > 0 && (
                                <button
                                    onClick={markAllRead}
                                    className="p-1 text-gray-500 hover:text-gray-300 transition-colors"
                                    title="Mark all read"
                                >
                                    <CheckCheck className="w-3.5 h-3.5" />
                                </button>
                            )}
                            <button
                                onClick={() => setIsOpen(false)}
                                className="p-1 text-gray-500 hover:text-gray-300 transition-colors"
                            >
                                <X className="w-3.5 h-3.5" />
                            </button>
                        </div>
                    </div>

                    {/* Notification list */}
                    <div className="max-h-80 overflow-y-auto">
                        {notifications.length === 0 ? (
                            <div className="flex flex-col items-center py-10 text-gray-600">
                                <Bell className="w-8 h-8 mb-2 opacity-20" />
                                <p className="text-xs">No notifications</p>
                            </div>
                        ) : (
                            notifications.map((n) => {
                                const Icon = iconMap[n.type];
                                return (
                                    <div
                                        key={n.id}
                                        className={`flex items-start gap-3 px-4 py-3 border-b border-[#272727] transition-colors cursor-pointer ${n.read ? "opacity-60" : "hover:bg-white/[0.02]"
                                            }`}
                                        onClick={() => {
                                            setReadIds(prev => new Set(prev).add(n.id));
                                            setNotifications((prev) =>
                                                prev.map((x) =>
                                                    x.id === n.id ? { ...x, read: true } : x
                                                )
                                            );
                                        }}
                                    >
                                        <div className={`w-7 h-7 rounded-lg flex items-center justify-center flex-shrink-0 mt-0.5 ${colorMap[n.type]}`}>
                                            <Icon className="w-3.5 h-3.5" />
                                        </div>
                                        <div className="min-w-0 flex-1">
                                            <div className="flex items-center gap-2">
                                                <span className="text-xs font-medium text-white truncate">{n.title}</span>
                                                {!n.read && (
                                                    <span className="w-1.5 h-1.5 rounded-full bg-[#e05246] flex-shrink-0" />
                                                )}
                                            </div>
                                            <p className="text-[11px] text-gray-500 mt-0.5 line-clamp-2">{n.message}</p>
                                            <div className="flex items-center gap-1 mt-1 text-[10px] text-gray-600">
                                                <Clock className="w-2.5 h-2.5" />
                                                {formatTime(n.timestamp)}
                                            </div>
                                        </div>
                                    </div>
                                );
                            })
                        )}
                    </div>

                    {/* Footer */}
                    {notifications.length > 0 && (
                        <div className="px-4 py-2 border-t border-[#333] text-center">
                            <button
                                onClick={clearAll}
                                className="text-[11px] text-[#e05246] hover:text-[#f06b60] transition-colors"
                            >
                                Clear all notifications
                            </button>
                        </div>
                    )}
                </div>
            )}
        </div>
    );
}
