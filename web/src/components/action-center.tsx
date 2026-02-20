'use client';

/**
 * System Health — alert panel with slide-out notification center.
 * Shows alerts generated from live agent data:
 * offline agents, high CPU, high memory, high disk usage.
 */

import { useState, useEffect, useRef, useCallback } from 'react';
import {
    AlertTriangle,
    X,
    WifiOff,
    Shield,
    Clock,
    CheckCircle2,
    ChevronRight,
    Trash2,
    Cpu,
    HardDrive,
} from 'lucide-react';
import Link from 'next/link';
import { api, type Agent } from '@/lib/api';
import { getAccessToken } from '@/lib/auth-store';

interface Alert {
    id: string;
    type: 'warning' | 'error' | 'info' | 'success';
    title: string;
    message: string;
    time: string;
    link?: string;
    dismissed: boolean;
}

type FilterKey = 'all' | 'error' | 'warning' | 'info';

const FILTER_TABS: { key: FilterKey; label: string }[] = [
    { key: 'all', label: 'All' },
    { key: 'error', label: 'Errors' },
    { key: 'warning', label: 'Warnings' },
    { key: 'info', label: 'Info' },
];

function timeAgo(dateStr: string | null): string {
    if (!dateStr) return "Unknown";
    const diff = Date.now() - new Date(dateStr).getTime();
    const mins = Math.floor(diff / 60000);
    if (mins < 1) return "Just now";
    if (mins < 60) return `${mins}m ago`;
    const hrs = Math.floor(mins / 60);
    if (hrs < 24) return `${hrs}h ago`;
    return `${Math.floor(hrs / 24)}d ago`;
}

function generateAlerts(agents: Agent[]): Alert[] {
    const alerts: Alert[] = [];

    for (const agent of agents) {
        // Offline agent alert
        if (agent.status === 'offline') {
            alerts.push({
                id: `offline-${agent.id}`,
                type: 'error',
                title: 'Agent Offline',
                message: `${agent.machine_name} is offline`,
                time: timeAgo(agent.last_seen),
                link: `/agents?select=${agent.id}`,
                dismissed: false,
            });
        }

        // High CPU alert (> 90%)
        if (agent.cpu_usage != null && agent.cpu_usage > 90) {
            alerts.push({
                id: `cpu-${agent.id}`,
                type: 'warning',
                title: 'High CPU Alert',
                message: `${agent.machine_name} CPU usage at ${agent.cpu_usage.toFixed(0)}%`,
                time: 'Now',
                link: `/agents?select=${agent.id}`,
                dismissed: false,
            });
        }

        // High memory alert (> 90%)
        if (agent.memory_used != null && agent.memory_total != null && agent.memory_total > 0) {
            const memPct = (agent.memory_used / agent.memory_total) * 100;
            if (memPct > 90) {
                alerts.push({
                    id: `mem-${agent.id}`,
                    type: 'warning',
                    title: 'High Memory Usage',
                    message: `${agent.machine_name} memory at ${memPct.toFixed(0)}%`,
                    time: 'Now',
                    link: `/agents?select=${agent.id}`,
                    dismissed: false,
                });
            }
        }

        // Critical disk alert (> 90%)
        if (agent.disk_used != null && agent.disk_total != null && agent.disk_total > 0) {
            const diskPct = (agent.disk_used / agent.disk_total) * 100;
            if (diskPct > 90) {
                alerts.push({
                    id: `disk-${agent.id}`,
                    type: 'error',
                    title: 'Disk Usage Critical',
                    message: `${agent.machine_name} disk at ${diskPct.toFixed(0)}%`,
                    time: 'Now',
                    link: `/agents?select=${agent.id}`,
                    dismissed: false,
                });
            }
        }
    }

    // If no problems, add a success alert
    if (alerts.length === 0) {
        alerts.push({
            id: 'all-clear',
            type: 'success',
            title: 'All Systems Healthy',
            message: `${agents.length} agent${agents.length !== 1 ? 's' : ''} running normally`,
            time: 'Now',
            dismissed: false,
        });
    }

    return alerts;
}

