'use client';
import { getAccessToken } from '@/lib/auth-store';

/**
 * Agent Groups management page.
 *
 * Create, edit, and organize agents into logical groups
 * for easier management and bulk operations.
 */

import { useState, useMemo, useCallback, useEffect } from 'react';
import {
    FolderTree,
    Plus,
    Edit2,
    Trash2,
    Users,
    Monitor,
    Search,
    X,
    Check,
    ChevronRight,
    ChevronDown,
    FolderOpen,
    Folder,
    MoreVertical,
    GripVertical,
    Zap,
    Settings2,
    Loader2,
} from 'lucide-react';
import { useToast } from '@/components/toast';
import { api, type AgentGroup } from '@/lib/api';

interface GroupRule {
    field: 'os' | 'name' | 'ip' | 'version' | 'tag';
    operator: 'equals' | 'contains' | 'starts_with' | 'regex';
    value: string;
}

const GROUP_COLORS = ['#e05246', '#3b82f6', '#22c55e', '#f59e0b', '#8b5cf6', '#ec4899', '#06b6d4', '#64748b'];
const GROUP_ICONS = ['📁', '🔴', '💻', '🐧', '⚡', '🖥️', '🌐', '🔧', '📊', '🛡️', '☁️', '🎯'];

// ─── Group Modal ─────────────────────────────────
function GroupModal({
    group,
    onSave,
    onClose,
}: {
    group: AgentGroup | null;
    onSave: (data: { name: string; description?: string; color?: string; icon?: string; filter_criteria?: unknown }, id?: string) => void;
    onClose: () => void;
}) {
    const rules = (group?.filter_criteria as GroupRule[] | null) || [];
    const [name, setName] = useState(group?.name || '');
    const [desc, setDesc] = useState(group?.description || '');
    const [color, setColor] = useState(group?.color || '#e05246');
    const [icon, setIcon] = useState(group?.icon || '📁');
    const [localRules, setLocalRules] = useState<GroupRule[]>(rules.length > 0 ? rules : [{ field: 'name', operator: 'contains', value: '' }]);

    const updateRule = (i: number, update: Partial<GroupRule>) => {
        setLocalRules(prev => prev.map((r, idx) => idx === i ? { ...r, ...update } : r));
    };

    const handleSave = () => {
        if (!name.trim()) return;
        onSave({
            name: name.trim(),
            description: desc.trim(),
            color,
            icon,
            filter_criteria: localRules.filter(r => r.value.trim()),
        }, group?.id);
    };

    return (
        <div className="fixed inset-0 bg-black/70 z-50 flex items-center justify-center p-4">
            <div className="bg-[#1e1e1e] border border-[#333] rounded-xl w-full max-w-lg shadow-2xl max-h-[90vh] overflow-y-auto">
                <div className="flex items-center justify-between px-5 py-4 border-b border-[#333]">
                    <h2 className="text-sm font-semibold text-white">{group ? 'Edit Group' : 'New Agent Group'}</h2>
                    <button onClick={onClose} className="p-1 text-gray-400 hover:text-white"><X className="w-4 h-4" /></button>
                </div>
                <div className="p-5 space-y-4">
                    {/* Name & Color */}
                    <div className="grid grid-cols-[1fr_auto] gap-3">
                        <div>
                            <label className="text-[10px] text-gray-500 uppercase tracking-wider mb-1 block">Group Name</label>
                            <input value={name} onChange={e => setName(e.target.value)} className="w-full bg-[#141414] border border-[#333] rounded-lg px-3 py-2 text-sm text-white focus:outline-none focus:border-[#e05246]" placeholder="Production Servers" />
                        </div>
                        <div>
                            <label className="text-[10px] text-gray-500 uppercase tracking-wider mb-1 block">Icon</label>
                            <div className="flex flex-wrap gap-1 bg-[#141414] border border-[#333] rounded-lg p-1.5">
                                {GROUP_ICONS.slice(0, 6).map(ic => (
                                    <button key={ic} onClick={() => setIcon(ic)} className={`w-7 h-7 rounded flex items-center justify-center text-sm ${icon === ic ? 'bg-[#e05246]/20 ring-1 ring-[#e05246]' : 'hover:bg-white/5'}`}>{ic}</button>
                                ))}
                            </div>
                        </div>
                    </div>

                    <div>
                        <label className="text-[10px] text-gray-500 uppercase tracking-wider mb-1 block">Description</label>
                        <input value={desc} onChange={e => setDesc(e.target.value)} className="w-full bg-[#141414] border border-[#333] rounded-lg px-3 py-2 text-sm text-white focus:outline-none focus:border-[#e05246]" placeholder="Describe this group..." />
                    </div>

                    {/* Color picker */}
                    <div>
                        <label className="text-[10px] text-gray-500 uppercase tracking-wider mb-1 block">Color</label>
                        <div className="flex gap-2">
                            {GROUP_COLORS.map(c => (
                                <button key={c} onClick={() => setColor(c)} className={`w-6 h-6 rounded-full transition-transform ${color === c ? 'ring-2 ring-white ring-offset-2 ring-offset-[#1e1e1e] scale-110' : 'hover:scale-110'}`} style={{ backgroundColor: c }} />
                            ))}
                        </div>
                    </div>

                    {/* Auto-assignment rules */}
                    <div>
                        <label className="text-[10px] text-gray-500 uppercase tracking-wider mb-2 block flex items-center gap-1">
                            <Zap className="w-3 h-3" /> Auto-Assignment Rules
                        </label>
                        <div className="space-y-2">
                            {localRules.map((rule, i) => (
                                <div key={i} className="flex items-center gap-2">
                                    <select value={rule.field} onChange={e => updateRule(i, { field: e.target.value as GroupRule['field'] })} className="bg-[#141414] border border-[#333] rounded-lg px-2 py-1.5 text-xs text-white">
                                        <option value="name">Name</option>
                                        <option value="os">OS</option>
                                        <option value="ip">IP Address</option>
                                        <option value="version">Agent Version</option>
                                        <option value="tag">Tag</option>
                                    </select>
                                    <select value={rule.operator} onChange={e => updateRule(i, { operator: e.target.value as GroupRule['operator'] })} className="bg-[#141414] border border-[#333] rounded-lg px-2 py-1.5 text-xs text-white">
                                        <option value="equals">equals</option>
                                        <option value="contains">contains</option>
                                        <option value="starts_with">starts with</option>
                                        <option value="regex">regex</option>
                                    </select>
                                    <input value={rule.value} onChange={e => updateRule(i, { value: e.target.value })} className="flex-1 bg-[#141414] border border-[#333] rounded-lg px-2 py-1.5 text-xs text-white font-mono focus:outline-none focus:border-[#e05246]" placeholder="Value..." />
                                    {localRules.length > 1 && (
                                        <button onClick={() => setLocalRules(prev => prev.filter((_, idx) => idx !== i))} className="p-1 text-red-400/50 hover:text-red-400"><X className="w-3 h-3" /></button>
                                    )}
                                </div>
                            ))}
                        </div>
                        <button onClick={() => setLocalRules(prev => [...prev, { field: 'name', operator: 'contains', value: '' }])} className="mt-2 flex items-center gap-1 text-[10px] text-gray-500 hover:text-white">
                            <Plus className="w-3 h-3" /> Add Rule
                        </button>
                    </div>
                </div>
                <div className="flex items-center justify-end gap-2 px-5 py-4 border-t border-[#333]">
                    <button onClick={onClose} className="px-4 py-2 text-sm text-gray-400 hover:text-white hover:bg-white/5 rounded-lg">Cancel</button>
                    <button onClick={handleSave} disabled={!name.trim()} className="flex items-center gap-2 px-4 py-2 text-sm font-medium text-white bg-[#e05246] hover:bg-[#c94539] rounded-lg disabled:opacity-40">
                        <Check className="w-3.5 h-3.5" /> {group ? 'Save' : 'Create Group'}
                    </button>
                </div>
            </div>
        </div>
    );
}

