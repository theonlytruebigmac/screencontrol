"use client";

import { useMemo, useState, useRef, useCallback, useEffect } from "react";
import {
    Monitor,
    Terminal,
    FolderOpen,
    MessageSquare,
    Search,
    ScreenShare,
    Pencil,
    MoreHorizontal,
    SlidersHorizontal,
    XCircle,
    Power,
    Code2,
    Cpu,
    MemoryStick,
    Check,
} from "lucide-react";
import type { Agent } from "@/lib/api";
import { useRouter } from "next/navigation";
import { api } from "@/lib/api";
import { launchDesktopSession, launchTerminalSession } from "@/lib/session-launcher";
import { useToast } from "@/components/toast";

interface MachineListProps {
    agents: Agent[];
    selectedId: string | null;
    onSelect: (agent: Agent) => void;
    onJoin: (agent: Agent) => void;
    onEnd: (agent: Agent) => void;
    filter: string;
    search: string;
    onSearchChange: (value: string) => void;
}

function formatIdleTime(lastSeen: string | null): string {
    if (!lastSeen) return "Never";
    const diff = Date.now() - new Date(lastSeen).getTime();
    const secs = Math.floor(diff / 1000);
    if (secs < 60) return "Online";
    const mins = Math.floor(secs / 60);
    if (mins < 60) return `Idle ${mins}m`;
    const hrs = Math.floor(mins / 60);
    if (hrs < 24) return `Idle ${hrs}h ${mins % 60}m`;
    const days = Math.floor(hrs / 24);
    return `Idle ${days}d ${hrs % 24}h`;
}

function getStatusBarWidth(agent: Agent): number {
    if (agent.status === "busy") return 100;
    if (agent.status === "online") {
        // Use CPU usage if available, fall back to time-based
        if (agent.cpu_usage != null) return Math.max(5, Math.round(agent.cpu_usage));
        if (!agent.last_seen) return 50;
        const diff = Date.now() - new Date(agent.last_seen).getTime();
        const mins = Math.floor(diff / 60000);
        return Math.max(10, 100 - Math.min(mins, 90));
    }
    return 0;
}

function getStatusBarClass(agent: Agent, idleText: string): string {
    if (agent.status === "busy") return "status-bar-fill-busy";
    if (agent.status !== "online") return "";
    // CPU-based coloring when metrics available
    if (agent.cpu_usage != null) {
        if (agent.cpu_usage > 80) return "status-bar-fill-busy"; // red
        if (agent.cpu_usage > 50) return "status-bar-fill-idle"; // amber
        return "status-bar-fill-online"; // green
    }
    return idleText === "Online" ? "status-bar-fill-online" : "status-bar-fill-idle";
}

function formatMetricLine(agent: Agent): string {
    if (agent.status === "offline") return "Offline";
    const parts: string[] = [];
    if (agent.cpu_usage != null) parts.push(`CPU ${agent.cpu_usage.toFixed(0)}%`);
    if (agent.memory_used != null && agent.memory_total != null && agent.memory_total > 0) {
        parts.push(`Mem ${((agent.memory_used / agent.memory_total) * 100).toFixed(0)}%`);
    }
    if (parts.length > 0) return parts.join(" · ");
    return formatIdleTime(agent.last_seen);
}