export function ActionCenter() {
    const [open, setOpen] = useState(false);
    const [alerts, setAlerts] = useState<Alert[]>([]);
    const [dismissedIds, setDismissedIds] = useState<Set<string>>(new Set());
    const [filter, setFilter] = useState<FilterKey>('all');
    const panelRef = useRef<HTMLDivElement>(null);
    const hasFetched = useRef(false);

    const fetchAlerts = useCallback(async () => {
        try {
            const token = getAccessToken();
            if (token) api.setToken(token);
            const agents = await api.getAgents();
            const generated = generateAlerts(agents).map(a => ({
                ...a,
                dismissed: dismissedIds.has(a.id),
            }));
            setAlerts(generated);
        } catch (e) {
            console.error('Failed to fetch action center data:', e);
        }
    }, [dismissedIds]);

    // Fetch on mount
    useEffect(() => {
        if (!hasFetched.current) {
            hasFetched.current = true;
            fetchAlerts();
        }
    }, [fetchAlerts]);

    // Refresh when panel opens
    useEffect(() => {
        if (open) fetchAlerts();
    }, [open, fetchAlerts]);

    const activeAlerts = alerts.filter(a => !a.dismissed);
    const filteredAlerts = filter === 'all' ? activeAlerts : activeAlerts.filter(a => a.type === filter);
    const unreadCount = activeAlerts.filter(a => a.type === 'error' || a.type === 'warning').length;

    // Close on outside click
    useEffect(() => {
        if (!open) return;
        const handler = (e: MouseEvent) => {
            if (panelRef.current && !panelRef.current.contains(e.target as Node)) {
                setOpen(false);
            }
        };
        document.addEventListener('mousedown', handler);
        return () => document.removeEventListener('mousedown', handler);
    }, [open]);

    // Close on Escape
    useEffect(() => {
        if (!open) return;
        const handler = (e: KeyboardEvent) => {
            if (e.key === 'Escape') setOpen(false);
        };
        document.addEventListener('keydown', handler);
        return () => document.removeEventListener('keydown', handler);
    }, [open]);

    const dismiss = useCallback((id: string) => {
        setDismissedIds(prev => new Set(prev).add(id));
        setAlerts(prev => prev.map(a => a.id === id ? { ...a, dismissed: true } : a));
    }, []);

    const dismissAll = useCallback(() => {
        setDismissedIds(prev => {
            const next = new Set(prev);
            alerts.forEach(a => next.add(a.id));
            return next;
        });
        setAlerts(prev => prev.map(a => ({ ...a, dismissed: true })));
    }, [alerts]);

    const iconForType = (type: Alert['type']) => {
        switch (type) {
            case 'error': return <WifiOff className="w-4 h-4 text-red-400" />;
            case 'warning': return <AlertTriangle className="w-4 h-4 text-amber-400" />;
            case 'info': return <Shield className="w-4 h-4 text-blue-400" />;
            case 'success': return <CheckCircle2 className="w-4 h-4 text-emerald-400" />;
        }
    };

    const borderForType = (type: Alert['type']) => {
        switch (type) {
            case 'error': return 'border-l-red-500';
            case 'warning': return 'border-l-amber-500';
            case 'info': return 'border-l-blue-500';
            case 'success': return 'border-l-emerald-500';
        }
    };

    return (
        <div className="relative" ref={panelRef}>
            {/* Bell button */}
            <button
                onClick={() => setOpen(!open)}
                className={`relative p-2 rounded-lg transition-colors ${open ? 'bg-white/5 text-white' : 'text-gray-500 hover:text-white hover:bg-white/5'
                    }`}
                title="System Health"
            >
                <AlertTriangle className="w-4.5 h-4.5" />
                {unreadCount > 0 && (
                    <span className="absolute -top-0.5 -right-0.5 w-4 h-4 bg-[#e05246] text-white text-[8px] font-bold rounded-full flex items-center justify-center border-2 border-[#141414]">
                        {unreadCount > 9 ? '9+' : unreadCount}
                    </span>
                )}
            </button>

            {/* Slide-out panel */}
            {open && (
                <div className="absolute left-full bottom-0 ml-2 w-96 bg-[#1a1a1a] border border-[#333] rounded-xl shadow-2xl z-[100] animate-fadeIn overflow-hidden">
                    {/* Header */}
                    <div className="flex items-center justify-between px-4 py-3 border-b border-[#333]">
                        <div className="flex items-center gap-2">
                            <AlertTriangle className="w-4 h-4 text-[#e05246]" />
                            <h3 className="text-sm font-semibold text-white">System Health</h3>
                            {unreadCount > 0 && (
                                <span className="px-1.5 py-0.5 bg-[#e05246]/15 text-[#e05246] text-[10px] font-medium rounded">
                                    {unreadCount}
                                </span>
                            )}
                        </div>
                        <div className="flex items-center gap-1">
                            {unreadCount > 0 && (
                                <button
                                    onClick={dismissAll}
                                    className="text-[10px] text-gray-500 hover:text-white px-2 py-1 rounded transition-colors hover:bg-white/5"
                                >
                                    Clear All
                                </button>
                            )}
                            <button
                                onClick={() => setOpen(false)}
                                className="p-1 text-gray-500 hover:text-white rounded transition-colors hover:bg-white/5"
                            >
                                <X className="w-3.5 h-3.5" />
                            </button>
                        </div>
                    </div>

                    {/* Filter tabs */}
                    <div className="flex items-center gap-0.5 px-3 py-2 border-b border-[#272727]">
                        {FILTER_TABS.map(tab => {
                            const count = tab.key === 'all' ? activeAlerts.length : activeAlerts.filter(a => a.type === tab.key).length;
                            return (
                                <button
                                    key={tab.key}
                                    onClick={() => setFilter(tab.key)}
                                    className={`px-2.5 py-1 text-[10px] rounded-md transition-colors ${filter === tab.key
                                        ? 'bg-white/10 text-white font-medium'
                                        : 'text-gray-500 hover:text-gray-300 hover:bg-white/5'
                                        }`}
                                >
                                    {tab.label}
                                    {count > 0 && (
                                        <span className="ml-1 text-[9px] opacity-60">({count})</span>
                                    )}
                                </button>
                            );
                        })}
                    </div>

                    {/* Alerts list */}
                    <div className="max-h-[400px] overflow-y-auto">
                        {filteredAlerts.length === 0 ? (
                            <div className="flex flex-col items-center justify-center py-12 text-gray-600">
                                <CheckCircle2 className="w-8 h-8 mb-2 opacity-20" />
                                <p className="text-xs">{filter === 'all' ? 'All clear' : `No ${filter} alerts`}</p>
                                <p className="text-[10px] text-gray-700 mt-0.5">No pending alerts</p>
                            </div>
                        ) : (
                            filteredAlerts.map((alert) => (
                                <div
                                    key={alert.id}
                                    className={`flex items-start gap-3 px-4 py-3 border-b border-[#272727] border-l-2 ${borderForType(alert.type)} hover:bg-white/[0.02] transition-colors group`}
                                >
                                    <div className="mt-0.5 flex-shrink-0">
                                        {iconForType(alert.type)}
                                    </div>
                                    <div className="flex-1 min-w-0">
                                        <p className="text-xs font-medium text-gray-200">{alert.title}</p>
                                        <p className="text-[11px] text-gray-500 mt-0.5 leading-relaxed">{alert.message}</p>
                                        <div className="flex items-center gap-2 mt-1.5">
                                            <span className="text-[9px] text-gray-600 flex items-center gap-0.5">
                                                <Clock className="w-2.5 h-2.5" />
                                                {alert.time}
                                            </span>
                                            {alert.link && (
                                                <Link
                                                    href={alert.link}
                                                    onClick={() => setOpen(false)}
                                                    className="text-[9px] text-[#e05246] hover:text-[#f06b60] flex items-center gap-0.5 transition-colors"
                                                >
                                                    View <ChevronRight className="w-2.5 h-2.5" />
                                                </Link>
                                            )}
                                        </div>
                                    </div>
                                    <button
                                        onClick={() => dismiss(alert.id)}
                                        className="opacity-0 group-hover:opacity-100 p-1 text-gray-600 hover:text-white rounded transition-all"
                                        title="Dismiss"
                                    >
                                        <Trash2 className="w-3 h-3" />
                                    </button>
                                </div>
                            ))
                        )}
                    </div>
                </div>
            )}
        </div>
    );
}
