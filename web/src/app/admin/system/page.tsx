'use client';
import { getAccessToken } from '@/lib/auth-store';

/**
 * System Status page — server info, component health,
 * resource gauges, and version/license details.
 */

import { useState, useEffect, useCallback } from 'react';
import {
    Server,
    Database,
    Radio,
    HardDrive,
    Cpu,
    MemoryStick,
    Globe,
    Clock,
    Shield,
    CheckCircle2,
    XCircle,
    AlertTriangle,
    RefreshCw,
    Activity,
    Monitor,
    Loader2,
} from 'lucide-react';
import { api, type DashboardStats, type SystemHealthResponse, type ComponentHealth, type ResourceInfo } from '@/lib/api';

// ─── Types ───────────────────────────────────
interface ResourceGauge {
    label: string;
    icon: React.ComponentType<{ className?: string }>;
    value: number;
    max: number;
    unit: string;
    color: string;
}

const COMPONENT_ICONS: Record<string, React.ComponentType<{ className?: string }>> = {
    'REST API': Globe,
    'PostgreSQL': Database,
    'Redis': Database,
    'WebSocket': Activity,
};

const RESOURCE_COLORS = ['#22c55e', '#3b82f6', '#f59e0b'];
const RESOURCE_ICONS = [Activity, Database, Monitor];

// ─── Status Helpers ──────────────────────────
function statusIcon(s: 'healthy' | 'degraded' | 'down') {
    switch (s) {
        case 'healthy': return <CheckCircle2 className="w-4 h-4 text-emerald-400" />;
        case 'degraded': return <AlertTriangle className="w-4 h-4 text-amber-400" />;
        case 'down': return <XCircle className="w-4 h-4 text-red-400" />;
    }
}

function statusBadge(s: 'healthy' | 'degraded' | 'down') {
    switch (s) {
        case 'healthy': return 'bg-emerald-500/15 text-emerald-300 border-emerald-500/20';
        case 'degraded': return 'bg-amber-500/15 text-amber-300 border-amber-500/20';
        case 'down': return 'bg-red-500/15 text-red-300 border-red-500/20';
    }
}

// ─── Gauge Component ─────────────────────────
function GaugeCard({ label, value, max, unit, color, icon: Icon }: ResourceGauge) {
    const pct = (value / max) * 100;
    return (
        <div className="bg-[#1e1e1e] border border-[#333] rounded-lg p-4 space-y-3">
            <div className="flex items-center justify-between">
                <div className="flex items-center gap-2">
                    <Icon className="w-4 h-4 text-gray-400" />
                    <span className="text-sm text-gray-300 font-medium">{label}</span>
                </div>
                <span className="text-lg font-semibold text-white">
                    {value}{unit === '%' ? '' : ` ${unit}`}
                    <span className="text-xs text-gray-600 font-normal ml-1">
                        {unit === '%' ? '%' : `/ ${max} ${unit}`}
                    </span>
                </span>
            </div>
            <div className="w-full h-2 bg-[#252525] rounded-full overflow-hidden">
                <div
                    className="h-full rounded-full transition-all duration-700"
                    style={{ width: `${pct}%`, backgroundColor: color }}
                />
            </div>
        </div>
    );
}

