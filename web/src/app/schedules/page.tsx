'use client';
import { getAccessToken } from '@/lib/auth-store';

/**
 * Scheduled Tasks page.
 *
 * Cron-like scheduler for running Toolbox scripts on agents
 * at scheduled intervals. Supports one-time and recurring schedules.
 */

import { useState, useMemo, useCallback, useEffect } from 'react';
import {
    CalendarClock,
    Plus,
    Play,
    Pause,
    Trash2,
    Edit2,
    CheckCircle2,
    XCircle,
    Clock,
    Search,
    Loader2,
    X,
    Check,
    RefreshCw,
    Zap,
} from 'lucide-react';
import { useToast } from '@/components/toast';
import { api, type ScheduledTask } from '@/lib/api';

const STATUS_COLORS: Record<string, string> = {
    active: 'text-emerald-400',
    paused: 'text-amber-400',
    completed: 'text-blue-400',
    error: 'text-red-400',
};

const TYPE_CONFIG: Record<string, { label: string; color: string }> = {
    script: { label: 'Script', color: 'bg-blue-500/15 text-blue-300' },
    patch: { label: 'Patch', color: 'bg-emerald-500/15 text-emerald-300' },
    scan: { label: 'Scan', color: 'bg-purple-500/15 text-purple-300' },
    backup: { label: 'Backup', color: 'bg-amber-500/15 text-amber-300' },
    restart: { label: 'Restart', color: 'bg-red-500/15 text-red-300' },
    report: { label: 'Report', color: 'bg-gray-500/15 text-gray-300' },
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
    return `${days}d ago`;
}

function timeUntil(dateStr: string | null): string {
    if (!dateStr) return 'N/A';
    const diff = new Date(dateStr).getTime() - Date.now();
    if (diff <= 0) return 'Now';
    const mins = Math.floor(diff / 60000);
    if (mins < 60) return `In ${mins}m`;
    const hours = Math.floor(mins / 60);
    if (hours < 24) return `In ${hours}h`;
    const days = Math.floor(hours / 24);
    return `In ${days}d`;
}

// ─── Create/Edit Task Modal ──────────────────────
function TaskModal({
    task,
    onSave,
    onClose,
}: {
    task: ScheduledTask | null;
    onSave: (data: { name: string; description?: string; task_type?: string; target_type?: string; target_value?: string; schedule?: string; config?: unknown }, id?: string) => void;
    onClose: () => void;
}) {
    const [name, setName] = useState(task?.name || '');
    const [desc, setDesc] = useState(task?.description || '');
    const [taskType, setTaskType] = useState(task?.task_type || 'script');
    const [targetType, setTargetType] = useState(task?.target_type || 'group');
    const [targetValue, setTargetValue] = useState(task?.target_value || '');
    const [schedule, setSchedule] = useState(task?.schedule || '0 * * * *');

    const handleSave = () => {
        if (!name.trim()) return;
        onSave({
            name: name.trim(),
            description: desc.trim() || undefined,
            task_type: taskType,
            target_type: targetType,
            target_value: targetValue,
            schedule,
        }, task?.id);
    };

    return (
        <div className="fixed inset-0 bg-black/70 z-50 flex items-center justify-center p-4">
            <div className="bg-[#1e1e1e] border border-[#333] rounded-xl w-full max-w-lg shadow-2xl max-h-[90vh] overflow-y-auto">
                <div className="flex items-center justify-between px-5 py-4 border-b border-[#333]">
                    <h2 className="text-sm font-semibold text-white">{task ? 'Edit Task' : 'New Scheduled Task'}</h2>
                    <button onClick={onClose} className="p-1 text-gray-400 hover:text-white"><X className="w-4 h-4" /></button>
                </div>
                <div className="p-5 space-y-4">
                    <div>
                        <label className="text-[10px] text-gray-500 uppercase tracking-wider mb-1 block">Task Name</label>
                        <input value={name} onChange={e => setName(e.target.value)} className="w-full bg-[#141414] border border-[#333] rounded-lg px-3 py-2 text-sm text-white focus:outline-none focus:border-[#e05246]" placeholder="Daily Server Cleanup" />
                    </div>
                    <div>
                        <label className="text-[10px] text-gray-500 uppercase tracking-wider mb-1 block">Description</label>
                        <input value={desc} onChange={e => setDesc(e.target.value)} className="w-full bg-[#141414] border border-[#333] rounded-lg px-3 py-2 text-sm text-white focus:outline-none focus:border-[#e05246]" placeholder="What does this task do?" />
                    </div>
                    <div className="grid grid-cols-2 gap-3">
                        <div>
                            <label className="text-[10px] text-gray-500 uppercase tracking-wider mb-1 block">Task Type</label>
                            <select value={taskType} onChange={e => setTaskType(e.target.value)} className="w-full bg-[#141414] border border-[#333] rounded-lg px-3 py-2 text-sm text-white focus:outline-none focus:border-[#e05246]">
                                {Object.entries(TYPE_CONFIG).map(([k, v]) => (<option key={k} value={k}>{v.label}</option>))}
                            </select>
                        </div>
                        <div>
                            <label className="text-[10px] text-gray-500 uppercase tracking-wider mb-1 block">Target Type</label>
                            <select value={targetType} onChange={e => setTargetType(e.target.value)} className="w-full bg-[#141414] border border-[#333] rounded-lg px-3 py-2 text-sm text-white focus:outline-none focus:border-[#e05246]">
                                <option value="group">Agent Group</option>
                                <option value="agent">Specific Agent</option>
                                <option value="all">All Agents</option>
                            </select>
                        </div>
                    </div>
                    <div>
                        <label className="text-[10px] text-gray-500 uppercase tracking-wider mb-1 block">Target (group/agent name)</label>
                        <input value={targetValue} onChange={e => setTargetValue(e.target.value)} className="w-full bg-[#141414] border border-[#333] rounded-lg px-3 py-2 text-sm text-white focus:outline-none focus:border-[#e05246]" placeholder="All Linux" />
                    </div>
                    <div>
                        <label className="text-[10px] text-gray-500 uppercase tracking-wider mb-1 block">Cron Schedule</label>
                        <input value={schedule} onChange={e => setSchedule(e.target.value)} className="w-full bg-[#141414] border border-[#333] rounded-lg px-3 py-2 text-sm text-white font-mono focus:outline-none focus:border-[#e05246]" placeholder="0 2 * * *" />
                        <p className="text-[9px] text-gray-600 mt-1">min hour day month weekday (0 2 * * * = daily at 2:00 AM)</p>
                    </div>
                </div>
                <div className="flex items-center justify-end gap-2 px-5 py-4 border-t border-[#333]">
                    <button onClick={onClose} className="px-4 py-2 text-sm text-gray-400 hover:text-white hover:bg-white/5 rounded-lg">Cancel</button>
                    <button onClick={handleSave} disabled={!name.trim()} className="flex items-center gap-2 px-4 py-2 text-sm font-medium text-white bg-[#e05246] hover:bg-[#c94539] rounded-lg disabled:opacity-40">
                        <Check className="w-3.5 h-3.5" /> {task ? 'Save' : 'Create Task'}
                    </button>
                </div>
            </div>
        </div>
    );
}

