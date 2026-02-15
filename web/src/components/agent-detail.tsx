"use client";

import { useState, useCallback, useRef, useEffect } from "react";
import {
    Home,
    Info,
    Clock,
    MessageSquare,
    Terminal,
    StickyNote,
    Monitor,
    Cpu,
    HardDrive,
    Globe,
    Wifi,
    User,
    Loader2,
    Send,
    Trash2,
    ChevronRight,
    Copy,
    Check,
    FolderOpen,
    Wrench,
    Activity,
    Play,
    Square,
    AlertTriangle,
    AlertCircle,
    InfoIcon,
} from "lucide-react";
import type { Agent, Session, Script } from "@/lib/api";
import { api } from "@/lib/api";
import { launchDesktopSession, launchTerminalSession } from "@/lib/session-launcher";
import { useAgentSocket } from "@/lib/use-agent-socket";
import { useRouter } from "next/navigation";
import { useToast } from "@/components/toast";

interface AgentDetailProps {
    agent: Agent | null;
}

function formatBytes(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
    return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
}

function formatUptime(secs: number): string {
    const days = Math.floor(secs / 86400);
    const hours = Math.floor((secs % 86400) / 3600);
    const mins = Math.floor((secs % 3600) / 60);
    if (days > 0) return `${days}d ${hours}h ${mins}m`;
    if (hours > 0) return `${hours}h ${mins}m`;
    return `${mins}m`;
}

const tabs = [
    { id: "start", icon: Home, label: "Start" },
    { id: "general", icon: Info, label: "General" },
    { id: "timeline", icon: Clock, label: "Timeline" },
    { id: "messages", icon: MessageSquare, label: "Messages" },
    { id: "commands", icon: Terminal, label: "Commands" },
    { id: "notes", icon: StickyNote, label: "Notes" },
    { id: "toolbox", icon: Wrench, label: "Toolbox" },
    { id: "diagnostics", icon: Activity, label: "Diagnostics" },
];

// Built-in fallback scripts (used when API is unreachable)
const BUILTIN_SCRIPTS: Pick<Script, 'id' | 'name' | 'description' | 'code' | 'language'>[] = [
    { id: "sysinfo", name: "System Info", description: "OS, hostname, uptime, kernel", code: "uname -a && uptime && hostname", language: "bash" },
    { id: "diskspace", name: "Disk Space", description: "Filesystem usage summary", code: "df -h", language: "bash" },
    { id: "processes", name: "Running Processes", description: "Top processes by CPU", code: "ps aux --sort=-%cpu | head -20", language: "bash" },
    { id: "memory", name: "Memory Usage", description: "RAM and swap stats", code: "free -h", language: "bash" },
    { id: "network", name: "Network Config", description: "IP addresses and interfaces", code: "ip addr show", language: "bash" },
    { id: "services", name: "Active Services", description: "All running services", code: "systemctl list-units --type=service --state=running", language: "bash" },
];

// Diagnostics types
interface ProcessInfo { pid: number; name: string; cpu: number; mem: number; status: string }
interface ServiceInfo { name: string; desc: string; status: "active" | "inactive" }
interface EventInfo { id: number; time: string; level: "info" | "warning" | "error"; source: string; message: string }

// ─── Types ───────────────────────────────────────────────

interface ChatMessage {
    id: string;
    text: string;
    sender: "host" | "agent-user" | "system";
    senderName?: string;
    timestamp: Date;
}

interface CommandEntry {
    id: string;
    command: string;
    output: string;
    status: "running" | "done" | "error";
    timestamp: Date;
}

interface Note {
    id: string;
    text: string;
    timestamp: Date;
}

// ─── Component ───────────────────────────────────────────

