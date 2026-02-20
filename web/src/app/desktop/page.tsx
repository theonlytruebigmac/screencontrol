"use client";

import { useState, useCallback } from "react";
import {
    Video,
    Plus,
    Check,
    Link2,
    Users,
    Monitor,
    LogIn,
    Loader2,
    Clock,
    X,
} from "lucide-react";
import { EmptyState } from "@/components/empty-state";
import { useToast } from "@/components/toast";

interface HostSession {
    id: string;
    code: string;
    host: string;
    participants: number;
    startedAt: Date;
    status: "active" | "ended";
}

export default function HostPage() {
    const [sessions, setSessions] = useState<HostSession[]>([]);
    const [joinCode, setJoinCode] = useState("");
    const [creating, setCreating] = useState(false);
    const [copied, setCopied] = useState<string | null>(null);
    const { info } = useToast();

    const handleStartSession = useCallback(() => {
        setCreating(true);
        setTimeout(() => {
            const code = Math.random().toString(36).substring(2, 8).toUpperCase();
            const session: HostSession = {
                id: (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') ? crypto.randomUUID() : Array.from(crypto.getRandomValues(new Uint8Array(16)), b => b.toString(16).padStart(2, '0')).join('').replace(/(.{8})(.{4})(.{4})(.{4})(.{12})/, '$1-$2-$3-$4-$5'),
                code,
                host: "Admin",
                participants: 1,
                startedAt: new Date(),
                status: "active",
            };
            setSessions((prev) => [session, ...prev]);
            setCreating(false);
        }, 800);
    }, []);

    const handleEndSession = useCallback((id: string) => {
        setSessions((prev) =>
            prev.map((m) => (m.id === id ? { ...m, status: "ended" as const } : m))
        );
    }, []);

    const copyToClipboard = useCallback((text: string, id: string) => {
        navigator.clipboard.writeText(text);
        setCopied(id);
        setTimeout(() => setCopied(null), 2000);
    }, []);

    const activeSessions = sessions.filter((m) => m.status === "active");

    return (
        <div className="flex h-full bg-[#141414]">
            {/* ─── Left Panel ─── */}
            <div className="w-[280px] border-r border-[#333] flex flex-col">
                {/* Header */}
                <div className="px-4 py-3 border-b border-[#333]">
                    <div className="flex items-center gap-2">
                        <Video className="w-5 h-5 text-[#e05246]" />
                        <div>
                            <h2 className="text-base font-bold text-white">Host</h2>
                            <p className="text-[10px] text-gray-500">
                                Host or join a shared session
                            </p>
                        </div>
                    </div>
                </div>

                <div className="p-3 flex-1 overflow-y-auto space-y-2">
                    {/* Host */}
                    <div className="bg-[#1e1e1e] border border-[#333] rounded-lg p-4">
                        <div className="flex items-center gap-2.5 mb-3">
                            <div className="w-7 h-7 rounded-md bg-[#e05246]/15 flex items-center justify-center">
                                <Monitor className="w-3.5 h-3.5 text-[#e05246]" />
                            </div>
                            <div>
                                <h3 className="text-xs font-semibold text-white">
                                    Host Session
                                </h3>
                                <p className="text-[10px] text-gray-500">
                                    Share your screen with others
                                </p>
                            </div>
                        </div>
                        <button
                            onClick={handleStartSession}
                            disabled={creating}
                            className="w-full flex items-center justify-center gap-2 py-2 bg-[#e05246] hover:bg-[#c43d32] text-white text-xs font-medium rounded-lg transition-colors disabled:opacity-50"
                        >
                            {creating ? (
                                <Loader2 className="w-3.5 h-3.5 animate-spin" />
                            ) : (
                                <Plus className="w-3.5 h-3.5" />
                            )}
                            Start Session
                        </button>
                    </div>

                    {/* Join */}
                    <div className="bg-[#1e1e1e] border border-[#333] rounded-lg p-4 overflow-hidden">
                        <div className="flex items-center gap-2.5 mb-3">
                            <div className="w-7 h-7 rounded-md bg-blue-500/15 flex items-center justify-center">
                                <LogIn className="w-3.5 h-3.5 text-blue-400" />
                            </div>
                            <div>
                                <h3 className="text-xs font-semibold text-white">
                                    Join Session
                                </h3>
                                <p className="text-[10px] text-gray-500">
                                    Enter a code to connect
                                </p>
                            </div>
                        </div>
                        <div className="flex gap-1.5 overflow-hidden">
                            <input
                                type="text"
                                placeholder="ENTER CODE..."
                                value={joinCode}
                                onChange={(e) =>
                                    setJoinCode(e.target.value.toUpperCase())
                                }
                                maxLength={6}
                                className="flex-1 min-w-0 px-3 py-2 bg-[#141414] border border-[#333] rounded-lg text-xs text-white text-center font-mono tracking-widest placeholder-gray-600 focus:outline-none focus:border-blue-500/50 uppercase transition-colors"
                            />
                            <button
                                disabled={joinCode.length < 4}
                                onClick={() => info("Coming Soon", "Session join will be available when the host backend is implemented")}
                                className="shrink-0 px-3 py-2 bg-blue-600 hover:bg-blue-700 text-white text-xs font-medium rounded-lg transition-colors disabled:opacity-40"
                            >
                                Join
                            </button>
                        </div>
                    </div>

                    {/* Stats */}
                    <div className="grid grid-cols-2 gap-1.5">
                        <div className="bg-[#1e1e1e] border border-[#333] rounded-lg p-2.5 text-center">
                            <div className="text-base font-bold text-white">
                                {activeSessions.length}
                            </div>
                            <div className="text-[9px] text-gray-500 uppercase tracking-wider">
                                Active
                            </div>
                        </div>
                        <div className="bg-[#1e1e1e] border border-[#333] rounded-lg p-2.5 text-center">
                            <div className="text-base font-bold text-white">
                                {sessions.length}
                            </div>
                            <div className="text-[9px] text-gray-500 uppercase tracking-wider">
                                Total
                            </div>
                        </div>
                    </div>
                </div>
            </div>

            {/* ─── Right Panel ─── */}
            <div className="flex-1 flex flex-col">
                <div className="flex items-center gap-3 px-4 py-2.5 border-b border-[#333]">
                    <h3 className="text-sm font-semibold text-white">Host Sessions</h3>
                    <span className="text-[11px] text-gray-600">
                        {activeSessions.length} active
                    </span>
                </div>

                <div className="flex-1 overflow-y-auto">
                    {sessions.length === 0 ? (
                        <EmptyState
                            icon={Video}
                            title="No host sessions"
                            description="Start a session to share your screen in real time with anyone using a simple code."
                            actionLabel="Start Session"
                            onAction={handleStartSession}
                            color="#e05246"
                        />
                    ) : (
                        sessions.map((session) => {
                            const duration = (() => {
                                const mins = Math.floor(
                                    (Date.now() - session.startedAt.getTime()) / 60000
                                );
                                if (mins < 60) return `${mins}m`;
                                return `${Math.floor(mins / 60)}h ${mins % 60}m`;
                            })();
                            const link = `${typeof window !== "undefined" ? window.location.origin : ""}/host/${session.code}`;
                            const isActive = session.status === "active";

                            return (
                                <div
                                    key={session.id}
                                    className="px-4 py-3 border-b border-[#272727] hover:bg-white/[0.02] transition-colors"
                                >
                                    <div className="flex items-center gap-3">
                                        <div
                                            className={`w-8 h-8 rounded-lg flex items-center justify-center ${isActive ? "bg-emerald-500/10" : "bg-gray-800"}`}
                                        >
                                            <Video
                                                className={`w-4 h-4 ${isActive ? "text-emerald-400" : "text-gray-600"}`}
                                            />
                                        </div>
                                        <div className="flex-1 min-w-0">
                                            <div className="flex items-center gap-2">
                                                <span className="text-sm font-medium text-white">
                                                    Session {session.code}
                                                </span>
                                                {isActive && (
                                                    <span className="flex items-center gap-1 text-[10px] text-emerald-400">
                                                        <span className="w-1.5 h-1.5 rounded-full bg-emerald-400 animate-pulse" />
                                                        Live
                                                    </span>
                                                )}
                                            </div>
                                            <div className="flex items-center gap-2 text-[11px] text-gray-500">
                                                <span className="flex items-center gap-1">
                                                    <Users className="w-3 h-3" />
                                                    {session.participants} participant
                                                    {session.participants !== 1
                                                        ? "s"
                                                        : ""}
                                                </span>
                                                <span>•</span>
                                                <span className="flex items-center gap-1">
                                                    <Clock className="w-3 h-3" />
                                                    {duration}
                                                </span>
                                            </div>
                                        </div>
                                        <div className="flex items-center gap-1.5">
                                            {isActive && (
                                                <>
                                                    <button
                                                        onClick={() =>
                                                            copyToClipboard(
                                                                link,
                                                                session.id
                                                            )
                                                        }
                                                        className="p-1.5 bg-[#333] hover:bg-[#444] rounded transition-colors"
                                                        title="Copy session link"
                                                    >
                                                        {copied === session.id ? (
                                                            <Check className="w-3.5 h-3.5 text-emerald-400" />
                                                        ) : (
                                                            <Link2 className="w-3.5 h-3.5 text-gray-400" />
                                                        )}
                                                    </button>
                                                    <button
                                                        onClick={() =>
                                                            handleEndSession(
                                                                session.id
                                                            )
                                                        }
                                                        className="px-2.5 py-1 bg-[#333] hover:bg-[#444] text-gray-400 text-[11px] rounded transition-colors"
                                                    >
                                                        <X className="w-3 h-3" />
                                                    </button>
                                                </>
                                            )}
                                            {!isActive && (
                                                <span className="text-[10px] text-gray-600 px-2 py-0.5 bg-[#333] rounded">
                                                    Ended
                                                </span>
                                            )}
                                        </div>
                                    </div>
                                </div>
                            );
                        })
                    )}
                </div>

                <div className="px-4 py-2 border-t border-[#333] text-[11px] text-gray-600">
                    {sessions.length} total sessions
                </div>
            </div>
        </div>
    );
}
