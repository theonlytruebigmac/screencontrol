'use client';
import { getAccessToken } from '@/lib/auth-store';

import { useState, useEffect, useMemo, useCallback } from 'react';
import { Search, Filter, Download, X, Loader2, ChevronDown } from 'lucide-react';
import { api, type AuditEntry } from '@/lib/api';

const PAGE_SIZE = 50;

const actionBadge: Record<string, string> = {
    'user.login': 'bg-blue-500/15 text-blue-300',
    'session.create': 'bg-emerald-500/15 text-emerald-300',
    'session.end': 'bg-red-500/15 text-red-300',
    'agent.register': 'bg-purple-500/15 text-purple-300',
    'agent.online': 'bg-green-500/15 text-green-300',
    'agent.offline': 'bg-gray-700/50 text-gray-400',
    'settings.update': 'bg-amber-500/15 text-amber-300',
};

export default function AuditPage() {
    const [entries, setEntries] = useState<AuditEntry[]>([]);
    const [loading, setLoading] = useState(true);
    const [loadingMore, setLoadingMore] = useState(false);
    const [hasMore, setHasMore] = useState(true);
    const [search, setSearch] = useState('');
    const [showFilters, setShowFilters] = useState(false);
    const [actionFilter, setActionFilter] = useState<string | null>(null);

    const fetchEntries = useCallback(async (offset = 0, append = false) => {
        try {
            const isMore = offset > 0;
            if (isMore) setLoadingMore(true); else setLoading(true);

            const token = getAccessToken();
            if (token) api.setToken(token);

            const data = await api.getAuditLog({
                limit: PAGE_SIZE,
                offset,
                action: actionFilter || undefined,
            });

            if (append) {
                setEntries(prev => [...prev, ...data]);
            } else {
                setEntries(data);
            }
            setHasMore(data.length === PAGE_SIZE);
        } catch (e) {
            console.error('Failed to load audit log:', e);
        } finally {
            setLoading(false);
            setLoadingMore(false);
        }
    }, [actionFilter]);

    useEffect(() => { fetchEntries(); }, [fetchEntries]);

    const loadMore = () => fetchEntries(entries.length, true);

    const actionTypes = useMemo(() => [...new Set(entries.map(e => e.action))], [entries]);

    const filtered = useMemo(() => {
        if (!search) return entries;
        const q = search.toLowerCase();
        return entries.filter(e =>
            [e.action, e.target_type || '', e.target_id || '', e.ip_address || '', e.user_id || '']
                .some(v => v.toLowerCase().includes(q))
        );
    }, [entries, search]);

    const handleExport = () => {
        const header = 'Timestamp,User,Action,Target Type,Target ID,IP';
        const rows = filtered.map(e =>
            `${e.created_at},${e.user_id || 'system'},${e.action},${e.target_type || ''},${e.target_id || ''},${e.ip_address || ''}`
        );
        const csv = [header, ...rows].join('\n');
        const blob = new Blob([csv], { type: 'text/csv' });
        const url = URL.createObjectURL(blob);
        const a = document.createElement('a');
        a.href = url;
        a.download = `audit-log-${new Date().toISOString().slice(0, 10)}.csv`;
        a.click();
        URL.revokeObjectURL(url);
    };

    const formatTime = (dateStr: string) => {
        const d = new Date(dateStr);
        const now = Date.now();
        const diff = now - d.getTime();
        const mins = Math.floor(diff / 60000);
        if (mins < 1) return 'Just now';
        if (mins < 60) return `${mins}m ago`;
        const hrs = Math.floor(mins / 60);
        if (hrs < 24) return `${hrs}h ago`;
        return d.toLocaleDateString() + ' ' + d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
    };

    const getMetadataLabel = (entry: AuditEntry): string => {
        const meta = entry.metadata;
        if (!meta) return entry.target_type || '';
        if (meta.email) return String(meta.email);
        if (meta.session_type && meta.agent_id) return `${meta.session_type} → ${String(meta.agent_id).slice(0, 8)}`;
        return entry.target_type || '';
    };

    return (
        <div className="p-6">
            <div className="flex items-center justify-between mb-6">
                <div>
                    <h2 className="text-lg font-semibold text-white">Audit Log</h2>
                    <p className="text-[11px] text-gray-500 mt-0.5">{entries.length} events loaded</p>
                </div>
                <button
                    onClick={handleExport}
                    className="flex items-center gap-2 bg-[#333] hover:bg-[#444] text-gray-300 px-4 py-2 rounded-lg text-sm transition-colors"
                >
                    <Download className="w-4 h-4" />
                    Export CSV
                </button>
            </div>

            {/* Filters */}
            <div className="flex gap-3 mb-4">
                <div className="relative flex-1 max-w-md">
                    <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-gray-600" />
                    <input
                        type="text"
                        placeholder="Search actions, users, targets..."
                        value={search}
                        onChange={(e) => setSearch(e.target.value)}
                        className="w-full bg-[#141414] border border-[#333] rounded-lg pl-10 pr-4 py-2.5 text-sm text-gray-100 placeholder-gray-600 focus:border-[#e05246] focus:outline-none"
                    />
                </div>
                <button
                    onClick={() => setShowFilters(!showFilters)}
                    className={`flex items-center gap-2 border px-4 py-2.5 rounded-lg text-sm transition-colors ${showFilters || actionFilter
                        ? "bg-[#e05246]/10 border-[#e05246]/30 text-[#e05246]"
                        : "bg-[#1e1e1e] border-[#333] hover:bg-[#333] text-gray-400"
                        }`}
                >
                    <Filter className="w-4 h-4" />
                    Filter
                    {actionFilter && (
                        <span className="bg-[#e05246] text-white text-[10px] px-1.5 py-0.5 rounded-full">1</span>
                    )}
                </button>
            </div>

            {/* Filter panel */}
            {showFilters && (
                <div className="flex items-center gap-2 mb-4 px-3 py-2.5 bg-[#1e1e1e] border border-[#333] rounded-lg slide-up">
                    <span className="text-[11px] text-gray-500 mr-1">Action:</span>
                    {actionTypes.map((action) => (
                        <button
                            key={action}
                            onClick={() => setActionFilter(actionFilter === action ? null : action)}
                            className={`px-2.5 py-1 rounded text-[11px] font-medium transition-colors ${actionFilter === action
                                ? (actionBadge[action] || "bg-gray-800 text-gray-400")
                                : "bg-[#141414] text-gray-500 hover:text-gray-300"
                                }`}
                        >
                            {action}
                        </button>
                    ))}
                    {actionFilter && (
                        <button
                            onClick={() => setActionFilter(null)}
                            className="ml-auto p-1 text-gray-500 hover:text-gray-300 transition-colors"
                        >
                            <X className="w-3.5 h-3.5" />
                        </button>
                    )}
                </div>
            )}

            {/* Table */}
            {loading ? (
                <div className="flex items-center justify-center h-48">
                    <Loader2 className="w-6 h-6 animate-spin text-gray-600" />
                </div>
            ) : (
                <div className="bg-[#1e1e1e] border border-[#333] rounded-lg overflow-hidden">
                    <div className="grid grid-cols-[140px_1fr_auto_auto_100px] gap-4 px-4 py-2 text-[10px] text-gray-500 uppercase tracking-wider border-b border-[#333]">
                        <span>Time</span>
                        <span>User</span>
                        <span>Action</span>
                        <span>Target</span>
                        <span>IP</span>
                    </div>
                    {filtered.map((entry) => (
                        <div key={entry.id} className="grid grid-cols-[140px_1fr_auto_auto_100px] gap-4 items-center px-4 py-2.5 border-b border-[#272727] hover:bg-white/[0.02] transition-colors text-sm">
                            <span className="text-gray-500 text-xs font-mono">{formatTime(entry.created_at)}</span>
                            <span className="text-gray-300 truncate">{entry.user_id ? entry.user_id.slice(0, 8) + '…' : 'system'}</span>
                            <span className={`px-2.5 py-0.5 rounded text-[11px] font-medium ${actionBadge[entry.action] || 'bg-gray-800 text-gray-400'}`}>
                                {entry.action}
                            </span>
                            <span className="text-gray-400 text-xs">{getMetadataLabel(entry)}</span>
                            <span className="text-gray-600 text-xs font-mono">{entry.ip_address || '—'}</span>
                        </div>
                    ))}
                    {filtered.length === 0 && (
                        <div className="px-4 py-8 text-center text-gray-600 text-sm">No matching events</div>
                    )}
                </div>
            )}

            {/* Load more / count */}
            <div className="mt-3 flex items-center justify-between">
                <span className="text-[11px] text-gray-600">
                    {filtered.length} event{filtered.length !== 1 ? 's' : ''}
                </span>
                {hasMore && !loading && (
                    <button
                        onClick={loadMore}
                        disabled={loadingMore}
                        className="flex items-center gap-1.5 text-xs text-gray-400 hover:text-white transition-colors disabled:opacity-50"
                    >
                        {loadingMore ? <Loader2 className="w-3 h-3 animate-spin" /> : <ChevronDown className="w-3 h-3" />}
                        Load more
                    </button>
                )}
            </div>
        </div>
    );
}
