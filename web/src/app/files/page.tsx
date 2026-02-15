"use client";
import { getAccessToken } from "@/lib/auth-store";

/**
 * Files page — file browser for a selected agent.
 * Supports arriving via:
 *   1. URL search param: /files?session=<id>
 *   2. Agent picker modal
 */

import { useState, useEffect, useCallback } from "react";
import { useSearchParams } from "next/navigation";
import {
    FolderOpen,
    Monitor,
    Plus,
    X,
    Loader2,
    HardDrive,
    Upload,
    ArrowLeftRight,
} from "lucide-react";
import { api, Agent } from "@/lib/api";
import FileManager from "@/components/file-manager";
import { EmptyState } from "@/components/empty-state";

// ─── Agent Picker Modal ──────────────────────────────────────

function AgentPickerModal({
    onSelect,
    onClose,
}: {
    onSelect: (agent: Agent) => void;
    onClose: () => void;
}) {
    const [agents, setAgents] = useState<Agent[]>([]);
    const [loading, setLoading] = useState(true);
    const [creating, setCreating] = useState<string | null>(null);

    useEffect(() => {
        const token = getAccessToken();
        if (token) api.setToken(token);

        api.getAgents()
            .then((data) => setAgents(data))
            .catch((e) => console.error("Failed to load agents:", e))
            .finally(() => setLoading(false));
    }, []);

    const onlineAgents = agents.filter((a) => a.status === "online");

    return (
        <div className="fixed inset-0 z-50 flex items-center justify-center">
            <div
                className="absolute inset-0 bg-black/60 backdrop-blur-sm"
                onClick={onClose}
            />
            <div className="relative w-full max-w-md mx-4 bg-[#1e1e1e] border border-[#333] rounded-xl shadow-2xl overflow-hidden">
                <div className="flex items-center justify-between px-5 py-4 border-b border-[#333]">
                    <div>
                        <h2 className="text-base font-semibold text-white">
                            Connect to Agent
                        </h2>
                        <p className="text-[11px] text-gray-500">Select an agent for file transfer</p>
                    </div>
                    <button
                        onClick={onClose}
                        className="p-1.5 rounded-lg hover:bg-white/5 text-gray-400 hover:text-white transition-colors"
                    >
                        <X className="w-5 h-5" />
                    </button>
                </div>
                <div className="p-3 max-h-80 overflow-y-auto space-y-1">
                    {loading ? (
                        <div className="text-center py-8 text-gray-500">
                            <Loader2 className="w-6 h-6 mx-auto mb-2 animate-spin" />
                            <p className="text-sm">Loading agents...</p>
                        </div>
                    ) : onlineAgents.length === 0 ? (
                        <div className="text-center py-8 text-gray-600">
                            <Monitor className="w-8 h-8 mx-auto mb-2 opacity-20" />
                            <p className="text-sm text-gray-500">No online agents</p>
                            <p className="text-xs mt-1 text-gray-600">
                                Deploy and start an agent first
                            </p>
                        </div>
                    ) : (
                        onlineAgents.map((agent) => (
                            <button
                                key={agent.id}
                                onClick={() => {
                                    setCreating(agent.id);
                                    onSelect(agent);
                                }}
                                disabled={creating !== null}
                                className="w-full flex items-center gap-3 p-3 rounded-lg hover:bg-white/[0.03] transition-colors text-left disabled:opacity-50"
                            >
                                <div className="w-8 h-8 rounded-lg bg-emerald-500/10 flex items-center justify-center">
                                    <span className="w-2 h-2 rounded-full bg-emerald-400" />
                                </div>
                                <div className="flex-1 min-w-0">
                                    <p className="text-sm font-medium text-white truncate">
                                        {agent.machine_name}
                                    </p>
                                    <p className="text-[11px] text-gray-500">
                                        {agent.os} {agent.os_version} • {agent.arch}
                                    </p>
                                </div>
                                {creating === agent.id ? (
                                    <Loader2 className="w-4 h-4 animate-spin text-[#e05246]" />
                                ) : (
                                    <FolderOpen className="w-4 h-4 text-gray-600" />
                                )}
                            </button>
                        ))
                    )}
                </div>
            </div>
        </div>
    );
}

// ─── Main Page ───────────────────────────────────────────────

export default function FilesPage() {
    const searchParams = useSearchParams();
    const sessionParam = searchParams.get("session");
    const [sessionId, setSessionId] = useState<string | null>(sessionParam);
    const [agentId, setAgentId] = useState<string | null>(null);
    const [showPicker, setShowPicker] = useState(false);
    const [error, setError] = useState<string | null>(null);

    // If we arrive with ?session=, use it directly
    useEffect(() => {
        if (sessionParam) {
            setSessionId(sessionParam);
        }
    }, [sessionParam]);

    const handleAgentSelect = useCallback(async (agent: Agent) => {
        try {
            const token = getAccessToken();
            if (token) api.setToken(token);

            const session = await api.createSession(agent.id, "file_transfer");
            setSessionId(session.id);
            setAgentId(agent.id);
            setShowPicker(false);
            setError(null);
        } catch (e) {
            console.error("Failed to create file transfer session:", e);
            setError("Failed to create file transfer session. Is the agent online?");
            setShowPicker(false);
        }
    }, []);

    return (
        <div className="flex flex-col h-full bg-[#141414]">
            {/* Header */}
            <div className="flex items-center justify-between px-5 py-3 border-b border-[#333]">
                <div className="flex items-center gap-2">
                    <HardDrive className="w-5 h-5 text-[#e05246]" />
                    <div>
                        <h1 className="text-base font-bold text-white">File Transfer</h1>
                        <p className="text-[10px] text-gray-500">Browse and manage files on remote agents</p>
                    </div>
                </div>
                {sessionId && (
                    <button
                        onClick={() => {
                            setSessionId(null);
                            setAgentId(null);
                            setShowPicker(true);
                        }}
                        className="flex items-center gap-2 px-3 py-1.5 bg-[#333] hover:bg-[#444] text-gray-300 text-xs rounded-lg transition-colors"
                    >
                        <ArrowLeftRight className="w-3.5 h-3.5" />
                        Switch Agent
                    </button>
                )}
            </div>

            {error && (
                <div className="mx-5 mt-3 p-3 bg-red-500/10 border border-red-500/20 rounded-lg text-sm text-red-400">
                    {error}
                </div>
            )}

            {sessionId ? (
                <div className="flex-1 overflow-hidden">
                    <FileManager sessionId={sessionId} agentId={agentId ?? undefined} className="h-full" />
                </div>
            ) : (
                <div className="flex-1 flex items-center justify-center">
                    <EmptyState
                        icon={FolderOpen}
                        title="No agent selected"
                        description="Connect to an agent to browse and manage its files remotely. Upload, download, and navigate the file system."
                        actionLabel="Connect to Agent"
                        onAction={() => setShowPicker(true)}
                        color="#22d3ee"
                    />
                </div>
            )}

            {showPicker && (
                <AgentPickerModal
                    onSelect={handleAgentSelect}
                    onClose={() => setShowPicker(false)}
                />
            )}
        </div>
    );
}
