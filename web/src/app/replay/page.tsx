'use client';
import { getAccessToken } from '@/lib/auth-store';

/**
 * Session Replay page.
 *
 * Browse and playback recorded remote sessions.
 * Shows a list of completed sessions with duration and details.
 * Clicking "Watch" fetches the pre-signed recording URL and plays
 * the WebM back in a native HTML5 video element.
 */

import { useState, useMemo, useRef, useEffect } from 'react';
import {
    PlayCircle,
    Play,
    Clock,
    Monitor,
    User,
    Download,
    Search,
    Eye,
    X,
    ChevronLeft,
    Loader2,
} from 'lucide-react';
import { useToast } from '@/components/toast';
import { api, type Session, type Agent } from '@/lib/api';

const OS_BADGE: Record<string, { className: string; label: string }> = {
    windows: { className: 'bg-blue-500/15 text-blue-300', label: 'WIN' },
    macos: { className: 'bg-gray-500/15 text-gray-300', label: 'MAC' },
    linux: { className: 'bg-emerald-500/15 text-emerald-300', label: 'LNX' },
};

interface RecordingView {
    session: Session;
    agentName: string;
    agentOs: string;
    durationSec: number;
    durationStr: string;
}

function formatDuration(seconds: number): string {
    const m = Math.floor(seconds / 60);
    const s = seconds % 60;
    return `${m}:${s.toString().padStart(2, '0')}`;
}

function timeAgo(dateStr: string): string {
    const diff = Date.now() - new Date(dateStr).getTime();
    const mins = Math.floor(diff / 60000);
    if (mins < 1) return 'Just now';
    if (mins < 60) return `${mins}m ago`;
    const hours = Math.floor(mins / 60);
    if (hours < 24) return `${hours}h ago`;
    const days = Math.floor(hours / 24);
    return `${days}d ago`;
}

function sessionDuration(session: Session): number {
    if (!session.ended_at) return 0;
    return Math.max(0, Math.floor((new Date(session.ended_at).getTime() - new Date(session.started_at).getTime()) / 1000));
}

// ─── Playback Viewer ─────────────────────────────
function PlaybackViewer({ recording, onClose }: { recording: RecordingView; onClose: () => void }) {
    const videoRef = useRef<HTMLVideoElement | null>(null);
    const [videoUrl, setVideoUrl] = useState<string | null>(null);
    const [loadError, setLoadError] = useState(false);
    const [loadingUrl, setLoadingUrl] = useState(true);

    // Fetch the pre-signed recording URL on mount
    useEffect(() => {
        let cancelled = false;
        (async () => {
            try {
                const token = getAccessToken();
                if (token) api.setToken(token);
                const { url } = await api.getRecordingUrl(recording.session.id);
                if (!cancelled) setVideoUrl(url);
            } catch {
                if (!cancelled) setLoadError(true);
            } finally {
                if (!cancelled) setLoadingUrl(false);
            }
        })();
        return () => { cancelled = true; };
    }, [recording.session.id]);

    return (
        <div className="fixed inset-0 bg-black/90 z-50 flex flex-col">
            {/* Header */}
            <div className="flex items-center justify-between px-4 py-3 bg-[#1a1a1a] border-b border-[#333]">
                <div className="flex items-center gap-3">
                    <button onClick={onClose} className="p-1.5 text-gray-400 hover:text-white rounded-lg hover:bg-white/5"><ChevronLeft className="w-4 h-4" /></button>
                    <div>
                        <h2 className="text-sm font-semibold text-white">{recording.agentName}</h2>
                        <span className="text-[10px] text-gray-500">{recording.session.session_type} • {timeAgo(recording.session.started_at)} • {recording.durationStr}</span>
                    </div>
                </div>
                <button onClick={onClose} className="p-1.5 text-gray-400 hover:text-white"><X className="w-4 h-4" /></button>
            </div>

            {/* Video area */}
            <div className="flex-1 flex items-center justify-center bg-black relative min-h-0">
                {loadingUrl && (
                    <div className="text-center">
                        <Loader2 className="w-8 h-8 animate-spin text-gray-600 mx-auto mb-2" />
                        <p className="text-gray-600 text-xs">Loading recording…</p>
                    </div>
                )}

                {loadError && !loadingUrl && (
                    <div className="text-center">
                        <Monitor className="w-16 h-16 text-gray-800 mx-auto mb-3" />
                        <p className="text-gray-500 text-sm">No recording available</p>
                        <p className="text-gray-700 text-xs mt-1">This session was not recorded or the recording has expired.</p>
                    </div>
                )}

                {videoUrl && !loadingUrl && (
                    <video
                        ref={videoRef}
                        src={videoUrl}
                        controls
                        autoPlay
                        className="max-w-full max-h-full rounded-lg"
                        style={{ background: '#000' }}
                        onError={() => setLoadError(true)}
                    />
                )}
            </div>
        </div>
    );
}