// ─── Main Component ──────────────────────────────
export default function ScheduledTasksPage() {
    const { success, info, error: toastError } = useToast();
    const [tasks, setTasks] = useState<ScheduledTask[]>([]);
    const [loading, setLoading] = useState(true);
    const [search, setSearch] = useState('');
    const [showModal, setShowModal] = useState(false);
    const [editTask, setEditTask] = useState<ScheduledTask | null>(null);

    const fetchTasks = useCallback(async () => {
        try {
            const token = getAccessToken();
            if (token) api.setToken(token);
            const data = await api.getScheduledTasks();
            setTasks(data);
        } catch (e) {
            console.error('Failed to load tasks:', e);
        } finally {
            setLoading(false);
        }
    }, []);

    useEffect(() => { fetchTasks(); }, [fetchTasks]);

    const filtered = useMemo(() => {
        if (!search) return tasks;
        const q = search.toLowerCase();
        return tasks.filter(t => t.name.toLowerCase().includes(q) || (t.description || '').toLowerCase().includes(q));
    }, [tasks, search]);

    const handleSave = useCallback(async (data: { name: string; description?: string; task_type?: string; target_type?: string; target_value?: string; schedule?: string; config?: unknown }, id?: string) => {
        try {
            const token = getAccessToken();
            if (token) api.setToken(token);
            if (id) {
                await api.updateScheduledTask(id, data);
            } else {
                await api.createScheduledTask(data);
            }
            await fetchTasks();
            setShowModal(false);
            setEditTask(null);
            success('Task saved', data.name);
        } catch (e) {
            toastError('Failed', String(e));
        }
    }, [success, toastError, fetchTasks]);

    const handleDelete = useCallback(async (id: string) => {
        try {
            const token = getAccessToken();
            if (token) api.setToken(token);
            await api.deleteScheduledTask(id);
            await fetchTasks();
            success('Task deleted');
        } catch (e) {
            toastError('Failed', String(e));
        }
    }, [success, toastError, fetchTasks]);

    const handleToggleStatus = useCallback(async (task: ScheduledTask) => {
        const newStatus = task.status === 'active' ? 'paused' : 'active';
        try {
            const token = getAccessToken();
            if (token) api.setToken(token);
            await api.updateScheduledTask(task.id, { status: newStatus });
            await fetchTasks();
            info(newStatus === 'active' ? 'Task resumed' : 'Task paused', task.name);
        } catch (e) {
            toastError('Failed', String(e));
        }
    }, [info, toastError, fetchTasks]);

    const activeCount = tasks.filter(t => t.status === 'active').length;

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
                        <CalendarClock className="w-5 h-5 text-[#e05246]" />
                        Scheduled Tasks
                    </h1>
                    <p className="text-xs text-gray-500 mt-0.5">{tasks.length} tasks • {activeCount} active</p>
                </div>
                <button onClick={() => { setEditTask(null); setShowModal(true); }} className="flex items-center gap-1.5 px-3 py-2 text-xs font-medium text-white bg-[#e05246] hover:bg-[#c94539] rounded-lg">
                    <Plus className="w-3.5 h-3.5" /> New Task
                </button>
            </header>

            <div className="p-6 space-y-4">
                <div className="relative">
                    <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-gray-600" />
                    <input value={search} onChange={e => setSearch(e.target.value)} placeholder="Search tasks..." className="w-full bg-[#1e1e1e] border border-[#333] rounded-lg pl-9 pr-3 py-2.5 text-xs text-gray-300 placeholder-gray-600 focus:outline-none focus:border-[#e05246]" />
                </div>

                <div className="space-y-2">
                    {filtered.map(task => {
                        const type = TYPE_CONFIG[task.task_type] || TYPE_CONFIG.script;
                        return (
                            <div key={task.id} className="bg-[#1e1e1e] border border-[#2a2a2a] rounded-xl p-4 hover:border-[#444] transition-colors">
                                <div className="flex items-center gap-3">
                                    <button onClick={() => handleToggleStatus(task)} className={`flex-shrink-0 p-1.5 rounded ${task.status === 'active' ? 'text-emerald-400 hover:bg-emerald-500/10' : 'text-amber-400 hover:bg-amber-500/10'}`} title={task.status === 'active' ? 'Pause' : 'Resume'}>
                                        {task.status === 'active' ? <Play className="w-4 h-4 fill-current" /> : <Pause className="w-4 h-4" />}
                                    </button>

                                    <div className="flex-1 min-w-0">
                                        <div className="flex items-center gap-2">
                                            <span className="text-sm font-medium text-white">{task.name}</span>
                                            <span className={`px-1.5 py-0.5 rounded text-[9px] font-medium ${type.color}`}>{type.label}</span>
                                            <span className={`text-[10px] ${STATUS_COLORS[task.status] || 'text-gray-400'}`}>● {task.status}</span>
                                        </div>
                                        <div className="flex items-center gap-3 mt-0.5 text-[10px] text-gray-500">
                                            <span className="font-mono">{task.schedule}</span>
                                            <span>Target: {task.target_value || task.target_type}</span>
                                            {task.description && <span className="truncate">— {task.description}</span>}
                                        </div>
                                    </div>

                                    <div className="flex items-center gap-4 flex-shrink-0 text-right">
                                        <div>
                                            <div className="text-[10px] text-gray-500">Last Run</div>
                                            <div className="text-xs text-gray-300">{timeAgo(task.last_run)}</div>
                                        </div>
                                        <div>
                                            <div className="text-[10px] text-gray-500">Next Run</div>
                                            <div className="text-xs text-gray-300">{task.status === 'paused' ? 'Paused' : timeUntil(task.next_run)}</div>
                                        </div>
                                        <div className="flex items-center gap-0.5">
                                            <button onClick={() => { setEditTask(task); setShowModal(true); }} className="p-1.5 text-gray-400 hover:text-white hover:bg-white/5 rounded" title="Edit"><Edit2 className="w-3 h-3" /></button>
                                            <button onClick={() => { if (confirm(`Delete "${task.name}"?`)) handleDelete(task.id); }} className="p-1.5 text-red-400/50 hover:text-red-400 hover:bg-red-500/10 rounded" title="Delete"><Trash2 className="w-3 h-3" /></button>
                                        </div>
                                    </div>
                                </div>
                            </div>
                        );
                    })}
                </div>

                {filtered.length === 0 && (
                    <div className="flex flex-col items-center justify-center py-12 text-gray-600">
                        <CalendarClock className="w-10 h-10 mb-2 opacity-30" />
                        <span className="text-sm">{tasks.length === 0 ? 'No scheduled tasks yet' : 'No tasks found'}</span>
                    </div>
                )}
            </div>

            {showModal && <TaskModal task={editTask} onSave={handleSave} onClose={() => { setShowModal(false); setEditTask(null); }} />}
        </div>
    );
}
