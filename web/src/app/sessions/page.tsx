"use client";
import { getAccessToken } from "@/lib/auth-store";

import { useState, useEffect, useCallback, useMemo } from "react";
import { useRouter } from "next/navigation";
import {
    Headset,
    Copy,
    Check,
    Link2,
    Plus,
    Monitor,
    Terminal,
    FileText,
    X,
    Clock,
    Loader2,
    ExternalLink,
    Users,
    Search,
    ArrowRight,
    CalendarDays,
    Info,
} from "lucide-react";
import { api, type Agent } from "@/lib/api";
import { EmptyState } from "@/components/empty-state";
import { useToast } from "@/components/toast";
import { launchDesktopSession, launchTerminalSession } from "@/lib/session-launcher";

interface Session {
    id: string;
    agent_id: string;
    session_type: string;
    status: string;
    started_at: string;
    ended_at: string | null;
}

const POLL_INTERVAL = 5000;

type SessionFilter = "all" | "desktop" | "terminal" | "file_transfer";

// ─── Ad-Hoc Code Generator ─────────────────────────────────

function AdHocCodeCard() {
    const [code, setCode] = useState<string | null>(null);
    const [link, setLink] = useState<string | null>(null);
    const [copied, setCopied] = useState<"code" | "link" | null>(null);
    const [generating, setGenerating] = useState(false);

    const generateCode = useCallback(() => {
        setGenerating(true);
        setTimeout(() => {
            const newCode = Math.random().toString(36).substring(2, 8).toUpperCase();
            setCode(newCode);
            setLink(`${window.location.origin}/adhoc/${newCode}`);
            setGenerating(false);
        }, 600);
    }, []);

    const copyToClipboard = useCallback((text: string, type: "code" | "link") => {
        navigator.clipboard.writeText(text);
        setCopied(type);
        setTimeout(() => setCopied(null), 2000);
    }, []);

    return (
        <div className="bg-[#1e1e1e] border border-[#333] rounded-lg p-5">
            <div className="flex items-center gap-2 mb-4">
                <div className="w-8 h-8 rounded-lg bg-[#e05246]/15 flex items-center justify-center">
                    <Headset className="w-4 h-4 text-[#e05246]" />
                </div>
                <div>
                    <h3 className="text-sm font-semibold text-white">Create Ad-Hoc Session</h3>
                    <p className="text-[10px] text-gray-500">Generate a code for your client</p>
                </div>
            </div>

            {!code ? (
                <button
                    onClick={generateCode}
                    disabled={generating}
                    className="w-full flex items-center justify-center gap-2 py-3 bg-[#e05246] hover:bg-[#c43d32] text-white text-sm font-medium rounded-lg transition-colors disabled:opacity-50"
                >
                    {generating ? (
                        <Loader2 className="w-4 h-4 animate-spin" />
                    ) : (
                        <Plus className="w-4 h-4" />
                    )}
                    Generate Ad-Hoc Code
                </button>
            ) : (
                <div className="space-y-3">
                    {/* Ad-hoc code */}
                    <div>
                        <label className="text-[10px] uppercase tracking-wider text-gray-500 mb-1 block">
                            Ad-Hoc Code
                        </label>
                        <div className="flex items-center gap-2">
                            <div className="flex-1 bg-[#141414] border border-[#333] rounded-lg px-4 py-3 text-center">
                                <span className="text-2xl font-mono font-bold text-white tracking-[0.3em]">
                                    {code}
                                </span>
                            </div>
                            <button
                                onClick={() => copyToClipboard(code, "code")}
                                className="p-3 bg-[#333] hover:bg-[#444] rounded-lg transition-colors"
                                title="Copy code"
                            >
                                {copied === "code" ? (
                                    <Check className="w-4 h-4 text-emerald-400" />
                                ) : (
                                    <Copy className="w-4 h-4 text-gray-400" />
                                )}
                            </button>
                        </div>
                    </div>

                    {/* Ad-hoc link */}
                    <div>
                        <label className="text-[10px] uppercase tracking-wider text-gray-500 mb-1 block">
                            Direct Link
                        </label>
                        <div className="flex items-center gap-2">
                            <div className="flex-1 bg-[#141414] border border-[#333] rounded-lg px-3 py-2 overflow-hidden">
                                <span className="text-xs text-gray-400 truncate block">
                                    {link}
                                </span>
                            </div>
                            <button
                                onClick={() => link && copyToClipboard(link, "link")}
                                className="p-2.5 bg-[#333] hover:bg-[#444] rounded-lg transition-colors"
                                title="Copy link"
                            >
                                {copied === "link" ? (
                                    <Check className="w-4 h-4 text-emerald-400" />
                                ) : (
                                    <Link2 className="w-4 h-4 text-gray-400" />
                                )}
                            </button>
                        </div>
                    </div>

                    {/* Regenerate */}
                    <div className="flex items-center justify-between pt-1">
                        <span className="text-[10px] text-gray-600">
                            Code expires in 10 minutes
                        </span>
                        <button
                            onClick={generateCode}
                            className="text-[11px] text-[#e05246] hover:text-[#f06b60] transition-colors"
                        >
                            Generate New
                        </button>
                    </div>
                </div>
            )}
        </div>
    );
}

