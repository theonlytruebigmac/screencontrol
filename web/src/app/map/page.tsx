'use client';
import { getAccessToken } from '@/lib/auth-store';

/**
 * Agent Map View — geographic visualization of agent locations
 * using a CSS-based world map with plotted agent dots.
 *
 * Uses real agent data from the API. Agents are assigned pseudo-random
 * map coordinates based on a hash of their ID (geo location is not
 * tracked yet — this gives a repeatable visual spread).
 */

import { useState, useMemo, useEffect, useCallback } from 'react';
import {
    Globe,
    MapPin,
    Monitor,
    Wifi,
    WifiOff,
    Search,
    ZoomIn,
    ZoomOut,
    Loader2,
} from 'lucide-react';
import { useRouter } from 'next/navigation';
import { api, type Agent } from '@/lib/api';

const STATUS_COLORS: Record<string, string> = {
    online: '#22c55e',
    offline: '#6b7280',
    busy: '#f59e0b',
};

const OS_BADGE: Record<string, { label: string; cls: string }> = {
    windows: { label: 'WIN', cls: 'bg-blue-500/15 text-blue-400' },
    linux: { label: 'LNX', cls: 'bg-amber-500/15 text-amber-400' },
    macos: { label: 'MAC', cls: 'bg-purple-500/15 text-purple-300' },
};

// Pseudo-random position based on agent id so it is stable across renders
function hashPos(id: string): { lat: number; lng: number } {
    let h = 0;
    for (let i = 0; i < id.length; i++) {
        h = (h * 31 + id.charCodeAt(i)) & 0x7fffffff;
    }
    const lat = 15 + (h % 55);           // 15-70% from top
    const lng = 5 + ((h >> 8) % 85);     // 5-90% from left
    return { lat, lng };
}

interface MapAgent extends Agent {
    lat: number;
    lng: number;
}