export function MachineList({
    agents,
    selectedId,
    onSelect,
    onJoin,
    onEnd,
    filter,
    search,
    onSearchChange,
}: MachineListProps) {
    const router = useRouter();
    const { info } = useToast();
    const [showMoreMenu, setShowMoreMenu] = useState(false);
    const [checkedIds, setCheckedIds] = useState<Set<string>>(new Set());
    const moreRef = useRef<HTMLDivElement>(null);

    // Context menu state
    const [ctxMenu, setCtxMenu] = useState<{ x: number; y: number; agent: Agent } | null>(null);
    const ctxRef = useRef<HTMLDivElement>(null);

    // Advanced filter state
    const [showAdvFilters, setShowAdvFilters] = useState(false);
    const [advFilterOS, setAdvFilterOS] = useState<Set<string>>(new Set());
    const [advFilterVer, setAdvFilterVer] = useState<Set<string>>(new Set());
    const advFilterRef = useRef<HTMLDivElement>(null);

    // Dynamically detect unique OS / version values from agents
    const uniqueOS = useMemo(() => Array.from(new Set(agents.map(a => a.os).filter(Boolean))).sort(), [agents]);
    const uniqueVer = useMemo(() => Array.from(new Set(agents.map(a => a.agent_version).filter(Boolean))).sort(), [agents]);
    const advFilterCount = advFilterOS.size + advFilterVer.size;

    // Close context menu on click outside or ESC
    useEffect(() => {
        if (!ctxMenu) return;
        const handleClick = (e: MouseEvent) => {
            if (ctxRef.current && !ctxRef.current.contains(e.target as Node)) setCtxMenu(null);
        };
        const handleKey = (e: KeyboardEvent) => { if (e.key === "Escape") setCtxMenu(null); };
        document.addEventListener("mousedown", handleClick);
        document.addEventListener("keydown", handleKey);
        return () => { document.removeEventListener("mousedown", handleClick); document.removeEventListener("keydown", handleKey); };
    }, [ctxMenu]);

    // Close advanced filters on click outside
    useEffect(() => {
        if (!showAdvFilters) return;
        const handleClick = (e: MouseEvent) => {
            if (advFilterRef.current && !advFilterRef.current.contains(e.target as Node)) setShowAdvFilters(false);
        };
        document.addEventListener("mousedown", handleClick);
        return () => document.removeEventListener("mousedown", handleClick);
    }, [showAdvFilters]);

    const handleContextMenu = useCallback((e: React.MouseEvent, agent: Agent) => {
        e.preventDefault();
        setCtxMenu({ x: e.clientX, y: e.clientY, agent });
    }, []);

    const handleCtxAction = useCallback(async (action: string) => {
        if (!ctxMenu) return;
        const agent = ctxMenu.agent;
        setCtxMenu(null);
        try {
            const token = localStorage.getItem("sc_access_token");
            if (token) api.setToken(token);
            if (action === "desktop" || action === "terminal" || action === "file_transfer") {
                const session = await api.createSession(agent.id, action);
                if (action === "desktop") launchDesktopSession(session.id);
                else if (action === "terminal") launchTerminalSession(session.id);
                else router.push(`/files?session=${session.id}`);
            } else if (action === "end") {
                onEnd(agent);
            }
        } catch (err) {
            console.error("Context action failed:", err);
        }
    }, [ctxMenu, router, onEnd]);

    const toggleCheck = useCallback((id: string, e: React.MouseEvent) => {
        e.stopPropagation();
        setCheckedIds((prev) => {
            const next = new Set(prev);
            if (next.has(id)) next.delete(id);
            else next.add(id);
            return next;
        });
    }, []);

    const toggleAll = useCallback(() => {
        setCheckedIds((prev) => {
            if (prev.size === agents.length) return new Set();
            return new Set(agents.map((a) => a.id));
        });
    }, [agents]);

    const filtered = useMemo(() => {
        let list = agents;

        // Apply group filter
        if (filter === "online") list = list.filter((a) => a.status === "online");
        else if (filter === "offline") list = list.filter((a) => a.status === "offline");
        else if (filter === "active") list = list.filter((a) => a.status === "busy");
        else if (filter === "authorized" || filter === "guest-connected")
            list = list.filter((a) => a.status === "online" || a.status === "busy");
        else if (filter.startsWith("os:")) {
            const os = filter.slice(3);
            list = list.filter((a) => a.os === os);
        } else if (filter.startsWith("ver:")) {
            const ver = filter.slice(4);
            list = list.filter((a) => a.agent_version === ver);
        } else if (filter.startsWith("custom:")) {
            try {
                const stored = localStorage.getItem("sc_custom_groups");
                if (stored) {
                    const groups = JSON.parse(stored) as { id: string; filter: string }[];
                    const match = groups.find((g) => g.id === filter);
                    if (match?.filter) {
                        const q = match.filter.toLowerCase();
                        list = list.filter((a) => a.machine_name.toLowerCase().includes(q));
                    }
                }
            } catch { /* ignore */ }
        }

        // Apply search
        if (search) {
            const q = search.toLowerCase();
            list = list.filter((a) => a.machine_name.toLowerCase().includes(q));
        }

        // Apply advanced filters
        if (advFilterOS.size > 0) {
            list = list.filter((a) => a.os && advFilterOS.has(a.os));
        }
        if (advFilterVer.size > 0) {
            list = list.filter((a) => a.agent_version && advFilterVer.has(a.agent_version));
        }

        return list;
    }, [agents, filter, search, advFilterOS, advFilterVer]);

    const selectedAgent = agents.find((a) => a.id === selectedId);

    return (
        <div className="flex flex-col h-full flex-1 min-w-0">
            {/* Toolbar */}
            <div className="flex items-center gap-1 px-3 py-2 border-b border-[#333] bg-[#1e1e1e]">
                <span className="text-sm font-semibold text-white mr-3">
                    All Machines
                </span>

                {/* Join */}
                <button
                    onClick={() => selectedAgent && onJoin(selectedAgent)}
                    disabled={!selectedId}
                    className="flex items-center gap-1 px-2.5 py-1.5 text-xs font-medium rounded transition-colors disabled:opacity-30 disabled:cursor-not-allowed text-emerald-400 hover:bg-emerald-500/10"
                >
                    <ScreenShare className="w-3.5 h-3.5" />
                    Join
                </button>

                {/* Edit */}
                <button
                    onClick={() => { if (selectedAgent) onSelect(selectedAgent); }}
                    disabled={!selectedId}
                    className="flex items-center gap-1 px-2.5 py-1.5 text-xs font-medium rounded transition-colors disabled:opacity-30 disabled:cursor-not-allowed text-blue-400 hover:bg-blue-500/10"
                >
                    <Pencil className="w-3.5 h-3.5" />
                    Edit
                </button>

                {/* End */}
                <button
                    onClick={() => selectedAgent && onEnd(selectedAgent)}
                    disabled={!selectedId}
                    className="flex items-center gap-1 px-2.5 py-1.5 text-xs font-medium rounded transition-colors disabled:opacity-30 disabled:cursor-not-allowed text-red-400 hover:bg-red-500/10"
                >
                    <XCircle className="w-3.5 h-3.5" />
                    End
                </button>

                {/* More dropdown */}
                <div className="relative" ref={moreRef}>
                    <button
                        onClick={() => setShowMoreMenu(!showMoreMenu)}
                        className="flex items-center gap-1 px-2.5 py-1.5 text-xs font-medium rounded transition-colors text-gray-400 hover:text-gray-200 hover:bg-white/5"
                    >
                        <MoreHorizontal className="w-3.5 h-3.5" />
                        More
                    </button>
                    {showMoreMenu && (
                        <div className="absolute top-full left-0 mt-1 w-44 bg-[#252525] border border-[#444] rounded-lg shadow-xl z-50 py-1">
                            <button
                                className="w-full text-left px-3 py-2 text-xs text-gray-300 hover:bg-white/5 transition-colors"
                                onClick={() => setShowMoreMenu(false)}
                            >
                                Reinstall / Reconnect
                            </button>
                            <button
                                className="w-full text-left px-3 py-2 text-xs text-gray-300 hover:bg-white/5 transition-colors"
                                onClick={() => { if (selectedAgent) onSelect(selectedAgent); setShowMoreMenu(false); }}
                            >
                                Run Command
                            </button>
                            <button
                                className="w-full text-left px-3 py-2 text-xs text-gray-300 hover:bg-white/5 transition-colors"
                                onClick={() => { if (selectedAgent) onSelect(selectedAgent); setShowMoreMenu(false); }}
                            >
                                Send Message
                            </button>
                            <div className="border-t border-[#444] my-1" />
                            <button
                                className="w-full text-left px-3 py-2 text-xs text-red-400 hover:bg-red-500/10 transition-colors"
                                onClick={() => {
                                    if (selectedAgent && confirm(`End all sessions for ${selectedAgent.machine_name}?`)) {
                                        onEnd(selectedAgent);
                                    }
                                    setShowMoreMenu(false);
                                }}
                            >
                                Remove Session
                            </button>
                        </div>
                    )}
                </div>

                <div className="flex-1" />

                {/* Search */}
                <div className="flex items-center gap-2 px-3 py-2 border-b border-[#333] bg-[#141414]">
                    <div className="relative flex-1">
                        <Search className="absolute left-2.5 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-gray-600" />
                        <input
                            type="text"
                            placeholder="Search machines..."
                            value={search}
                            onChange={(e) => onSearchChange(e.target.value)}
                            className="w-full bg-[#1a1a1a] border border-[#333] rounded pl-8 pr-3 py-1.5 text-xs text-gray-300 placeholder-gray-600 focus:outline-none focus:border-[#e05246] transition-colors"
                        />
                    </div>
                    <div className="relative" ref={advFilterRef}>
                        <button
                            onClick={() => setShowAdvFilters(!showAdvFilters)}
                            className={`p-1.5 rounded transition-colors ${advFilterCount > 0 ? 'text-[#e05246] bg-[#e05246]/10' : 'text-gray-500 hover:text-gray-300 hover:bg-white/5'}`}
                            title="Advanced Filters"
                        >
                            <SlidersHorizontal className="w-3.5 h-3.5" />
                            {advFilterCount > 0 && (
                                <span className="absolute -top-0.5 -right-0.5 w-3.5 h-3.5 rounded-full bg-[#e05246] text-[8px] text-white flex items-center justify-center font-bold">
                                    {advFilterCount}
                                </span>
                            )}
                        </button>
                        {showAdvFilters && (
                            <div className="absolute top-full right-0 mt-1 w-56 bg-[#252525] border border-[#444] rounded-lg shadow-xl z-50 py-2 slide-up">
                                <div className="px-3 pb-1.5 text-[10px] text-gray-500 uppercase tracking-wider">Operating System</div>
                                {uniqueOS.map(os => {
                                    const checked = advFilterOS.has(os);
                                    return (
                                        <button
                                            key={os}
                                            onClick={() => setAdvFilterOS(prev => {
                                                const next = new Set(prev);
                                                if (next.has(os)) next.delete(os); else next.add(os);
                                                return next;
                                            })}
                                            className="w-full flex items-center gap-2 px-3 py-1.5 text-xs text-gray-300 hover:bg-white/5 transition-colors"
                                        >
                                            <div className={`w-3.5 h-3.5 rounded border flex items-center justify-center ${checked ? 'bg-[#e05246] border-[#e05246]' : 'border-gray-600'}`}>
                                                {checked && <Check className="w-2.5 h-2.5 text-white" />}
                                            </div>
                                            {os === 'windows' ? 'Windows' : os === 'macos' ? 'macOS' : os === 'linux' ? 'Linux' : os}
                                        </button>
                                    );
                                })}
                                {uniqueOS.length === 0 && <div className="px-3 py-1 text-[10px] text-gray-600">No OS data</div>}

                                <div className="border-t border-[#444] my-1.5" />
                                <div className="px-3 pb-1.5 text-[10px] text-gray-500 uppercase tracking-wider">Agent Version</div>
                                {uniqueVer.map(ver => {
                                    const checked = advFilterVer.has(ver);
                                    return (
                                        <button
                                            key={ver}
                                            onClick={() => setAdvFilterVer(prev => {
                                                const next = new Set(prev);
                                                if (next.has(ver)) next.delete(ver); else next.add(ver);
                                                return next;
                                            })}
                                            className="w-full flex items-center gap-2 px-3 py-1.5 text-xs text-gray-300 hover:bg-white/5 transition-colors"
                                        >
                                            <div className={`w-3.5 h-3.5 rounded border flex items-center justify-center ${checked ? 'bg-[#e05246] border-[#e05246]' : 'border-gray-600'}`}>
                                                {checked && <Check className="w-2.5 h-2.5 text-white" />}
                                            </div>
                                            v{ver}
                                        </button>
                                    );
                                })}
                                {uniqueVer.length === 0 && <div className="px-3 py-1 text-[10px] text-gray-600">No version data</div>}

                                {advFilterCount > 0 && (
                                    <>
                                        <div className="border-t border-[#444] my-1.5" />
                                        <button
                                            onClick={() => { setAdvFilterOS(new Set()); setAdvFilterVer(new Set()); }}
                                            className="w-full flex items-center gap-2 px-3 py-1.5 text-xs text-[#e05246] hover:bg-[#e05246]/10 transition-colors"
                                        >
                                            <XCircle className="w-3 h-3" /> Clear All Filters
                                        </button>
                                    </>
                                )}
                            </div>
                        )}
                    </div>
                </div>

                {/* Bulk actions bar */}
                {checkedIds.size >= 2 && (
                    <div className="flex items-center gap-1.5 px-3 py-2 border-b border-[#e05246]/30 bg-[#e05246]/5 slide-up">
                        <span className="text-xs font-medium text-[#e05246]">{checkedIds.size} selected</span>
                        <div className="w-px h-4 bg-[#e05246]/20 mx-1" />
                        <button
                            onClick={() => {
                                const checkedAgents = agents.filter((a) => checkedIds.has(a.id) && a.status === "online");
                                checkedAgents.forEach((a) => onJoin(a));
                                setCheckedIds(new Set());
                            }}
                            className="flex items-center gap-1 px-2 py-1 text-[11px] font-medium text-emerald-400 bg-emerald-500/10 hover:bg-emerald-500/20 rounded transition-colors"
                        >
                            <ScreenShare className="w-3 h-3" /> Connect All
                        </button>
                        <button
                            onClick={() => {
                                info("Bulk Command", `Send command to ${checkedIds.size} agents — coming soon`);
                            }}
                            className="flex items-center gap-1 px-2 py-1 text-[11px] font-medium text-blue-400 bg-blue-500/10 hover:bg-blue-500/20 rounded transition-colors"
                        >
                            <Terminal className="w-3 h-3" /> Run Command
                        </button>
                        <button
                            onClick={() => {
                                info("Bulk Message", `Message ${checkedIds.size} agents — coming soon`);
                            }}
                            className="flex items-center gap-1 px-2 py-1 text-[11px] font-medium text-cyan-400 bg-cyan-500/10 hover:bg-cyan-500/20 rounded transition-colors"
                        >
                            <MessageSquare className="w-3 h-3" /> Message
                        </button>
                        <div className="flex-1" />
                        <button
                            onClick={() => {
                                const checkedAgents = agents.filter((a) => checkedIds.has(a.id));
                                checkedAgents.forEach((a) => onEnd(a));
                                setCheckedIds(new Set());
                            }}
                            className="flex items-center gap-1 px-2 py-1 text-[11px] font-medium text-red-400 bg-red-500/10 hover:bg-red-500/20 rounded transition-colors"
                        >
                            <XCircle className="w-3 h-3" /> End
                        </button>
                        <button
                            onClick={() => setCheckedIds(new Set())}
                            className="px-2 py-1 text-[11px] font-medium text-gray-400 hover:text-gray-200 hover:bg-white/5 rounded transition-colors"
                        >
                            Clear
                        </button>
                    </div>
                )}
            </div>

            {/* Column headers */}
            <div className="grid grid-cols-[auto_1fr_auto_200px] items-center gap-2 px-3 py-1.5 text-[10px] text-gray-600 uppercase tracking-wider border-b border-[#333] bg-[#1a1a1a]">
                <div className="w-5 flex items-center justify-center">
                    <input
                        type="checkbox"
                        checked={checkedIds.size > 0 && checkedIds.size === filtered.length}
                        onChange={toggleAll}
                        className="w-3.5 h-3.5 rounded border-gray-600 accent-[#e05246] cursor-pointer"
                    />
                </div>
                <span>Machine</span>
                <span>Session</span>
                <span>Status</span>
            </div>

            {/* Machine rows */}
            <div className="flex-1 overflow-y-auto">
                {filtered.map((agent) => {
                    const isSelected = selectedId === agent.id;
                    const barWidth = getStatusBarWidth(agent);
                    const idleText = formatIdleTime(agent.last_seen);

                    return (
                        <div
                            key={agent.id}
                            onClick={() => onSelect(agent)}
                            onDoubleClick={() => onJoin(agent)}
                            onContextMenu={(e) => handleContextMenu(e, agent)}
                            className={`machine-row ${isSelected ? "selected" : ""}`}
                        >
                            <div className="grid grid-cols-[auto_1fr_auto_200px] items-center gap-2 px-3 py-2">
                                {/* Checkbox */}
                                <div className="w-5 flex items-center justify-center">
                                    <input
                                        type="checkbox"
                                        checked={checkedIds.has(agent.id)}
                                        onChange={() => { }}
                                        className="w-3.5 h-3.5 rounded border-gray-600 accent-[#e05246] cursor-pointer"
                                        onClick={(e) => toggleCheck(agent.id, e)}
                                    />
                                </div>

                                {/* Machine info */}
                                <div className="min-w-0">
                                    <div className="flex items-center gap-1.5 min-w-0">
                                        <span className="text-[13px] font-medium text-white truncate">{agent.machine_name}</span>
                                        {/* Agent tags */}
                                        {agent.os && (
                                            <span className={`flex-shrink-0 px-1.5 py-0.5 rounded text-[8px] font-semibold uppercase tracking-wider ${agent.os === 'windows' ? 'bg-blue-500/15 text-blue-300' :
                                                agent.os === 'macos' ? 'bg-gray-500/15 text-gray-300' :
                                                    'bg-emerald-500/15 text-emerald-300'
                                                }`}>
                                                {agent.os === 'windows' ? 'WIN' : agent.os === 'macos' ? 'MAC' : 'LNX'}
                                            </span>
                                        )}
                                        {agent.status === 'busy' && (
                                            <span className="flex-shrink-0 px-1.5 py-0.5 rounded text-[8px] font-semibold bg-amber-500/15 text-amber-300">IN SESSION</span>
                                        )}
                                    </div>
                                    <div className="text-[11px] text-gray-500 truncate">
                                        {formatMetricLine(agent)}
                                    </div>
                                </div>

                                {/* Session icons */}
                                <div className="flex items-center gap-1">
                                    <Monitor className="w-3.5 h-3.5 text-gray-600" />
                                    <Terminal className="w-3.5 h-3.5 text-gray-600" />
                                    <FolderOpen className="w-3.5 h-3.5 text-gray-600" />
                                    <MessageSquare className="w-3.5 h-3.5 text-gray-600" />
                                </div>

                                {/* Status bar */}
                                <div className="flex items-center gap-2">
                                    <div className="status-bar flex-1">
                                        {agent.status !== "offline" && (
                                            <div
                                                className={getStatusBarClass(agent, idleText)}
                                                style={{ width: `${barWidth}%` }}
                                            />
                                        )}
                                    </div>
                                    <span className={`text-[10px] font-medium whitespace-nowrap ${agent.status === "online" && idleText === "Online"
                                        ? "text-emerald-400"
                                        : agent.status === "busy"
                                            ? "text-[#f06b60]"
                                            : "text-gray-500"
                                        }`}>
                                        {agent.status === "offline" ? "" : idleText}
                                    </span>
                                </div>
                            </div>
                        </div>
                    );
                })}

                {filtered.length === 0 && (
                    <div className="flex flex-col items-center justify-center py-16 text-gray-600">
                        <Monitor className="w-10 h-10 mb-2 opacity-30" />
                        <p className="text-sm">No machines found</p>
                    </div>
                )}
            </div>

            <div className="px-3 py-1.5 border-t border-[#333] bg-[#1a1a1a] text-[11px] text-gray-600">
                {filtered.filter(a => a.status !== 'offline').length} online / {filtered.length} machine{filtered.length !== 1 ? "s" : ""}
            </div>

            {/* Right-click context menu */}
            {ctxMenu && (
                <div
                    ref={ctxRef}
                    className="fixed bg-[#252525] border border-[#444] rounded-lg shadow-2xl z-[200] py-1 w-44 slide-up"
                    style={{ left: ctxMenu.x, top: ctxMenu.y }}
                >
                    <div className="px-3 py-1.5 text-[10px] text-gray-600 uppercase tracking-wider border-b border-[#333] mb-1">
                        {ctxMenu.agent.machine_name}
                    </div>
                    <button onClick={() => handleCtxAction("desktop")} className="w-full flex items-center gap-2 px-3 py-2 text-xs text-gray-300 hover:bg-white/5 transition-colors">
                        <Monitor className="w-3.5 h-3.5 text-emerald-400" /> Remote Desktop
                    </button>
                    <button onClick={() => handleCtxAction("terminal")} className="w-full flex items-center gap-2 px-3 py-2 text-xs text-gray-300 hover:bg-white/5 transition-colors">
                        <Terminal className="w-3.5 h-3.5 text-cyan-400" /> Terminal
                    </button>
                    <button onClick={() => handleCtxAction("file_transfer")} className="w-full flex items-center gap-2 px-3 py-2 text-xs text-gray-300 hover:bg-white/5 transition-colors">
                        <FolderOpen className="w-3.5 h-3.5 text-amber-400" /> File Transfer
                    </button>
                    <button onClick={() => { setCtxMenu(null); router.push('/toolbox'); }} className="w-full flex items-center gap-2 px-3 py-2 text-xs text-gray-300 hover:bg-white/5 transition-colors">
                        <Code2 className="w-3.5 h-3.5 text-purple-400" /> Run Script
                    </button>
                    {ctxMenu.agent.status === 'offline' && (
                        <button onClick={() => { info('Wake-on-LAN', `Sending WoL packet to ${ctxMenu.agent.machine_name}...`); setCtxMenu(null); }} className="w-full flex items-center gap-2 px-3 py-2 text-xs text-gray-300 hover:bg-white/5 transition-colors">
                            <Power className="w-3.5 h-3.5 text-green-400" /> Wake-on-LAN
                        </button>
                    )}
                    <div className="border-t border-[#444] my-1" />
                    <button onClick={() => handleCtxAction("end")} className="w-full flex items-center gap-2 px-3 py-2 text-xs text-red-400 hover:bg-red-500/10 transition-colors">
                        <XCircle className="w-3.5 h-3.5" /> End Session
                    </button>
                </div>
            )}
        </div>
    );
}