// ─── Main Component ──────────────────────────────
export default function GroupsPage() {
    const { success, info, error: toastError } = useToast();
    const [groups, setGroups] = useState<AgentGroup[]>([]);
    const [loading, setLoading] = useState(true);
    const [search, setSearch] = useState('');
    const [showModal, setShowModal] = useState(false);
    const [editGroup, setEditGroup] = useState<AgentGroup | null>(null);
    const [expandedId, setExpandedId] = useState<string | null>(null);

    const fetchGroups = useCallback(async () => {
        try {
            const token = getAccessToken();
            if (token) api.setToken(token);
            const data = await api.getGroups();
            setGroups(data);
        } catch (e) {
            console.error('Failed to load groups:', e);
        } finally {
            setLoading(false);
        }
    }, []);

    useEffect(() => { fetchGroups(); }, [fetchGroups]);

    const filtered = useMemo(() => {
        if (!search) return groups;
        const q = search.toLowerCase();
        return groups.filter(g => g.name.toLowerCase().includes(q) || (g.description || '').toLowerCase().includes(q));
    }, [groups, search]);

    const handleSave = useCallback(async (data: { name: string; description?: string; color?: string; icon?: string; filter_criteria?: unknown }, id?: string) => {
        try {
            const token = getAccessToken();
            if (token) api.setToken(token);
            if (id) {
                await api.updateGroup(id, data);
            } else {
                await api.createGroup(data);
            }
            await fetchGroups();
            setShowModal(false);
            setEditGroup(null);
            success('Group saved', data.name);
        } catch (e) {
            toastError('Failed', String(e));
        }
    }, [success, toastError, fetchGroups]);

    const handleDelete = useCallback(async (id: string) => {
        try {
            const token = getAccessToken();
            if (token) api.setToken(token);
            await api.deleteGroup(id);
            await fetchGroups();
            success('Group deleted');
        } catch (e) {
            toastError('Failed', String(e));
        }
    }, [success, toastError, fetchGroups]);

    const totalAgents = useMemo(() => groups.reduce((s, g) => s + (g.agent_count || 0), 0), [groups]);

    if (loading) {
        return (
            <div className="flex items-center justify-center h-64">
                <Loader2 className="w-6 h-6 animate-spin text-gray-600" />
            </div>
        );
    }

    return (
        <div className="flex flex-col h-full overflow-y-auto">
            {/* Header */}
            <header className="flex flex-wrap items-center justify-between gap-3 px-6 py-4 border-b border-[#333] flex-shrink-0">
                <div>
                    <h1 className="text-lg font-bold text-white flex items-center gap-2">
                        <FolderTree className="w-5 h-5 text-[#e05246]" />
                        Agent Groups
                    </h1>
                    <p className="text-xs text-gray-500 mt-0.5">{groups.length} groups • {totalAgents} agents organized</p>
                </div>
                <button onClick={() => { setEditGroup(null); setShowModal(true); }} className="flex items-center gap-1.5 px-3 py-2 text-xs font-medium text-white bg-[#e05246] hover:bg-[#c94539] rounded-lg transition-colors">
                    <Plus className="w-3.5 h-3.5" /> New Group
                </button>
            </header>

            <div className="p-6 space-y-4">
                {/* Search */}
                <div className="relative">
                    <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-gray-600" />
                    <input value={search} onChange={e => setSearch(e.target.value)} placeholder="Search groups..." className="w-full bg-[#1e1e1e] border border-[#333] rounded-lg pl-9 pr-3 py-2.5 text-xs text-gray-300 placeholder-gray-600 focus:outline-none focus:border-[#e05246]" />
                </div>

                {/* Groups list */}
                <div className="space-y-2">
                    {filtered.map(group => {
                        const isExpanded = expandedId === group.id;
                        const rules = (group.filter_criteria as GroupRule[] | null) || [];
                        return (
                            <div key={group.id} className="bg-[#1e1e1e] border border-[#2a2a2a] rounded-xl overflow-hidden hover:border-[#444] transition-colors">
                                {/* Group header */}
                                <div className="flex items-center gap-3 px-4 py-3 cursor-pointer" onClick={() => setExpandedId(isExpanded ? null : group.id)}>
                                    <button className="p-0.5 text-gray-500">
                                        {isExpanded ? <ChevronDown className="w-3.5 h-3.5" /> : <ChevronRight className="w-3.5 h-3.5" />}
                                    </button>
                                    <div className="w-8 h-8 rounded-lg flex items-center justify-center text-lg" style={{ backgroundColor: `${group.color || '#e05246'}15` }}>
                                        {group.icon || '📁'}
                                    </div>
                                    <div className="flex-1 min-w-0">
                                        <div className="flex items-center gap-2">
                                            <span className="text-sm font-medium text-white">{group.name}</span>
                                            <div className="w-2 h-2 rounded-full" style={{ backgroundColor: group.color || '#e05246' }} />
                                        </div>
                                        <span className="text-[10px] text-gray-500">{group.description || ''}</span>
                                    </div>
                                    <div className="flex items-center gap-4 flex-shrink-0">
                                        <div className="text-right">
                                            <div className="text-sm font-bold text-white">{group.agent_count || 0}</div>
                                            <div className="text-[9px] text-gray-600">agents</div>
                                        </div>
                                        <div className="flex items-center gap-0.5">
                                            <button onClick={e => { e.stopPropagation(); setEditGroup(group); setShowModal(true); }} className="p-1.5 text-gray-400 hover:text-white hover:bg-white/5 rounded" title="Edit">
                                                <Edit2 className="w-3 h-3" />
                                            </button>
                                            <button onClick={e => { e.stopPropagation(); if (confirm(`Delete "${group.name}"?`)) handleDelete(group.id); }} className="p-1.5 text-red-400/50 hover:text-red-400 hover:bg-red-500/10 rounded" title="Delete">
                                                <Trash2 className="w-3 h-3" />
                                            </button>
                                        </div>
                                    </div>
                                </div>

                                {/* Expanded details */}
                                {isExpanded && (
                                    <div className="px-4 pb-4 border-t border-[#2a2a2a] pt-3">
                                        <div className="grid grid-cols-2 gap-3 mb-3">
                                            <div className="text-[10px] text-gray-500"><span className="text-gray-400 font-medium">Created:</span> {new Date(group.created_at).toLocaleDateString()}</div>
                                            <div className="text-[10px] text-gray-500"><span className="text-gray-400 font-medium">Rules:</span> {rules.length} active</div>
                                        </div>
                                        {rules.length > 0 && (
                                            <div>
                                                <div className="text-[9px] text-gray-600 uppercase tracking-wider mb-1.5">Auto-Assignment Rules</div>
                                                <div className="space-y-1">
                                                    {rules.map((rule, i) => (
                                                        <div key={i} className="flex items-center gap-2 text-[11px]">
                                                            <span className="px-1.5 py-0.5 bg-blue-500/10 text-blue-300 rounded text-[9px] font-mono">{rule.field}</span>
                                                            <span className="text-gray-500">{rule.operator.replace('_', ' ')}</span>
                                                            <span className="text-white font-mono">&quot;{rule.value}&quot;</span>
                                                        </div>
                                                    ))}
                                                </div>
                                            </div>
                                        )}
                                    </div>
                                )}
                            </div>
                        );
                    })}
                </div>

                {filtered.length === 0 && (
                    <div className="flex flex-col items-center justify-center py-12 text-gray-600">
                        <FolderTree className="w-10 h-10 mb-2 opacity-30" />
                        <span className="text-sm">{groups.length === 0 ? 'No groups created yet' : 'No groups found'}</span>
                    </div>
                )}
            </div>

            {showModal && <GroupModal group={editGroup} onSave={handleSave} onClose={() => { setShowModal(false); setEditGroup(null); }} />}
        </div>
    );
}
