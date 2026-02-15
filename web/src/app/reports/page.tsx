'use client';
import { getAccessToken } from '@/lib/auth-store';

/**
 * Reports & Analytics page.
 *
 * Session activity trends, agent performance, user stats,
 * all computed from real API data with CSV export.
 */

import { useState, useMemo, useCallback, useEffect } from 'react';
import {
    BarChart3,
    TrendingUp,
    TrendingDown,
    Download,
    CalendarDays,
    Users,
    Monitor,
    Clock,
    Activity,
    ArrowUpRight,
    ArrowDownRight,
    FileText,
    Filter,
    RefreshCw,
    Loader2,
} from 'lucide-react';
import { useToast } from '@/components/toast';
import { api, type Agent, type Session } from '@/lib/api';

// ─── Types ───────────────────────────────────────
type Range = '7d' | '30d' | '90d';

interface DayStat {
    date: string;
    desktop: number;
    terminal: number;
    fileTransfer: number;
}

function timeAgo(dateStr: string | null): string {
    if (!dateStr) return 'Never';
    const diff = Date.now() - new Date(dateStr).getTime();
    const mins = Math.floor(diff / 60000);
    if (mins < 1) return 'Just now';
    if (mins < 60) return `${mins}m ago`;
    const hours = Math.floor(mins / 60);
    if (hours < 24) return `${hours}h ago`;
    const days = Math.floor(hours / 24);
    return `${days}d ago`;
}

// ─── Compute daily stats from real sessions ──────
function computeDailyStats(sessions: Session[], days: number): DayStat[] {
    const result: DayStat[] = [];
    const now = new Date();
    const cutoff = new Date(now);
    cutoff.setDate(cutoff.getDate() - days);

    for (let i = days - 1; i >= 0; i--) {
        const d = new Date(now);
        d.setDate(d.getDate() - i);
        const dateKey = d.toISOString().slice(0, 10);
        const label = d.toLocaleDateString('en-US', { month: 'short', day: 'numeric' });

        const daySessions = sessions.filter(s => s.started_at.slice(0, 10) === dateKey);
        result.push({
            date: label,
            desktop: daySessions.filter(s => s.session_type === 'desktop').length,
            terminal: daySessions.filter(s => s.session_type === 'terminal').length,
            fileTransfer: daySessions.filter(s => s.session_type === 'file_transfer').length,
        });
    }
    return result;
}

// ─── MiniBar Chart (CSS-only) ────────────────────
function MiniBarChart({ data, maxH = 80 }: { data: DayStat[]; maxH?: number }) {
    const maxVal = Math.max(...data.map(d => d.desktop + d.terminal + d.fileTransfer), 1);
    const visible = data.length > 30 ? data.filter((_, i) => i % 3 === 0) : data;

    return (
        <div className="flex items-end gap-[2px] h-full" style={{ height: maxH }}>
            {visible.map((d, i) => {
                const dH = (d.desktop / maxVal) * maxH;
                const tH = (d.terminal / maxVal) * maxH;
                const fH = (d.fileTransfer / maxVal) * maxH;
                return (
                    <div key={i} className="flex flex-col-reverse flex-1 min-w-[3px] group relative">
                        <div className="rounded-t-sm bg-[#e05246]" style={{ height: dH }} />
                        <div className="bg-blue-500" style={{ height: tH }} />
                        <div className="rounded-t-sm bg-emerald-500" style={{ height: fH }} />
                        <div className="absolute bottom-full left-1/2 -translate-x-1/2 mb-1 hidden group-hover:block z-10">
                            <div className="bg-[#1a1a1a] border border-[#444] rounded-lg px-2 py-1 text-[9px] text-gray-300 whitespace-nowrap shadow-lg">
                                <div className="font-medium text-white mb-0.5">{d.date}</div>
                                <div className="flex items-center gap-1"><span className="w-1.5 h-1.5 rounded-full bg-[#e05246]" />Desktop: {d.desktop}</div>
                                <div className="flex items-center gap-1"><span className="w-1.5 h-1.5 rounded-full bg-blue-500" />Terminal: {d.terminal}</div>
                                <div className="flex items-center gap-1"><span className="w-1.5 h-1.5 rounded-full bg-emerald-500" />Files: {d.fileTransfer}</div>
                            </div>
                        </div>
                    </div>
                );
            })}
        </div>
    );
}

