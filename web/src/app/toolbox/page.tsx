'use client';
import { getAccessToken } from '@/lib/auth-store';

/**
 * Toolbox — Script Library page.
 *
 * Save, organize, categorise, and run scripts across agents.
 * Supports PowerShell, Bash, Python, and Batch scripts with
 * syntax highlighting, folder organization, and one-click execution.
 */

import { useState, useCallback, useMemo, useRef, useEffect } from 'react';
import {
    Code2,
    Plus,
    Search,
    FolderOpen,
    Play,
    Copy,
    Trash2,
    Edit2,
    Star,
    StarOff,
    FileCode,
    X,
    Check,
    ChevronRight,
    ChevronDown,
    Upload,
    Loader2,
} from 'lucide-react';
import { useToast } from '@/components/toast';
import { api, type Script, type Agent } from '@/lib/api';
import { useAgentSocket, type CommandResult } from '@/lib/use-agent-socket';

const LANG_CONFIG: Record<string, { label: string; color: string; bg: string; ext: string }> = {
    powershell: { label: 'PowerShell', color: '#5391fe', bg: 'bg-blue-500/15', ext: '.ps1' },
    bash: { label: 'Bash', color: '#4ec9b0', bg: 'bg-emerald-500/15', ext: '.sh' },
    python: { label: 'Python', color: '#f5c842', bg: 'bg-amber-500/15', ext: '.py' },
    batch: { label: 'Batch', color: '#c586c0', bg: 'bg-purple-500/15', ext: '.bat' },
};

const FOLDERS = ['All', 'Diagnostics', 'Maintenance', 'Deployment', 'Security', 'General'];

