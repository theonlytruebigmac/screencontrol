"use client";
import { getAccessToken } from "@/lib/auth-store";

import { useState, useEffect, useRef, useCallback } from "react";
import {
    TerminalSquare,
    Plus,
    X,
    Wifi,
    WifiOff,
    Loader2,
    Monitor,
} from "lucide-react";
import { api, Agent } from "@/lib/api";
import { EmptyState } from "@/components/empty-state";
import {
    encodeTerminalData,
    encodeTerminalResize,
    encodeSessionEnd,
    decodeEnvelope,
} from "@/lib/proto";
import "@xterm/xterm/css/xterm.css";

const WS_BASE =
    typeof window !== "undefined"
        ? `ws://${window.location.hostname}:8080`
        : "ws://localhost:8080";

// ─── Types ────────────────────────────────────────────────────

interface TerminalTab {
    id: string; // session ID
    label: string;
    agentId: string;
    status: "connecting" | "active" | "closed";
}

// ─── Terminal Instance ───────────────────────────────────────

function TerminalInstance({
    sessionId,
    visible,
    onStatusChange,
}: {
    sessionId: string;
    visible: boolean;
    onStatusChange: (status: "connecting" | "active" | "closed") => void;
}) {
    const termRef = useRef<HTMLDivElement>(null);
    const xtermRef = useRef<import("@xterm/xterm").Terminal | null>(null);
    const wsRef = useRef<WebSocket | null>(null);
    const fitAddonRef = useRef<import("@xterm/addon-fit").FitAddon | null>(null);

    useEffect(() => {
        let cancelled = false;

        async function init() {
            const { Terminal } = await import("@xterm/xterm");
            const { FitAddon } = await import("@xterm/addon-fit");
            const { WebLinksAddon } = await import("@xterm/addon-web-links");

            if (cancelled || !termRef.current) return;

            const term = new Terminal({
                cursorBlink: true,
                fontSize: 14,
                fontFamily: "'JetBrains Mono', 'Fira Code', 'Cascadia Code', monospace",
                theme: {
                    background: "#0a0a0f",
                    foreground: "#e4e4e7",
                    cursor: "#818cf8",
                    selectionBackground: "#818cf840",
                    black: "#09090b",
                    red: "#f87171",
                    green: "#4ade80",
                    yellow: "#facc15",
                    blue: "#60a5fa",
                    magenta: "#c084fc",
                    cyan: "#22d3ee",
                    white: "#e4e4e7",
                    brightBlack: "#52525b",
                    brightRed: "#fca5a5",
                    brightGreen: "#86efac",
                    brightYellow: "#fde68a",
                    brightBlue: "#93c5fd",
                    brightMagenta: "#d8b4fe",
                    brightCyan: "#67e8f9",
                    brightWhite: "#fafafa",
                },
                allowProposedApi: true,
            });

            const fitAddon = new FitAddon();
            term.loadAddon(fitAddon);
            term.loadAddon(new WebLinksAddon());

            term.open(termRef.current);
            fitAddon.fit();

            xtermRef.current = term;
            fitAddonRef.current = fitAddon;

            term.writeln("\x1b[1;34m● Connecting to agent...\x1b[0m\r\n");

            // ── WebSocket ────────────────────────────────
            const ws = new WebSocket(`${WS_BASE}/ws/console/${sessionId}`);
            ws.binaryType = "arraybuffer";
            wsRef.current = ws;

            ws.onopen = () => {
                onStatusChange("active");
                term.writeln("\x1b[1;32m● Connected\x1b[0m\r\n");

                // Send initial resize
                const dims = fitAddon.proposeDimensions();
                if (dims) {
                    ws.send(encodeTerminalResize(sessionId, dims.cols, dims.rows));
                }
            };

            ws.onmessage = (evt) => {
                const data = new Uint8Array(evt.data);
                const envelope = decodeEnvelope(data);
                if (!envelope) return;

                switch (envelope.payload.type) {
                    case "terminal_data":
                        term.write(envelope.payload.data);
                        break;
                    case "session_offer":
                        // Terminal ready signal from agent
                        if (envelope.payload.sdp === "terminal-ready") {
                            term.writeln("\x1b[1;32m● Terminal ready\x1b[0m\r\n");
                        }
                        break;
                    case "session_end":
                        term.writeln(
                            `\r\n\x1b[1;31m● Session ended: ${envelope.payload.reason}\x1b[0m`
                        );
                        onStatusChange("closed");
                        break;
                }
            };

            ws.onclose = () => {
                term.writeln("\r\n\x1b[1;33m● Disconnected\x1b[0m");
                onStatusChange("closed");
            };

            ws.onerror = () => {
                term.writeln("\r\n\x1b[1;31m● Connection error\x1b[0m");
                onStatusChange("closed");
            };

            // ── Terminal input → WebSocket ───────────────
            term.onData((input) => {
                if (ws.readyState === WebSocket.OPEN) {
                    const encoded = new TextEncoder().encode(input);
                    ws.send(encodeTerminalData(sessionId, encoded));
                }
            });

            // ── Resize handling ──────────────────────────
            const resizeObserver = new ResizeObserver(() => {
                fitAddon.fit();
            });
            if (termRef.current) {
                resizeObserver.observe(termRef.current);
            }

            term.onResize(({ cols, rows }) => {
                if (ws.readyState === WebSocket.OPEN) {
                    ws.send(encodeTerminalResize(sessionId, cols, rows));
                }
            });
        }

        init();

        return () => {
            cancelled = true;
            if (wsRef.current && wsRef.current.readyState === WebSocket.OPEN) {
                wsRef.current.send(encodeSessionEnd(sessionId, "user_closed"));
                wsRef.current.close();
            }
            xtermRef.current?.dispose();
        };
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [sessionId]);

    // Re-fit when tab becomes visible
    useEffect(() => {
        if (visible && fitAddonRef.current) {
            setTimeout(() => fitAddonRef.current?.fit(), 50);
        }
    }, [visible]);

    return (
        <div
            ref={termRef}
            className="w-full h-full"
            style={{ display: visible ? "block" : "none" }}
        />
    );
}

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
            <div className="relative w-full max-w-md mx-4 bg-[#1e1e1e] border border-[#333] rounded-2xl shadow-2xl overflow-hidden">
                <div className="flex items-center justify-between px-5 py-4 border-b border-[#333]">
                    <h2 className="text-lg font-semibold text-white">
                        Connect to Agent
                    </h2>
                    <button
                        onClick={onClose}
                        className="p-1.5 rounded-lg hover:bg-gray-800 text-gray-400 hover:text-white transition-colors"
                    >
                        <X className="w-5 h-5" />
                    </button>
                </div>
                <div className="p-4 max-h-80 overflow-y-auto space-y-1">
                    {loading ? (
                        <div className="text-center py-8 text-gray-500">
                            <Loader2 className="w-6 h-6 mx-auto mb-2 animate-spin" />
                            <p className="text-sm">Loading agents...</p>
                        </div>
                    ) : onlineAgents.length === 0 ? (
                        <div className="text-center py-8 text-gray-500">
                            <Monitor className="w-8 h-8 mx-auto mb-2 opacity-30" />
                            <p className="text-sm">No online agents</p>
                            <p className="text-xs mt-1">
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
                                className="w-full flex items-center gap-3 p-3 rounded-xl hover:bg-white/[0.04] transition-colors text-left disabled:opacity-50"
                            >
                                <div className="w-8 h-8 rounded-lg bg-emerald-500/10 flex items-center justify-center">
                                    <span className="w-2 h-2 rounded-full bg-emerald-400" />
                                </div>
                                <div className="flex-1 min-w-0">
                                    <p className="text-sm font-medium text-white truncate">
                                        {agent.machine_name}
                                    </p>
                                    <p className="text-xs text-gray-500">
                                        {agent.os} {agent.os_version} • {agent.arch}
                                    </p>
                                </div>
                                {creating === agent.id ? (
                                    <Loader2 className="w-4 h-4 animate-spin text-[#e05246]" />
                                ) : (
                                    <TerminalSquare className="w-4 h-4 text-gray-600" />
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

export default function TerminalPage() {
    const [tabs, setTabs] = useState<TerminalTab[]>([]);
    const [activeTab, setActiveTab] = useState<string | null>(null);
    const [showPicker, setShowPicker] = useState(false);

    const handleAgentSelect = useCallback(async (agent: Agent) => {
        try {
            const token = getAccessToken();
            if (token) api.setToken(token);

            const session = await api.createSession(agent.id, "terminal");
            const tab: TerminalTab = {
                id: session.id,
                label: agent.machine_name,
                agentId: agent.id,
                status: "connecting",
            };
            setTabs((prev) => [...prev, tab]);
            setActiveTab(session.id);
            setShowPicker(false);
        } catch (e) {
            console.error("Failed to create session:", e);
            alert("Failed to create terminal session. Is the agent online?");
            setShowPicker(false);
        }
    }, []);

    const handleCloseTab = useCallback(
        (tabId: string, e?: React.MouseEvent) => {
            e?.stopPropagation();
            setTabs((prev) => prev.filter((t) => t.id !== tabId));
            if (activeTab === tabId) {
                setActiveTab((prev) => {
                    const remaining = tabs.filter((t) => t.id !== tabId);
                    return remaining.length > 0 ? remaining[remaining.length - 1].id : null;
                });
            }
        },
        [activeTab, tabs]
    );

    const handleStatusChange = useCallback(
        (tabId: string, status: "connecting" | "active" | "closed") => {
            setTabs((prev) =>
                prev.map((t) => (t.id === tabId ? { ...t, status } : t))
            );
        },
        []
    );

    return (
        <div className="flex flex-col h-[calc(100vh-0px)]">
            {/* Tab Bar */}
            <div className="flex items-center gap-1 px-4 pt-3 pb-0 bg-[#141414] border-b border-[#333] shrink-0">
                {tabs.map((tab) => (
                    <div
                        key={tab.id}
                        onClick={() => setActiveTab(tab.id)}
                        className={`flex items-center gap-2 px-4 py-2 border border-b-0 rounded-t-lg text-sm cursor-pointer transition-colors ${activeTab === tab.id
                            ? "bg-[#1e1e1e] border-[#333] text-white"
                            : "bg-[#141414] border-[#252525] text-gray-500 hover:text-gray-300"
                            }`}
                    >
                        {tab.status === "connecting" ? (
                            <Loader2 className="w-3.5 h-3.5 text-amber-400 animate-spin" />
                        ) : tab.status === "active" ? (
                            <Wifi className="w-3.5 h-3.5 text-emerald-400" />
                        ) : (
                            <WifiOff className="w-3.5 h-3.5 text-red-400" />
                        )}
                        <span className="max-w-[120px] truncate">{tab.label}</span>
                        <button
                            onClick={(e) => handleCloseTab(tab.id, e)}
                            className="ml-1 p-0.5 rounded hover:bg-[#333] text-gray-500 hover:text-white transition-colors"
                        >
                            <X className="w-3 h-3" />
                        </button>
                    </div>
                ))}
                <button
                    onClick={() => setShowPicker(true)}
                    className="flex items-center gap-1.5 px-3 py-2 text-gray-500 hover:text-[#f06b60] hover:bg-[#e05246]/10 rounded-t-lg text-sm transition-colors"
                >
                    <Plus className="w-4 h-4" />
                    New Terminal
                </button>
            </div>

            {/* Terminal Area */}
            <div className="flex-1 bg-[#0a0a0f] relative overflow-hidden">
                {tabs.length === 0 ? (
                    <EmptyState
                        icon={TerminalSquare}
                        title="No Active Terminals"
                        description="Connect to an agent to open a remote terminal session."
                        actionLabel="Connect to Agent"
                        onAction={() => setShowPicker(true)}
                        color="#10b981"
                    />
                ) : (
                    tabs.map((tab) => (
                        <TerminalInstance
                            key={tab.id}
                            sessionId={tab.id}
                            visible={activeTab === tab.id}
                            onStatusChange={(s) => handleStatusChange(tab.id, s)}
                        />
                    ))
                )}
            </div>

            {/* Agent Picker Modal */}
            {showPicker && (
                <AgentPickerModal
                    onSelect={handleAgentSelect}
                    onClose={() => setShowPicker(false)}
                />
            )}
        </div>
    );
}
