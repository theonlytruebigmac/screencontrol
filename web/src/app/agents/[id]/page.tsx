'use client';
import { getAccessToken } from '@/lib/auth-store';

/**
 * Agent Detail page — real session history + audit events,
 * system info from agent record, quick actions.
 */

import { useState, useEffect, useCallback } from 'react';
import { useParams, useRouter } from 'next/navigation';
import {
    Monitor,
    Cpu,
    HardDrive,
    MemoryStick,
    Network,
    Globe,
    Clock,
    Activity,
    Terminal,
    FolderOpen,
    ChevronLeft,
    RefreshCw,
    Power,
    Shield,
    Wifi,
    WifiOff,
    User,
    Zap,
    Server,
    Play,
    CheckCircle,
    XCircle,
    AlertTriangle,
    Info,
    Download,
    Loader2,
    Tag,
    Plus,
    X,
    Save,
    MessageSquare,
} from 'lucide-react';
import { useToast } from '@/components/toast';
import { api, type Agent, type Session, type AuditEntry } from '@/lib/api';
import { launchDesktopSession, launchTerminalSession } from '@/lib/session-launcher';

// ─── Stat chip ───────────────────────────────────
function Chip({ icon: Icon, label, value }: { icon: typeof Cpu; label: string; value: string }) {
    return (
        <div className="bg-[#141414] border border-[#2a2a2a] rounded-lg p-3">
            <div className="flex items-center gap-1.5 mb-1">
                <Icon className="w-3 h-3 text-gray-500" />
                <span className="text-[9px] text-gray-600 uppercase tracking-wider">{label}</span>
            </div>
            <p className="text-xs text-white font-medium">{value}</p>
        </div>
    );
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

const EVENT_ICON: Record<string, typeof Info> = {
    'session.create': Monitor,
    'session.end': XCircle,
    'user.login': User,
    'agent.register': Server,
    'user.create': User,
    'user.delete': Shield,
};

function timeAgo(dateStr: string | null): string {
    if (!dateStr) return 'Never';
    const diff = Date.now() - new Date(dateStr).getTime();
    const mins = Math.floor(diff / 60000);
    if (mins < 1) return 'Just now';
    if (mins < 60) return `${mins}m ago`;
    const hours = Math.floor(mins / 60);
    if (hours < 24) return `${hours}h ago`;
    const days = Math.floor(hours / 24);
    if (days < 30) return `${days}d ago`;
    return `${Math.floor(days / 30)}mo ago`;
}

function durationStr(started: string, ended: string | null): string {
    const s = new Date(started).getTime();
    const e = ended ? new Date(ended).getTime() : Date.now();
    const diff = e - s;
    const mins = Math.round(diff / 60000);
    if (mins < 1) return '< 1 min';
    if (mins < 60) return `${mins} min`;
    const hrs = Math.floor(mins / 60);
    const remMins = mins % 60;
    return `${hrs}h ${remMins}m`;
}

export default function AgentDetailPage() {
    const { id } = useParams<{ id: string }>();
    const router = useRouter();
    const { info, success } = useToast();
    const [agent, setAgent] = useState<Agent | null>(null);
    const [sessions, setSessions] = useState<Session[]>([]);
    const [events, setEvents] = useState<AuditEntry[]>([]);
    const [loading, setLoading] = useState(true);
    const [editNotes, setEditNotes] = useState('');
    const [newTag, setNewTag] = useState('');
    const [savingNotes, setSavingNotes] = useState(false);

    useEffect(() => {
        const token = getAccessToken();
        if (token) api.setToken(token);

        // Fetch agent
        api.getAgents().then(agents => {
            const found = agents.find(a => a.id === id);
            if (found) setAgent(found);
            else setAgent({ id: id || 'unknown', machine_name: 'Unknown', os: 'linux', os_version: '', arch: '', agent_version: '', status: 'offline', last_seen: null, created_at: new Date().toISOString(), tags: [], admin_notes: '', cpu_usage: null, memory_used: null, memory_total: null, disk_used: null, disk_total: null, uptime_secs: null, ip_address: null, logged_in_user: null, cpu_model: null, group_name: null });
        }).catch(() => {
            setAgent({ id: id || 'unknown', machine_name: 'Unknown', os: 'linux', os_version: '', arch: '', agent_version: '', status: 'offline', last_seen: null, created_at: new Date().toISOString(), tags: [], admin_notes: '', cpu_usage: null, memory_used: null, memory_total: null, disk_used: null, disk_total: null, uptime_secs: null, ip_address: null, logged_in_user: null, cpu_model: null, group_name: null });
        });

        // Fetch sessions for this agent
        api.getSessions().then(all => {
            const agentSessions = all.filter(s => s.agent_id === id).sort((a, b) => new Date(b.started_at).getTime() - new Date(a.started_at).getTime());
            setSessions(agentSessions);
        }).catch(() => { });

        // Fetch recent audit events (filtered by agent target)
        api.getAuditLog({ limit: 20 }).then(all => {
            const agentEvents = all.filter(e => e.target_id === id || (e.metadata as Record<string, unknown>)?.agent_id === id);
            setEvents(agentEvents.length > 0 ? agentEvents : all.slice(0, 8));
        }).catch(() => { });

        setLoading(false);
    }, [id]);

    // Sync notes when agent loads
    useEffect(() => {
        if (agent) setEditNotes(agent.admin_notes || '');
    }, [agent]);

    const handleSaveNotes = useCallback(async () => {
        if (!agent) return;
        setSavingNotes(true);
        try {
            const token = getAccessToken();
            if (token) api.setToken(token);
            const updated = await api.updateAgent(agent.id, { admin_notes: editNotes });
            setAgent(updated);
            success('Notes Saved', 'Admin notes have been updated');
        } catch {
            info('Error', 'Failed to save notes');
        }
        setSavingNotes(false);
    }, [agent, editNotes, success, info]);

    const handleAddTag = useCallback(async () => {
        if (!agent || !newTag.trim()) return;
        const updated = [...(agent.tags || []), newTag.trim()];
        try {
            const token = getAccessToken();
            if (token) api.setToken(token);
            const result = await api.updateAgent(agent.id, { tags: updated });
            setAgent(result);
            setNewTag('');
            success('Tag Added', `"${newTag.trim()}" has been added`);
        } catch {
            info('Error', 'Failed to add tag');
        }
    }, [agent, newTag, success, info]);

    const handleRemoveTag = useCallback(async (tagToRemove: string) => {
        if (!agent) return;
        const updated = (agent.tags || []).filter(t => t !== tagToRemove);
        try {
            const token = getAccessToken();
            if (token) api.setToken(token);
            const result = await api.updateAgent(agent.id, { tags: updated });
            setAgent(result);
        } catch {
            info('Error', 'Failed to remove tag');
        }
    }, [agent, info]);

    if (!agent || loading) return (
        <div className="flex items-center justify-center h-full">
            <Loader2 className="w-6 h-6 animate-spin text-gray-600" />
        </div>
    );

    const isOnline = agent.status !== 'offline';

    const handleConnect = async (sessionType: string) => {
        try {
            const token = getAccessToken();
            if (token) api.setToken(token);
            const session = await api.createSession(agent.id, sessionType);
            if (sessionType === 'desktop') launchDesktopSession(session.id);
            else if (sessionType === 'terminal') launchTerminalSession(session.id);
        } catch (e) {
            console.error(`Failed to create ${sessionType} session:`, e);
        }
    };

    return (
        <div className="flex flex-col h-full overflow-y-auto">
            {/* Header */}
            <header className="flex flex-wrap items-center justify-between gap-3 px-6 py-4 border-b border-[#333] flex-shrink-0">
                <div className="flex items-center gap-3">
                    <button onClick={() => router.push('/agents')} className="p-1.5 text-gray-400 hover:text-white rounded-lg hover:bg-white/5">
                        <ChevronLeft className="w-4 h-4" />
                    </button>
                    <div className="flex items-center gap-2">
                        <div className={`w-10 h-10 rounded-xl flex items-center justify-center ${isOnline ? 'bg-emerald-500/15' : 'bg-gray-500/15'}`}>
                            <Monitor className={`w-5 h-5 ${isOnline ? 'text-emerald-400' : 'text-gray-500'}`} />
                        </div>
                        <div>
                            <h1 className="text-lg font-bold text-white">{agent.machine_name}</h1>
                            <div className="flex items-center gap-2 text-xs">
                                <div className={`flex items-center gap-1 ${isOnline ? 'text-emerald-400' : 'text-gray-500'}`}>
                                    {isOnline ? <Wifi className="w-3 h-3" /> : <WifiOff className="w-3 h-3" />}
                                    {isOnline ? 'Online' : 'Offline'}
                                </div>
                                <span className="text-gray-600">•</span>
                                <span className="text-gray-500">{agent.os} {agent.os_version}</span>
                                {agent.agent_version && (
                                    <>
                                        <span className="text-gray-600">•</span>
                                        <span className="text-gray-500">v{agent.agent_version}</span>
                                    </>
                                )}
                            </div>
                        </div>
                    </div>
                </div>
                <div className="flex items-center gap-2">
                    <button onClick={() => handleConnect('desktop')} className="flex items-center gap-1.5 px-3 py-2 text-xs font-medium text-white bg-[#e05246] hover:bg-[#c94539] rounded-lg transition-colors">
                        <Monitor className="w-3.5 h-3.5" /> Remote Desktop
                    </button>
                    <button onClick={() => handleConnect('terminal')} className="flex items-center gap-1.5 px-3 py-2 text-xs text-gray-300 hover:text-white hover:bg-white/5 rounded-lg border border-[#333] transition-colors">
                        <Terminal className="w-3.5 h-3.5" /> Terminal
                    </button>
                    <button onClick={() => info('Restarting', 'Agent restart command sent')} className="p-2 text-gray-400 hover:text-white hover:bg-white/5 rounded-lg border border-[#333] transition-colors" title="Restart Agent">
                        <RefreshCw className="w-3.5 h-3.5" />
                    </button>
                </div>
            </header>

            <div className="p-6 space-y-5">
                {/* System overview chips */}
                <div className="grid grid-cols-2 md:grid-cols-4 lg:grid-cols-6 gap-2">
                    <Chip icon={Server} label="OS" value={`${agent.os} ${agent.os_version}`} />
                    <Chip icon={Cpu} label="Architecture" value={agent.arch || 'N/A'} />
                    <Chip icon={Shield} label="Agent Version" value={agent.agent_version || 'N/A'} />
                    <Chip icon={Clock} label="Last Seen" value={timeAgo(agent.last_seen)} />
                    <Chip icon={Activity} label="Status" value={agent.status} />
                    <Chip icon={Monitor} label="Sessions" value={`${sessions.length} total`} />
                    <Chip icon={Cpu} label="CPU Usage" value={agent.cpu_usage != null ? `${agent.cpu_usage.toFixed(1)}%` : '—'} />
                    <Chip icon={MemoryStick} label="Memory" value={agent.memory_used != null && agent.memory_total != null ? `${formatBytes(agent.memory_used)} / ${formatBytes(agent.memory_total)}` : '—'} />
                    <Chip icon={HardDrive} label="Disk" value={agent.disk_used != null && agent.disk_total != null ? `${formatBytes(agent.disk_used)} / ${formatBytes(agent.disk_total)}` : '—'} />
                    <Chip icon={Clock} label="Uptime" value={agent.uptime_secs != null ? formatUptime(agent.uptime_secs) : '—'} />
                </div>

                <div className="grid grid-cols-1 lg:grid-cols-2 gap-5">
                    {/* Agent info */}
                    <div className="bg-[#1e1e1e] border border-[#2a2a2a] rounded-xl p-5">
                        <h2 className="text-sm font-semibold text-white mb-3 flex items-center gap-2">
                            <Globe className="w-4 h-4 text-[#e05246]" /> Agent Details
                        </h2>
                        <div className="space-y-2 text-xs">
                            {[
                                ['Machine Name', agent.machine_name],
                                ['Agent ID', agent.id],
                                ['OS', `${agent.os} ${agent.os_version}`],
                                ['Architecture', agent.arch || 'N/A'],
                                ['Agent Version', agent.agent_version || 'N/A'],
                                ['Created', new Date(agent.created_at).toLocaleString()],
                            ].map(([label, val]) => (
                                <div key={label} className="flex items-center justify-between py-1.5 border-b border-[#2a2a2a] last:border-0">
                                    <span className="text-gray-500">{label}</span>
                                    <span className="text-gray-300 font-mono text-[11px] max-w-[60%] truncate">{val}</span>
                                </div>
                            ))}
                        </div>
                    </div>

                    {/* Tags & Admin Notes */}
                    <div className="bg-[#1e1e1e] border border-[#2a2a2a] rounded-xl p-5">
                        <h2 className="text-sm font-semibold text-white mb-3 flex items-center gap-2">
                            <Tag className="w-4 h-4 text-[#e05246]" /> Tags & Notes
                        </h2>
                        {/* Tags */}
                        <div className="mb-4">
                            <label className="text-[10px] text-gray-500 uppercase tracking-wider block mb-2">Tags</label>
                            <div className="flex flex-wrap gap-1.5 mb-2">
                                {(agent.tags || []).map(tag => (
                                    <span key={tag} className="flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-medium bg-[#e05246]/10 text-[#e05246] border border-[#e05246]/20">
                                        {tag}
                                        <button onClick={() => handleRemoveTag(tag)} className="hover:text-white transition-colors">
                                            <X className="w-2.5 h-2.5" />
                                        </button>
                                    </span>
                                ))}
                                {(agent.tags || []).length === 0 && <span className="text-[11px] text-gray-600">No tags</span>}
                            </div>
                            <div className="flex gap-1.5">
                                <input
                                    value={newTag}
                                    onChange={e => setNewTag(e.target.value)}
                                    onKeyDown={e => e.key === 'Enter' && handleAddTag()}
                                    placeholder="Add tag..."
                                    className="flex-1 bg-[#141414] border border-[#333] rounded-lg px-3 py-1.5 text-xs text-gray-100 placeholder-gray-600 focus:border-[#e05246] focus:outline-none"
                                />
                                <button onClick={handleAddTag} disabled={!newTag.trim()} className="px-2 py-1.5 text-xs bg-[#252525] border border-[#333] rounded-lg hover:bg-[#333] text-gray-400 hover:text-white disabled:opacity-40 transition-colors">
                                    <Plus className="w-3 h-3" />
                                </button>
                            </div>
                        </div>
                        {/* Admin Notes */}
                        <div>
                            <label className="text-[10px] text-gray-500 uppercase tracking-wider block mb-2">Admin Notes</label>
                            <textarea
                                value={editNotes}
                                onChange={e => setEditNotes(e.target.value)}
                                rows={3}
                                placeholder="Add notes about this agent..."
                                className="w-full bg-[#141414] border border-[#333] rounded-lg px-3 py-2 text-xs text-gray-100 placeholder-gray-600 focus:border-[#e05246] focus:outline-none resize-none"
                            />
                            <button onClick={handleSaveNotes} disabled={savingNotes} className="mt-2 flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium bg-[#e05246] hover:bg-[#c94539] text-white rounded-lg disabled:opacity-50 transition-colors">
                                {savingNotes ? <Loader2 className="w-3 h-3 animate-spin" /> : <Save className="w-3 h-3" />}
                                Save Notes
                            </button>
                        </div>
                    </div>

                    {/* Event timeline (from audit log) */}
                    <div className="bg-[#1e1e1e] border border-[#2a2a2a] rounded-xl p-5">
                        <h2 className="text-sm font-semibold text-white mb-3 flex items-center gap-2">
                            <Clock className="w-4 h-4 text-[#e05246]" /> Event Timeline
                        </h2>
                        {events.length === 0 ? (
                            <p className="text-xs text-gray-600 py-4 text-center">No events recorded yet</p>
                        ) : (
                            <div className="space-y-0 relative">
                                <div className="absolute left-[11px] top-2 bottom-2 w-px bg-[#333]" />
                                {events.map(event => {
                                    const EvIcon = EVENT_ICON[event.action] || Info;
                                    const severity = event.action.includes('delete') || event.action.includes('error') ? 'error' :
                                        event.action.includes('end') ? 'warning' : 'info';
                                    return (
                                        <div key={event.id} className="flex items-start gap-3 py-2 relative">
                                            <div className={`w-6 h-6 rounded-full flex items-center justify-center flex-shrink-0 z-10 ${severity === 'error' ? 'bg-red-500/15 text-red-400' :
                                                severity === 'warning' ? 'bg-amber-500/15 text-amber-400' :
                                                    'bg-[#252525] text-gray-500'
                                                }`}>
                                                <EvIcon className="w-3 h-3" />
                                            </div>
                                            <div className="flex-1 min-w-0">
                                                <p className="text-[11px] text-gray-300">{event.action.replace(/\./g, ' › ')}</p>
                                                <span className="text-[9px] text-gray-600">{timeAgo(event.created_at)}</span>
                                            </div>
                                        </div>
                                    );
                                })}
                            </div>
                        )}
                    </div>
                </div>

                {/* Session history (from real API) */}
                <div className="bg-[#1e1e1e] border border-[#2a2a2a] rounded-xl p-5">
                    <h2 className="text-sm font-semibold text-white mb-3 flex items-center gap-2">
                        <Monitor className="w-4 h-4 text-[#e05246]" /> Session History
                    </h2>
                    {sessions.length === 0 ? (
                        <p className="text-xs text-gray-600 py-4 text-center">No sessions recorded yet</p>
                    ) : (
                        <div className="space-y-1">
                            {sessions.map(session => (
                                <div key={session.id} className="flex items-center gap-3 py-2.5 px-3 rounded-lg bg-[#141414] border border-[#252525]">
                                    <div className={`w-2 h-2 rounded-full flex-shrink-0 ${session.status === 'active' ? 'bg-emerald-400 animate-pulse' : 'bg-gray-600'}`} />
                                    <span className={`text-xs font-medium w-24 ${session.session_type === 'desktop' ? 'text-[#e05246]' : session.session_type === 'terminal' ? 'text-cyan-400' : 'text-amber-400'}`}>
                                        {session.session_type}
                                    </span>
                                    <span className="text-[10px] text-gray-500 flex-1">{timeAgo(session.started_at)}</span>
                                    <span className="text-[10px] text-gray-500">{durationStr(session.started_at, session.ended_at)}</span>
                                    <span className={`text-[9px] px-1.5 py-0.5 rounded font-medium ${session.status === 'active' ? 'bg-emerald-500/15 text-emerald-400' : 'bg-gray-500/10 text-gray-500'}`}>
                                        {session.status}
                                    </span>
                                </div>
                            ))}
                        </div>
                    )}
                </div>
            </div>
        </div>
    );
}
