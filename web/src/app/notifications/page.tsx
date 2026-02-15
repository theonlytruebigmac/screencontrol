'use client';

/**
 * Notification Log page — real-time alerts from WebSocket events
 * with filters, read/unread state, and bulk actions.
 */

import { useState, useMemo } from 'react';
import {
    Bell,
    BellOff,
    Search,
    Check,
    CheckCheck,
    Trash2,
    AlertTriangle,
    Info,
    Shield,
    X,
    Zap,
} from 'lucide-react';
import { useToast } from '@/components/toast';
import { useEvents, type LiveNotification } from '@/lib/use-agent-status';

type NotifType = 'info' | 'warning' | 'error' | 'success';

const TYPE_CONFIG: Record<NotifType, { icon: typeof Info; color: string; bg: string }> = {
    info: { icon: Info, color: 'text-blue-400', bg: 'bg-blue-500/10' },
    warning: { icon: AlertTriangle, color: 'text-amber-400', bg: 'bg-amber-500/10' },
    error: { icon: Shield, color: 'text-red-400', bg: 'bg-red-500/10' },
    success: { icon: Check, color: 'text-emerald-400', bg: 'bg-emerald-500/10' },
};

function formatTime(timeStr: string): string {
    const d = new Date(timeStr);
    const now = Date.now();
    const diff = now - d.getTime();
    const mins = Math.floor(diff / 60000);
    if (mins < 1) return 'Just now';
    if (mins < 60) return `${mins}m ago`;
    const hrs = Math.floor(mins / 60);
    if (hrs < 24) return `${hrs}h ago`;
    return d.toLocaleDateString() + ' ' + d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}