export default function MapPage() {
    const router = useRouter();
    const [agents, setAgents] = useState<MapAgent[]>([]);
    const [loading, setLoading] = useState(true);
    const [selected, setSelected] = useState<MapAgent | null>(null);
    const [search, setSearch] = useState('');
    const [statusFilter, setStatusFilter] = useState<'all' | 'online' | 'offline' | 'busy'>('all');
    const [zoom, setZoom] = useState(1);

    const fetchAgents = useCallback(async () => {
        try {
            const token = getAccessToken();
            if (token) api.setToken(token);
            const data = await api.getAgents();
            setAgents(data.map(a => ({ ...a, ...hashPos(a.id) })));
        } catch (e) {
            console.error('Failed to load agents:', e);
        } finally {
            setLoading(false);
        }
    }, []);

    useEffect(() => { fetchAgents(); }, [fetchAgents]);

    const filteredAgents = useMemo(() => {
        let list = agents;
        if (statusFilter !== 'all') list = list.filter(a => a.status === statusFilter);
        if (search) {
            const q = search.toLowerCase();
            list = list.filter(a => a.machine_name.toLowerCase().includes(q) || a.os.toLowerCase().includes(q));
        }
        return list;
    }, [agents, search, statusFilter]);

    const counts = useMemo(() => ({
        total: agents.length,
        online: agents.filter(a => a.status === 'online').length,
        offline: agents.filter(a => a.status === 'offline').length,
        busy: agents.filter(a => a.status === 'busy').length,
    }), [agents]);

    if (loading) {
        return (
            <div className="flex items-center justify-center h-full">
                <Loader2 className="w-6 h-6 text-[#e05246] animate-spin" />
            </div>
        );
    }

    return (
        <div className="flex flex-col h-full overflow-hidden">
            {/* Header */}
            <header className="flex flex-wrap items-center justify-between gap-3 px-6 py-4 border-b border-[#333] flex-shrink-0">
                <div>
                    <h1 className="text-lg font-bold text-white flex items-center gap-2">
                        <Globe className="w-5 h-5 text-[#e05246]" />
                        Agent Map
                    </h1>
                    <p className="text-xs text-gray-500 mt-0.5">{counts.total} agents • {counts.online} online • {counts.offline} offline</p>
                </div>
                <div className="flex items-center gap-2">
                    {(['all', 'online', 'busy', 'offline'] as const).map(s => (
                        <button key={s} onClick={() => setStatusFilter(s)} className={`px-3 py-1.5 rounded-lg text-[11px] font-medium capitalize transition-colors ${statusFilter === s ? 'bg-[#e05246] text-white' : 'bg-[#1e1e1e] text-gray-400 border border-[#333] hover:border-[#555]'}`}>
                            {s === 'all' ? `All (${counts.total})` : `${s} (${counts[s as keyof typeof counts]})`}
                        </button>
                    ))}
                </div>
            </header>

            <div className="flex flex-1 min-h-0">
                {/* Map area */}
                <div className="flex-1 relative bg-[#0d1117] overflow-hidden">
                    {/* Zoom controls */}
                    <div className="absolute top-4 right-4 z-10 flex flex-col gap-1">
                        <button onClick={() => setZoom(z => Math.min(z + 0.25, 2))} className="p-2 bg-[#1e1e1e] border border-[#333] rounded-lg text-gray-400 hover:text-white transition-colors"><ZoomIn className="w-4 h-4" /></button>
                        <button onClick={() => setZoom(z => Math.max(z - 0.25, 0.5))} className="p-2 bg-[#1e1e1e] border border-[#333] rounded-lg text-gray-400 hover:text-white transition-colors"><ZoomOut className="w-4 h-4" /></button>
                    </div>

                    {/* Map grid */}
                    <div className="absolute inset-0 transition-transform duration-300" style={{ transform: `scale(${zoom})` }}>
                        {/* Grid lines */}
                        <svg className="w-full h-full absolute inset-0 opacity-10">
                            {Array.from({ length: 12 }).map((_, i) => (
                                <line key={`v${i}`} x1={`${(i + 1) * 8}%`} y1="0" x2={`${(i + 1) * 8}%`} y2="100%" stroke="#3b82f6" strokeWidth="0.5" />
                            ))}
                            {Array.from({ length: 8 }).map((_, i) => (
                                <line key={`h${i}`} x1="0" y1={`${(i + 1) * 12}%`} x2="100%" y2={`${(i + 1) * 12}%`} stroke="#3b82f6" strokeWidth="0.5" />
                            ))}
                        </svg>

                        {/* Simplified continent outlines (decorative) */}
                        <div className="absolute inset-0">
                            <div className="absolute w-[16%] h-[25%] top-[18%] left-[10%] border border-[#1e3a5f] rounded-xl opacity-30" />
                            <div className="absolute w-[10%] h-[15%] top-[20%] left-[45%] border border-[#1e3a5f] rounded-xl opacity-30" />
                            <div className="absolute w-[20%] h-[25%] top-[18%] left-[58%] border border-[#1e3a5f] rounded-xl opacity-30" />
                            <div className="absolute w-[8%] h-[22%] top-[50%] left-[22%] border border-[#1e3a5f] rounded-xl opacity-30" />
                            <div className="absolute w-[10%] h-[28%] top-[35%] left-[47%] border border-[#1e3a5f] rounded-xl opacity-30" />
                            <div className="absolute w-[8%] h-[12%] top-[60%] left-[75%] border border-[#1e3a5f] rounded-xl opacity-30" />
                        </div>

                        {/* Agent dots */}
                        {filteredAgents.map(agent => (
                            <button
                                key={agent.id}
                                onClick={() => setSelected(selected?.id === agent.id ? null : agent)}
                                className="absolute group"
                                style={{ top: `${agent.lat}%`, left: `${agent.lng}%`, transform: 'translate(-50%, -50%)' }}
                            >
                                {agent.status === 'online' && (
                                    <div className="absolute inset-0 w-6 h-6 -m-1.5 rounded-full animate-ping opacity-20" style={{ backgroundColor: STATUS_COLORS[agent.status] }} />
                                )}
                                <div className={`w-3 h-3 rounded-full border-2 border-[#0d1117] transition-transform group-hover:scale-150 ${selected?.id === agent.id ? 'scale-150 ring-2 ring-white/30' : ''}`} style={{ backgroundColor: STATUS_COLORS[agent.status] || '#6b7280' }} />
                                <div className="absolute bottom-full left-1/2 -translate-x-1/2 mb-2 opacity-0 group-hover:opacity-100 transition-opacity pointer-events-none whitespace-nowrap">
                                    <div className="bg-[#1e1e1e] border border-[#333] rounded-lg px-2 py-1 shadow-xl">
                                        <span className="text-[10px] text-white font-medium">{agent.machine_name}</span>
                                    </div>
                                </div>
                            </button>
                        ))}
                    </div>

                    {/* Legend */}
                    <div className="absolute bottom-4 left-4 bg-[#1e1e1e]/90 backdrop-blur border border-[#333] rounded-lg px-3 py-2 flex items-center gap-3">
                        {[['online', '#22c55e'], ['busy', '#f59e0b'], ['offline', '#6b7280']].map(([label, color]) => (
                            <div key={label} className="flex items-center gap-1.5">
                                <div className="w-2 h-2 rounded-full" style={{ backgroundColor: color as string }} />
                                <span className="text-[10px] text-gray-400 capitalize">{label}</span>
                            </div>
                        ))}
                    </div>

                    {agents.length === 0 && (
                        <div className="absolute inset-0 flex flex-col items-center justify-center text-gray-600">
                            <Globe className="w-12 h-12 mb-3 opacity-20" />
                            <span className="text-sm">No agents registered</span>
                        </div>
                    )}
                </div>

                {/* Sidebar - agent list & detail */}
                <aside className="w-72 border-l border-[#333] bg-[#1a1a1a] flex flex-col flex-shrink-0">
                    <div className="px-3 py-3 border-b border-[#333]">
                        <div className="relative">
                            <Search className="absolute left-2.5 top-1/2 -translate-y-1/2 w-3 h-3 text-gray-600" />
                            <input value={search} onChange={e => setSearch(e.target.value)} placeholder="Search agents..." className="w-full bg-[#141414] border border-[#333] rounded-lg pl-8 pr-3 py-2 text-[11px] text-gray-300 placeholder-gray-600 focus:outline-none focus:border-[#e05246]" />
                        </div>
                    </div>

                    <div className="flex-1 overflow-y-auto">
                        {filteredAgents.map(agent => {
                            const osBadge = OS_BADGE[agent.os.toLowerCase()] || OS_BADGE.linux;
                            return (
                                <button key={agent.id} onClick={() => setSelected(agent)} className={`w-full text-left px-3 py-2.5 border-b border-[#252525] hover:bg-white/3 transition-colors ${selected?.id === agent.id ? 'bg-[#e05246]/10 border-l-2 border-l-[#e05246]' : ''}`}>
                                    <div className="flex items-center gap-2">
                                        <div className="w-2 h-2 rounded-full flex-shrink-0" style={{ backgroundColor: STATUS_COLORS[agent.status] || '#6b7280' }} />
                                        <span className="text-xs text-white font-medium truncate">{agent.machine_name}</span>
                                        <span className={`text-[8px] px-1 py-0.5 rounded font-mono ${osBadge.cls}`}>{osBadge.label}</span>
                                    </div>
                                    <div className="flex items-center gap-1 mt-0.5 ml-4">
                                        <MapPin className="w-2.5 h-2.5 text-gray-600" />
                                        <span className="text-[10px] text-gray-500">{agent.os} {agent.arch}</span>
                                    </div>
                                </button>
                            );
                        })}
                    </div>

                    {/* Selected detail */}
                    {selected && (
                        <div className="border-t border-[#333] p-3 space-y-2 bg-[#141414]">
                            <div className="flex items-center gap-2">
                                <Monitor className="w-4 h-4 text-[#e05246]" />
                                <span className="text-xs font-semibold text-white">{selected.machine_name}</span>
                            </div>
                            <div className="space-y-1 text-[10px]">
                                {[
                                    ['Status', selected.status],
                                    ['OS', `${selected.os} ${selected.os_version}`],
                                    ['Arch', selected.arch],
                                    ['Version', selected.agent_version],
                                ].map(([k, v]) => (
                                    <div key={k} className="flex justify-between">
                                        <span className="text-gray-500">{k}</span>
                                        <span className="text-gray-300 capitalize">{v}</span>
                                    </div>
                                ))}
                            </div>
                            <button onClick={() => router.push(`/agents/${selected.id}`)} className="w-full mt-1 px-3 py-2 text-[11px] font-medium text-white bg-[#e05246] hover:bg-[#c94539] rounded-lg transition-colors text-center">
                                View Details
                            </button>
                        </div>
                    )}
                </aside>
            </div>
        </div>
    );
}