export function AgentDetail({ agent }: AgentDetailProps) {
    const router = useRouter();
    const { info } = useToast();
    const [activeTab, setActiveTab] = useState("general");
    const [connecting, setConnecting] = useState<string | null>(null);
    const [agentSessions, setAgentSessions] = useState<Session[]>([]);

    // Messages
    const [messages, setMessages] = useState<ChatMessage[]>([]);
    const [chatLoaded, setChatLoaded] = useState(false);
    const [msgText, setMsgText] = useState("");
    const messagesEndRef = useRef<HTMLDivElement>(null);

    // Commands
    const [commands, setCommands] = useState<CommandEntry[]>([]);
    const [commandText, setCommandText] = useState("");
    const [cmdCopied, setCmdCopied] = useState<string | null>(null);
    // Track the pending command id so we can match responses
    const pendingCmdRef = useRef<string | null>(null);

    // Diagnostics
    const [diagProcesses, setDiagProcesses] = useState<ProcessInfo[]>([]);
    const [diagServices, setDiagServices] = useState<ServiceInfo[]>([]);
    const [diagEvents, setDiagEvents] = useState<EventInfo[]>([]);
    const [diagLoading, setDiagLoading] = useState(false);
    // Track which diagnostic stage we're expecting a response for
    // null = normal command mode, 'processes' | 'services' | 'events' = diagnostics
    const diagStageRef = useRef<'processes' | 'services' | 'events' | null>(null);

    // Toolbox scripts (fetched from API)
    const [toolboxScripts, setToolboxScripts] = useState<Pick<Script, 'id' | 'name' | 'description' | 'code' | 'language'>[]>(BUILTIN_SCRIPTS);
    const [toolboxLoading, setToolboxLoading] = useState(false);
    const toolboxFetchedRef = useRef(false);

    // ─── Agent WebSocket (commands + chat) ────────────────

    const { connect: connectAgentWs, disconnect: disconnectAgentWs, sendCommand, sendChat, status: agentWsStatus } = useAgentSocket({
        agentId: agent?.id ?? '',
        onCommandResponse: useCallback((result: { exitCode: number; stdout: string; stderr: string; timedOut: boolean }) => {
            // Check if this is a diagnostics response
            const stage = diagStageRef.current;
            if (stage) {
                const output = result.stdout || '';
                if (stage === 'processes') {
                    // Parse `ps aux --sort=-%cpu` output
                    const lines = output.split('\n').slice(1).filter(Boolean); // skip header
                    const procs: ProcessInfo[] = lines.slice(0, 20).map((line) => {
                        const parts = line.trim().split(/\s+/);
                        return {
                            pid: parseInt(parts[1] || '0'),
                            name: parts.slice(10).join(' ') || parts[10] || 'unknown',
                            cpu: parseFloat(parts[2] || '0'),
                            mem: parseFloat(parts[3] || '0'),
                            status: parts[7] || 'unknown',
                        };
                    }).filter(p => p.pid > 0);
                    setDiagProcesses(procs);
                } else if (stage === 'services') {
                    // Parse `systemctl list-units --type=service --no-legend` output
                    const lines = output.split('\n').filter(Boolean);
                    const svcs: ServiceInfo[] = lines.slice(0, 20).map((line, i) => {
                        const parts = line.trim().split(/\s+/);
                        const name = (parts[0] || '').replace(/^●\s*/, '');
                        const active = parts[2] === 'active';
                        const desc = parts.slice(4).join(' ') || name;
                        return { name, desc, status: active ? 'active' as const : 'inactive' as const };
                    }).filter(s => s.name.length > 0);
                    setDiagServices(svcs);
                } else if (stage === 'events') {
                    // Parse `journalctl --no-pager -n 15 -o short` output
                    const lines = output.split('\n').filter(Boolean);
                    const evts: EventInfo[] = lines.slice(0, 15).map((line, i) => {
                        // Format: "Mon DD HH:MM:SS hostname process[pid]: message"
                        const timeMatch = line.match(/(\d{2}:\d{2}:\d{2})/);
                        const time = timeMatch?.[1] || '';
                        const colonIdx = line.indexOf(': ');
                        const message = colonIdx >= 0 ? line.slice(colonIdx + 2).trim() : line;
                        const bracketIdx = line.indexOf('[');
                        const spaceBeforeBracket = line.lastIndexOf(' ', bracketIdx > 0 ? bracketIdx : undefined);
                        const source = spaceBeforeBracket > 0 ? line.slice(spaceBeforeBracket + 1, bracketIdx > 0 ? bracketIdx : undefined).trim() : 'system';
                        const level: EventInfo['level'] = line.toLowerCase().includes('error') || line.toLowerCase().includes('fail')
                            ? 'error'
                            : line.toLowerCase().includes('warn')
                                ? 'warning'
                                : 'info';
                        return { id: i + 1, time, level, source, message };
                    });
                    setDiagEvents(evts);
                    setDiagLoading(false);
                }
                diagStageRef.current = null;
                return;
            }

            // Normal command mode
            const cmdId = pendingCmdRef.current;
            if (cmdId) {
                const output = result.timedOut
                    ? '[timed out]'
                    : result.stderr
                        ? `${result.stdout}\n${result.stderr}`.trim()
                        : result.stdout;
                const status: CommandEntry['status'] = result.exitCode === 0 ? 'done' : 'error';
                setCommands((prev) =>
                    prev.map((c) => c.id === cmdId
                        ? { ...c, output, status }
                        : c
                    ),
                );
                pendingCmdRef.current = null;
            }
        }, []),
        onChatMessage: useCallback((msg: { senderId: string; senderName: string; content: string; timestamp: Date }) => {
            const sender = msg.senderId === 'agent-user' ? 'agent-user' as const : 'system' as const;
            setMessages((prev) => [
                ...prev,
                { id: `msg-${Date.now()}`, text: msg.content, sender, senderName: msg.senderName, timestamp: msg.timestamp },
            ]);
        }, []),
    });

    // Auto-connect WS when switching to commands, messages, toolbox, or diagnostics tab
    useEffect(() => {
        if ((activeTab === 'commands' || activeTab === 'messages' || activeTab === 'toolbox' || activeTab === 'diagnostics') && agent?.status === 'online') {
            connectAgentWs();
        }
        return () => {
            // Don't disconnect on every tab switch — only on unmount
        };
    }, [activeTab, agent?.status, connectAgentWs]);

    // Load chat history from API when Messages tab is first opened
    useEffect(() => {
        if (activeTab === 'messages' && !chatLoaded && agent) {
            setChatLoaded(true);
            api.getAgentChat(agent.id, 100).then((rows) => {
                const history: ChatMessage[] = rows.reverse().map((r) => ({
                    id: r.id,
                    text: r.content,
                    sender: r.sender_type === 'tech' ? 'host' as const
                        : r.sender_type === 'agent-user' ? 'agent-user' as const
                            : 'system' as const,
                    senderName: r.sender_name,
                    timestamp: new Date(r.created_at),
                }));
                setMessages((prev) => [...history, ...prev]);
            }).catch((e) => console.error('Failed to load chat history:', e));
        }
    }, [activeTab, chatLoaded, agent]);

    // Fetch toolbox scripts from API when tab is activated
    useEffect(() => {
        if (activeTab === 'toolbox' && !toolboxFetchedRef.current) {
            toolboxFetchedRef.current = true;
            setToolboxLoading(true);
            api.getScripts()
                .then((scripts) => {
                    if (scripts.length > 0) {
                        setToolboxScripts(scripts);
                    }
                    // If API returns empty, keep the built-in fallbacks
                })
                .catch(() => {
                    // API unavailable — keep built-in fallbacks, they still work
                })
                .finally(() => setToolboxLoading(false));
        }
    }, [activeTab]);

    // Run diagnostics when diagnostics tab is selected and WS is connected
    const runDiagnostics = useCallback(() => {
        if (agentWsStatus !== 'connected') return;
        setDiagLoading(true);
        setDiagProcesses([]);
        setDiagServices([]);
        setDiagEvents([]);

        // Send commands sequentially using timeouts to avoid collisions
        diagStageRef.current = 'processes';
        sendCommand('ps aux --sort=-%cpu | head -25', [], 10);

        setTimeout(() => {
            diagStageRef.current = 'services';
            sendCommand('systemctl list-units --type=service --no-legend --no-pager | head -20', [], 10);
        }, 500);

        setTimeout(() => {
            diagStageRef.current = 'events';
            sendCommand('journalctl --no-pager -n 15 -o short', [], 10);
        }, 1000);
    }, [agentWsStatus, sendCommand]);

    // Auto-run diagnostics when tab opens and WS is connected
    useEffect(() => {
        if (activeTab === 'diagnostics' && agentWsStatus === 'connected') {
            runDiagnostics();
        }
    }, [activeTab, agentWsStatus, runDiagnostics]);

    // Cleanup WS on unmount
    useEffect(() => {
        return () => { disconnectAgentWs(); };
    }, [disconnectAgentWs]);

    // Desktop thumbnail — fetch pre-signed URL on mount and periodically
    const [thumbnailUrl, setThumbnailUrl] = useState<string | null>(null);
    useEffect(() => {
        if (!agent) return;
        const fetchThumb = () => {
            const token = localStorage.getItem("sc_access_token");
            if (token) api.setToken(token);
            api.getAgentThumbnail(agent.id)
                .then(r => setThumbnailUrl(r.url))
                .catch(() => setThumbnailUrl(null));
        };
        fetchThumb();
        const interval = setInterval(fetchThumb, 30_000);
        return () => clearInterval(interval);
    }, [agent]);

    // Notes — persisted to server via admin_notes + localStorage fallback
    const [notes, setNotes] = useState<Note[]>(() => {
        // Try loading from agent's admin_notes first
        if (agent?.admin_notes) {
            try {
                const parsed = JSON.parse(agent.admin_notes);
                if (Array.isArray(parsed)) {
                    return parsed.map((n: { id: string; text: string; timestamp: string }) => ({
                        ...n, timestamp: new Date(n.timestamp),
                    }));
                }
            } catch { /* not JSON — ignore */ }
        }
        // Fallback: migrate from localStorage if any exist
        if (typeof window !== 'undefined' && agent) {
            try {
                const stored = localStorage.getItem(`sc_notes_${agent.id}`);
                if (stored) {
                    const migrated = JSON.parse(stored).map((n: { id: string; text: string; timestamp: string }) => ({
                        ...n, timestamp: new Date(n.timestamp),
                    }));
                    // Clear localStorage after migration
                    localStorage.removeItem(`sc_notes_${agent.id}`);
                    return migrated;
                }
            } catch { /* ignore */ }
        }
        return [];
    });
    const [noteText, setNoteText] = useState("");

    // Auto-save notes to server
    const notesSaveTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
    useEffect(() => {
        if (!agent) return;
        // Debounce saves — wait 500ms after last change
        if (notesSaveTimerRef.current) clearTimeout(notesSaveTimerRef.current);
        notesSaveTimerRef.current = setTimeout(() => {
            const token = localStorage.getItem("sc_access_token");
            if (token) api.setToken(token);
            api.updateAgent(agent.id, { admin_notes: JSON.stringify(notes) }).catch(() => {
                // Fallback: save to localStorage if server is unreachable
                try { localStorage.setItem(`sc_notes_${agent.id}`, JSON.stringify(notes)); } catch { /* ignore */ }
            });
        }, 500);
        return () => { if (notesSaveTimerRef.current) clearTimeout(notesSaveTimerRef.current); };
    }, [notes, agent]);

    useEffect(() => {
        messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
    }, [messages]);

    // Fetch sessions for this agent
    useEffect(() => {
        if (!agent) return;
        const token = localStorage.getItem("sc_access_token");
        if (token) api.setToken(token);
        api.getSessions()
            .then((all) => setAgentSessions(all.filter((s) => s.agent_id === agent.id)))
            .catch(console.error);
    }, [agent]);

    const handleConnect = useCallback(async (sessionType: string) => {
        if (!agent) return;
        try {
            setConnecting(sessionType);
            const token = localStorage.getItem("sc_access_token");
            if (token) api.setToken(token);
            const session = await api.createSession(agent.id, sessionType);

            if (sessionType === "desktop") launchDesktopSession(session.id);
            else if (sessionType === "terminal") launchTerminalSession(session.id);
            else if (sessionType === "file_transfer") router.push(`/files?session=${session.id}`);
        } catch (e) {
            console.error(`Failed to create ${sessionType} session:`, e);
        } finally {
            setConnecting(null);
        }
    }, [agent, router]);

    // ─── Handlers ────────────────────────────────────────

    const handleSendMessage = () => {
        if (!msgText.trim()) return;
        const text = msgText.trim();
        setMessages((prev) => [
            ...prev,
            { id: `msg-${Date.now()}`, text, sender: "host", timestamp: new Date() },
        ]);
        setMsgText("");
        // Send over WS
        sendChat(text);
    };

    const handleRunCommand = () => {
        if (!commandText.trim() || agent?.status === "offline") return;
        const cmd = commandText.trim();
        const entry: CommandEntry = {
            id: `cmd-${Date.now()}`,
            command: cmd,
            output: "",
            status: "running",
            timestamp: new Date(),
        };
        setCommands((prev) => [entry, ...prev]);
        setCommandText("");
        pendingCmdRef.current = entry.id;

        // Send over WS — agent will execute and respond
        sendCommand(cmd);
    };

    const handleCopyCommand = (text: string, id: string) => {
        navigator.clipboard.writeText(text);
        setCmdCopied(id);
        setTimeout(() => setCmdCopied(null), 1500);
    };

    const handleAddNote = () => {
        if (!noteText.trim()) return;
        setNotes((prev) => [
            { id: `note-${Date.now()}`, text: noteText.trim(), timestamp: new Date() },
            ...prev,
        ]);
        setNoteText("");
    };

    const handleDeleteNote = (id: string) => {
        setNotes((prev) => prev.filter((n) => n.id !== id));
    };

    // ─── Empty State ─────────────────────────────────────

    if (!agent) {
        return (
            <div className="flex flex-col h-full bg-[#1e1e1e] panel-border-l items-center justify-center" style={{ width: "var(--detail-width)" }}>
                <Monitor className="w-12 h-12 text-gray-700 mb-3" />
                <p className="text-sm text-gray-600">Select a machine</p>
            </div>
        );
    }

    const idleTime = agent.last_seen
        ? (() => {
            const diff = Date.now() - new Date(agent.last_seen).getTime();
            const mins = Math.floor(diff / 60000);
            if (mins < 1) return "0m";
            if (mins < 60) return `${mins}m`;
            const hrs = Math.floor(mins / 60);
            if (hrs < 24) return `${hrs}h ${mins % 60}m`;
            const days = Math.floor(hrs / 24);
            return `${days}d ${hrs % 24}h`;
        })()
        : "Unknown";

    // ─── Render ──────────────────────────────────────────

    return (
        <div className="flex flex-col h-full bg-[#1e1e1e] panel-border-l" style={{ width: "var(--detail-width)" }}>
            {/* Header */}
            <div className="px-4 py-3 border-b border-[#333]">
                <div className="flex items-center gap-2">
                    <span className={`w-2 h-2 rounded-full flex-shrink-0 ${agent.status === "online" ? "bg-emerald-400 pulse-dot"
                        : agent.status === "busy" ? "bg-[#e05246]"
                            : "bg-gray-600"
                        }`} />
                    <h3 className="text-sm font-bold text-white truncate">{agent.machine_name}</h3>
                </div>
                <p className="mt-0.5 text-[10px] text-gray-500">{agent.os} {agent.os_version} • {agent.arch}</p>
            </div>

            {/* Tabs — icon only */}
            <div className="flex border-b border-[#333]">
                {tabs.map((tab) => (
                    <button
                        key={tab.id}
                        onClick={() => setActiveTab(tab.id)}
                        className={`flex-1 flex items-center justify-center py-2.5 transition-colors border-b-2 relative ${activeTab === tab.id
                            ? "border-[#e05246] text-white bg-[#e05246]/5"
                            : "border-transparent text-gray-500 hover:text-gray-300 hover:bg-white/[0.03]"
                            }`}
                        title={tab.label}
                    >
                        <tab.icon className="w-4 h-4" />
                        {/* Unread badge for messages tab */}
                        {tab.id === "messages" && activeTab !== "messages" && messages.length > 1 && (
                            <span className="absolute top-1.5 right-1/2 translate-x-3 w-2 h-2 bg-[#e05246] rounded-full border border-[#1e1e1e]" />
                        )}
                        {/* WS status indicator for interactive tabs */}
                        {(tab.id === "commands" || tab.id === "messages" || tab.id === "diagnostics") && agentWsStatus === "connected" && (
                            <span className="absolute bottom-1 right-1/2 translate-x-2.5 w-1.5 h-1.5 bg-emerald-400 rounded-full" />
                        )}
                        {(tab.id === "commands" || tab.id === "messages" || tab.id === "diagnostics") && agentWsStatus === "connecting" && (
                            <span className="absolute bottom-1 right-1/2 translate-x-2.5 w-1.5 h-1.5 bg-amber-400 rounded-full animate-pulse" />
                        )}
                    </button>
                ))}
            </div>

            {/* Tab content */}
            <div className="flex-1 overflow-y-auto">
                {/* ─── Start Tab ────────────────────────────────── */}
                {activeTab === "start" && (
                    <div className="p-4 space-y-4">
                        {/* Quick connect */}
                        <div className="grid grid-cols-2 gap-2">
                            <button
                                onClick={() => handleConnect("desktop")}
                                disabled={agent.status === "offline" || connecting !== null}
                                className="flex items-center justify-center gap-1.5 py-2.5 bg-[#e05246] hover:bg-[#c43d32] text-white text-xs font-medium rounded-lg transition-colors disabled:opacity-40"
                            >
                                {connecting === "desktop" ? <Loader2 className="w-3.5 h-3.5 animate-spin" /> : <Monitor className="w-3.5 h-3.5" />}
                                Remote
                            </button>
                            <button
                                onClick={() => handleConnect("terminal")}
                                disabled={agent.status === "offline" || connecting !== null}
                                className="flex items-center justify-center gap-1.5 py-2.5 bg-[#333] hover:bg-[#444] text-white text-xs font-medium rounded-lg transition-colors disabled:opacity-40"
                            >
                                {connecting === "terminal" ? <Loader2 className="w-3.5 h-3.5 animate-spin" /> : <Terminal className="w-3.5 h-3.5" />}
                                Terminal
                            </button>
                            <button
                                onClick={() => handleConnect("file_transfer")}
                                disabled={agent.status === "offline" || connecting !== null}
                                className="col-span-2 flex items-center justify-center gap-1.5 py-2 bg-[#252525] hover:bg-[#333] text-gray-300 text-xs font-medium rounded-lg transition-colors disabled:opacity-40 border border-[#333]"
                            >
                                {connecting === "file_transfer" ? <Loader2 className="w-3.5 h-3.5 animate-spin" /> : <FolderOpen className="w-3.5 h-3.5" />}
                                File Transfer
                            </button>
                        </div>

                        {/* Session overview */}
                        <div className="space-y-0">
                            <InfoRow label="Name" value={agent.machine_name} />
                            <InfoRow label="Status" value={
                                <span className={`inline-flex items-center gap-1.5 ${agent.status === "online" ? "text-emerald-400"
                                    : agent.status === "busy" ? "text-[#f06b60]"
                                        : "text-gray-500"
                                    }`}>
                                    <span className={`w-1.5 h-1.5 rounded-full ${agent.status === "online" ? "bg-emerald-400"
                                        : agent.status === "busy" ? "bg-[#e05246]"
                                            : "bg-gray-600"
                                        }`} />
                                    {agent.status.charAt(0).toUpperCase() + agent.status.slice(1)}
                                </span>
                            } />
                            <InfoRow label="Operating System" value={`${agent.os} ${agent.os_version}`} />
                            <InfoRow label="Client Version" value={`v${agent.agent_version}`} />
                        </div>

                        {/* Screenshot */}
                        <div className="border border-[#333] rounded-lg bg-[#141414] aspect-video flex items-center justify-center overflow-hidden group cursor-pointer hover:border-[#444] transition-colors relative"
                            onClick={() => handleConnect("desktop")}>
                            {thumbnailUrl ? (
                                <>
                                    <img src={thumbnailUrl} alt="Desktop preview" className="w-full h-full object-cover" onError={() => setThumbnailUrl(null)} />
                                    <div className="absolute inset-0 bg-black/0 group-hover:bg-black/40 transition-colors flex items-center justify-center">
                                        <p className="text-white/0 group-hover:text-white text-xs font-medium transition-colors">Click to join session</p>
                                    </div>
                                </>
                            ) : (
                                <div className="text-center text-gray-600 group-hover:text-gray-500 transition-colors">
                                    <Monitor className="w-8 h-8 mx-auto mb-1.5 opacity-40 group-hover:opacity-60 transition-opacity" />
                                    <p className="text-[11px]">Desktop preview</p>
                                    <p className="text-[10px] text-gray-700 group-hover:text-[#e05246] transition-colors">Click to join session</p>
                                </div>
                            )}
                        </div>
                        <div className="flex items-center justify-between text-[10px] text-gray-600">
                            <span>Last updated: {thumbnailUrl ? "Recently" : "—"}</span>
                            <button
                                onClick={() => {
                                    const token = localStorage.getItem("sc_access_token");
                                    if (token) api.setToken(token);
                                    api.getAgentThumbnail(agent.id)
                                        .then(r => setThumbnailUrl(r.url))
                                        .catch(() => setThumbnailUrl(null));
                                    info("Refreshing", "Desktop preview and guest info updated");
                                }}
                                className="text-[#e05246] hover:text-[#f06b60]"
                            >Update Guest Info</button>
                        </div>
                    </div>
                )}

                {/* ─── General Tab ──────────────────────────────── */}
                {activeTab === "general" && (
                    <div className="p-4 space-y-0">
                        {/* Screenshot */}
                        <div className="mb-4 border border-[#333] rounded-lg bg-[#141414] aspect-video flex items-center justify-center overflow-hidden">
                            {thumbnailUrl ? (
                                <img src={thumbnailUrl} alt="Desktop preview" className="w-full h-full object-cover" onError={() => setThumbnailUrl(null)} />
                            ) : (
                                <div className="text-center text-gray-600">
                                    <Monitor className="w-10 h-10 mx-auto mb-2 opacity-30" />
                                    <p className="text-[11px]">Guest screenshot</p>
                                </div>
                            )}
                        </div>

                        <InfoRow label="Name" value={agent.machine_name} />
                        <InfoRow label="Organization" value={agent.group_name || "—"} icon={<Globe className="w-3.5 h-3.5 text-gray-600" />} />
                        <InfoRow label="Hosts Connected" value={agentSessions.filter(s => s.status === "active").length || "—"} />
                        <InfoRow label="Guests Connected" value={
                            agent.status !== "offline"
                                ? <span className="text-emerald-400">Guest ({idleTime})</span>
                                : "—"
                        } />
                        <InfoRow label="Guest Last Connected" value={idleTime} />
                        <InfoRow label="Logged On User" value={agent.logged_in_user || "—"} icon={<User className="w-3.5 h-3.5 text-gray-600" />} />
                        <InfoRow label="Idle Time" value={idleTime} icon={<Clock className="w-3.5 h-3.5 text-gray-600" />} />
                        <InfoRow label="Machine" value={agent.machine_name} icon={<Monitor className="w-3.5 h-3.5 text-gray-600" />} />
                        <InfoRow label="Operating System" value={`${agent.os} ${agent.os_version}`} icon={<Globe className="w-3.5 h-3.5 text-gray-600" />} />
                        <InfoRow label="Processor(s)" value={agent.cpu_model || agent.arch || "—"} icon={<Cpu className="w-3.5 h-3.5 text-gray-600" />} />
                        <InfoRow
                            label="CPU Usage"
                            value={agent.cpu_usage != null ? `${agent.cpu_usage.toFixed(1)}%` : "—"}
                            icon={<Activity className="w-3.5 h-3.5 text-gray-600" />}
                        />
                        <InfoRow
                            label="Memory"
                            value={agent.memory_used != null && agent.memory_total != null
                                ? `${formatBytes(agent.memory_used)} / ${formatBytes(agent.memory_total)} (${((agent.memory_used / agent.memory_total) * 100).toFixed(0)}%)`
                                : "—"}
                            icon={<HardDrive className="w-3.5 h-3.5 text-gray-600" />}
                        />
                        <InfoRow
                            label="Disk Usage"
                            value={agent.disk_used != null && agent.disk_total != null
                                ? `${formatBytes(agent.disk_used)} / ${formatBytes(agent.disk_total)} (${((agent.disk_used / agent.disk_total) * 100).toFixed(0)}%)`
                                : "—"}
                            icon={<HardDrive className="w-3.5 h-3.5 text-gray-600" />}
                        />
                        <InfoRow
                            label="Uptime"
                            value={agent.uptime_secs != null ? formatUptime(agent.uptime_secs) : "—"}
                            icon={<Clock className="w-3.5 h-3.5 text-gray-600" />}
                        />
                        <InfoRow label="Network Address" value={agent.ip_address || "—"} icon={<Wifi className="w-3.5 h-3.5 text-gray-600" />} />
                        <InfoRow label="Client Version" value={`v${agent.agent_version}`} icon={<HardDrive className="w-3.5 h-3.5 text-gray-600" />} />
                        <InfoRow
                            label="Registered"
                            value={new Date(agent.created_at).toLocaleDateString()}
                        />
                    </div>
                )}

                {/* ─── Timeline Tab ─────────────────────────────── */}
                {activeTab === "timeline" && (
                    <div className="p-4">
                        <div className="relative">
                            {/* Vertical line */}
                            <div className="absolute left-[7px] top-3 bottom-3 w-px bg-[#333]" />
                            <div className="space-y-4">
                                {agent.status === "online" && (
                                    <TimelineEntry time="Now" event="Agent online" type="success" />
                                )}
                                {agentSessions
                                    .sort((a, b) => new Date(b.started_at).getTime() - new Date(a.started_at).getTime())
                                    .slice(0, 8)
                                    .map((s) => {
                                        const label = s.session_type === "desktop" ? "Desktop session" : s.session_type === "terminal" ? "Terminal session" : "File transfer";
                                        const mins = Math.floor((Date.now() - new Date(s.started_at).getTime()) / 60000);
                                        const ago = mins < 1 ? "Just now" : mins < 60 ? `${mins}m ago` : mins < 1440 ? `${Math.floor(mins / 60)}h ago` : `${Math.floor(mins / 1440)}d ago`;
                                        return (
                                            <TimelineEntry
                                                key={s.id}
                                                time={ago}
                                                event={`${label} ${s.status === "active" ? "started" : s.status === "ended" ? "ended" : "pending"}`}
                                                type={s.status === "active" ? "info" : s.status === "ended" ? "neutral" : "success"}
                                            />
                                        );
                                    })}
                                <TimelineEntry
                                    time={agent.last_seen ? new Date(agent.last_seen).toLocaleTimeString() : "—"}
                                    event="Agent connected"
                                    type="success"
                                />
                                <TimelineEntry
                                    time={new Date(agent.created_at).toLocaleDateString()}
                                    event="Agent registered"
                                    type="neutral"
                                />
                            </div>
                        </div>
                    </div>
                )}

                {/* ─── Messages Tab ─────────────────────────────── */}
                {activeTab === "messages" && (
                    <div className="flex flex-col h-full">
                        <div className="flex-1 overflow-y-auto p-3 space-y-2">
                            {messages.length === 0 && (
                                <div className="flex flex-col items-center justify-center h-full text-gray-600 p-4">
                                    <MessageSquare className="w-8 h-8 mb-2 opacity-40" />
                                    <p className="text-xs text-center">No messages yet. Send a message to<br />the remote user on this machine.</p>
                                </div>
                            )}
                            {messages.map((msg) => (
                                <div
                                    key={msg.id}
                                    className={`max-w-[85%] px-3 py-2 rounded-lg text-xs ${msg.sender === "host"
                                            ? "ml-auto bg-[#e05246]/20 text-gray-200 rounded-br-sm"
                                            : msg.sender === "agent-user"
                                                ? "bg-[#1e3a5f] text-blue-200 rounded-bl-sm"
                                                : "bg-[#252525] text-gray-400 italic rounded-bl-sm"
                                        }`}
                                >
                                    {msg.sender === "agent-user" && msg.senderName && (
                                        <p className="text-[9px] font-semibold text-blue-400 mb-0.5">{msg.senderName}</p>
                                    )}
                                    <p>{msg.text}</p>
                                    <p className="text-[9px] mt-1 opacity-50">
                                        {msg.timestamp.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}
                                    </p>
                                </div>
                            ))}
                            <div ref={messagesEndRef} />
                        </div>
                        <div className="p-3 border-t border-[#333] flex gap-2">
                            <input
                                type="text"
                                value={msgText}
                                onChange={(e) => setMsgText(e.target.value)}
                                onKeyDown={(e) => e.key === "Enter" && handleSendMessage()}
                                placeholder="Send a message..."
                                className="flex-1 px-3 py-1.5 bg-[#141414] border border-[#333] rounded-lg text-xs text-gray-300 placeholder-gray-600 focus:outline-none focus:border-[#e05246] transition-colors"
                            />
                            <button
                                onClick={handleSendMessage}
                                disabled={!msgText.trim()}
                                className="px-3 py-1.5 bg-[#e05246] hover:bg-[#c43d32] text-white text-xs rounded-lg transition-colors disabled:opacity-40"
                            >
                                <Send className="w-3.5 h-3.5" />
                            </button>
                        </div>
                    </div>
                )}

                {/* ─── Commands Tab ──────────────────────────────── */}
                {activeTab === "commands" && (
                    <div className="flex flex-col h-full">
                        <div className="p-3 border-b border-[#333]">
                            <div className="flex gap-2">
                                <div className="flex-1 flex items-center bg-[#141414] border border-[#333] rounded-lg overflow-hidden focus-within:border-[#e05246] transition-colors">
                                    <span className="px-2 text-[#e05246]"><ChevronRight className="w-3.5 h-3.5" /></span>
                                    <input
                                        type="text"
                                        placeholder="Enter command..."
                                        value={commandText}
                                        onChange={(e) => setCommandText(e.target.value)}
                                        onKeyDown={(e) => e.key === "Enter" && handleRunCommand()}
                                        className="flex-1 py-1.5 pr-2 bg-transparent text-xs text-gray-300 placeholder-gray-600 focus:outline-none font-mono"
                                    />
                                </div>
                                <button
                                    onClick={handleRunCommand}
                                    disabled={!commandText.trim() || agent.status === "offline"}
                                    className="px-3 py-1.5 bg-[#e05246] hover:bg-[#c43d32] text-white text-xs rounded-lg transition-colors disabled:opacity-40 font-medium"
                                >
                                    Run
                                </button>
                            </div>
                            {agent.status === "offline" && (
                                <p className="text-[10px] text-amber-500/80 mt-1.5">⚠ Agent is offline — commands unavailable</p>
                            )}
                        </div>
                        <div className="flex-1 overflow-y-auto">
                            {commands.length === 0 ? (
                                <div className="flex flex-col items-center justify-center h-full text-gray-600 p-4">
                                    <Terminal className="w-8 h-8 mb-2 opacity-20" />
                                    <p className="text-[11px]">No command history</p>
                                    <p className="text-[10px] text-gray-700 mt-0.5">Try typing <code className="text-[#e05246] bg-[#e05246]/10 px-1 rounded">whoami</code></p>
                                </div>
                            ) : (
                                <div className="divide-y divide-[#252525]">
                                    {commands.map((cmd) => (
                                        <div key={cmd.id} className="p-3">
                                            <div className="flex items-center gap-2 mb-1.5">
                                                <span className="text-[#e05246] text-[10px] font-mono">$</span>
                                                <code className="text-xs text-gray-200 font-mono flex-1 truncate">{cmd.command}</code>
                                                <button
                                                    onClick={() => handleCopyCommand(cmd.command, cmd.id)}
                                                    className="text-gray-600 hover:text-gray-400 transition-colors"
                                                    title="Copy command"
                                                >
                                                    {cmdCopied === cmd.id ? <Check className="w-3 h-3 text-emerald-400" /> : <Copy className="w-3 h-3" />}
                                                </button>
                                            </div>
                                            <div className="bg-[#141414] rounded-md px-3 py-2 font-mono text-[11px]">
                                                {cmd.status === "running" ? (
                                                    <span className="text-gray-500 flex items-center gap-1.5">
                                                        <Loader2 className="w-3 h-3 animate-spin" />
                                                        Running...
                                                    </span>
                                                ) : (
                                                    <span className={cmd.status === "error" ? "text-red-400" : "text-gray-400"}>
                                                        {cmd.output}
                                                    </span>
                                                )}
                                            </div>
                                            <p className="text-[9px] text-gray-700 mt-1">
                                                {cmd.timestamp.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" })}
                                            </p>
                                        </div>
                                    ))}
                                </div>
                            )}
                        </div>
                    </div>
                )}

                {/* ─── Notes Tab ─────────────────────────────────── */}
                {activeTab === "notes" && (
                    <div className="flex flex-col h-full">
                        <div className="flex-1 overflow-y-auto p-3">
                            {notes.length === 0 ? (
                                <div className="flex flex-col items-center justify-center h-full text-gray-600">
                                    <StickyNote className="w-8 h-8 mb-2 opacity-20" />
                                    <p className="text-[11px]">No notes yet</p>
                                    <p className="text-[10px] text-gray-700 mt-0.5">Add notes to keep track of this machine</p>
                                </div>
                            ) : (
                                <div className="space-y-2">
                                    {notes.map((note) => (
                                        <div key={note.id} className="group p-3 bg-[#252525] rounded-lg border border-[#333] hover:border-[#444] transition-colors">
                                            <p className="text-xs text-gray-300 whitespace-pre-wrap leading-relaxed">{note.text}</p>
                                            <div className="flex items-center justify-between mt-2">
                                                <p className="text-[9px] text-gray-600">
                                                    {note.timestamp.toLocaleDateString()} {note.timestamp.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}
                                                </p>
                                                <button
                                                    onClick={() => handleDeleteNote(note.id)}
                                                    className="opacity-0 group-hover:opacity-100 text-gray-600 hover:text-red-400 transition-all"
                                                    title="Delete note"
                                                >
                                                    <Trash2 className="w-3 h-3" />
                                                </button>
                                            </div>
                                        </div>
                                    ))}
                                </div>
                            )}
                        </div>
                        <div className="p-3 border-t border-[#333]">
                            <textarea
                                placeholder="Add a note..."
                                value={noteText}
                                onChange={(e) => setNoteText(e.target.value)}
                                onKeyDown={(e) => {
                                    if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) handleAddNote();
                                }}
                                className="w-full px-3 py-2 bg-[#141414] border border-[#333] rounded-lg text-xs text-gray-300 placeholder-gray-600 focus:outline-none focus:border-[#e05246] resize-none h-16 transition-colors"
                            />
                            <div className="flex items-center justify-between mt-2">
                                <span className="text-[9px] text-gray-700">Ctrl+Enter to save</span>
                                <button
                                    onClick={handleAddNote}
                                    disabled={!noteText.trim()}
                                    className="px-3 py-1.5 bg-[#e05246] hover:bg-[#c43d32] text-white text-xs rounded-lg transition-colors disabled:opacity-40 font-medium"
                                >
                                    Add Note
                                </button>
                            </div>
                        </div>
                    </div>
                )}

                {/* ─── Toolbox Tab ──────────────────────────────── */}
                {activeTab === "toolbox" && (
                    <div className="p-4 space-y-2">
                        <p className="text-[10px] text-gray-600 uppercase tracking-wider mb-3">Script Library</p>
                        {toolboxLoading ? (
                            <div className="flex items-center justify-center py-8 text-gray-500 text-sm gap-2">
                                <Loader2 className="w-4 h-4 animate-spin" />
                                Loading scripts...
                            </div>
                        ) : toolboxScripts.map((script) => (
                            <div
                                key={script.id}
                                className="p-3 bg-[#252525] border border-[#333] rounded-lg hover:border-[#444] transition-colors group"
                            >
                                <div className="flex items-center justify-between mb-1">
                                    <div className="flex items-center gap-2">
                                        <span className="text-xs font-medium text-gray-200">{script.name}</span>
                                        {script.language && script.language !== 'bash' && (
                                            <span className="px-1.5 py-0.5 text-[9px] bg-[#e05246]/15 text-[#e05246] rounded font-mono">{script.language}</span>
                                        )}
                                    </div>
                                    <div className="flex gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
                                        <button
                                            onClick={() => {
                                                navigator.clipboard.writeText(script.code);
                                                info("Copied", `Command copied to clipboard`);
                                            }}
                                            className="p-1 text-gray-500 hover:text-white rounded transition-colors"
                                            title="Copy command"
                                        >
                                            <Copy className="w-3 h-3" />
                                        </button>
                                        <button
                                            onClick={() => {
                                                // Inject into commands tab
                                                setCommandText(script.code);
                                                setActiveTab("commands");
                                            }}
                                            className="p-1 text-gray-500 hover:text-emerald-400 rounded transition-colors"
                                            title="Run on remote"
                                        >
                                            <Play className="w-3 h-3" />
                                        </button>
                                    </div>
                                </div>
                                {script.description && <p className="text-[10px] text-gray-500">{script.description}</p>}
                                <code className="block mt-1.5 text-[10px] text-[#e05246] bg-[#1a1a1a] px-2 py-1 rounded font-mono truncate">{script.code}</code>
                            </div>
                        ))}
                    </div>
                )}

                {/* ─── Diagnostics Tab ─────────────────────────── */}
                {activeTab === "diagnostics" && (
                    <div className="p-4 space-y-4">
                        {/* Header with Refresh */}
                        <div className="flex items-center justify-between">
                            <p className="text-[10px] text-gray-600 uppercase tracking-wider">
                                Live Diagnostics {diagLoading && <Loader2 className="inline w-3 h-3 ml-1 animate-spin" />}
                            </p>
                            <button
                                onClick={runDiagnostics}
                                disabled={agentWsStatus !== 'connected' || diagLoading}
                                className="text-[10px] text-[#e05246] hover:text-[#ff6b5e] disabled:opacity-30 disabled:cursor-not-allowed transition-colors"
                            >
                                ↻ Refresh
                            </button>
                        </div>

                        {agentWsStatus !== 'connected' && !diagLoading && (
                            <p className="text-[10px] text-gray-600 text-center py-6">Agent offline — connect to view diagnostics</p>
                        )}

                        {/* Processes */}
                        {diagProcesses.length > 0 && (
                            <div>
                                <p className="text-[10px] text-gray-600 uppercase tracking-wider mb-2 flex items-center gap-1">
                                    <Cpu className="w-3 h-3" /> Processes ({diagProcesses.length})
                                </p>
                                <div className="bg-[#141414] border border-[#333] rounded-lg overflow-hidden">
                                    <div className="grid grid-cols-[auto_1fr_auto_auto] gap-2 px-3 py-1.5 text-[9px] text-gray-600 uppercase border-b border-[#333]">
                                        <span>PID</span><span>Name</span><span>CPU%</span><span>MEM%</span>
                                    </div>
                                    {diagProcesses.map((p) => (
                                        <div key={p.pid} className="grid grid-cols-[auto_1fr_auto_auto] gap-2 px-3 py-1.5 text-[10px] border-b border-[#222] last:border-0 hover:bg-white/[0.02]">
                                            <span className="text-gray-600 font-mono w-10">{p.pid}</span>
                                            <span className="text-gray-300 truncate">{p.name}</span>
                                            <span className={`font-mono w-10 text-right ${p.cpu > 5 ? "text-amber-400" : "text-gray-500"}`}>{p.cpu.toFixed(1)}</span>
                                            <span className="text-gray-500 font-mono w-10 text-right">{p.mem.toFixed(1)}</span>
                                        </div>
                                    ))}
                                </div>
                            </div>
                        )}

                        {/* Services */}
                        {diagServices.length > 0 && (
                            <div>
                                <p className="text-[10px] text-gray-600 uppercase tracking-wider mb-2 flex items-center gap-1">
                                    <HardDrive className="w-3 h-3" /> Services ({diagServices.length})
                                </p>
                                <div className="space-y-1">
                                    {diagServices.map((s) => (
                                        <div key={s.name} className="flex items-center gap-2 px-3 py-2 bg-[#252525] rounded-lg border border-[#333] hover:border-[#444] transition-colors">
                                            <span className={`w-1.5 h-1.5 rounded-full flex-shrink-0 ${s.status === "active" ? "bg-emerald-400" : "bg-gray-600"}`} />
                                            <div className="flex-1 min-w-0">
                                                <p className="text-[11px] text-gray-300 truncate">{s.name}</p>
                                                <p className="text-[9px] text-gray-600">{s.desc}</p>
                                            </div>
                                            <span className={`text-[9px] px-1.5 py-0.5 rounded font-medium ${s.status === "active" ? "bg-emerald-500/10 text-emerald-400" : "bg-gray-500/10 text-gray-500"
                                                }`}>{s.status}</span>
                                        </div>
                                    ))}
                                </div>
                            </div>
                        )}

                        {/* Event Log */}
                        {diagEvents.length > 0 && (
                            <div>
                                <p className="text-[10px] text-gray-600 uppercase tracking-wider mb-2 flex items-center gap-1">
                                    <AlertTriangle className="w-3 h-3" /> Event Log
                                </p>
                                <div className="bg-[#141414] border border-[#333] rounded-lg overflow-hidden">
                                    {diagEvents.map((ev) => (
                                        <div key={ev.id} className="flex items-start gap-2 px-3 py-2 border-b border-[#222] last:border-0 hover:bg-white/[0.02]">
                                            {ev.level === "error" ? <AlertCircle className="w-3 h-3 text-red-400 flex-shrink-0 mt-0.5" />
                                                : ev.level === "warning" ? <AlertTriangle className="w-3 h-3 text-amber-400 flex-shrink-0 mt-0.5" />
                                                    : <InfoIcon className="w-3 h-3 text-blue-400 flex-shrink-0 mt-0.5" />}
                                            <div className="flex-1 min-w-0">
                                                <p className="text-[10px] text-gray-300">{ev.message}</p>
                                                <p className="text-[9px] text-gray-600 mt-0.5">{ev.source} • {ev.time}</p>
                                            </div>
                                        </div>
                                    ))}
                                </div>
                            </div>
                        )}
                    </div>
                )}
            </div>
        </div>
    );
}

