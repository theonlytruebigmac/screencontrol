"use client";

import { useMemo, useState, useEffect, useCallback } from "react";
import {
    Monitor,
    ChevronRight,
    ChevronDown,
    Plus,
    Apple,
    Laptop,
    X,
    Pencil,
    Trash2,
    FolderPlus,
    Check,
} from "lucide-react";
import type { Agent } from "@/lib/api";
import { useToast } from "@/components/toast";

interface SessionGroup {
    id: string;
    label: string;
    count: number;
    indent?: boolean;
    children?: SessionGroup[];
}

interface CustomGroup {
    id: string;
    label: string;
    filter: string; // text match against machine_name
}

const STORAGE_KEY = "sc_custom_groups";

interface SessionGroupsProps {
    agents: Agent[];
    activeGroup: string;
    onGroupChange: (groupId: string) => void;
    onDeploy: () => void;
}

export function SessionGroups({ agents, activeGroup, onGroupChange, onDeploy }: SessionGroupsProps) {
    const { info, success } = useToast();
    const [expanded, setExpanded] = useState<Record<string, boolean>>({
        "by-os": false,
        "by-version": false,
        "custom": false,
    });
    const [showEditor, setShowEditor] = useState(false);
    const [customGroups, setCustomGroups] = useState<CustomGroup[]>([]);
    const [newLabel, setNewLabel] = useState("");
    const [newFilter, setNewFilter] = useState("");
    const [editingId, setEditingId] = useState<string | null>(null);
    const [editLabel, setEditLabel] = useState("");

    // Load custom groups from localStorage
    useEffect(() => {
        try {
            const stored = localStorage.getItem(STORAGE_KEY);
            if (stored) setCustomGroups(JSON.parse(stored));
        } catch { /* ignore */ }
    }, []);

    const persistGroups = useCallback((groups: CustomGroup[]) => {
        setCustomGroups(groups);
        localStorage.setItem(STORAGE_KEY, JSON.stringify(groups));
    }, []);

    const handleAddGroup = () => {
        if (!newLabel.trim()) return;
        const id = `custom:${Date.now()}`;
        persistGroups([...customGroups, { id, label: newLabel.trim(), filter: newFilter.trim() }]);
        setNewLabel("");
        setNewFilter("");
        success("Group Created", `"${newLabel.trim()}" added`);
    };

    const handleDeleteGroup = (id: string) => {
        persistGroups(customGroups.filter((g) => g.id !== id));
    };

    const handleRenameGroup = (id: string) => {
        if (!editLabel.trim()) return;
        persistGroups(customGroups.map((g) => g.id === id ? { ...g, label: editLabel.trim() } : g));
        setEditingId(null);
        setEditLabel("");
    };

    const groups = useMemo(() => {
        const online = agents.filter((a) => a.status === "online").length;
        const offline = agents.filter((a) => a.status === "offline").length;
        const busy = agents.filter((a) => a.status === "busy").length;
        const connected = agents.filter((a) => a.status === "online" || a.status === "busy").length;

        // Count by OS
        const osCounts: Record<string, number> = {};
        agents.forEach((a) => {
            const os = a.os || "Unknown";
            osCounts[os] = (osCounts[os] || 0) + 1;
        });

        // Count by version
        const versionCounts: Record<string, number> = {};
        agents.forEach((a) => {
            const ver = a.agent_version || "unknown";
            versionCounts[ver] = (versionCounts[ver] || 0) + 1;
        });

        // Custom group counts
        const customGroupItems: SessionGroup[] = customGroups.map((cg) => ({
            id: cg.id,
            label: cg.label,
            count: cg.filter
                ? agents.filter((a) => a.machine_name.toLowerCase().includes(cg.filter.toLowerCase())).length
                : agents.length,
        }));

        return {
            top: [
                { id: "all", label: "All Machines", count: agents.length },
                {
                    id: "by-os", label: "All Machines by OS", count: agents.length,
                    children: Object.entries(osCounts).map(([os, count]) => ({
                        id: `os:${os}`, label: os, count,
                    })),
                },
                {
                    id: "by-version", label: "All Machines by Client Vers...", count: agents.length,
                    children: Object.entries(versionCounts).map(([ver, count]) => ({
                        id: `ver:${ver}`, label: `v${ver}`, count,
                    })),
                },
            ] as SessionGroup[],
            status: [
                { id: "authorized", label: "Authorized Sessions", count: connected },
                { id: "online", label: "Online", count: online },
                { id: "active", label: "Active", count: busy },
                { id: "guest-connected", label: "Guest Connected", count: connected },
                { id: "unacknowledged", label: "Unacknowledged", count: 0 },
                { id: "offline", label: "Offline", count: offline },
            ] as SessionGroup[],
            custom: customGroupItems,
        };
    }, [agents, customGroups]);

    const toggleExpand = (id: string) => {
        setExpanded((prev) => ({ ...prev, [id]: !prev[id] }));
    };

    const renderGroup = (group: SessionGroup, depth = 0) => {
        const hasChildren = group.children && group.children.length > 0;
        const isExpanded = expanded[group.id];
        const isActive = activeGroup === group.id;
        const ChevronIcon = isExpanded ? ChevronDown : ChevronRight;

        return (
            <div key={group.id}>
                <button
                    onClick={() => {
                        if (hasChildren) toggleExpand(group.id);
                        onGroupChange(group.id);
                    }}
                    className={`w-full flex items-center justify-between px-3 py-1.5 text-left transition-colors ${isActive
                        ? "bg-[#e05246]/10 border-l-[3px] border-[#e05246]"
                        : "border-l-[3px] border-transparent hover:bg-white/3"
                        }`}
                    style={{ paddingLeft: `${12 + depth * 16}px` }}
                >
                    <span className="flex items-center gap-1 min-w-0 truncate">
                        {(hasChildren || depth === 0) && (
                            <ChevronIcon className="w-3 h-3 text-gray-500 flex-shrink-0" />
                        )}
                        {depth > 0 && !hasChildren && group.id.startsWith("os:") && (
                            <span className="flex-shrink-0 text-gray-500">
                                {group.label.toLowerCase().includes("windows") ? <Monitor className="w-3 h-3" /> :
                                    group.label.toLowerCase().includes("macos") || group.label.toLowerCase().includes("darwin") ? <Apple className="w-3 h-3" /> :
                                        <Laptop className="w-3 h-3" />}
                            </span>
                        )}
                        <span className={`text-[13px] truncate ${isActive ? "text-white font-medium" : "text-gray-400"}`}>
                            {group.label}
                        </span>
                    </span>
                    <span className={`ml-2 text-[11px] font-semibold px-1.5 py-0.5 rounded-sm flex-shrink-0 ${isActive
                        ? "bg-[#e05246] text-white"
                        : "bg-[#333] text-gray-500"
                        }`}>
                        {group.count}
                    </span>
                </button>
                {hasChildren && isExpanded && group.children!.map((child) => renderGroup(child, depth + 1))}
            </div>
        );
    };

    return (
        <div className="flex flex-col h-full bg-[#1e1e1e] panel-border-r" style={{ width: "var(--groups-width)" }}>
            {/* Header */}
            <div className="px-4 pt-5 pb-3 border-b border-[#333]">
                <div className="flex items-center gap-2 mb-1">
                    <Monitor className="w-5 h-5 text-[#e05246]" />
                    <h2 className="text-base font-bold text-white">Access</h2>
                </div>
                <p className="text-[11px] text-gray-500 leading-snug">
                    Use access sessions to provide indefinite access to familiar computers.
                </p>
            </div>

            {/* Build button */}
            <div className="px-3 py-3 border-b border-[#333]">
                <button
                    onClick={onDeploy}
                    className="w-full flex items-center justify-center gap-2 py-2 bg-[#e05246] hover:bg-[#c43d32] text-white text-sm font-medium rounded transition-colors"
                >
                    <Plus className="w-4 h-4" />
                    Build +
                </button>
            </div>

            {/* Groups */}
            <div className="flex-1 overflow-y-auto py-1">
                {groups.top.map((g) => renderGroup(g))}

                <div className="border-t border-[#333] my-1" />

                {groups.status.map((g) => renderGroup(g))}

                {/* Custom groups section */}
                {groups.custom.length > 0 && (
                    <>
                        <div className="border-t border-[#333] my-1" />
                        {groups.custom.map((g) => renderGroup(g))}
                    </>
                )}
            </div>

            {/* Footer */}
            <div className="px-3 py-2 border-t border-[#333] text-center">
                <button
                    onClick={() => setShowEditor(true)}
                    className="text-[11px] text-[#e05246] hover:text-[#f06b60] transition-colors"
                >
                    Manage Session Groups
                </button>
            </div>

            {/* ── Session Group Editor Modal ── */}
            {showEditor && (
                <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm" onClick={() => setShowEditor(false)}>
                    <div className="bg-[#1e1e1e] border border-[#333] rounded-xl w-[420px] max-h-[70vh] flex flex-col shadow-2xl" onClick={(e) => e.stopPropagation()}>
                        {/* Header */}
                        <div className="flex items-center justify-between px-5 py-4 border-b border-[#333]">
                            <h3 className="text-sm font-bold text-white">Manage Session Groups</h3>
                            <button onClick={() => setShowEditor(false)} className="text-gray-500 hover:text-white transition-colors">
                                <X className="w-4 h-4" />
                            </button>
                        </div>

                        {/* Create new */}
                        <div className="px-5 py-3 border-b border-[#333] space-y-2">
                            <div className="flex gap-2">
                                <input
                                    type="text"
                                    placeholder="Group name..."
                                    value={newLabel}
                                    onChange={(e) => setNewLabel(e.target.value)}
                                    onKeyDown={(e) => e.key === "Enter" && handleAddGroup()}
                                    className="flex-1 px-3 py-1.5 bg-[#141414] border border-[#333] rounded-lg text-xs text-gray-300 placeholder-gray-600 focus:outline-none focus:border-[#e05246] transition-colors"
                                />
                                <button
                                    onClick={handleAddGroup}
                                    disabled={!newLabel.trim()}
                                    className="flex items-center gap-1 px-3 py-1.5 bg-[#e05246] hover:bg-[#c43d32] text-white text-xs font-medium rounded-lg transition-colors disabled:opacity-40"
                                >
                                    <FolderPlus className="w-3 h-3" /> Add
                                </button>
                            </div>
                            <input
                                type="text"
                                placeholder="Filter text (matches machine name)..."
                                value={newFilter}
                                onChange={(e) => setNewFilter(e.target.value)}
                                className="w-full px-3 py-1.5 bg-[#141414] border border-[#333] rounded-lg text-xs text-gray-300 placeholder-gray-600 focus:outline-none focus:border-[#e05246] transition-colors"
                            />
                        </div>

                        {/* List */}
                        <div className="flex-1 overflow-y-auto p-3 space-y-1">
                            {customGroups.length === 0 && (
                                <p className="text-center text-xs text-gray-600 py-6">No custom groups yet. Create one above.</p>
                            )}
                            {customGroups.map((g) => (
                                <div key={g.id} className="flex items-center gap-2 px-3 py-2 rounded-lg hover:bg-white/5 group">
                                    {editingId === g.id ? (
                                        <div className="flex-1 flex gap-1">
                                            <input
                                                type="text"
                                                value={editLabel}
                                                onChange={(e) => setEditLabel(e.target.value)}
                                                onKeyDown={(e) => e.key === "Enter" && handleRenameGroup(g.id)}
                                                autoFocus
                                                className="flex-1 px-2 py-1 bg-[#141414] border border-[#e05246] rounded text-xs text-white focus:outline-none"
                                            />
                                            <button onClick={() => handleRenameGroup(g.id)} className="text-emerald-400 hover:text-emerald-300">
                                                <Check className="w-3.5 h-3.5" />
                                            </button>
                                            <button onClick={() => setEditingId(null)} className="text-gray-500 hover:text-gray-300">
                                                <X className="w-3.5 h-3.5" />
                                            </button>
                                        </div>
                                    ) : (
                                        <>
                                            <span className="flex-1 text-xs text-gray-300 truncate">{g.label}</span>
                                            {g.filter && <span className="text-[10px] text-gray-600 bg-[#252525] px-1.5 py-0.5 rounded">filter: {g.filter}</span>}
                                            <button
                                                onClick={() => { setEditingId(g.id); setEditLabel(g.label); }}
                                                className="opacity-0 group-hover:opacity-100 text-gray-500 hover:text-white transition-all"
                                            >
                                                <Pencil className="w-3 h-3" />
                                            </button>
                                            <button
                                                onClick={() => handleDeleteGroup(g.id)}
                                                className="opacity-0 group-hover:opacity-100 text-gray-500 hover:text-red-400 transition-all"
                                            >
                                                <Trash2 className="w-3 h-3" />
                                            </button>
                                        </>
                                    )}
                                </div>
                            ))}
                        </div>
                    </div>
                </div>
            )}
        </div>
    );
}