export default function NotificationsPage() {
    const { info: toastInfo } = useToast();
    const { notifications, markRead, markAllRead, dismiss, clearAll, unreadCount } = useEvents();
    const [search, setSearch] = useState('');
    const [typeFilter, setTypeFilter] = useState<NotifType | 'all'>('all');
    const [readFilter, setReadFilter] = useState<'all' | 'unread' | 'read'>('all');

    const filtered = useMemo(() => {
        let list = notifications;
        if (typeFilter !== 'all') list = list.filter(n => n.type === typeFilter);
        if (readFilter === 'unread') list = list.filter(n => !n.read);
        if (readFilter === 'read') list = list.filter(n => n.read);
        if (search) {
            const q = search.toLowerCase();
            list = list.filter(n => n.title.toLowerCase().includes(q) || n.message.toLowerCase().includes(q) || n.source.toLowerCase().includes(q));
        }
        return list;
    }, [notifications, search, typeFilter, readFilter]);

    const handleMarkAllRead = () => {
        markAllRead();
        toastInfo('Done', 'All notifications marked as read');
    };

    const handleClearAll = () => {
        clearAll();
        toastInfo('Cleared', 'All notifications removed');
    };

    return (
        <div className="flex flex-col h-full overflow-y-auto">
            {/* Header */}
            <header className="flex flex-wrap items-center justify-between gap-3 px-6 py-4 border-b border-[#333] flex-shrink-0">
                <div>
                    <h1 className="text-lg font-bold text-white flex items-center gap-2">
                        <Bell className="w-5 h-5 text-[#e05246]" />
                        Notifications
                        {unreadCount > 0 && (
                            <span className="text-[10px] font-semibold bg-[#e05246] text-white px-2 py-0.5 rounded-full">{unreadCount} new</span>
                        )}
                    </h1>
                    <p className="text-xs text-gray-500 mt-0.5">
                        {notifications.length} notifications
                        {notifications.length === 0 && ' — live events will appear here as they happen'}
                    </p>
                </div>
                <div className="flex items-center gap-2">
                    <button onClick={handleMarkAllRead} disabled={unreadCount === 0} className="flex items-center gap-1.5 px-3 py-2 text-xs text-gray-400 hover:text-white hover:bg-white/5 rounded-lg border border-[#333] transition-colors disabled:opacity-30">
                        <CheckCheck className="w-3.5 h-3.5" /> Mark all read
                    </button>
                    <button onClick={handleClearAll} disabled={notifications.length === 0} className="flex items-center gap-1.5 px-3 py-2 text-xs text-gray-400 hover:text-red-400 hover:bg-red-500/5 rounded-lg border border-[#333] transition-colors disabled:opacity-30">
                        <Trash2 className="w-3.5 h-3.5" /> Clear all
                    </button>
                </div>
            </header>

            <div className="p-6 space-y-4">
                {/* Filters */}
                <div className="flex flex-wrap items-center gap-2">
                    {(['all', 'error', 'warning', 'info', 'success'] as const).map(t => (
                        <button key={t} onClick={() => setTypeFilter(t)} className={`px-3 py-1.5 rounded-lg text-[11px] font-medium capitalize transition-colors ${typeFilter === t ? 'bg-[#e05246] text-white' : 'bg-[#1e1e1e] text-gray-400 border border-[#333] hover:border-[#555]'}`}>
                            {t === 'all' ? `All (${notifications.length})` : t}
                        </button>
                    ))}

                    <div className="ml-auto flex gap-2">
                        {(['all', 'unread', 'read'] as const).map(r => (
                            <button key={r} onClick={() => setReadFilter(r)} className={`px-2.5 py-1.5 rounded-lg text-[11px] capitalize transition-colors ${readFilter === r ? 'bg-white/10 text-white' : 'text-gray-500 hover:text-gray-300'}`}>
                                {r}
                            </button>
                        ))}
                    </div>
                </div>

                {/* Search */}
                <div className="relative">
                    <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-gray-600" />
                    <input value={search} onChange={e => setSearch(e.target.value)} placeholder="Search notifications..." className="w-full bg-[#1e1e1e] border border-[#333] rounded-lg pl-9 pr-3 py-2.5 text-xs text-gray-300 placeholder-gray-600 focus:outline-none focus:border-[#e05246]" />
                </div>

                {/* List */}
                <div className="space-y-1">
                    {filtered.map(notif => {
                        const cfg = TYPE_CONFIG[notif.type];
                        const Icon = cfg.icon;

                        return (
                            <div key={notif.id} className={`flex items-start gap-3 px-4 py-3 rounded-lg border transition-all ${notif.read ? 'bg-[#1a1a1a] border-[#252525] opacity-60 hover:opacity-100' : 'bg-[#1e1e1e] border-[#333]'}`}>
                                <div className={`w-8 h-8 rounded-lg flex items-center justify-center flex-shrink-0 mt-0.5 ${cfg.bg}`}>
                                    <Icon className={`w-4 h-4 ${cfg.color}`} />
                                </div>

                                <div className="flex-1 min-w-0">
                                    <div className="flex items-center gap-2 mb-0.5">
                                        <span className="text-xs font-medium text-white">{notif.title}</span>
                                        <span className={`text-[9px] px-1.5 py-0.5 rounded font-medium ${cfg.bg} ${cfg.color}`}>{notif.source}</span>
                                        {!notif.read && <div className="w-1.5 h-1.5 rounded-full bg-[#e05246]" />}
                                    </div>
                                    <p className="text-[11px] text-gray-500 leading-relaxed">{notif.message}</p>
                                    <span className="text-[9px] text-gray-600 mt-1 block">{formatTime(notif.time)}</span>
                                </div>

                                <div className="flex items-center gap-1 flex-shrink-0">
                                    <button onClick={() => markRead(notif.id)} className="p-1.5 text-gray-600 hover:text-white rounded transition-colors" title={notif.read ? 'Already read' : 'Mark read'}>
                                        {notif.read ? <Bell className="w-3 h-3" /> : <Check className="w-3 h-3" />}
                                    </button>
                                    <button onClick={() => dismiss(notif.id)} className="p-1.5 text-gray-600 hover:text-red-400 rounded transition-colors" title="Dismiss">
                                        <X className="w-3 h-3" />
                                    </button>
                                </div>
                            </div>
                        );
                    })}
                </div>

                {filtered.length === 0 && (
                    <div className="flex flex-col items-center justify-center py-16 text-gray-600">
                        <BellOff className="w-12 h-12 mb-3 opacity-20" />
                        <p className="text-sm font-medium">No notifications</p>
                        <p className="text-xs text-gray-700 mt-1">
                            {notifications.length === 0
                                ? 'Live events from agents and sessions will appear here'
                                : 'You\u0027re all caught up!'}
                        </p>
                    </div>
                )}
            </div>
        </div>
    );
}