// ─── Session Row ─────────────────────────────────────────────

function SessionRow({
    session,
    agentName,
    selected,
    onSelect,
    onJoin,
    onEnd,
    isEnding,
}: {
    session: Session;
    agentName: string;
    selected: boolean;
    onSelect: () => void;
    onJoin: () => void;
    onEnd: () => void;
    isEnding: boolean;
}) {
    const isActive = session.status === "active" || session.status === "pending";
    const typeIcon =
        session.session_type === "desktop" ? Monitor :
            session.session_type === "terminal" ? Terminal :
                FileText;
    const TypeIcon = typeIcon;

    const duration = (() => {
        const start = new Date(session.started_at).getTime();
        const end = session.ended_at ? new Date(session.ended_at).getTime() : Date.now();
        const mins = Math.floor((end - start) / 60000);
        if (mins < 60) return `${mins}m`;
        const hrs = Math.floor(mins / 60);
        return `${hrs}h ${mins % 60}m`;
    })();

    return (
        <div
            onClick={onSelect}
            className={`flex items-center gap-3 px-4 py-3 border-b border-[#272727] cursor-pointer transition-colors ${selected ? "bg-white/[0.04] border-l-2 border-l-[#e05246]" : "hover:bg-white/[0.02]"}`}
        >
            <div className={`w-8 h-8 rounded-lg flex items-center justify-center ${isActive ? "bg-emerald-500/10" : "bg-gray-800"}`}>
                <TypeIcon className={`w-4 h-4 ${isActive ? "text-emerald-400" : "text-gray-600"}`} />
            </div>
            <div className="flex-1 min-w-0">
                <div className="text-sm text-white font-medium truncate">{agentName}</div>
                <div className="flex items-center gap-2 text-[11px] text-gray-500">
                    <span className="capitalize">{session.session_type.replace("_", " ")}</span>
                    <span>•</span>
                    <span className="flex items-center gap-1">
                        <Clock className="w-3 h-3" />
                        {duration}
                    </span>
                </div>
            </div>
            <div className="flex items-center gap-1.5">
                {isActive ? (
                    <>
                        <button
                            onClick={(e) => { e.stopPropagation(); onJoin(); }}
                            className="px-2.5 py-1 bg-[#e05246] hover:bg-[#c43d32] text-white text-[11px] rounded transition-colors"
                        >
                            Join
                        </button>
                        <button
                            onClick={(e) => { e.stopPropagation(); onEnd(); }}
                            disabled={isEnding}
                            className="px-2.5 py-1 bg-[#333] hover:bg-[#444] text-gray-400 text-[11px] rounded transition-colors disabled:opacity-40"
                        >
                            {isEnding ? <Loader2 className="w-3 h-3 animate-spin" /> : <X className="w-3 h-3" />}
                        </button>
                    </>
                ) : (
                    <span className="text-[10px] text-gray-600 px-2 py-0.5 bg-[#333] rounded">
                        Ended
                    </span>
                )}
            </div>
        </div>
    );
}