// ─── Sub-components ──────────────────────────────────────

function InfoRow({ label, value, icon }: { label: string; value: React.ReactNode; icon?: React.ReactNode }) {
    return (
        <div className="flex items-center py-2.5 px-1 border-b border-[#252525] last:border-0">
            {icon && <span className="mr-2 flex-shrink-0">{icon}</span>}
            <span className="text-[11px] text-gray-500 flex-shrink-0 w-28">{label}</span>
            <span className="text-[11px] text-gray-300 truncate ml-auto text-right">{value}</span>
        </div>
    );
}

function TimelineEntry({
    time, event, type,
}: { time: string; event: string; type: "info" | "success" | "neutral" }) {
    const dotColor = type === "success" ? "bg-emerald-400" : type === "info" ? "bg-[#e05246]" : "bg-gray-600";
    return (
        <div className="flex items-start gap-3 relative pl-1">
            <span className={`w-[14px] h-[14px] rounded-full ${dotColor} flex-shrink-0 ring-2 ring-[#1e1e1e] z-10`} />
            <div className="min-w-0 -mt-0.5">
                <p className="text-xs text-gray-300 font-medium">{event}</p>
                <p className="text-[10px] text-gray-600 mt-0.5">{time}</p>
            </div>
        </div>
    );
}
