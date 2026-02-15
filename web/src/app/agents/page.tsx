"use client";
import { getAccessToken } from "@/lib/auth-store";

import { useState, useEffect, useCallback, useMemo } from "react";
import { useRouter, useSearchParams } from "next/navigation";
import { api, Agent } from "@/lib/api";
import { SessionGroups } from "@/components/session-groups";
import { launchDesktopSession } from "@/lib/session-launcher";
import { MachineList } from "@/components/machine-list";
import { AgentDetail } from "@/components/agent-detail";
import { BuildInstallerDialog } from "@/components/build-installer-dialog";
import { AgentListSkeleton } from "@/components/skeleton";
import { useEvents } from "@/lib/use-agent-status";

const POLL_INTERVAL = 10_000;

// ─── Main Access Page ─────────────────────────────────────────

export default function AgentsPage() {
    const router = useRouter();
    const [agents, setAgents] = useState<Agent[]>([]);
    const [loading, setLoading] = useState(true);
    const [selectedId, setSelectedId] = useState<string | null>(null);
    const [activeGroup, setActiveGroup] = useState("all");
    const [search, setSearch] = useState("");
    const [showDeploy, setShowDeploy] = useState(false);
    const { statusMap: liveStatus } = useEvents();

    // Merge live WebSocket status into polled agent data
    const liveAgents = useMemo(() => {
        if (liveStatus.size === 0) return agents;
        return agents.map((a) => {
            const ws = liveStatus.get(a.id);
            return ws ? { ...a, status: ws } : a;
        });
    }, [agents, liveStatus]);

    const fetchAgents = useCallback(async () => {
        try {
            const token = getAccessToken();
            if (token) api.setToken(token);
            const data = await api.getAgents();
            setAgents(data);
        } catch (e) {
            console.error("Failed to load agents:", e);
        } finally {
            setLoading(false);
        }
    }, []);

    useEffect(() => {
        fetchAgents();
        const interval = setInterval(fetchAgents, POLL_INTERVAL);
        return () => clearInterval(interval);
    }, [fetchAgents]);

    // Auto-select agent from ?select= query param (e.g. from View Agent)
    const searchParams = useSearchParams();
    useEffect(() => {
        const selectId = searchParams.get("select");
        if (selectId && agents.length > 0) {
            const match = agents.find((a) => a.id === selectId);
            if (match) setSelectedId(match.id);
        }
    }, [searchParams, agents]);

    const selectedAgent = liveAgents.find((a) => a.id === selectedId) || null;

    const handleJoin = useCallback(async (agent: Agent) => {
        try {
            const token = getAccessToken();
            if (token) api.setToken(token);
            const session = await api.createSession(agent.id, "desktop");
            launchDesktopSession(session.id);
        } catch (e) {
            console.error("Failed to create session:", e);
        }
    }, [router]);

    const handleEnd = useCallback(async (agent: Agent) => {
        try {
            const token = getAccessToken();
            if (token) api.setToken(token);
            const sessions = await api.getSessions();
            const activeSessions = sessions.filter(
                (s: { agent_id: string; status: string }) =>
                    s.agent_id === agent.id && (s.status === "active" || s.status === "pending")
            );
            if (activeSessions.length > 0) {
                await api.endSession(activeSessions[0].id);
                await fetchAgents();
            }
        } catch (e) {
            console.error("Failed to end session:", e);
        }
    }, [fetchAgents]);

    if (loading && agents.length === 0) {
        return (
            <div className="flex h-full">
                <AgentListSkeleton />
            </div>
        );
    }

    return (
        <>
            <div className="flex h-full">
                <SessionGroups
                    agents={liveAgents}
                    activeGroup={activeGroup}
                    onGroupChange={setActiveGroup}
                    onDeploy={() => setShowDeploy(true)}
                />
                <MachineList
                    agents={liveAgents}
                    selectedId={selectedId}
                    onSelect={(agent) => setSelectedId(agent.id)}
                    onJoin={handleJoin}
                    onEnd={handleEnd}
                    filter={activeGroup}
                    search={search}
                    onSearchChange={setSearch}
                />
                <AgentDetail agent={selectedAgent} />
            </div>
            {showDeploy && <BuildInstallerDialog open={showDeploy} onClose={() => setShowDeploy(false)} />}
        </>
    );
}