// ─── Session Detail Panel ────────────────────────────────────

function SessionDetail({
    session,
    agentName,
    onJoin,
    onClose,
}: {
    session: Session;
    agentName: string;
    onJoin: () => void;
    onClose: () => void;
}) {
    const router = useRouter();
    const isActive = session.status === "active" || session.status === "pending";
    const TypeIcon =
        session.session_type === "desktop" ? Monitor :
            session.session_type === "terminal" ? Terminal :
                FileText;

    const typeLabel =
        session.session_type === "desktop" ? "Desktop" :
            session.session_type === "terminal" ? "Terminal" :
                session.session_type === "file_transfer" ? "File Transfer" :
                    session.session_type;

    const startDate = new Date(session.started_at);
    const endDate = session.ended_at ? new Date(session.ended_at) : null;
    const durationMs = (endDate?.getTime() || Date.now()) - startDate.getTime();
    const durationMins = Math.floor(durationMs / 60000);
    const durationStr = durationMins < 60
        ? `${durationMins}m`
        : `${Math.floor(durationMins / 60)}h ${durationMins % 60}m`;

    return (
        <div className="w-[320px] border-l border-[#333] flex flex-col bg-[#141414]">
            {/* Header */}
            <div className="px-4 py-3 border-b border-[#333] flex items-center justify-between">
                <div className="flex items-center gap-2">
                    <div className={`w-7 h-7 rounded-md flex items-center justify-center ${isActive ? "bg-emerald-500/10" : "bg-gray-800"}`}>
                        <TypeIcon className={`w-3.5 h-3.5 ${isActive ? "text-emerald-400" : "text-gray-600"}`} />
                    </div>
                    <div>
                        <h3 className="text-sm font-semibold text-white">{agentName}</h3>
                        <p className="text-[10px] text-gray-500">{typeLabel} Session</p>
                    </div>
                </div>
                <button
                    onClick={onClose}
                    className="text-gray-500 hover:text-gray-300 transition-colors"
                >
                    <X className="w-4 h-4" />
                </button>
            </div>

            {/* Status badge */}
            <div className="px-4 py-3 border-b border-[#272727]">
                <div className="flex items-center gap-2">
                    <span className={`inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-[11px] font-medium ${isActive
                        ? "bg-emerald-500/10 text-emerald-400"
                        : "bg-gray-800 text-gray-500"
                        }`}>
                        {isActive && (
                            <span className="w-1.5 h-1.5 rounded-full bg-emerald-400 animate-pulse" />
                        )}
                        {isActive ? "Active" : "Ended"}
                    </span>
                    <span className="text-[11px] text-gray-600 flex items-center gap-1">
                        <Clock className="w-3 h-3" />
                        {durationStr}
                    </span>
                </div>
            </div>

            {/* Details */}
            <div className="px-4 py-3 space-y-3 flex-1 overflow-y-auto">
                <div>
                    <label className="text-[10px] uppercase tracking-wider text-gray-500 mb-1 block">
                        Session Type
                    </label>
                    <div className="flex items-center gap-2 text-xs text-white">
                        <TypeIcon className="w-3.5 h-3.5 text-gray-400" />
                        {typeLabel}
                    </div>
                </div>

                <div>
                    <label className="text-[10px] uppercase tracking-wider text-gray-500 mb-1 block">
                        Started
                    </label>
                    <div className="flex items-center gap-2 text-xs text-white">
                        <CalendarDays className="w-3.5 h-3.5 text-gray-400" />
                        {startDate.toLocaleString()}
                    </div>
                </div>

                {endDate && (
                    <div>
                        <label className="text-[10px] uppercase tracking-wider text-gray-500 mb-1 block">
                            Ended
                        </label>
                        <div className="flex items-center gap-2 text-xs text-white">
                            <CalendarDays className="w-3.5 h-3.5 text-gray-400" />
                            {endDate.toLocaleString()}
                        </div>
                    </div>
                )}

                <div>
                    <label className="text-[10px] uppercase tracking-wider text-gray-500 mb-1 block">
                        Session ID
                    </label>
                    <div className="text-xs text-gray-400 font-mono break-all">
                        {session.id}
                    </div>
                </div>

                <div>
                    <label className="text-[10px] uppercase tracking-wider text-gray-500 mb-1 block">
                        Agent ID
                    </label>
                    <div className="text-xs text-gray-400 font-mono break-all">
                        {session.agent_id}
                    </div>
                </div>
            </div>

            {/* Actions */}
            <div className="px-4 py-3 border-t border-[#333] space-y-2">
                {isActive && (
                    <button
                        onClick={onJoin}
                        className="w-full flex items-center justify-center gap-2 py-2 bg-[#e05246] hover:bg-[#c43d32] text-white text-xs font-medium rounded-lg transition-colors"
                    >
                        <ExternalLink className="w-3.5 h-3.5" />
                        Reconnect
                    </button>
                )}
                <button
                    onClick={() => router.push(`/agents?select=${session.agent_id}`)}
                    className="w-full flex items-center justify-center gap-2 py-2 bg-[#1e1e1e] border border-[#333] hover:bg-[#252525] text-gray-300 text-xs font-medium rounded-lg transition-colors"
                >
                    <Info className="w-3.5 h-3.5" />
                    View Agent
                </button>
            </div>
        </div>
    );
}