// ─── Stat Card ───────────────────────────────────
function StatCard({
    label, value, subtext, icon: Icon,
}: {
    label: string; value: string; subtext: string; icon: React.ComponentType<{ className?: string; style?: React.CSSProperties }>;
}) {
    return (
        <div className="bg-[#1e1e1e] border border-[#2a2a2a] rounded-xl p-4 hover:border-[#444] transition-colors">
            <div className="flex items-center justify-between mb-2">
                <span className="text-[10px] text-gray-500 uppercase tracking-wider">{label}</span>
                <Icon className="w-4 h-4 text-gray-600" />
            </div>
            <div className="text-2xl font-bold text-white">{value}</div>
            <div className="text-[11px] text-gray-500 mt-1">{subtext}</div>
        </div>
    );
}

// ─── Main Component ──────────────────────────────
export default function ReportsPage() {
    const { success, info } = useToast();
    const [range, setRange] = useState<Range>('30d');
    const [loading, setLoading] = useState(true);
    const [agents, setAgents] = useState<Agent[]>([]);
    const [sessions, setSessions] = useState<Session[]>([]);

    const fetchData = useCallback(async () => {
        setLoading(true);
        try {
            const token = getAccessToken();
            if (token) api.setToken(token);
            const [a, s] = await Promise.all([api.getAgents(), api.getSessions()]);
            setAgents(a);
            setSessions(s);
        } catch (e) {
            console.error('Failed to load report data:', e);
        } finally {
            setLoading(false);
        }
    }, []);

    useEffect(() => { fetchData(); }, [fetchData]);

    const days = range === '7d' ? 7 : range === '30d' ? 30 : 90;
    const dailyStats = useMemo(() => computeDailyStats(sessions, days), [sessions, days]);

    const totals = useMemo(() => {
        const total = dailyStats.reduce((acc, d) => ({
            desktop: acc.desktop + d.desktop,
            terminal: acc.terminal + d.terminal,
            file: acc.file + d.fileTransfer,
        }), { desktop: 0, terminal: 0, file: 0 });
        return { ...total, all: total.desktop + total.terminal + total.file };
    }, [dailyStats]);

    // Compute top agents from real session data
    const topAgents = useMemo(() => {
        const agentMap = new Map<string, { name: string; os: string; sessions: number; totalMs: number }>();
        sessions.forEach(s => {
            const agent = agents.find(a => a.id === s.agent_id);
            if (!agent) return;
            const entry = agentMap.get(agent.id) || { name: agent.machine_name, os: agent.os, sessions: 0, totalMs: 0 };
            entry.sessions++;
            const start = new Date(s.started_at).getTime();
            const end = s.ended_at ? new Date(s.ended_at).getTime() : Date.now();
            entry.totalMs += (end - start);
            agentMap.set(agent.id, entry);
        });
        return [...agentMap.values()]
            .sort((a, b) => b.sessions - a.sessions)
            .slice(0, 5);
    }, [sessions, agents]);

    // Compute active users count
    const uniqueUserIds = useMemo(() => new Set(sessions.map(s => s.user_id).filter(Boolean)), [sessions]);

    // Avg session duration
    const avgDuration = useMemo(() => {
        const ended = sessions.filter(s => s.ended_at);
        if (ended.length === 0) return 'N/A';
        const totalMs = ended.reduce((sum, s) => sum + (new Date(s.ended_at!).getTime() - new Date(s.started_at).getTime()), 0);
        const avgMins = Math.round(totalMs / ended.length / 60000);
        return avgMins < 60 ? `${avgMins}m` : `${Math.floor(avgMins / 60)}h ${avgMins % 60}m`;
    }, [sessions]);

    const handleExport = useCallback(() => {
        const header = 'Date,Desktop,Terminal,File Transfer\n';
        const csv = dailyStats.map(d => `${d.date},${d.desktop},${d.terminal},${d.fileTransfer}`).join('\n');
        const blob = new Blob([header + csv], { type: 'text/csv' });
        const url = URL.createObjectURL(blob);
        const a = document.createElement('a');
        a.href = url; a.download = `screencontrol-report-${range}.csv`; a.click();
        URL.revokeObjectURL(url);
        success('Report exported', `${dailyStats.length} days of data`);
    }, [dailyStats, range, success]);

    return (
        <div className="flex flex-col h-full overflow-y-auto">
            {/* Header */}
            <header className="flex flex-wrap items-center justify-between gap-3 px-6 py-4 border-b border-[#333] flex-shrink-0">
                <div>
                    <h1 className="text-lg font-bold text-white flex items-center gap-2">
                        <BarChart3 className="w-5 h-5 text-[#e05246]" />
                        Reports
                    </h1>
                    <p className="text-xs text-gray-500 mt-0.5">Session analytics & agent performance — real data</p>
                </div>
                <div className="flex items-center gap-2">
                    <div className="flex bg-[#1e1e1e] border border-[#333] rounded-lg overflow-hidden">
                        {(['7d', '30d', '90d'] as Range[]).map(r => (
                            <button
                                key={r}
                                onClick={() => setRange(r)}
                                className={`px-3 py-1.5 text-[11px] font-medium transition-colors ${range === r ? 'bg-[#e05246] text-white' : 'text-gray-400 hover:text-white hover:bg-white/5'
                                    }`}
                            >
                                {r === '7d' ? '7 Days' : r === '30d' ? '30 Days' : '90 Days'}
                            </button>
                        ))}
                    </div>
                    <button onClick={() => { fetchData(); info('Refreshing', 'Updating report data...'); }} className="p-2 text-gray-400 hover:text-white hover:bg-white/5 rounded-lg border border-[#333] transition-colors">
                        <RefreshCw className={`w-3.5 h-3.5 ${loading ? 'animate-spin' : ''}`} />
                    </button>
                    <button
                        onClick={handleExport}
                        className="flex items-center gap-1.5 px-3 py-2 text-xs font-medium text-white bg-[#e05246] hover:bg-[#c94539] rounded-lg transition-colors"
                    >
                        <Download className="w-3.5 h-3.5" /> Export CSV
                    </button>
                </div>
            </header>

            {loading ? (
                <div className="flex items-center justify-center h-64">
                    <Loader2 className="w-6 h-6 animate-spin text-gray-600" />
                </div>
            ) : (
                <div className="p-6 space-y-6">
                    {/* Summary cards */}
                    <div className="grid grid-cols-2 lg:grid-cols-4 gap-3">
                        <StatCard label="Total Sessions" value={sessions.length.toLocaleString()} subtext={`${totals.all} in last ${days} days`} icon={Activity} />
                        <StatCard label="Desktop Sessions" value={totals.desktop.toLocaleString()} subtext={`${Math.round((totals.desktop / Math.max(totals.all, 1)) * 100)}% of total`} icon={Monitor} />
                        <StatCard label="Avg Duration" value={avgDuration} subtext={`${sessions.filter(s => s.ended_at).length} completed sessions`} icon={Clock} />
                        <StatCard label="Active Users" value={uniqueUserIds.size.toString()} subtext={`${agents.length} agents total`} icon={Users} />
                    </div>

                    {/* Session Activity Chart */}
                    <div className="bg-[#1e1e1e] border border-[#2a2a2a] rounded-xl p-5">
                        <div className="flex items-center justify-between mb-4">
                            <div>
                                <h2 className="text-sm font-semibold text-white">Session Activity</h2>
                                <p className="text-[10px] text-gray-500 mt-0.5">Sessions by type over the last {days} days</p>
                            </div>
                            <div className="flex items-center gap-4 text-[10px]">
                                <span className="flex items-center gap-1"><span className="w-2 h-2 rounded-full bg-[#e05246]" />Desktop</span>
                                <span className="flex items-center gap-1"><span className="w-2 h-2 rounded-full bg-blue-500" />Terminal</span>
                                <span className="flex items-center gap-1"><span className="w-2 h-2 rounded-full bg-emerald-500" />File Transfer</span>
                            </div>
                        </div>
                        <MiniBarChart data={dailyStats} maxH={120} />
                        <div className="flex justify-between mt-2 text-[9px] text-gray-600">
                            <span>{dailyStats[0]?.date}</span>
                            <span>{dailyStats[Math.floor(dailyStats.length / 2)]?.date}</span>
                            <span>{dailyStats[dailyStats.length - 1]?.date}</span>
                        </div>
                    </div>

                    <div className="grid grid-cols-1 lg:grid-cols-2 gap-5">
                        {/* Top Agents (from real data) */}
                        <div className="bg-[#1e1e1e] border border-[#2a2a2a] rounded-xl p-5">
                            <h2 className="text-sm font-semibold text-white mb-3 flex items-center gap-2">
                                <Monitor className="w-4 h-4 text-[#e05246]" />
                                Top Agents
                            </h2>
                            {topAgents.length === 0 ? (
                                <p className="text-xs text-gray-600 py-4 text-center">No session data yet</p>
                            ) : (
                                <div className="space-y-2">
                                    {topAgents.map((agent, i) => (
                                        <div key={agent.name} className="flex items-center gap-3 py-2 border-b border-[#2a2a2a] last:border-0">
                                            <span className="text-[10px] text-gray-600 w-4">{i + 1}</span>
                                            <div className="flex-1 min-w-0">
                                                <div className="flex items-center gap-1.5">
                                                    <span className="text-xs font-medium text-white truncate">{agent.name}</span>
                                                    <span className={`px-1.5 py-0.5 rounded text-[8px] font-semibold ${agent.os === 'windows' ? 'bg-blue-500/15 text-blue-300' :
                                                        agent.os === 'macos' ? 'bg-gray-500/15 text-gray-300' :
                                                            'bg-emerald-500/15 text-emerald-300'
                                                        }`}>
                                                        {agent.os === 'windows' ? 'WIN' : agent.os === 'macos' ? 'MAC' : 'LNX'}
                                                    </span>
                                                </div>
                                                <span className="text-[10px] text-gray-500">{agent.sessions} sessions • {Math.round(agent.totalMs / 60000)}m total</span>
                                            </div>
                                        </div>
                                    ))}
                                </div>
                            )}
                        </div>

                        {/* Agent Fleet Overview */}
                        <div className="bg-[#1e1e1e] border border-[#2a2a2a] rounded-xl p-5">
                            <h2 className="text-sm font-semibold text-white mb-3 flex items-center gap-2">
                                <Users className="w-4 h-4 text-[#e05246]" />
                                Agent Fleet
                            </h2>
                            {agents.length === 0 ? (
                                <p className="text-xs text-gray-600 py-4 text-center">No agents registered</p>
                            ) : (
                                <div className="space-y-2">
                                    {agents.slice(0, 5).map((agent, i) => (
                                        <div key={agent.id} className="flex items-center gap-3 py-2 border-b border-[#2a2a2a] last:border-0">
                                            <span className="text-[10px] text-gray-600 w-4">{i + 1}</span>
                                            <div className={`w-7 h-7 rounded-full flex items-center justify-center text-[10px] font-bold text-white flex-shrink-0 ${['bg-[#e05246]', 'bg-blue-600', 'bg-emerald-600', 'bg-purple-600', 'bg-amber-600'][i % 5]
                                                }`}>
                                                {agent.machine_name.slice(0, 2).toUpperCase()}
                                            </div>
                                            <div className="flex-1 min-w-0">
                                                <div className="text-xs font-medium text-white truncate">{agent.machine_name}</div>
                                                <span className="text-[10px] text-gray-500">{agent.os} • {agent.status}</span>
                                            </div>
                                            <span className="text-[10px] text-gray-500">{timeAgo(agent.last_seen)}</span>
                                        </div>
                                    ))}
                                </div>
                            )}
                        </div>
                    </div>

                    {/* Session breakdown */}
                    <div className="bg-[#1e1e1e] border border-[#2a2a2a] rounded-xl p-5">
                        <h2 className="text-sm font-semibold text-white mb-4">Session Breakdown</h2>
                        <div className="grid grid-cols-3 gap-4">
                            {[
                                { label: 'Desktop', count: totals.desktop, color: 'bg-[#e05246]', pct: totals.all > 0 ? Math.round((totals.desktop / totals.all) * 100) : 0 },
                                { label: 'Terminal', count: totals.terminal, color: 'bg-blue-500', pct: totals.all > 0 ? Math.round((totals.terminal / totals.all) * 100) : 0 },
                                { label: 'File Transfer', count: totals.file, color: 'bg-emerald-500', pct: totals.all > 0 ? Math.round((totals.file / totals.all) * 100) : 0 },
                            ].map(item => (
                                <div key={item.label} className="text-center">
                                    <div className="text-xl font-bold text-white">{item.count}</div>
                                    <div className="text-[10px] text-gray-500 mb-2">{item.label}</div>
                                    <div className="w-full h-2 bg-[#333] rounded-full overflow-hidden">
                                        <div className={`h-full ${item.color} rounded-full transition-all duration-500`} style={{ width: `${item.pct}%` }} />
                                    </div>
                                    <div className="text-[10px] text-gray-600 mt-1">{item.pct}%</div>
                                </div>
                            ))}
                        </div>
                    </div>
                </div>
            )}
        </div>
    );
}