// ─── Main Component ──────────────────────────────
export default function ReplayPage() {
    const { info } = useToast();
    const [recordings, setRecordings] = useState<RecordingView[]>([]);
    const [loading, setLoading] = useState(true);
    const [search, setSearch] = useState('');
    const [typeFilter, setTypeFilter] = useState<'all' | 'desktop' | 'terminal'>('all');
    const [viewing, setViewing] = useState<RecordingView | null>(null);

    useEffect(() => {
        (async () => {
            try {
                const token = getAccessToken();
                if (token) api.setToken(token);
                const [sessions, agents] = await Promise.all([api.getSessions(), api.getAgents()]);
                const agentMap = new Map<string, Agent>();
                agents.forEach(a => agentMap.set(a.id, a));

                // Build recordings from ended sessions
                const recs: RecordingView[] = sessions
                    .filter(s => s.ended_at)
                    .map(s => {
                        const agent = agentMap.get(s.agent_id);
                        const dur = sessionDuration(s);
                        return {
                            session: s,
                            agentName: agent?.machine_name || 'Unknown Agent',
                            agentOs: (agent?.os || 'linux').toLowerCase(),
                            durationSec: dur,
                            durationStr: formatDuration(dur),
                        };
                    })
                    .sort((a, b) => new Date(b.session.started_at).getTime() - new Date(a.session.started_at).getTime());

                setRecordings(recs);
            } catch (e) {
                console.error('Failed to load recordings:', e);
            } finally {
                setLoading(false);
            }
        })();
    }, []);

    const filtered = useMemo(() => {
        let list = recordings;
        if (typeFilter !== 'all') list = list.filter(r => r.session.session_type === typeFilter);
        if (search) {
            const q = search.toLowerCase();
            list = list.filter(r => r.agentName.toLowerCase().includes(q));
        }
        return list;
    }, [recordings, search, typeFilter]);

    if (loading) {
        return (
            <div className="flex items-center justify-center h-64">
                <Loader2 className="w-6 h-6 animate-spin text-gray-600" />
            </div>
        );
    }

    return (
        <div className="flex flex-col h-full overflow-y-auto">
            <header className="flex flex-wrap items-center justify-between gap-3 px-6 py-4 border-b border-[#333] flex-shrink-0">
                <div>
                    <h1 className="text-lg font-bold text-white flex items-center gap-2">
                        <PlayCircle className="w-5 h-5 text-[#e05246]" />
                        Session Replay
                    </h1>
                    <p className="text-xs text-gray-500 mt-0.5">{recordings.length} completed sessions</p>
                </div>
                <div className="flex items-center gap-2">
                    <div className="flex bg-[#1e1e1e] border border-[#333] rounded-lg overflow-hidden">
                        {(['all', 'desktop', 'terminal'] as const).map(t => (
                            <button key={t} onClick={() => setTypeFilter(t)} className={`px-3 py-1.5 text-[11px] font-medium capitalize transition-colors ${typeFilter === t ? 'bg-[#e05246] text-white' : 'text-gray-400 hover:text-white hover:bg-white/5'}`}>
                                {t}
                            </button>
                        ))}
                    </div>
                </div>
            </header>

            <div className="p-6 space-y-4">
                <div className="relative">
                    <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-gray-600" />
                    <input value={search} onChange={e => setSearch(e.target.value)} placeholder="Search by agent name..." className="w-full bg-[#1e1e1e] border border-[#333] rounded-lg pl-9 pr-3 py-2.5 text-xs text-gray-300 placeholder-gray-600 focus:outline-none focus:border-[#e05246]" />
                </div>

                <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
                    {filtered.map(rec => {
                        const badge = OS_BADGE[rec.agentOs];
                        return (
                            <div key={rec.session.id} className="bg-[#1e1e1e] border border-[#2a2a2a] rounded-xl overflow-hidden hover:border-[#444] transition-colors group">
                                <div
                                    className="relative h-36 bg-[#0d0d0d] flex items-center justify-center cursor-pointer"
                                    onClick={() => setViewing(rec)}
                                >
                                    <Monitor className="w-12 h-12 text-gray-800" />
                                    <div className="absolute inset-0 bg-black/40 flex items-center justify-center opacity-0 group-hover:opacity-100 transition-opacity">
                                        <div className="w-12 h-12 rounded-full bg-[#e05246] flex items-center justify-center shadow-lg">
                                            <Play className="w-5 h-5 text-white ml-0.5" />
                                        </div>
                                    </div>
                                    <div className="absolute bottom-2 right-2 bg-black/80 px-2 py-0.5 rounded text-[10px] text-white font-mono">
                                        {rec.durationStr}
                                    </div>
                                    <div className={`absolute top-2 left-2 px-1.5 py-0.5 rounded text-[8px] font-semibold uppercase ${rec.session.session_type === 'desktop' ? 'bg-[#e05246]/20 text-[#e05246]' : 'bg-cyan-500/20 text-cyan-400'}`}>
                                        {rec.session.session_type}
                                    </div>
                                </div>

                                <div className="p-3">
                                    <div className="flex items-center gap-1.5 mb-1">
                                        <span className="text-xs font-medium text-white truncate">{rec.agentName}</span>
                                        {badge && <span className={`px-1 py-0.5 rounded text-[8px] font-semibold ${badge.className}`}>{badge.label}</span>}
                                    </div>
                                    <div className="flex items-center gap-3 text-[10px] text-gray-500 mb-2">
                                        <span className="flex items-center gap-1"><Clock className="w-3 h-3" />{timeAgo(rec.session.started_at)}</span>
                                        <span className="flex items-center gap-1"><User className="w-3 h-3" />{rec.session.session_type}</span>
                                    </div>
                                    <div className="flex items-center gap-1">
                                        <button onClick={() => setViewing(rec)} className="flex items-center gap-1 px-2 py-1 text-[10px] font-medium text-white bg-[#e05246] hover:bg-[#c94539] rounded transition-colors">
                                            <Eye className="w-3 h-3" /> Watch
                                        </button>
                                        <button onClick={async () => {
                                            try {
                                                const { url } = await api.getRecordingUrl(rec.session.id);
                                                window.open(url, '_blank');
                                            } catch {
                                                info('Download', 'Recording not yet available for this session');
                                            }
                                        }} className="p-1 text-gray-400 hover:text-white hover:bg-white/5 rounded transition-colors">
                                            <Download className="w-3 h-3" />
                                        </button>
                                    </div>
                                </div>
                            </div>
                        );
                    })}
                </div>

                {filtered.length === 0 && (
                    <div className="flex flex-col items-center justify-center py-12 text-gray-600">
                        <PlayCircle className="w-10 h-10 mb-2 opacity-30" />
                        <span className="text-sm">{recordings.length === 0 ? 'No completed sessions yet' : 'No recordings found'}</span>
                    </div>
                )}
            </div>

            {viewing && <PlaybackViewer recording={viewing} onClose={() => setViewing(null)} />}
        </div>
    );
}