// ─── Filter Tabs ─────────────────────────────────────────────

const FILTER_TABS: { key: SessionFilter; label: string; icon: typeof Monitor }[] = [
    { key: "all", label: "All", icon: Users },
    { key: "desktop", label: "Desktop", icon: Monitor },
    { key: "terminal", label: "Terminal", icon: Terminal },
    { key: "file_transfer", label: "Files", icon: FileText },
];

// ─── Main Page ───────────────────────────────────────────────

export default function AdHocPage() {
    const router = useRouter();
    const [sessions, setSessions] = useState<Session[]>([]);
    const [agents, setAgents] = useState<Agent[]>([]);
    const [loading, setLoading] = useState(true);
    const [endingId, setEndingId] = useState<string | null>(null);
    const [search, setSearch] = useState("");
    const [selectedId, setSelectedId] = useState<string | null>(null);
    const [typeFilter, setTypeFilter] = useState<SessionFilter>("all");

    const fetchData = useCallback(async () => {
        try {
            const token = getAccessToken();
            if (token) api.setToken(token);
            const [sessionData, agentData] = await Promise.all([
                api.getSessions(),
                api.getAgents(),
            ]);
            setSessions(sessionData);
            setAgents(agentData);
        } catch (e) {
            console.error("Failed to load data:", e);
        } finally {
            setLoading(false);
        }
    }, []);

    useEffect(() => {
        fetchData();
        const interval = setInterval(fetchData, POLL_INTERVAL);
        return () => clearInterval(interval);
    }, [fetchData]);

    const handleEnd = useCallback(async (id: string) => {
        try {
            setEndingId(id);
            const token = getAccessToken();
            if (token) api.setToken(token);
            await api.endSession(id);
            await fetchData();
        } catch (e) {
            console.error("Failed to end session:", e);
        } finally {
            setEndingId(null);
        }
    }, [fetchData]);

    const handleJoin = useCallback((session: Session) => {
        if (session.session_type === "desktop") launchDesktopSession(session.id);
        else if (session.session_type === "terminal") launchTerminalSession(session.id);
        else if (session.session_type === "file_transfer") router.push(`/files?session=${session.id}`);
    }, [router]);

    const { info } = useToast();

    const getAgentName = useCallback((agentId: string) => {
        return agents.find((a) => a.id === agentId)?.machine_name || agentId.slice(0, 8);
    }, [agents]);

    // Type counts for filter badges
    const typeCounts = useMemo(() => {
        const counts = { all: sessions.length, desktop: 0, terminal: 0, file_transfer: 0 };
        sessions.forEach((s) => {
            if (s.session_type === "desktop") counts.desktop++;
            else if (s.session_type === "terminal") counts.terminal++;
            else if (s.session_type === "file_transfer") counts.file_transfer++;
        });
        return counts;
    }, [sessions]);

    // Filtered sessions
    const filteredSessions = useMemo(() => {
        let list = sessions;
        if (typeFilter !== "all") {
            list = list.filter((s) => s.session_type === typeFilter);
        }
        if (search) {
            const q = search.toLowerCase();
            list = list.filter((s) => {
                const name = getAgentName(s.agent_id);
                return name.toLowerCase().includes(q);
            });
        }
        return list;
    }, [sessions, typeFilter, search, getAgentName]);

    const activeSessions = filteredSessions.filter((s) => s.status === "active" || s.status === "pending");
    const recentSessions = filteredSessions
        .filter((s) => s.status === "ended")
        .slice(0, 20);

    const selectedSession = sessions.find((s) => s.id === selectedId) || null;

    return (
        <div className="flex h-full bg-[#141414]">
            {/* Left panel — Ad-hoc code */}
            <div className="w-[280px] border-r border-[#333] flex flex-col">
                <div className="px-4 py-3 border-b border-[#333]">
                    <div className="flex items-center gap-2">
                        <Headset className="w-5 h-5 text-[#e05246]" />
                        <div>
                            <h2 className="text-base font-bold text-white">Ad-Hoc</h2>
                            <p className="text-[10px] text-gray-500">Ad-hoc remote sessions</p>
                        </div>
                    </div>
                </div>

                <div className="p-3 flex-1 overflow-y-auto space-y-3">
                    <AdHocCodeCard />

                    {/* Quick stats */}
                    <div className="grid grid-cols-2 gap-2">
                        <div className="bg-[#1e1e1e] border border-[#333] rounded-lg p-3 text-center">
                            <div className="text-lg font-bold text-white">{activeSessions.length}</div>
                            <div className="text-[10px] text-gray-500 uppercase tracking-wider">Active</div>
                        </div>
                        <div className="bg-[#1e1e1e] border border-[#333] rounded-lg p-3 text-center">
                            <div className="text-lg font-bold text-white">{sessions.length}</div>
                            <div className="text-[10px] text-gray-500 uppercase tracking-wider">Total</div>
                        </div>
                    </div>
                </div>
            </div>

            {/* Center panel — Sessions list */}
            <div className="flex-1 flex flex-col min-w-0">
                {/* Toolbar with tabs */}
                <div className="border-b border-[#333]">
                    <div className="flex items-center gap-3 px-4 py-2.5">
                        <h3 className="text-sm font-semibold text-white">Ad-Hoc Sessions</h3>
                        <div className="flex-1" />
                        <div className="relative">
                            <Search className="absolute left-2.5 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-gray-600" />
                            <input
                                type="text"
                                placeholder="Search sessions..."
                                value={search}
                                onChange={(e) => setSearch(e.target.value)}
                                className="pl-8 pr-3 py-1.5 bg-[#1e1e1e] border border-[#333] rounded text-xs text-gray-300 placeholder-gray-600 focus:outline-none focus:border-[#e05246] w-48"
                            />
                        </div>
                    </div>

                    {/* Type filter tabs */}
                    <div className="flex items-center gap-0.5 px-4 pb-0">
                        {FILTER_TABS.map((tab) => {
                            const TabIcon = tab.icon;
                            const active = typeFilter === tab.key;
                            const count = typeCounts[tab.key];
                            return (
                                <button
                                    key={tab.key}
                                    onClick={() => setTypeFilter(tab.key)}
                                    className={`flex items-center gap-1.5 px-3 py-2 text-xs font-medium rounded-t transition-colors relative ${active
                                        ? "text-white bg-[#1e1e1e]"
                                        : "text-gray-500 hover:text-gray-300 hover:bg-white/[0.02]"
                                        }`}
                                >
                                    <TabIcon className="w-3.5 h-3.5" />
                                    {tab.label}
                                    <span className={`text-[10px] px-1.5 py-0.5 rounded-full ${active
                                        ? "bg-[#e05246]/20 text-[#e05246]"
                                        : "bg-[#333] text-gray-500"
                                        }`}>
                                        {count}
                                    </span>
                                    {active && (
                                        <span className="absolute bottom-0 left-0 right-0 h-[2px] bg-[#e05246]" />
                                    )}
                                </button>
                            );
                        })}
                    </div>
                </div>

                {/* Sessions list */}
                <div className="flex-1 overflow-y-auto">
                    {loading ? (
                        <div className="flex items-center justify-center h-48">
                            <Loader2 className="w-6 h-6 animate-spin text-gray-600" />
                        </div>
                    ) : activeSessions.length === 0 && recentSessions.length === 0 ? (
                        <EmptyState
                            icon={Headset}
                            title="No ad-hoc sessions"
                            description="Generate an ad-hoc code on the left panel to start a remote session with a client."
                            color="#e05246"
                        />
                    ) : (
                        <>
                            {activeSessions.length > 0 && (
                                <div>
                                    <div className="px-4 py-2 text-[10px] uppercase tracking-wider text-gray-500 bg-[#1a1a1a] border-b border-[#272727]">
                                        Active ({activeSessions.length})
                                    </div>
                                    {activeSessions.map((s) => (
                                        <SessionRow
                                            key={s.id}
                                            session={s}
                                            agentName={getAgentName(s.agent_id)}
                                            selected={selectedId === s.id}
                                            onSelect={() => setSelectedId(s.id)}
                                            onJoin={() => handleJoin(s)}
                                            onEnd={() => handleEnd(s.id)}
                                            isEnding={endingId === s.id}
                                        />
                                    ))}
                                </div>
                            )}

                            {recentSessions.length > 0 && (
                                <div>
                                    <div className="px-4 py-2 text-[10px] uppercase tracking-wider text-gray-500 bg-[#1a1a1a] border-b border-[#272727]">
                                        Recent
                                    </div>
                                    {recentSessions.map((s) => (
                                        <SessionRow
                                            key={s.id}
                                            session={s}
                                            agentName={getAgentName(s.agent_id)}
                                            selected={selectedId === s.id}
                                            onSelect={() => setSelectedId(s.id)}
                                            onJoin={() => handleJoin(s)}
                                            onEnd={() => handleEnd(s.id)}
                                            isEnding={endingId === s.id}
                                        />
                                    ))}
                                </div>
                            )}
                        </>
                    )}
                </div>

                {/* Footer */}
                <div className="px-4 py-2 border-t border-[#333] text-[11px] text-gray-600 flex items-center justify-between">
                    <span>{filteredSessions.length} total sessions</span>
                    <button
                        onClick={() => info("All Sessions", "Showing all sessions in the current view")}
                        className="text-[#e05246] hover:text-[#f06b60] flex items-center gap-1"
                    >
                        <ExternalLink className="w-3 h-3" />
                        View All
                    </button>
                </div>
            </div>

            {/* Right panel — Session detail */}
            {selectedSession && (
                <SessionDetail
                    session={selectedSession}
                    agentName={getAgentName(selectedSession.agent_id)}
                    onJoin={() => handleJoin(selectedSession)}
                    onClose={() => setSelectedId(null)}
                />
            )}
        </div>
    );
}