// ─── Script Editor Modal ─────────────────────────
function ScriptEditor({
    script,
    onSave,
    onClose,
}: {
    script: Script | null;
    onSave: (data: { name: string; code: string; description?: string; language?: string; folder?: string; tags?: string[]; starred?: boolean }, id?: string) => void;
    onClose: () => void;
}) {
    const [name, setName] = useState(script?.name || '');
    const [description, setDescription] = useState(script?.description || '');
    const [language, setLanguage] = useState(script?.language || 'bash');
    const [code, setCode] = useState(script?.code || '');
    const [folder, setFolder] = useState(script?.folder || 'General');
    const [tagInput, setTagInput] = useState('');
    const [tags, setTags] = useState<string[]>(script?.tags || []);

    const handleSave = () => {
        if (!name.trim() || !code.trim()) return;
        onSave({ name: name.trim(), code, description: description.trim(), language, folder, tags, starred: script?.starred || false }, script?.id);
    };

    const addTag = () => {
        if (tagInput.trim() && !tags.includes(tagInput.trim())) {
            setTags([...tags, tagInput.trim()]);
            setTagInput('');
        }
    };

    const lang = LANG_CONFIG[language] || LANG_CONFIG.bash;

    return (
        <div className="fixed inset-0 bg-black/70 z-50 flex items-center justify-center p-4">
            <div className="bg-[#1e1e1e] border border-[#333] rounded-xl w-full max-w-3xl max-h-[90vh] overflow-y-auto shadow-2xl">
                <div className="flex items-center justify-between px-5 py-4 border-b border-[#333]">
                    <h2 className="text-sm font-semibold text-white">{script ? 'Edit Script' : 'New Script'}</h2>
                    <button onClick={onClose} className="p-1 text-gray-400 hover:text-white"><X className="w-4 h-4" /></button>
                </div>
                <div className="p-5 space-y-4">
                    <div className="grid grid-cols-[1fr_150px] gap-3">
                        <div>
                            <label className="text-[10px] text-gray-500 uppercase tracking-wider mb-1 block">Name</label>
                            <input value={name} onChange={e => setName(e.target.value)} className="w-full bg-[#141414] border border-[#333] rounded-lg px-3 py-2 text-sm text-white focus:outline-none focus:border-[#e05246]" placeholder="Script name" />
                        </div>
                        <div>
                            <label className="text-[10px] text-gray-500 uppercase tracking-wider mb-1 block">Language</label>
                            <select value={language} onChange={e => setLanguage(e.target.value)} className="w-full bg-[#141414] border border-[#333] rounded-lg px-3 py-2 text-sm text-white focus:outline-none focus:border-[#e05246]">
                                {Object.entries(LANG_CONFIG).map(([k, v]) => (<option key={k} value={k}>{v.label}</option>))}
                            </select>
                        </div>
                    </div>
                    <div>
                        <label className="text-[10px] text-gray-500 uppercase tracking-wider mb-1 block">Description</label>
                        <input value={description} onChange={e => setDescription(e.target.value)} className="w-full bg-[#141414] border border-[#333] rounded-lg px-3 py-2 text-sm text-white focus:outline-none focus:border-[#e05246]" placeholder="Brief description..." />
                    </div>
                    <div className="grid grid-cols-2 gap-3">
                        <div>
                            <label className="text-[10px] text-gray-500 uppercase tracking-wider mb-1 block">Folder</label>
                            <select value={folder} onChange={e => setFolder(e.target.value)} className="w-full bg-[#141414] border border-[#333] rounded-lg px-3 py-2 text-sm text-white focus:outline-none focus:border-[#e05246]">
                                {FOLDERS.filter(f => f !== 'All').map(f => (<option key={f} value={f}>{f}</option>))}
                            </select>
                        </div>
                        <div>
                            <label className="text-[10px] text-gray-500 uppercase tracking-wider mb-1 block">Tags</label>
                            <div className="flex items-center gap-1">
                                <input value={tagInput} onChange={e => setTagInput(e.target.value)} onKeyDown={e => { if (e.key === 'Enter') { e.preventDefault(); addTag(); } }} className="flex-1 bg-[#141414] border border-[#333] rounded-lg px-3 py-2 text-sm text-white focus:outline-none focus:border-[#e05246]" placeholder="Add tag..." />
                                <button onClick={addTag} className="p-2 text-gray-400 hover:text-white bg-[#141414] border border-[#333] rounded-lg"><Plus className="w-3.5 h-3.5" /></button>
                            </div>
                            {tags.length > 0 && (
                                <div className="flex flex-wrap gap-1 mt-1.5">
                                    {tags.map(t => (
                                        <span key={t} className="flex items-center gap-1 px-2 py-0.5 text-[10px] rounded-full bg-[#e05246]/15 text-[#f06b60]">
                                            {t}
                                            <button onClick={() => setTags(tags.filter(x => x !== t))} className="hover:text-white"><X className="w-2.5 h-2.5" /></button>
                                        </span>
                                    ))}
                                </div>
                            )}
                        </div>
                    </div>
                    <div>
                        <label className="text-[10px] text-gray-500 uppercase tracking-wider mb-1 block">
                            Script Code <span className="ml-2 text-gray-600 normal-case">{lang.ext}</span>
                        </label>
                        <textarea value={code} onChange={e => setCode(e.target.value)} className="w-full h-64 bg-[#0d0d0d] border border-[#333] rounded-lg px-4 py-3 text-sm text-emerald-300 font-mono resize-none focus:outline-none focus:border-[#e05246] leading-relaxed" placeholder={`# Enter your ${lang.label} script here...`} spellCheck={false} />
                    </div>
                </div>
                <div className="flex items-center justify-end gap-2 px-5 py-4 border-t border-[#333]">
                    <button onClick={onClose} className="px-4 py-2 text-sm text-gray-400 hover:text-white hover:bg-white/5 rounded-lg">Cancel</button>
                    <button onClick={handleSave} disabled={!name.trim() || !code.trim()} className="flex items-center gap-2 px-4 py-2 text-sm font-medium text-white bg-[#e05246] hover:bg-[#c94539] rounded-lg disabled:opacity-40">
                        <Check className="w-3.5 h-3.5" /> {script ? 'Save Changes' : 'Create Script'}
                    </button>
                </div>
            </div>
        </div>
    );
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

// ─── Main Component ──────────────────────────────
export default function ToolboxPage() {
    const { success, info, error: toastError } = useToast();
    const [scripts, setScripts] = useState<Script[]>([]);
    const [loading, setLoading] = useState(true);
    const [search, setSearch] = useState('');
    const [activeFolder, setActiveFolder] = useState('All');
    const [activeLang, setActiveLang] = useState<string | null>(null);
    const [showEditor, setShowEditor] = useState(false);
    const [editScript, setEditScript] = useState<Script | null>(null);
    const [expandedId, setExpandedId] = useState<string | null>(null);
    const [ctxMenu, setCtxMenu] = useState<{ x: number; y: number; script: Script } | null>(null);
    const ctxRef = useRef<HTMLDivElement>(null);

    // Agent picker for script execution
    const [agents, setAgents] = useState<Agent[]>([]);
    const [selectedAgentId, setSelectedAgentId] = useState<string | null>(null);
    const [runResult, setRunResult] = useState<{ script: string; result: CommandResult } | null>(null);
    const [running, setRunning] = useState(false);

    const fetchScripts = useCallback(async () => {
        try {
            const token = getAccessToken();
            if (token) api.setToken(token);
            const data = await api.getScripts();
            setScripts(data);
        } catch (e) {
            console.error('Failed to load scripts:', e);
        } finally {
            setLoading(false);
        }
    }, []);

    useEffect(() => { fetchScripts(); }, [fetchScripts]);

    // Fetch online agents for the agent picker
    useEffect(() => {
        const token = getAccessToken();
        if (token) api.setToken(token);
        api.getAgents()
            .then((data) => {
                const online = data.filter(a => a.status === 'online');
                setAgents(online);
                if (online.length > 0 && !selectedAgentId) setSelectedAgentId(online[0].id);
            })
            .catch(() => { /* silently fail */ });
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, []);

    // Agent WebSocket for command execution
    const { connect: connectWs, disconnect: disconnectWs, sendCommand, status: wsStatus } = useAgentSocket({
        agentId: selectedAgentId || '',
        onCommandResponse: useCallback((result: CommandResult) => {
            setRunResult(prev => prev ? { ...prev, result } : null);
            setRunning(false);
        }, []),
    });

    // Cleanup WS on unmount
    useEffect(() => {
        return () => { disconnectWs(); };
    }, [disconnectWs]);

    useEffect(() => {
        if (!ctxMenu) return;
        const handler = (e: MouseEvent) => {
            if (ctxRef.current && !ctxRef.current.contains(e.target as Node)) setCtxMenu(null);
        };
        document.addEventListener('mousedown', handler);
        return () => document.removeEventListener('mousedown', handler);
    }, [ctxMenu]);

    const filtered = useMemo(() => {
        let list = scripts;
        if (activeFolder !== 'All') list = list.filter(s => (s.folder || 'General') === activeFolder);
        if (activeLang) list = list.filter(s => s.language === activeLang);
        if (search) {
            const q = search.toLowerCase();
            list = list.filter(s =>
                s.name.toLowerCase().includes(q) ||
                (s.description || '').toLowerCase().includes(q) ||
                (s.tags || []).some(t => t.includes(q))
            );
        }
        return list.sort((a, b) => ((b.starred ? 1 : 0) - (a.starred ? 1 : 0)));
    }, [scripts, activeFolder, activeLang, search]);

    const handleSave = useCallback(async (data: { name: string; code: string; description?: string; language?: string; folder?: string; tags?: string[]; starred?: boolean }, id?: string) => {
        try {
            const token = getAccessToken();
            if (token) api.setToken(token);
            if (id) {
                await api.updateScript(id, data);
            } else {
                await api.createScript(data);
            }
            await fetchScripts();
            setShowEditor(false);
            setEditScript(null);
            success('Script saved', data.name);
        } catch (e) {
            toastError('Failed', String(e));
        }
    }, [success, toastError, fetchScripts]);

    const handleDelete = useCallback(async (id: string) => {
        try {
            const token = getAccessToken();
            if (token) api.setToken(token);
            await api.deleteScript(id);
            await fetchScripts();
            success('Script deleted');
        } catch (e) {
            toastError('Failed', String(e));
        }
    }, [success, toastError, fetchScripts]);

    const handleToggleStar = useCallback(async (script: Script) => {
        try {
            const token = getAccessToken();
            if (token) api.setToken(token);
            await api.updateScript(script.id, { starred: !script.starred });
            await fetchScripts();
        } catch { /* ignore */ }
    }, [fetchScripts]);

    const handleRun = useCallback(async (script: Script) => {
        if (!selectedAgentId) {
            toastError('No Agent Selected', 'Please select an agent from the dropdown to run scripts.');
            return;
        }
        setRunning(true);
        setRunResult({ script: script.name, result: { exitCode: -1, stdout: '', stderr: '', timedOut: false } });

        // Ensure WS is connected
        if (wsStatus !== 'connected') {
            await connectWs();
            // Give WebSocket time to connect
            await new Promise(r => setTimeout(r, 800));
        }

        sendCommand(script.code, [], 60);

        // Increment run count on server (fire-and-forget)
        const token = getAccessToken();
        if (token) api.setToken(token);
        api.updateScript(script.id, { run_count: (script.run_count || 0) + 1 }).then(() => fetchScripts());
    }, [selectedAgentId, wsStatus, connectWs, sendCommand, toastError, fetchScripts]);

    const handleDuplicate = useCallback(async (script: Script) => {
        try {
            const token = getAccessToken();
            if (token) api.setToken(token);
            await api.createScript({
                name: `${script.name} (copy)`,
                code: script.code,
                description: script.description || undefined,
                language: script.language,
                folder: script.folder || 'General',
                tags: script.tags || [],
            });
            await fetchScripts();
            success('Script duplicated');
        } catch (e) {
            toastError('Failed', String(e));
        }
        setCtxMenu(null);
    }, [success, toastError, fetchScripts]);

    const folderCounts = useMemo(() => {
        const counts: Record<string, number> = { All: scripts.length };
        scripts.forEach(s => { const f = s.folder || 'General'; counts[f] = (counts[f] || 0) + 1; });
        return counts;
    }, [scripts]);

    if (loading) {
        return (
            <div className="flex items-center justify-center h-64">
                <Loader2 className="w-6 h-6 animate-spin text-gray-600" />
            </div>
        );
    }

    return (
        <div className="flex flex-col h-full">
            {/* Header */}
            <header className="flex items-center justify-between px-6 py-4 border-b border-[#333] flex-shrink-0">
                <div>
                    <h1 className="text-lg font-bold text-white flex items-center gap-2">
                        <Code2 className="w-5 h-5 text-[#e05246]" />
                        Toolbox
                    </h1>
                    <p className="text-xs text-gray-500 mt-0.5">Script library — save, organize & run across agents</p>
                </div>
                <div className="flex items-center gap-2">
                    {/* Agent picker */}
                    <div className="flex items-center gap-1.5">
                        <span className="text-[10px] text-gray-600 uppercase tracking-wider">Target:</span>
                        <select
                            value={selectedAgentId || ''}
                            onChange={e => { setSelectedAgentId(e.target.value || null); disconnectWs(); }}
                            className="bg-[#141414] border border-[#333] text-xs text-gray-300 rounded-lg px-2 py-1.5 focus:outline-none focus:border-[#e05246] min-w-[140px] appearance-none"
                        >
                            {agents.length === 0 && <option value="">No agents online</option>}
                            {agents.map(a => (
                                <option key={a.id} value={a.id}>{a.machine_name}</option>
                            ))}
                        </select>
                        {wsStatus === 'connected' && (
                            <span className="w-1.5 h-1.5 rounded-full bg-emerald-400" title="Connected" />
                        )}
                    </div>
                    <button onClick={() => info('Import', 'Drop a .ps1, .sh, .py, or .bat file to import')} className="flex items-center gap-1.5 px-3 py-2 text-xs text-gray-400 hover:text-white hover:bg-white/5 rounded-lg border border-[#333]">
                        <Upload className="w-3.5 h-3.5" /> Import
                    </button>
                    <button onClick={() => { setEditScript(null); setShowEditor(true); }} className="flex items-center gap-1.5 px-3 py-2 text-xs font-medium text-white bg-[#e05246] hover:bg-[#c94539] rounded-lg">
                        <Plus className="w-3.5 h-3.5" /> New Script
                    </button>
                </div>
            </header>

            <div className="flex flex-1 min-h-0">
                {/* Sidebar */}
                <aside className="w-52 border-r border-[#333] bg-[#1a1a1a] flex-shrink-0 overflow-y-auto">
                    <div className="px-3 pt-3 pb-1">
                        <span className="text-[10px] text-gray-600 uppercase tracking-wider font-medium">Folders</span>
                    </div>
                    {FOLDERS.map(f => (
                        <button key={f} onClick={() => setActiveFolder(f)} className={`w-full flex items-center gap-2 px-3 py-2 text-xs transition-colors ${activeFolder === f ? 'bg-[#e05246]/10 text-[#f06b60] font-medium' : 'text-gray-400 hover:text-white hover:bg-white/5'}`}>
                            <FolderOpen className="w-3.5 h-3.5" />
                            <span className="flex-1 text-left">{f}</span>
                            <span className="text-[10px] text-gray-600">{folderCounts[f] || 0}</span>
                        </button>
                    ))}
                    <div className="border-t border-[#333] mx-3 my-2" />
                    <div className="px-3 pb-1">
                        <span className="text-[10px] text-gray-600 uppercase tracking-wider font-medium">Language</span>
                    </div>
                    {Object.entries(LANG_CONFIG).map(([key, cfg]) => (
                        <button key={key} onClick={() => setActiveLang(activeLang === key ? null : key)} className={`w-full flex items-center gap-2 px-3 py-2 text-xs transition-colors ${activeLang === key ? 'bg-[#e05246]/10 text-[#f06b60] font-medium' : 'text-gray-400 hover:text-white hover:bg-white/5'}`}>
                            <FileCode className="w-3.5 h-3.5" style={{ color: cfg.color }} />
                            <span className="flex-1 text-left">{cfg.label}</span>
                        </button>
                    ))}
                </aside>

                {/* Main content */}
                <div className="flex-1 flex flex-col min-w-0">
                    <div className="flex items-center gap-2 px-4 py-2.5 border-b border-[#333] bg-[#1e1e1e]">
                        <div className="relative flex-1">
                            <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-gray-600" />
                            <input type="text" value={search} onChange={e => setSearch(e.target.value)} placeholder="Search scripts..." className="w-full bg-[#141414] border border-[#333] rounded-lg pl-9 pr-3 py-2 text-xs text-gray-300 placeholder-gray-600 focus:outline-none focus:border-[#e05246]" />
                        </div>
                        <span className="text-[10px] text-gray-600">{filtered.length} script{filtered.length !== 1 ? 's' : ''}</span>
                    </div>

                    <div className="flex-1 overflow-y-auto">
                        {filtered.length === 0 ? (
                            <div className="flex flex-col items-center justify-center h-full text-gray-600">
                                <Code2 className="w-10 h-10 mb-3 opacity-30" />
                                <span className="text-sm">{scripts.length === 0 ? 'No scripts yet' : 'No scripts found'}</span>
                                <button onClick={() => { setEditScript(null); setShowEditor(true); }} className="mt-3 text-xs text-[#e05246] hover:underline">Create your first script</button>
                            </div>
                        ) : (
                            filtered.map(script => {
                                const lang = LANG_CONFIG[script.language] || LANG_CONFIG.bash;
                                const isExpanded = expandedId === script.id;
                                return (
                                    <div key={script.id} className="border-b border-[#2a2a2a] hover:bg-white/[0.02] transition-colors" onContextMenu={e => { e.preventDefault(); setCtxMenu({ x: e.clientX, y: e.clientY, script }); }}>
                                        <div className="flex items-center gap-3 px-4 py-3">
                                            <button onClick={() => handleToggleStar(script)} className={`flex-shrink-0 ${script.starred ? 'text-amber-400' : 'text-gray-700 hover:text-gray-400'}`}>
                                                {script.starred ? <Star className="w-3.5 h-3.5 fill-current" /> : <StarOff className="w-3.5 h-3.5" />}
                                            </button>
                                            <span className={`flex-shrink-0 px-2 py-0.5 rounded text-[9px] font-medium ${lang.bg}`} style={{ color: lang.color }}>{lang.label}</span>
                                            <button onClick={() => setExpandedId(isExpanded ? null : script.id)} className="flex-1 text-left min-w-0">
                                                <div className="text-xs font-medium text-white truncate">{script.name}</div>
                                                <div className="text-[10px] text-gray-500 truncate">{script.description || ''}</div>
                                            </button>
                                            <div className="hidden lg:flex items-center gap-1 flex-shrink-0">
                                                {(script.tags || []).slice(0, 2).map(t => (
                                                    <span key={t} className="px-1.5 py-0.5 text-[9px] rounded bg-[#333] text-gray-400">{t}</span>
                                                ))}
                                            </div>
                                            <div className="flex items-center gap-1 text-[10px] text-gray-600 flex-shrink-0 w-16">
                                                <Play className="w-3 h-3" />{script.run_count || 0} runs
                                            </div>
                                            <span className="text-[10px] text-gray-600 flex-shrink-0 w-16">{timeAgo(script.last_run)}</span>
                                            <div className="flex items-center gap-0.5 flex-shrink-0">
                                                <button onClick={() => handleRun(script)} className="p-1.5 text-emerald-400 hover:bg-emerald-500/10 rounded" title="Run"><Play className="w-3.5 h-3.5" /></button>
                                                <button onClick={() => { setEditScript(script); setShowEditor(true); }} className="p-1.5 text-gray-400 hover:text-white hover:bg-white/5 rounded" title="Edit"><Edit2 className="w-3.5 h-3.5" /></button>
                                                <button onClick={() => setExpandedId(isExpanded ? null : script.id)} className="p-1.5 text-gray-400 hover:text-white hover:bg-white/5 rounded">
                                                    {isExpanded ? <ChevronDown className="w-3.5 h-3.5" /> : <ChevronRight className="w-3.5 h-3.5" />}
                                                </button>
                                            </div>
                                        </div>
                                        {isExpanded && (
                                            <div className="px-4 pb-3">
                                                <pre className="bg-[#0d0d0d] border border-[#333] rounded-lg p-4 text-xs text-emerald-300 font-mono overflow-x-auto whitespace-pre-wrap leading-relaxed max-h-48 overflow-y-auto">
                                                    {script.code}
                                                </pre>
                                                <div className="flex items-center gap-2 mt-2">
                                                    <button onClick={() => handleRun(script)} className="flex items-center gap-1.5 px-3 py-1.5 text-[11px] font-medium text-white bg-emerald-600 hover:bg-emerald-500 rounded-lg"><Play className="w-3 h-3" /> Run on Selected</button>
                                                    <button onClick={() => { navigator.clipboard.writeText(script.code); success('Copied to clipboard'); }} className="flex items-center gap-1.5 px-3 py-1.5 text-[11px] text-gray-400 hover:text-white hover:bg-white/5 rounded-lg border border-[#333]"><Copy className="w-3 h-3" /> Copy</button>
                                                    <button onClick={() => { setEditScript(script); setShowEditor(true); }} className="flex items-center gap-1.5 px-3 py-1.5 text-[11px] text-gray-400 hover:text-white hover:bg-white/5 rounded-lg border border-[#333]"><Edit2 className="w-3 h-3" /> Edit</button>
                                                </div>
                                            </div>
                                        )}
                                    </div>
                                );
                            })
                        )}
                    </div>
                </div>
            </div>

            {showEditor && <ScriptEditor script={editScript} onSave={handleSave} onClose={() => { setShowEditor(false); setEditScript(null); }} />}

            {/* Script execution output panel */}
            {runResult && (
                <div className="fixed inset-x-0 bottom-0 z-40 bg-[#1a1a1a] border-t border-[#333] shadow-2xl animate-fadeIn" style={{ maxHeight: '40vh' }}>
                    <div className="flex items-center justify-between px-4 py-2 border-b border-[#333]">
                        <div className="flex items-center gap-2">
                            {running ? (
                                <Loader2 className="w-3.5 h-3.5 animate-spin text-[#e05246]" />
                            ) : (
                                <Check className={`w-3.5 h-3.5 ${runResult.result.exitCode === 0 ? 'text-emerald-400' : 'text-red-400'}`} />
                            )}
                            <span className="text-xs font-medium text-white">{runResult.script}</span>
                            {!running && (
                                <span className={`text-[10px] px-1.5 py-0.5 rounded ${runResult.result.exitCode === 0 ? 'bg-emerald-500/15 text-emerald-400' : 'bg-red-500/15 text-red-400'}`}>
                                    Exit {runResult.result.exitCode}
                                </span>
                            )}
                            {runResult.result.timedOut && (
                                <span className="text-[10px] px-1.5 py-0.5 rounded bg-amber-500/15 text-amber-400">Timed Out</span>
                            )}
                        </div>
                        <button onClick={() => { setRunResult(null); setRunning(false); }} className="p-1 text-gray-400 hover:text-white hover:bg-white/5 rounded">
                            <X className="w-3.5 h-3.5" />
                        </button>
                    </div>
                    <div className="overflow-y-auto p-4 space-y-2" style={{ maxHeight: 'calc(40vh - 40px)' }}>
                        {running ? (
                            <div className="text-xs text-gray-500 py-4 text-center">Executing script on agent...</div>
                        ) : (
                            <>
                                {runResult.result.stdout && (
                                    <pre className="text-xs font-mono text-gray-300 whitespace-pre-wrap leading-relaxed">{runResult.result.stdout}</pre>
                                )}
                                {runResult.result.stderr && (
                                    <pre className="text-xs font-mono text-red-400 whitespace-pre-wrap leading-relaxed">{runResult.result.stderr}</pre>
                                )}
                                {!runResult.result.stdout && !runResult.result.stderr && (
                                    <div className="text-xs text-gray-600 py-4 text-center">No output</div>
                                )}
                            </>
                        )}
                    </div>
                </div>
            )}

            {ctxMenu && (
                <div ref={ctxRef} className="fixed bg-[#252525] border border-[#444] rounded-lg shadow-xl z-50 py-1 w-40" style={{ left: ctxMenu.x, top: ctxMenu.y }}>
                    <button className="w-full flex items-center gap-2 px-3 py-2 text-xs text-gray-300 hover:bg-white/5" onClick={() => { handleRun(ctxMenu.script); setCtxMenu(null); }}><Play className="w-3.5 h-3.5 text-emerald-400" /> Run</button>
                    <button className="w-full flex items-center gap-2 px-3 py-2 text-xs text-gray-300 hover:bg-white/5" onClick={() => { setEditScript(ctxMenu.script); setShowEditor(true); setCtxMenu(null); }}><Edit2 className="w-3.5 h-3.5" /> Edit</button>
                    <button className="w-full flex items-center gap-2 px-3 py-2 text-xs text-gray-300 hover:bg-white/5" onClick={() => handleDuplicate(ctxMenu.script)}><Copy className="w-3.5 h-3.5" /> Duplicate</button>
                    <button className="w-full flex items-center gap-2 px-3 py-2 text-xs text-gray-300 hover:bg-white/5" onClick={() => { navigator.clipboard.writeText(ctxMenu.script.code); success('Copied'); setCtxMenu(null); }}><Copy className="w-3.5 h-3.5" /> Copy Code</button>
                    <div className="border-t border-[#444] my-1" />
                    <button className="w-full flex items-center gap-2 px-3 py-2 text-xs text-red-400 hover:bg-red-500/10" onClick={() => { if (confirm(`Delete "${ctxMenu.script.name}"?`)) handleDelete(ctxMenu.script.id); }}><Trash2 className="w-3.5 h-3.5" /> Delete</button>
                </div>
            )}
        </div>
    );
}