// ─── Main Component ──────────────────────────
export default function SystemPage() {
    const [loading, setLoading] = useState(true);
    const [lastRefresh, setLastRefresh] = useState(new Date());
    const [stats, setStats] = useState<DashboardStats | null>(null);
    const [health, setHealth] = useState<SystemHealthResponse | null>(null);
    const [licenseInfo, setLicenseInfo] = useState<Record<string, string>>({});

    const fetchData = useCallback(async () => {
        setLoading(true);
        try {
            const token = getAccessToken();
            if (token) api.setToken(token);
            const [statsData, healthData, licenseRows] = await Promise.all([
                api.getStats(),
                api.getSystemHealth(),
                api.getSettings('license'),
            ]);
            setStats(statsData);
            setHealth(healthData);
            const lic: Record<string, string> = {};
            for (const r of licenseRows) lic[r.key] = r.value as string;
            setLicenseInfo(lic);
        } catch (e) {
            console.error('Failed to load stats:', e);
        } finally {
            setLoading(false);
            setLastRefresh(new Date());
        }
    }, []);

    useEffect(() => { fetchData(); }, [fetchData]);

    const handleRefresh = () => {
        fetchData();
    };

    if (loading) {
        return (
            <div className="flex items-center justify-center h-full">
                <Loader2 className="w-6 h-6 text-[#e05246] animate-spin" />
            </div>
        );
    }

    const components = health?.components ?? [];
    const healthyCount = components.filter(c => c.status === 'healthy').length;
    const allHealthy = healthyCount === components.length && components.length > 0;

    const resourceGauges: ResourceGauge[] = (health?.resources ?? []).map((r, i) => ({
        label: r.label,
        icon: RESOURCE_ICONS[i % RESOURCE_ICONS.length],
        value: r.value,
        max: r.max,
        unit: r.unit,
        color: RESOURCE_COLORS[i % RESOURCE_COLORS.length],
    }));

    const formatUptime = (seconds: number) => {
        const days = Math.floor(seconds / 86400);
        const hrs = Math.floor((seconds % 86400) / 3600);
        const mins = Math.floor((seconds % 3600) / 60);
        return `${days}d ${hrs}h ${mins}m`;
    };

    return (
        <div className="p-6 space-y-6">
            {/* Header */}
            <div className="flex items-center justify-between">
                <div>
                    <h2 className="text-lg font-semibold text-white">System Status</h2>
                    <p className="text-[11px] text-gray-500 mt-0.5">
                        Last refreshed {lastRefresh.toLocaleTimeString()}
                    </p>
                </div>
                <button
                    onClick={handleRefresh}
                    className="flex items-center gap-2 px-3 py-2 rounded-lg text-xs font-medium bg-[#252525] border border-[#333] hover:bg-[#333] text-gray-300 transition-colors"
                >
                    <RefreshCw className="w-3.5 h-3.5" />
                    Refresh
                </button>
            </div>

            {/* Overall Status Banner */}
            <div className={`rounded-lg border p-4 flex items-center gap-3 ${allHealthy
                ? 'bg-emerald-500/5 border-emerald-500/20'
                : 'bg-amber-500/5 border-amber-500/20'
                }`}>
                {allHealthy
                    ? <CheckCircle2 className="w-5 h-5 text-emerald-400" />
                    : <AlertTriangle className="w-5 h-5 text-amber-400" />
                }
                <div>
                    <p className={`text-sm font-medium ${allHealthy ? 'text-emerald-300' : 'text-amber-300'}`}>
                        {allHealthy ? 'All Systems Operational' : 'Some Systems Degraded'}
                    </p>
                    <p className="text-[11px] text-gray-500">
                        {healthyCount}/{components.length} components healthy
                    </p>
                </div>
            </div>

            {/* Server Info + License in 2-col */}
            <div className="grid grid-cols-2 gap-4">
                {/* Server Info */}
                <div className="bg-[#1e1e1e] border border-[#333] rounded-lg p-5 space-y-3">
                    <h3 className="text-sm font-medium text-gray-300 flex items-center gap-2">
                        <Server className="w-4 h-4 text-[#e05246]" />
                        Server Information
                    </h3>
                    <div className="space-y-2 text-sm">
                        {[
                            ['Version', `v${health?.server.version ?? '?'}`],
                            ['Runtime', health?.server.rust_version ?? '?'],
                            ['Platform', health?.server.os ?? '?'],
                            ['Hostname', health?.server.hostname ?? '?'],
                            ['Uptime', formatUptime(health?.server.uptime_seconds ?? 0)],
                        ].map(([k, v]) => (
                            <div key={k} className="flex items-center justify-between">
                                <span className="text-gray-500 text-xs">{k}</span>
                                <span className="text-gray-300 text-xs font-mono">{v}</span>
                            </div>
                        ))}
                    </div>
                </div>

                {/* License */}
                <div className="bg-[#1e1e1e] border border-[#333] rounded-lg p-5 space-y-3">
                    <h3 className="text-sm font-medium text-gray-300 flex items-center gap-2">
                        <Shield className="w-4 h-4 text-[#e05246]" />
                        License
                    </h3>
                    <div className="space-y-2 text-sm">
                        {[
                            ['Plan', licenseInfo.plan || 'Community'],
                            ['Company', licenseInfo.company || '—'],
                            ['Seats', licenseInfo.seats || 'Unlimited'],
                            ['Expires', licenseInfo.expires || 'Never'],
                        ].map(([k, v]) => (
                            <div key={k} className="flex items-center justify-between">
                                <span className="text-gray-500 text-xs">{k}</span>
                                <span className="text-gray-300 text-xs font-mono">{v}</span>
                            </div>
                        ))}
                    </div>
                    <div className="pt-2 border-t border-[#333]">
                        <div className="flex items-center gap-2">
                            <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded text-[10px] font-medium bg-emerald-500/15 text-emerald-300 border border-emerald-500/20">
                                <CheckCircle2 className="w-3 h-3" />
                                {licenseInfo.status || 'Active'}
                            </span>
                        </div>
                    </div>
                </div>
            </div>

            {/* Resource Usage */}
            <div>
                <h3 className="text-sm font-medium text-gray-300 mb-3">Resource Usage</h3>
                <div className="grid grid-cols-3 gap-4">
                    {resourceGauges.map(g => (
                        <GaugeCard key={g.label} {...g} />
                    ))}
                </div>
            </div>

            {/* Component Health */}
            <div>
                <h3 className="text-sm font-medium text-gray-300 mb-3">Component Health</h3>
                <div className="grid grid-cols-3 gap-3">
                    {components.map(comp => {
                        const Icon = COMPONENT_ICONS[comp.name] || Server;
                        return (
                            <div
                                key={comp.name}
                                className="bg-[#1e1e1e] border border-[#333] rounded-lg p-4 hover:border-[#444] transition-colors"
                            >
                                <div className="flex items-center justify-between mb-3">
                                    <div className="flex items-center gap-2">
                                        <Icon className="w-4 h-4 text-gray-400" />
                                        <span className="text-sm font-medium text-gray-200">{comp.name}</span>
                                    </div>
                                    {statusIcon(comp.status as 'healthy' | 'degraded' | 'down')}
                                </div>
                                <div className="space-y-1.5">
                                    <div className="flex items-center justify-between text-[11px]">
                                        <span className="text-gray-500">Status</span>
                                        <span className={`px-1.5 py-0.5 rounded border text-[9px] font-medium uppercase tracking-wider ${statusBadge(comp.status as 'healthy' | 'degraded' | 'down')}`}>
                                            {comp.status}
                                        </span>
                                    </div>
                                    <div className="flex items-center justify-between text-[11px]">
                                        <span className="text-gray-500">Latency</span>
                                        <span className="text-gray-300 font-mono">{comp.latency_ms.toFixed(1)}ms</span>
                                    </div>
                                    {comp.version && (
                                        <div className="flex items-center justify-between text-[11px]">
                                            <span className="text-gray-500">Version</span>
                                            <span className="text-gray-300 font-mono">{comp.version}</span>
                                        </div>
                                    )}
                                </div>
                            </div>
                        );
                    })}
                </div>
            </div>

            {/* Connected Agents */}
            <div className="bg-[#1e1e1e] border border-[#333] rounded-lg p-5">
                <div className="flex items-center justify-between">
                    <div className="flex items-center gap-2">
                        <Monitor className="w-4 h-4 text-[#e05246]" />
                        <span className="text-sm font-medium text-gray-300">Connected Agents</span>
                    </div>
                    <div className="flex items-center gap-4">
                        <div className="flex items-center gap-1.5">
                            <span className="w-2 h-2 rounded-full bg-emerald-400" />
                            <span className="text-xs text-gray-400">{stats?.agents_online ?? 0} online</span>
                        </div>
                        <div className="flex items-center gap-1.5">
                            <span className="w-2 h-2 rounded-full bg-gray-600" />
                            <span className="text-xs text-gray-500">{(stats?.agents_total ?? 0) - (stats?.agents_online ?? 0)} offline</span>
                        </div>
                    </div>
                </div>
                {/* Mini sparkline placeholder */}
                <div className="mt-3 h-8 flex items-end gap-0.5">
                    {[40, 45, 42, 48, 55, 52, 60, 58, 65, 62, 70, 68, 75, 72, 78, 80, 85, 82, 90, 88, 92, 95, 100, 98].map((v, i) => (
                        <div
                            key={i}
                            className="flex-1 rounded-sm bg-emerald-500/30"
                            style={{ height: `${v}%` }}
                        />
                    ))}
                </div>
                <p className="text-[10px] text-gray-600 mt-1">Agent connections — last 24h</p>
            </div>
        </div>
    );
}
